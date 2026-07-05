<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FileCode2, RefreshCw, Save, Search, RotateCcw, AlertTriangle } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type ConfigFile = {
    path: string;
    name: string;
    extension: string;
    size: number;
    modified?: number | null;
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

  async function openFile(file: ConfigFile) {
    if (!$projectPath) return;
    if (dirty && !window.confirm("Discard unsaved changes?")) return;
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

  /// Pretty-prints the current buffer as JSON (2-space indent). Only wired
  /// up for .json files since JSON5/TOML/CFG have comments and trailing
  /// commas that `JSON.parse` would reject — formatting those needs a
  /// real parser, which isn't implemented yet (tracked in the roadmap).
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

  $: canFormat = selected?.extension?.toLowerCase() === "json";

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
  $: if ($projectPath && lastLoadedPath !== $projectPath) loadFiles(true);
</script>

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
      <button class="secondary" on:click={formatJson} disabled={!canFormat || saving} title={canFormat ? "Pretty-print JSON" : "Formatting is only available for .json files"}>
        <FileCode2 size={16} />
        Format
      </button>
      <button class="secondary" on:click={resetFile} disabled={!dirty || saving}>
        <RotateCcw size={16} />
        Reset
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
          <input bind:value={filter} placeholder="Search config files..." />
        </div>

        {#if loading && files.length === 0}
          <div class="muted">Scanning project...</div>
        {:else if filtered.length === 0}
          <div class="muted">No editable config files found in config/, defaultconfigs/, kubejs/ or scripts/.</div>
        {:else}
          <div class="groups">
            {#each Object.entries(grouped) as [root, group]}
              <section>
                <h3>{root}</h3>
                {#each group as file}
                  <button
                    class="file-row"
                    class:selected={selected?.path === file.path}
                    on:click={() => openFile(file)}
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
            </div>
          </div>
          <textarea bind:value={content} spellcheck="false" />
        {:else}
          <div class="empty editor-empty">
            Select a config file. Supported MVP formats: JSON, JSON5, TOML, properties, CFG, JS, ZS, YAML and TXT.
          </div>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
   .config-editor {
    max-width: none;
    width: 100%;
  }

  .toolbar,
  .toolbar-actions,
  .title,
  .editor-header,
  .editor-stats,
  .notice {
    display: flex;
    align-items: center;
  }

  .toolbar {
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  .title {
    gap: 10px;
    color: var(--text-secondary);
    font-weight: 700;
  }

  .toolbar-actions {
    gap: 10px;
  }

  .notice {
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--border-radius-lg);
    margin-bottom: 14px;
    border: 1px solid var(--border-color);
  }

  .notice.error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.28);
  }

  .notice.success {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
  }

  .layout {
    display: grid;
    grid-template-columns: 360px minmax(0, 1fr);
    gap: 16px;
    min-height: calc(100vh - 150px);
  }

  .file-panel,
  .editor-panel,
  .empty {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }

  .file-panel {
    padding: 14px;
    overflow: auto;
  }

  .search {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    margin-bottom: 14px;
    background: var(--bg-secondary);
    padding-bottom: 10px;
  }

  .search :global(svg) {
    position: absolute;
    left: 12px;
    color: var(--text-muted);
  }

  .search input {
    width: 100%;
    padding-left: 38px;
  }

  h3 {
    margin: 16px 6px 8px;
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .file-row {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
    text-align: left;
    background: transparent;
    color: var(--text-secondary);
    padding: 10px 12px;
    margin-bottom: 4px;
    border: 1px solid transparent;
  }

  .file-row:hover,
  .file-row.selected {
    background: var(--bg-tertiary);
    border-color: rgba(27, 217, 106, 0.35);
    color: var(--text-primary);
    transform: none;
  }

  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 700;
  }

  .file-meta,
  .muted,
  .editor-header p,
  .editor-stats span {
    color: var(--text-muted);
    font-size: 12px;
  }

  .muted {
    padding: 24px 10px;
    line-height: 1.5;
  }

  .editor-panel {
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .editor-header {
    justify-content: space-between;
    gap: 16px;
    padding: 16px 18px;
    border-bottom: 1px solid var(--border-color);
  }

  .editor-header h2 {
    margin: 0 0 3px;
    font-size: 18px;
  }

  .editor-stats {
    gap: 10px;
    white-space: nowrap;
  }

  .editor-stats strong {
    color: var(--accent-warning);
    font-size: 12px;
  }

  textarea {
    flex: 1;
    width: 100%;
    min-height: 560px;
    resize: none;
    border: 0;
    outline: none;
    padding: 18px 20px;
    background: #0d0d10;
    color: #e5e7eb;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    font-size: 13px;
    line-height: 1.65;
    tab-size: 2;
  }

  .empty {
    color: var(--text-muted);
    padding: 80px;
    text-align: center;
  }

  .editor-empty {
    margin: 16px;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 1050px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
