<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { FileCode2, RefreshCw, Save, Search, RotateCcw, AlertTriangle, FileSearch, ChevronRight, ChevronDown, File, Folder, FolderOpen } from "lucide-svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { projectPath } from "../lib/store";
  import CodeMirror from "svelte-codemirror-editor";
  import { json } from "@codemirror/lang-json";
  import { javascript } from "@codemirror/lang-javascript";
  import { yaml } from "@codemirror/lang-yaml";
  import { StreamLanguage, LanguageSupport } from "@codemirror/language";
  import { toml } from "@codemirror/legacy-modes/mode/toml";
  import { oneDark } from "@codemirror/theme-one-dark";

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
  };

  let files: ConfigFile[] = [];
  let selected: ConfigFile | null = null;
  let content = "";
  let originalContent = "";
  let filter = "";
  let loading = false;
  let saving = false;
  let error: string | null = null;
  let message: string | null = null;
  let lastLoadedPath: string | null = null;

  let searchQuery = "";
  let searchResults: SearchHit[] = [];
  let searchLoading = false;
  let searchError: string | null = null;

  let expandedDirs = new Set<string>();
  let flatTree: FlatNode[] = [];

  function buildFlatTree(fileList: ConfigFile[], filterQuery: string): FlatNode[] {
    const dirs = new Map<string, { expanded: boolean; children: ConfigFile[] }>();
    const q = filterQuery.toLowerCase().trim();

    for (const file of fileList) {
      const parts = file.path.split("/");
      for (let i = 0; i < parts.length - 1; i++) {
        const dirPath = parts.slice(0, i + 1).join("/");
        if (!dirs.has(dirPath)) dirs.set(dirPath, { expanded: expandedDirs.has(dirPath), children: [] });
      }
      if (parts.length > 1) {
        const parentPath = parts.slice(0, -1).join("/");
        dirs.get(parentPath)?.children.push(file);
      }
    }

    if (q) {
      for (const [dirPath] of dirs) {
        const dirParts = dirPath.split("/");
        for (let i = 0; i < dirParts.length; i++) {
          const ancestor = dirParts.slice(0, i + 1).join("/");
          expandedDirs.add(ancestor);
        }
      }
    }

    const result: FlatNode[] = [];
    function walk(dirPath: string, depth: number) {
      const dir = dirs.get(dirPath);
      if (!dir) return;
      const name = dirPath.split("/").pop() ?? dirPath;
      result.push({ name, fullPath: dirPath, isDir: true, depth, expanded: dir.expanded });
      if (dir.expanded) {
        for (const child of dir.children) {
          const childName = child.path.split("/").pop() ?? child.path;
          result.push({ name: childName, fullPath: child.path, isDir: false, depth: depth + 1, expanded: false, file: child });
        }
        const subDirs = [...dirs.keys()]
          .filter((k) => k.startsWith(dirPath + "/") && !k.slice(dirPath.length + 1).includes("/"))
          .sort();
        for (const sub of subDirs) {
          walk(sub, depth + 1);
        }
      }
    }

    const topDirs = [...dirs.keys()]
      .filter((k) => !k.includes("/") || dirs.has(k.split("/").slice(0, -1).join("/")) === false)
      .filter((k) => k.split("/").length === 1)
      .sort();
    for (const d of topDirs) walk(d, 0);

    const topLevelFiles = fileList.filter((f) => !f.path.includes("/"));
    for (const f of topLevelFiles) {
      result.push({ name: f.path, fullPath: f.path, isDir: false, depth: 0, expanded: false, file: f });
    }

    return result;
  }

  function toggleDir(fullPath: string) {
    if (expandedDirs.has(fullPath)) {
      expandedDirs.delete(fullPath);
    } else {
      expandedDirs.add(fullPath);
    }
    flatTree = buildFlatTree(files, filter);
  }

  $: flatTree = buildFlatTree(files, filter);

  function langForExt(ext: string) {
    switch (ext) {
      case "json": case "json5": return json();
      case "js": case "zs": return javascript();
      case "yaml": case "yml": return yaml();
      case "toml": return StreamLanguage.define(toml) as unknown as LanguageSupport;
      default: return undefined;
    }
  }

  $: currentLang = langForExt(selected?.extension?.toLowerCase() ?? "");

  async function loadFiles(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && files.length > 0) return;
    loading = true;
    error = null;
    message = null;
    try {
      files = await invoke("list_config_files", { path: $projectPath });
      lastLoadedPath = $projectPath;
      if (selected && !files.some((f) => f.path === selected?.path)) {
        selected = null;
        content = "";
        originalContent = "";
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  let confirmOpen = false;
  let pendingFile: ConfigFile | null = null;

  function tryOpenFile(file: ConfigFile) {
    if (dirty) { pendingFile = file; confirmOpen = true; return; }
    openFileInternal(file);
  }

  async function openFileInternal(file: ConfigFile) {
    if (!$projectPath) return;
    selected = file;
    loading = true;
    error = null;
    message = null;
    try {
      content = await invoke("read_config_file", {
        path: $projectPath,
        relativePath: file.path,
      });
      originalContent = content;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function saveFile() {
    if (!$projectPath || !selected || !dirty) return;
    saving = true;
    error = null;
    message = null;
    try {
      await invoke("write_config_file", {
        path: $projectPath,
        relativePath: selected.path,
        content,
      });
      originalContent = content;
      message = `Saved ${selected.path}. Auto snapshot created.`;
      await loadFiles(true);
      selected = files.find((f) => f.path === selected?.path) ?? selected;
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

  function formatJson() {
    if (!selected) return;
    try {
      const parsed = JSON.parse(content);
      content = JSON.stringify(parsed, null, 2) + "\n";
      message = null;
      error = null;
    } catch (e) {
      error = `Cannot format: invalid JSON (${e instanceof Error ? e.message : e})`;
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
    if (file) tryOpenFile(file);
  }

  $: canFormat = selected?.extension?.toLowerCase() === "json";

  function formatSize(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  $: dirty = content !== originalContent;

  let lintIssues: any[] = [];
  let lintLoading = false;

  async function lintFile() {
    if (!$projectPath || !selected) return;
    lintLoading = true;
    try { lintIssues = await invoke("lint_config", { path: $projectPath, relativePath: selected.path }); }
    catch { lintIssues = []; }
    finally { lintLoading = false; }
  }

  $: if ($projectPath && lastLoadedPath !== $projectPath) loadFiles(true);

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      if (dirty && selected) saveFile();
    }
  }

  function handleCmChange(e: any) {
    content = e.detail;
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="config-editor">
  <div class="toolbar">
    <div class="title">
      <FileCode2 size={18} />
      <span>Config editor</span>
    </div>
    <div class="toolbar-actions">
      <button class="ghost" on:click={() => loadFiles(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        Refresh
      </button>
      <button class="secondary" on:click={lintFile} disabled={!selected || lintLoading} title={lintIssues.length ? `Found ${lintIssues.length} issue(s)` : "Lint config"}>
        <AlertTriangle size={16} />
        {lintLoading ? "..." : lintIssues.length > 0 ? `${lintIssues.length} issues` : "Lint"}
      </button>
      <button class="secondary" on:click={formatJson} disabled={!canFormat || saving} title={canFormat ? "Pretty-print JSON" : "Formatting is only available for .json files"}>
        <FileCode2 size={16} /> Format
      </button>
      <button class="secondary" on:click={resetFile} disabled={!dirty || saving}>
        <RotateCcw size={16} /> Reset
      </button>
      <button on:click={saveFile} disabled={!dirty || saving || !selected}>
        <Save size={16} />
        {saving ? "Saving..." : "Save"}
      </button>
    </div>
  </div>

  {#if error}
    <div class="notice error"><AlertTriangle size={16} /> {error}</div>
  {/if}
  {#if message}
    <div class="notice success">{message}</div>
  {/if}

  {#if !$projectPath}
    <EmptyState icon={FileCode2} title="No project selected" description="Open a project to edit configs." />
  {:else}
    <div class="layout">
      <aside class="file-panel">
        <div class="search">
          <Search size={16} />
          <input bind:value={filter} placeholder="Filter files..." />
        </div>

        <div class="search-across">
          <div class="search-across-row">
            <input bind:value={searchQuery} placeholder="Search in file contents..." on:keydown={(e) => e.key === "Enter" && doSearch()} />
            <button class="mini-btn" on:click={doSearch} disabled={searchLoading || !searchQuery.trim()}>
              <FileSearch size={14} />
            </button>
          </div>
          {#if searchError}
            <div class="search-error">{searchError}</div>
          {/if}
          {#if searchResults.length > 0}
            <div class="search-results">
              {#each searchResults.slice(0, 40) as hit}
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
            <div class="search-status">Searching...</div>
          {/if}
        </div>

        {#if loading && files.length === 0}
          <div class="muted">Scanning project...</div>
        {:else if files.length === 0}
          <div class="muted">No editable config files found.</div>
        {:else}
          <div class="tree">
            {#each flatTree as node}
              {#if node.isDir}
                <button
                  class="tree-dir"
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
              <span class="lang-badge">{selected.extension || "text"}</span>
            </div>
          </div>
          <div class="cm-wrapper">
            <CodeMirror
              value={content}
              lang={currentLang}
              theme={oneDark}
              on:change={handleCmChange}
            />
          </div>

          {#if lintIssues.length > 0}
            <div class="lint-panel">
              {#each lintIssues as issue}
                <div class="lint-item {issue.severity}">
                  <span class="lint-sev">{issue.severity}</span>
                  <code>{issue.code}</code>
                  <span>{issue.message}</span>
                  {#if issue.line}<small>line {issue.line}</small>{/if}
                </div>
              {/each}
            </div>
          {/if}

        {:else}
          <EmptyState icon={FileCode2} compact={true} title="No file selected" description="Select a config file from the tree. Supported: JSON, TOML, JS, ZS, YAML, properties." />
        {/if}
      </section>
    </div>
  {/if}
</div>

{#if confirmOpen}
  <ConfirmDialog title="Discard changes?" message="You have unsaved changes. Discard them?" danger={false}
    confirmLabel="Discard" on:confirm={() => { confirmOpen = false; if (pendingFile) { openFileInternal(pendingFile); pendingFile = null; } }}
    on:cancel={() => (confirmOpen = false, pendingFile = null)} />
{/if}

<style>
  .config-editor { max-width: none; width: 100%; }
  .toolbar, .toolbar-actions, .title, .editor-header, .editor-stats, .notice { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .toolbar-actions { gap: 10px; }
  .notice { gap: 10px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .layout { display: grid; grid-template-columns: 320px minmax(0, 1fr); gap: 16px; min-height: calc(100vh - 150px); }
  .file-panel, .editor-panel, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .file-panel { padding: 14px; overflow: auto; max-height: calc(100vh - 150px); }

  .search { position: sticky; top: 0; z-index: 1; display: flex; align-items: center; margin-bottom: 10px; background: var(--bg-secondary); padding-bottom: 8px; }
  .search :global(svg) { position: absolute; left: 12px; color: var(--text-muted); }
  .search input { width: 100%; padding-left: 38px; }

  .search-across { margin-bottom: 14px; padding-bottom: 12px; border-bottom: 1px solid var(--border-color); }
  .search-across-row { display: flex; gap: 6px; }
  .search-across-row input { flex: 1; font-size: 12px; padding: 7px 10px; }
  .mini-btn { width: 32px; height: 32px; padding: 0; display: flex; align-items: center; justify-content: center; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); color: var(--text-secondary); cursor: pointer; }
  .mini-btn:hover { border-color: var(--accent-primary); color: var(--accent-primary); }
  .search-error, .search-status { color: #fecaca; font-size: 11px; margin-top: 6px; }
  .search-status { color: var(--text-muted); }
  .search-results { max-height: 280px; overflow: auto; margin-top: 8px; }
  .search-hit { width: 100%; display: grid; gap: 2px; text-align: left; padding: 6px 8px; margin-bottom: 2px; background: transparent; border: 1px solid transparent; color: var(--text-secondary); transform: none; }
  .search-hit:hover { background: var(--bg-tertiary); border-color: rgba(27,217,106,0.25); }
  .hit-path { font-size: 11px; color: var(--accent-primary); font-family: ui-monospace, monospace; }
  .hit-text { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .search-truncated { font-size: 11px; color: var(--text-muted); padding: 6px 8px; }

  .tree { display: flex; flex-direction: column; }
  .tree-children { display: flex; flex-direction: column; }

  .tree-dir, .tree-file {
    width: 100%; display: flex; align-items: center; gap: 6px;
    text-align: left; background: transparent; border: 1px solid transparent;
    color: var(--text-secondary); padding: 6px 12px; font-size: 13px;
    cursor: pointer; transform: none; border-radius: 6px;
  }
  .tree-dir { font-weight: 600; color: var(--text-muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.03em; }
  .tree-dir:hover { background: var(--bg-tertiary); color: var(--text-primary); }
  .tree-file { padding-left: 12px; }
  .tree-file:hover, .tree-file.selected { background: var(--bg-tertiary); border-color: rgba(27,217,106,0.35); color: var(--text-primary); }
  .tree-dir :global(.folder-icon) { color: var(--accent-primary); opacity: 0.7; }
  .tree-dir-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tree-file-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .tree-file-meta { color: var(--text-muted); font-size: 11px; white-space: nowrap; }

  .muted { padding: 24px 10px; line-height: 1.5; color: var(--text-muted); font-size: 13px; }

  .editor-panel { min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
  .editor-header { justify-content: space-between; gap: 16px; padding: 16px 18px; border-bottom: 1px solid var(--border-color); }
  .editor-header h2 { margin: 0 0 3px; font-size: 18px; }
  .editor-stats { gap: 10px; white-space: nowrap; }
  .editor-stats strong { color: var(--accent-warning); font-size: 12px; }
  .lang-badge { background: rgba(139,92,246,0.15); color: var(--accent-secondary); padding: 2px 8px; border-radius: 999px; font-weight: 700; font-size: 11px; text-transform: uppercase; }

  .cm-wrapper { flex: 1; min-height: 0; overflow: auto; }
  .cm-wrapper :global(.cm-editor) { height: 100%; min-height: 500px; }
  .cm-wrapper :global(.cm-scroller) { overflow: auto; }

  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  .editor-empty { margin: 16px; flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 8px; }
  .editor-empty small { font-size: 12px; color: var(--text-muted); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .lint-panel { padding: 10px; border-top: 1px solid var(--border-color); max-height: 160px; overflow: auto; }
  .lint-item { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: 4px; font-size: 11px; margin-bottom: 3px; }
  .lint-item.error { background: rgba(239,68,68,.08); color: #fca5a5; }
  .lint-item.warning { background: rgba(245,158,11,.08); color: #fde68a; }
  .lint-sev { font-weight: 800; text-transform: uppercase; font-size: 9px; padding: 1px 4px; border-radius: 3px; }
  .lint-item.error .lint-sev { background: rgba(239,68,68,.2); }
  .lint-item.warning .lint-sev { background: rgba(245,158,11,.2); }
  .lint-item code { font-size: 10px; color: var(--accent-primary); }
  .lint-item span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lint-item small { color: var(--text-muted); font-size: 10px; }
  @media (max-width: 1050px) { .layout { grid-template-columns: 1fr; } }
</style>
