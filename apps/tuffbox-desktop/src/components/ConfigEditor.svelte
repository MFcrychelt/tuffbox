<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    FileCode2, RefreshCw, Save, Search, RotateCcw, AlertTriangle, FileSearch,
    ChevronRight, ChevronDown, File, Folder, FolderOpen, History, Camera, Code2,
  } from "lucide-svelte";
  import { onDestroy, tick } from "svelte";
  import { EditorView } from "@codemirror/view";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { ideStageRequest, projectPath, tuneDirty } from "../lib/store";
  import CodeMirror from "svelte-codemirror-editor";
  import { json } from "@codemirror/lang-json";
  import { javascript } from "@codemirror/lang-javascript";
  import { yaml } from "@codemirror/lang-yaml";
  import { StreamLanguage, LanguageSupport } from "@codemirror/language";
  import { toml } from "@codemirror/legacy-modes/mode/toml";
  import { properties as propertiesMode } from "@codemirror/legacy-modes/mode/properties";
  import { oneDark } from "@codemirror/theme-one-dark";

  /** CM6 doesn't map legacy token "quote" → string; remap so key=value actually colors. */
  const mcProperties = {
    name: "mc-properties",
    startState: propertiesMode.startState,
    token(stream: any, state: any) {
      const t = propertiesMode.token(stream, state);
      if (t === "quote") return "string";
      if (t === "def") return "property";
      return t;
    },
  };

  function propsLang(): LanguageSupport {
    return new LanguageSupport(StreamLanguage.define(mcProperties as any));
  }

  function tomlLang(): LanguageSupport {
    return new LanguageSupport(StreamLanguage.define(toml));
  }

  type ConfigFile = {
    path: string;
    name: string;
    extension: string;
    size: number;
    modified?: number | null;
  };

  type SearchHit = {
    path: string;
    line: number;
    text: string;
  };

  type FlatNode = {
    name: string;
    fullPath: string;
    isDir: boolean;
    depth: number;
    expanded: boolean;
    file?: ConfigFile;
    isRoot?: boolean;
  };

  type Snippet = { id: string; label: string; body: string };

  const ROOT_CHIPS = ["config", "defaultconfigs", "kubejs", "scripts", "overrides"] as const;

  const SNIPPETS: Snippet[] = [
    {
      id: "kjs-recipes",
      label: "KubeJS · ServerEvents.recipes",
      body: `ServerEvents.recipes(event => {
  // event.remove({ id: 'modid:recipe_id' })
  // event.shaped('minecraft:diamond', ['AAA', 'A A', 'AAA'], { A: 'minecraft:stone' })
})
`,
    },
    {
      id: "kjs-remove",
      label: "KubeJS · remove by id",
      body: `ServerEvents.recipes(event => {
  event.remove({ id: 'modid:recipe_id' })
})
`,
    },
    {
      id: "kjs-shaped",
      label: "KubeJS · shaped",
      body: `ServerEvents.recipes(event => {
  event.shaped('minecraft:diamond', [
    'AAA',
    'A A',
    'AAA'
  ], {
    A: 'minecraft:stone'
  })
})
`,
    },
    {
      id: "kjs-modify",
      label: "KubeJS · item.modify",
      body: `ItemEvents.modification(event => {
  event.modify('minecraft:diamond', item => {
    item.maxStackSize = 16
  })
})
`,
    },
    {
      id: "ct-remove",
      label: "CraftTweaker · remove",
      body: `craftingTable.removeByName("modid:recipe_id");
`,
    },
    {
      id: "ct-shaped",
      label: "CraftTweaker · shaped",
      body: `craftingTable.addShaped("tuffbox/example_shaped", <item:minecraft:diamond>, [
  [<item:minecraft:stone>, <item:minecraft:stone>, <item:minecraft:stone>],
  [<item:minecraft:stone>, <item:minecraft:air>, <item:minecraft:stone>],
  [<item:minecraft:stone>, <item:minecraft:stone>, <item:minecraft:stone>]
]);
`,
    },
  ];

  let files: ConfigFile[] = [];
  let selected: ConfigFile | null = null;
  let content = "";
  let originalContent = "";
  let filter = "";
  let rootFilter: string | null = null;
  let loading = false;
  let saving = false;
  let formatting = false;
  let error: string | null = null;
  let message: string | null = null;
  let lastSnapshotId: string | null = null;
  let lastLoadedPath: string | null = null;

  let searchQuery = "";
  let searchResults: SearchHit[] = [];
  let searchLoading = false;
  let searchError: string | null = null;

  let expandedDirs = new Set<string>();
  let flatTree: FlatNode[] = [];

  let confirmOpen = false;
  let pendingFile: ConfigFile | null = null;
  let pendingJumpLine: number | null = null;
  let highlightLine: number | null = null;
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;

  let lintIssues: { severity: string; code: string; message: string; line?: number | null }[] = [];
  let lintLoading = false;

  let cmView: EditorView | null = null;
  let showSnippets = false;

  function buildFlatTree(fileList: ConfigFile[], filterQuery: string, rootPrefix: string | null): FlatNode[] {
    const q = filterQuery.toLowerCase().trim();
    let list = fileList;
    if (rootPrefix) {
      list = list.filter((f) => f.path === rootPrefix || f.path.startsWith(rootPrefix + "/"));
    }
    if (q) {
      list = list.filter(
        (f) => f.path.toLowerCase().includes(q) || f.name.toLowerCase().includes(q),
      );
    }

    const dirs = new Map<string, { expanded: boolean; children: ConfigFile[] }>();

    for (const file of list) {
      const parts = file.path.split("/");
      for (let i = 0; i < parts.length - 1; i++) {
        const dirPath = parts.slice(0, i + 1).join("/");
        if (!dirs.has(dirPath)) {
          dirs.set(dirPath, { expanded: expandedDirs.has(dirPath) || !!q, children: [] });
        }
      }
      if (parts.length > 1) {
        const parentPath = parts.slice(0, -1).join("/");
        dirs.get(parentPath)?.children.push(file);
      }
    }

    if (q) {
      for (const file of list) {
        const parts = file.path.split("/");
        for (let i = 0; i < parts.length - 1; i++) {
          expandedDirs.add(parts.slice(0, i + 1).join("/"));
        }
      }
      for (const [dirPath, dir] of dirs) {
        dir.expanded = expandedDirs.has(dirPath) || true;
      }
    }

    const result: FlatNode[] = [];
    function walk(dirPath: string, depth: number) {
      const dir = dirs.get(dirPath);
      if (!dir) return;
      // Skip empty dirs when filtering
      if (q || rootPrefix) {
        const hasMatch =
          dir.children.length > 0 ||
          [...dirs.keys()].some((k) => k.startsWith(dirPath + "/"));
        if (!hasMatch) return;
      }
      const name = dirPath.split("/").pop() ?? dirPath;
      result.push({
        name,
        fullPath: dirPath,
        isDir: true,
        depth,
        expanded: dir.expanded,
        isRoot: depth === 0,
      });
      if (dir.expanded) {
        const children = [...dir.children].sort((a, b) => {
          const aZero = (a.size ?? 0) === 0 ? 1 : 0;
          const bZero = (b.size ?? 0) === 0 ? 1 : 0;
          if (aZero !== bZero) return aZero - bZero;
          return a.name.localeCompare(b.name);
        });
        for (const child of children) {
          const childName = child.path.split("/").pop() ?? child.path;
          result.push({
            name: childName,
            fullPath: child.path,
            isDir: false,
            depth: depth + 1,
            expanded: false,
            file: child,
          });
        }
        const subDirs = [...dirs.keys()]
          .filter((k) => k.startsWith(dirPath + "/") && !k.slice(dirPath.length + 1).includes("/"))
          .sort();
        for (const sub of subDirs) walk(sub, depth + 1);
      }
    }

    const topDirs = [...dirs.keys()]
      .filter((k) => k.split("/").length === 1)
      .sort();
    for (const d of topDirs) walk(d, 0);

    const topLevelFiles = list
      .filter((f) => !f.path.includes("/"))
      .sort((a, b) => {
        const aZero = (a.size ?? 0) === 0 ? 1 : 0;
        const bZero = (b.size ?? 0) === 0 ? 1 : 0;
        if (aZero !== bZero) return aZero - bZero;
        return a.name.localeCompare(b.name);
      });
    for (const f of topLevelFiles) {
      result.push({ name: f.path, fullPath: f.path, isDir: false, depth: 0, expanded: false, file: f });
    }

    return result;
  }

  function toggleDir(fullPath: string) {
    if (expandedDirs.has(fullPath)) expandedDirs.delete(fullPath);
    else expandedDirs.add(fullPath);
    flatTree = buildFlatTree(files, filter, rootFilter);
  }

  function setRootChip(root: string | null) {
    rootFilter = rootFilter === root ? null : root;
    if (rootFilter) expandedDirs.add(rootFilter);
    flatTree = buildFlatTree(files, filter, rootFilter);
  }

  $: flatTree = buildFlatTree(files, filter, rootFilter);
  $: presentRoots = ROOT_CHIPS.filter((r) => files.some((f) => f.path === r || f.path.startsWith(r + "/")));

  function langForFile(file: ConfigFile | null) {
    if (!file) return undefined;
    const ext = file.extension?.toLowerCase() ?? "";
    switch (ext) {
      case "json":
      case "json5":
        return json();
      case "js":
      case "zs":
        return javascript();
      case "yaml":
      case "yml":
        return yaml();
      case "toml":
        return tomlLang();
      // Minecraft options.txt + mod configs: key=value / key:value
      case "txt":
      case "properties":
      case "cfg":
      case "conf":
      case "ini":
        return propsLang();
      default:
        return undefined;
    }
  }

  function looksLikeProps(text: string): boolean {
    const lines = text.split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith("#") && !l.startsWith("!") && !l.startsWith(";"));
    if (lines.length < 1) return false;
    const hit = lines.filter((l) => /^[\w.\[\]/-]+\s*[=:]/.test(l)).length;
    return hit / lines.length >= 0.6;
  }

  function langBadge(file: ConfigFile | null) {
    if (!file) return "text";
    const ext = (file.extension || "text").toLowerCase();
    const path = file.path.toLowerCase().replace(/\\/g, "/");
    if (path === "options.txt" || path.endsWith("/options.txt")) return "options";
    if (ext === "txt" || ext === "properties" || ext === "cfg" || ext === "conf" || ext === "ini") {
      return "props";
    }
    return ext || "text";
  }

  $: currentLang = langForFile(selected) ?? (looksLikeProps(content) ? propsLang() : undefined);
  $: dirty = content !== originalContent;
  $: tuneDirty.set(dirty);
  $: canFormat = ["json", "toml"].includes(selected?.extension?.toLowerCase() ?? "");
  $: isKubejsFile = !!selected?.path.startsWith("kubejs/");
  $: lintErrorCount = lintIssues.filter((i) => i.severity === "error").length;
  $: lintWarnCount = lintIssues.filter((i) => i.severity !== "error").length;

  onDestroy(() => {
    tuneDirty.set(false);
    if (highlightTimer) clearTimeout(highlightTimer);
  });

  async function loadFiles(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && files.length > 0) return;
    loading = true;
    error = null;
    try {
      files = await invoke("list_config_files", { path: $projectPath });
      lastLoadedPath = $projectPath;
      if (selected && !files.some((f) => f.path === selected?.path)) {
        selected = null;
        content = "";
        originalContent = "";
        lintIssues = [];
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function tryOpenFile(file: ConfigFile, line?: number) {
    if (dirty && file.path !== selected?.path) {
      pendingFile = file;
      pendingJumpLine = line ?? null;
      confirmOpen = true;
      return;
    }
    openFileInternal(file, line);
  }

  async function openFileInternal(file: ConfigFile, line?: number) {
    if (!$projectPath) return;
    selected = file;
    content = "";
    originalContent = "";
    loading = true;
    error = null;
    message = null;
    lintIssues = [];
    try {
      content = await invoke("read_config_file", {
        path: $projectPath,
        relativePath: file.path,
      });
      originalContent = content;
      await lintFile();
      if (line != null) {
        pendingJumpLine = line;
        await tick();
        jumpToLine(line);
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function onCmReady(e: CustomEvent<EditorView>) {
    cmView = e.detail;
    if (pendingJumpLine != null) {
      jumpToLine(pendingJumpLine);
    }
  }

  function jumpToLine(line: number) {
    pendingJumpLine = line;
    highlightLine = line;
    if (highlightTimer) clearTimeout(highlightTimer);
    highlightTimer = setTimeout(() => {
      highlightLine = null;
    }, 2000);

    if (!cmView) return;
    const doc = cmView.state.doc;
    const target = Math.max(1, Math.min(line, doc.lines));
    const info = doc.line(target);
    cmView.dispatch({
      selection: { anchor: info.from, head: info.to },
      effects: EditorView.scrollIntoView(info.from, { y: "center" }),
    });
    cmView.focus();
    pendingJumpLine = null;
  }

  async function saveFile() {
    if (!$projectPath || !selected || !dirty) return;
    saving = true;
    error = null;
    message = null;
    lastSnapshotId = null;
    try {
      const res: { snapshotId: string } = await invoke("write_config_file", {
        path: $projectPath,
        relativePath: selected.path,
        content,
      });
      originalContent = content;
      lastSnapshotId = res?.snapshotId ?? null;
      message = lastSnapshotId
        ? `Saved ${selected.path} · snapshot ${lastSnapshotId}`
        : `Saved ${selected.path} · snapshot created`;
      await loadFiles(true);
      selected = files.find((f) => f.path === selected?.path) ?? selected;
      await lintFile();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function resetFile() {
    content = originalContent;
    message = null;
  }

  async function formatFile() {
    if (!selected) return;
    const ext = selected.extension.toLowerCase();
    formatting = true;
    error = null;
    try {
      if (ext === "json") {
        const parsed = JSON.parse(content);
        content = JSON.stringify(parsed, null, 2) + "\n";
      } else if (ext === "toml") {
        content = await invoke("format_toml", { content });
        if (!content.endsWith("\n")) content += "\n";
      }
      message = null;
    } catch (e) {
      error = `Cannot format: ${e instanceof Error ? e.message : e}`;
    } finally {
      formatting = false;
    }
  }

  async function doSearch() {
    if (!$projectPath || !searchQuery.trim()) return;
    searchLoading = true;
    searchError = null;
    searchResults = [];
    try {
      searchResults = await invoke("search_in_configs", {
        path: $projectPath,
        query: searchQuery.trim(),
      });
    } catch (e) {
      searchError = String(e);
    } finally {
      searchLoading = false;
    }
  }

  function openSearchHit(hit: SearchHit) {
    const file = files.find((f) => f.path === hit.path);
    if (file) tryOpenFile(file, hit.line);
  }

  function openLintIssue(issue: { line?: number | null }) {
    if (issue.line) jumpToLine(issue.line);
  }

  async function lintFile() {
    if (!$projectPath || !selected) return;
    lintLoading = true;
    try {
      lintIssues = await invoke("lint_config", {
        path: $projectPath,
        relativePath: selected.path,
      });
    } catch {
      lintIssues = [];
    } finally {
      lintLoading = false;
    }
  }

  function insertSnippet(snippet: Snippet) {
    showSnippets = false;
    if (!cmView) {
      content = content + (content.endsWith("\n") || !content ? "" : "\n") + snippet.body;
      return;
    }
    const pos = cmView.state.selection.main.head;
    cmView.dispatch({
      changes: { from: pos, insert: snippet.body },
      selection: { anchor: pos + snippet.body.length },
    });
    content = cmView.state.doc.toString();
  }

  async function insertFromRecipeGenerator() {
    showSnippets = false;
    try {
      const res: any = await invoke("generate_kubejs_recipe_script", {
        kind: "remove",
        recipeIds: ["modid:example_recipe"],
        newItem: null,
        count: null,
      });
      const script = typeof res === "string" ? res : (res?.content ?? res?.script ?? "");
      if (!script) {
        error = "Recipe generator returned empty script.";
        return;
      }
      insertSnippet({ id: "gen", label: "gen", body: String(script).endsWith("\n") ? String(script) : String(script) + "\n" });
    } catch (e) {
      error = String(e);
    }
  }

  function openHistory() {
    ideStageRequest.set("history");
  }

  function openSnapshots() {
    ideStageRequest.set("snapshots");
  }

  function formatSize(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  $: if ($projectPath && lastLoadedPath !== $projectPath) loadFiles(true);

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      if (dirty && selected) saveFile();
    }
  }

  function handleCmChange(e: CustomEvent<string>) {
    content = e.detail;
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="config-editor">
  <div class="toolbar">
    <div class="title">
      <FileCode2 size={18} />
      <span>Tune · configs</span>
    </div>
    <div class="toolbar-actions">
      <button class="ghost" on:click={() => loadFiles(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        Refresh
      </button>
      <button
        class="secondary"
        class:lint-bad={lintErrorCount > 0}
        class:lint-warn={lintErrorCount === 0 && lintWarnCount > 0}
        on:click={lintFile}
        disabled={!selected || lintLoading}
        title={lintIssues.length ? `${lintIssues.length} issue(s)` : "Lint config"}
      >
        <AlertTriangle size={16} />
        {#if lintLoading}
          …
        {:else if lintIssues.length > 0}
          {lintIssues.length} issues
        {:else}
          Lint
        {/if}
      </button>
      <div class="snippet-wrap">
        <button class="secondary" on:click={() => (showSnippets = !showSnippets)} disabled={!selected} title="Insert snippet">
          <Code2 size={16} /> Snippets
        </button>
        {#if showSnippets}
          <div class="snippet-menu">
            {#each SNIPPETS as sn (sn.id)}
              <button type="button" on:click={() => insertSnippet(sn)}>{sn.label}</button>
            {/each}
            {#if isKubejsFile}
              <button type="button" class="gen" on:click={insertFromRecipeGenerator}>Insert from recipe generator</button>
            {/if}
          </div>
        {/if}
      </div>
      <button class="secondary" on:click={formatFile} disabled={!canFormat || saving || formatting} title={canFormat ? "Pretty-print JSON/TOML" : "Format: .json or .toml"}>
        <FileCode2 size={16} /> {formatting ? "…" : "Format"}
      </button>
      <button class="secondary" on:click={resetFile} disabled={!dirty || saving}>
        <RotateCcw size={16} /> Reset
      </button>
      <button on:click={saveFile} disabled={!dirty || saving || !selected}>
        <Save size={16} />
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
  </div>

  {#if error}
    <div class="notice error"><AlertTriangle size={16} /> {error}</div>
  {/if}
  {#if message}
    <div class="notice success">
      <span>{message}</span>
      <div class="trail-actions">
        <button class="ghost mini" on:click={openHistory}><History size={12} /> History</button>
        <button class="ghost mini" on:click={openSnapshots}><Camera size={12} /> Snapshots</button>
      </div>
    </div>
  {/if}

  {#if !$projectPath}
    <EmptyState icon={FileCode2} title="No project selected" description="Open a project to edit configs." />
  {:else}
    <div class="layout">
      <aside class="file-panel">
        <div class="root-chips">
          <button class="chip" class:active={rootFilter === null} on:click={() => setRootChip(null)}>All</button>
          {#each presentRoots as root (root)}
            <button class="chip" class:active={rootFilter === root} on:click={() => setRootChip(root)}>{root}</button>
          {/each}
        </div>

        <div class="search">
          <div class="search-field">
            <span class="search-glyph"><Search size={16} /></span>
            <input bind:value={filter} placeholder="Filter files…" />
          </div>
        </div>

        <div class="search-across">
          <div class="search-across-row">
            <input bind:value={searchQuery} placeholder="Search in contents…" on:keydown={(e) => e.key === "Enter" && doSearch()} />
            <button class="mini-btn" on:click={doSearch} disabled={searchLoading || !searchQuery.trim()}>
              <FileSearch size={14} />
            </button>
          </div>
          {#if searchError}
            <div class="search-error">{searchError}</div>
          {/if}
          {#if searchResults.length > 0}
            <div class="search-results">
              {#each searchResults.slice(0, 40) as hit (hit.path + ':' + hit.line + hit.text)}
                <button class="search-hit" on:click={() => openSearchHit(hit)}>
                  <span class="hit-path">{hit.path}:{hit.line}</span>
                  <span class="hit-text">{hit.text}</span>
                </button>
              {/each}
              {#if searchResults.length >= 40}
                <div class="search-truncated">… and {searchResults.length - 40} more results</div>
              {/if}
            </div>
          {:else if searchLoading}
            <div class="search-status">Searching…</div>
          {/if}
        </div>

        {#if loading && files.length === 0}
          <div class="muted">Scanning project…</div>
        {:else if files.length === 0}
          <div class="muted">No editable config files found.</div>
        {:else if flatTree.length === 0}
          <div class="muted">No files match filter.</div>
        {:else}
          <div class="tree">
            {#each flatTree as node (node.fullPath + (node.isDir ? '-d' : '-f'))}
              {#if node.isDir}
                <button
                  class="tree-dir"
                  class:root={node.isRoot}
                  style:padding-left="{12 + node.depth * 16}px"
                  on:click={() => toggleDir(node.fullPath)}
                >
                  {#if node.expanded}
                    <ChevronDown size={14} />
                    <FolderOpen size={14} class="folder-icon" />
                  {:else}
                    <ChevronRight size={14} />
                    <Folder size={14} class="folder-icon" />
                  {/if}
                  <span class="tree-dir-name">{node.name}</span>
                </button>
              {:else if node.file}
                <button
                  class="tree-file"
                  class:selected={selected?.path === node.file.path}
                  style:padding-left="{12 + node.depth * 16}px"
                  on:click={() => { if (node.file) tryOpenFile(node.file); }}
                  title={node.file.path}
                >
                  <File size={14} />
                  <span class="tree-file-name">{node.name}</span>
                  <span class="tree-file-meta">{formatSize(node.file.size)}</span>
                </button>
              {/if}
            {/each}
          </div>
        {/if}
      </aside>

      <section class="editor-panel">
        {#if selected}
          <div class="editor-header">
            <div>
              <h2>{selected.name}</h2>
              <p>{selected.path}</p>
            </div>
            <div class="editor-stats">
              <span>{content.split("\n").length} lines</span>
              <span>{formatSize(content.length)}</span>
              {#if dirty}<strong>Unsaved</strong>{/if}
              <span class="lang-badge">{langBadge(selected)}</span>
            </div>
          </div>
          <div class="cm-wrapper" class:line-hl={highlightLine != null}>
            {#key selected.path}
              <CodeMirror
                value={content}
                lang={currentLang}
                theme={oneDark}
                on:change={handleCmChange}
                on:ready={onCmReady}
              />
            {/key}
          </div>

          {#if lintIssues.length > 0}
            <div class="lint-panel">
              {#each lintIssues as issue, i (issue.code + '-' + i + '-' + (issue.line ?? 0))}
                <button type="button" class="lint-item {issue.severity}" on:click={() => openLintIssue(issue)}>
                  <span class="lint-sev">{issue.severity}</span>
                  <code>{issue.code}</code>
                  <span>{issue.message}</span>
                  {#if issue.line}<small>line {issue.line}</small>{/if}
                </button>
              {/each}
            </div>
          {/if}
        {:else}
          <EmptyState icon={FileCode2} compact={true} title="No file selected" description="Select a config from the tree. Roots: config, defaultconfigs, kubejs, scripts, overrides, options.txt." />
        {/if}
      </section>
    </div>
  {/if}
</div>

{#if confirmOpen}
  <ConfirmDialog title="Discard changes?" message="You have unsaved changes. Discard them?" danger={false}
    confirmLabel="Discard" on:confirm={() => {
      confirmOpen = false;
      if (pendingFile) {
        const line = pendingJumpLine;
        openFileInternal(pendingFile, line ?? undefined);
        pendingFile = null;
      }
    }}
    on:cancel={() => { confirmOpen = false; pendingFile = null; pendingJumpLine = null; }} />
{/if}

<style>
  .config-editor {
    max-width: none;
    width: 100%;
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    padding: 8px 10px 10px;
  }
  .toolbar, .toolbar-actions, .title, .editor-header, .editor-stats, .notice, .trail-actions, .root-chips { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 10px; margin-bottom: 6px; flex-shrink: 0; min-height: 36px; }
  .title { gap: 8px; color: var(--text-secondary); font-weight: 700; font-size: 14px; }
  .toolbar-actions { gap: 6px; flex-wrap: wrap; }
  .toolbar-actions button {
    transform: none !important;
    padding: 5px 10px;
    font-size: 12px;
  }
  .notice { gap: 8px; padding: 8px 10px; border-radius: var(--border-radius-md); margin-bottom: 8px; border: 1px solid var(--border-color); flex-shrink: 0; justify-content: space-between; flex-wrap: wrap; font-size: 13px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .trail-actions { gap: 6px; }
  .mini { padding: 4px 8px; font-size: 11px; }
  .lint-bad { border-color: rgba(239, 68, 68, 0.45) !important; color: #fca5a5 !important; }
  .lint-warn { border-color: rgba(245, 158, 11, 0.45) !important; color: #fde68a !important; }

  .snippet-wrap { position: relative; }
  .snippet-menu {
    position: absolute; top: calc(100% + 4px); right: 0; z-index: 20;
    min-width: 260px; max-height: 320px; overflow: auto;
    background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0,0,0,.35); padding: 6px; display: grid; gap: 2px;
  }
  .snippet-menu button {
    text-align: left; background: transparent; border: none; color: var(--text-secondary);
    padding: 8px 10px; border-radius: 6px; font-size: 12px; cursor: pointer;
  }
  .snippet-menu button:hover { background: var(--bg-tertiary); color: var(--text-primary); }
  .snippet-menu button.gen { color: var(--accent-primary); border-top: 1px solid var(--border-color); margin-top: 4px; border-radius: 0 0 6px 6px; }

  .root-chips { gap: 4px; flex-wrap: wrap; margin-bottom: 8px; flex-shrink: 0; }
  .chip {
    font-size: 11px; font-weight: 700; text-transform: lowercase; letter-spacing: .02em;
    padding: 3px 7px; border-radius: 999px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-muted); cursor: pointer;
  }
  .chip.active, .chip:hover { border-color: rgba(27, 217, 106, 0.4); color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); }

  .layout {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 300px minmax(0, 1fr);
    gap: 12px;
    overflow: hidden;
  }
  .file-panel, .editor-panel { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .file-panel {
    padding: 10px;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .search {
    position: sticky;
    top: 0;
    z-index: 1;
    margin-bottom: 8px;
    background: var(--bg-secondary);
    padding-bottom: 4px;
    flex-shrink: 0;
  }
  .search-field {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
  }
  .search-glyph {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    pointer-events: none;
    z-index: 1;
  }
  .search input {
    width: 100%;
    min-height: 32px;
    box-sizing: border-box;
    padding: 6px 12px 6px 34px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font: inherit;
    font-size: 12px;
  }

  .search-across { margin-bottom: 8px; padding-bottom: 8px; border-bottom: 1px solid var(--border-color); flex-shrink: 0; }
  .search-across-row { display: flex; gap: 6px; align-items: center; }
  .search-across-row input {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    padding: 5px 8px;
    min-height: 28px;
    box-sizing: border-box;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font: inherit;
  }
  .mini-btn { width: 28px; height: 28px; padding: 0; display: flex; align-items: center; justify-content: center; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); color: var(--text-secondary); cursor: pointer; }
  .mini-btn:hover { border-color: var(--accent-primary); color: var(--accent-primary); }
  .search-error, .search-status { color: #fecaca; font-size: 11px; margin-top: 4px; }
  .search-status { color: var(--text-muted); }
  .search-results { max-height: 140px; overflow: auto; margin-top: 6px; }
  .search-hit { width: 100%; display: grid; gap: 2px; text-align: left; padding: 5px 6px; margin-bottom: 2px; background: transparent; border: 1px solid transparent; color: var(--text-secondary); transform: none; }
  .search-hit:hover { background: var(--bg-tertiary); border-color: rgba(27,217,106,0.25); }
  .hit-path { font-size: 11px; color: var(--accent-primary); font-family: ui-monospace, monospace; }
  .hit-text { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .search-truncated { font-size: 11px; color: var(--text-muted); padding: 6px 8px; }

  .tree {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .tree-dir, .tree-file {
    width: 100%; display: flex; align-items: center; gap: 6px;
    text-align: left; background: transparent; border: 1px solid transparent;
    color: var(--text-secondary); padding: 5px 10px; font-size: 13px;
    cursor: pointer; transform: none; border-radius: 6px;
    content-visibility: auto;
    contain-intrinsic-size: 28px;
  }
  .tree-dir { font-weight: 600; color: var(--text-muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.03em; }
  .tree-dir.root { color: var(--accent-primary); opacity: 0.9; }
  .tree-dir:hover { background: var(--bg-tertiary); color: var(--text-primary); }
  .tree-file:hover, .tree-file.selected { background: var(--bg-tertiary); border-color: rgba(27,217,106,0.35); color: var(--text-primary); }
  .tree-dir :global(.folder-icon) { color: var(--accent-primary); opacity: 0.7; }
  .tree-dir-name, .tree-file-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tree-file-name { font-weight: 500; }
  .tree-file-meta { color: var(--text-muted); font-size: 11px; white-space: nowrap; }

  .muted { padding: 16px 8px; line-height: 1.5; color: var(--text-muted); font-size: 12px; flex-shrink: 0; }

  .editor-panel { min-width: 0; min-height: 0; height: 100%; display: flex; flex-direction: column; overflow: hidden; }
  .editor-header { justify-content: space-between; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border-color); flex-shrink: 0; }
  .editor-header h2 { margin: 0 0 2px; font-size: 16px; }
  .editor-header p { margin: 0; font-size: 12px; color: var(--text-muted); }
  .editor-stats { gap: 10px; white-space: nowrap; }
  .editor-stats strong { color: var(--accent-warning); font-size: 12px; }
  .lang-badge { background: rgba(139,92,246,0.15); color: var(--accent-secondary); padding: 2px 8px; border-radius: 999px; font-weight: 700; font-size: 11px; text-transform: uppercase; }

  .cm-wrapper { flex: 1; min-height: 0; overflow: hidden; }
  .cm-wrapper :global(.cm-editor) { height: 100%; }
  .cm-wrapper :global(.cm-scroller) { overflow: auto; }
  .cm-wrapper.line-hl :global(.cm-selectionBackground) { background: rgba(27, 217, 106, 0.22) !important; }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .lint-panel { padding: 10px; border-top: 1px solid var(--border-color); max-height: 160px; overflow: auto; display: grid; gap: 3px; }
  .lint-item {
    display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: 4px; font-size: 11px;
    width: 100%; text-align: left; border: 1px solid transparent; cursor: pointer; background: transparent; transform: none;
  }
  .lint-item.error { background: rgba(239,68,68,.08); color: #fca5a5; }
  .lint-item.warning { background: rgba(245,158,11,.08); color: #fde68a; }
  .lint-item:hover { border-color: rgba(255,255,255,.12); }
  .lint-sev { font-weight: 800; text-transform: uppercase; font-size: 9px; padding: 1px 4px; border-radius: 3px; }
  .lint-item.error .lint-sev { background: rgba(239,68,68,.2); }
  .lint-item.warning .lint-sev { background: rgba(245,158,11,.2); }
  .lint-item code { font-size: 10px; color: var(--accent-primary); }
  .lint-item span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lint-item small { color: var(--text-muted); font-size: 10px; }
  @media (max-width: 1050px) { .layout { grid-template-columns: 1fr; } }
</style>
