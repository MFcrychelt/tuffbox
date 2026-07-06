<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FileCode2, RefreshCw, Save, Search, RotateCcw, AlertTriangle, FileSearch, Eye, EyeOff } from "lucide-svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { projectPath } from "../lib/store";

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

  // Search across files
  let searchQuery = "";
  let searchResults: SearchHit[] = [];
  let searchLoading = false;
  let searchError: string | null = null;

  // Syntax highlight toggle
  let highlightMode = false;

  async function loadFiles(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && files.length > 0) return;
    loading = true;
    error = null;
    message = null;
    try {
      files = await invoke("list_config_files", { path: $projectPath });
      lastLoadedPath = $projectPath;
      if (selected && !files.some((file) => file.path === selected?.path)) {
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
      selected = files.find((file) => file.path === selected?.path) ?? selected;
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
    if (file) openFile(file);
  }

  // Lightweight regex syntax highlighter for JSON, TOML, JS/ZS, YAML
  function syntaxHighlight(code: string, ext: string): string {
    let escaped = code
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    if (ext === "json" || ext === "json5") {
      escaped = escaped
        .replace(/"([^"\\]|\\.)*"/g, '<span class="hl-string">$&</span>')
        .replace(/\b(true|false|null)\b/g, '<span class="hl-bool">$1</span>')
        .replace(/\b(-?\d+\.?\d*([eE][+-]?\d+)?)\b/g, '<span class="hl-number">$1</span>');
    } else if (ext === "toml" || ext === "properties" || ext === "cfg") {
      escaped = escaped
        .replace(/^(\s*[A-Za-z0-9_.-]+)\s*=/gm, '<span class="hl-key">$1</span>=')
        .replace(/^(\s*#.*)/gm, '<span class="hl-comment">$1</span>')
        .replace(/"([^"\\]|\\.)*"/g, '<span class="hl-string">$&</span>')
        .replace(/\b(true|false)\b/g, '<span class="hl-bool">$1</span>')
        .replace(/\b(\d+\.?\d*)\b/g, '<span class="hl-number">$1</span>');
    } else if (ext === "js" || ext === "zs") {
      escaped = escaped
        .replace(/(\/\/.*)/g, '<span class="hl-comment">$1</span>')
        .replace(/("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')/g, '<span class="hl-string">$1</span>')
        .replace(/\b(const|let|var|function|return|if|else|for|while|class|import|export|from|as|new|this|true|false|null|undefined|event|require|module)\b/g, '<span class="hl-keyword">$1</span>')
        .replace(/\b(\d+\.?\d*)\b/g, '<span class="hl-number">$1</span>');
    } else if (ext === "yaml" || ext === "yml") {
      escaped = escaped
        .replace(/^(\s*[A-Za-z0-9_.-]+):/gm, '<span class="hl-key">$1</span>:')
        .replace(/^(\s*#.*)/gm, '<span class="hl-comment">$1</span>')
        .replace(/"([^"\\]|\\.)*"/g, '<span class="hl-string">$&</span>')
        .replace(/\b(true|false|null)\b/g, '<span class="hl-bool">$1</span>')
        .replace(/\b(\d+\.?\d*)\b/g, '<span class="hl-number">$1</span>');
    }
    return escaped;
  }

  $: canFormat = selected?.extension?.toLowerCase() === "json";
  $: highlighted = highlightMode ? syntaxHighlight(content, selected?.extension?.toLowerCase() ?? "") : "";

  function formatSize(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

  $: dirty = content !== originalContent;
  $: filtered = files.filter((file) =>
    file.path.toLowerCase().includes(filter.toLowerCase())
  );
  $: grouped = filtered.reduce<Record<string, ConfigFile[]>>((acc, file) => {
    const root = file.path.split("/")[0] ?? "project";
    acc[root] = acc[root] ?? [];
    acc[root].push(file);
    return acc;
  }, {});
  $: lineCount = content ? content.split("\n").length : 0;

  // Config linter
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

  // Ctrl+S shortcut
  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      if (dirty && selected) saveFile();
    }
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
      <button class="secondary" on:click={() => (highlightMode = !highlightMode)} disabled={!selected} title={highlightMode ? "Edit mode" : "Syntax highlight preview"}>
        {#if highlightMode}<EyeOff size={16} /> Edit{:else}<Eye size={16} /> Highlight{/if}
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
    <div class="empty">Open a project to edit configs.</div>
  {:else}
    <div class="layout">
      <aside class="file-panel">
        <div class="search">
          <Search size={16} />
          <input bind:value={filter} placeholder="Search files..." />
        </div>

        <!-- Search across all configs -->
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
        {:else if filtered.length === 0}
          <div class="muted">No editable config files found.</div>
        {:else}
          <div class="groups">
            {#each Object.entries(grouped) as [root, group]}
              <section>
                <h3>{root}</h3>
                {#each group as file}
                  <button
                    class="file-row"
                    class:selected={selected?.path === file.path}
                    on:click={() => tryOpenFile(file)}
                    title={file.path}
                  >
                    <span class="file-name">{file.path.replace(`${root}/`, "")}</span>
                    <span class="file-meta">{file.extension || "file"} · {formatSize(file.size)}</span>
                  </button>
                {/each}
              </section>
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
              <span>{lineCount} lines</span>
              <span>{formatSize(content.length)}</span>
              {#if dirty}<strong>Unsaved</strong>{/if}
              {#if highlightMode}<span class="hl-badge">Preview</span>{/if}
            </div>
          </div>
          {#if highlightMode}
            <pre class="highlighted-code">{@html highlighted}</pre>
          {:else}
            <textarea bind:value={content} spellcheck="false" />
          {/if}

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
          <div class="empty editor-empty">
            Select a config file. Supported formats: JSON, JSON5, TOML, properties, CFG, JS, ZS, YAML, TXT.
            <br /><small>Ctrl+S to save · Ctrl+click search result to open · Toggle "Highlight" for syntax preview</small>
          </div>
        {/if}
      </section>
    </div>
  {/if}
{#if confirmOpen}
    <ConfirmDialog title="Discard changes?" message="You have unsaved changes. Discard them?" danger={false}
      confirmLabel="Discard" on:confirm={() => { confirmOpen = false; if (pendingFile) { openFileInternal(pendingFile); pendingFile = null; } }}
      on:cancel={() => (confirmOpen = false, pendingFile = null)} />
  {/if}
</div>

<style>
  .config-editor { max-width: none; width: 100%; }
  .toolbar, .toolbar-actions, .title, .editor-header, .editor-stats, .notice { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .toolbar-actions { gap: 10px; }
  .notice { gap: 10px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .layout { display: grid; grid-template-columns: 360px minmax(0, 1fr); gap: 16px; min-height: calc(100vh - 150px); }
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

  h3 { margin: 16px 6px 8px; color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em; }
  .file-row { width: 100%; display: flex; flex-direction: column; align-items: stretch; gap: 3px; text-align: left; background: transparent; color: var(--text-secondary); padding: 10px 12px; margin-bottom: 4px; border: 1px solid transparent; }
  .file-row:hover, .file-row.selected { background: var(--bg-tertiary); border-color: rgba(27,217,106,0.35); color: var(--text-primary); transform: none; }
  .file-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 700; }
  .file-meta, .muted, .editor-header p, .editor-stats span { color: var(--text-muted); font-size: 12px; }
  .muted { padding: 24px 10px; line-height: 1.5; }

  .editor-panel { min-width: 0; display: flex; flex-direction: column; overflow: hidden; }
  .editor-header { justify-content: space-between; gap: 16px; padding: 16px 18px; border-bottom: 1px solid var(--border-color); }
  .editor-header h2 { margin: 0 0 3px; font-size: 18px; }
  .editor-stats { gap: 10px; white-space: nowrap; }
  .editor-stats strong { color: var(--accent-warning); font-size: 12px; }
  .hl-badge { background: rgba(139,92,246,0.15); color: var(--accent-secondary); padding: 2px 8px; border-radius: 999px; font-weight: 700; font-size: 11px; }

  textarea { flex: 1; width: 100%; min-height: 560px; resize: none; border: 0; outline: none; padding: 18px 20px; background: #0d0d10; color: #e5e7eb; font-family: ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,"Liberation Mono",monospace; font-size: 13px; line-height: 1.65; tab-size: 2; }

  .highlighted-code { flex: 1; margin: 0; padding: 18px 20px; background: #0d0d10; color: #e5e7eb; font-family: ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,"Liberation Mono",monospace; font-size: 13px; line-height: 1.65; tab-size: 2; overflow: auto; white-space: pre-wrap; word-break: break-all; }
  .highlighted-code :global(.hl-string) { color: #86efac; }
  .highlighted-code :global(.hl-number) { color: #fbbf24; }
  .highlighted-code :global(.hl-bool) { color: #c084fc; }
  .highlighted-code :global(.hl-key) { color: #67e8f9; }
  .highlighted-code :global(.hl-comment) { color: #6b7280; font-style: italic; }
  .highlighted-code :global(.hl-keyword) { color: #f472b6; }

  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  .editor-empty { margin: 16px; flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 8px; }
  .editor-empty small { font-size: 12px; color: var(--text-muted); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .lint-panel { margin-top: 10px; padding: 10px; border-top: 1px solid var(--border-color); max-height: 160px; overflow: auto; }
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
