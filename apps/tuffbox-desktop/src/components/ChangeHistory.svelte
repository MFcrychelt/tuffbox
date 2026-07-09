<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { History, RefreshCw, Search, FileText, Maximize2, Save, X, RotateCcw, ChevronDown, ChevronRight } from "lucide-svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { projectPath } from "../lib/store";

  type ChangeEntry = {
    id: string;
    snapshotId: string;
    operation: string;
    reason: string;
    createdAt: string;
    path: string;
    category: string;
    kind: string;
    preview: string;
    diff: string;
    canOpen: boolean;
  };

  let entries: ChangeEntry[] = [];
  let selectedId = "";
  let filter = "";
  let categoryFilter = "All";
  let loading = false;
  let error: string | null = null;
  let message: string | null = null;
  let lastLoadedPath: string | null = null;

  let editorOpen = false;
  let editorPath = "";
  let editorContent = "";
  let editorOriginal = "";
  let saving = false;
  let expanded: Record<string, boolean> = {};
  let tracked: Record<string, boolean> = {
    Mods: true,
    Configs: true,
    Shaders: true,
    "Resource Packs": true,
    "World/Data": false,
    Other: true,
  };
  const rootsByCategory: Record<string, string[]> = {
    Mods: ["mods"],
    Configs: ["config", "defaultconfigs", "kubejs", "scripts", "options.txt", "servers.dat"],
    Shaders: ["shaderpacks", "shaders"],
    "Resource Packs": ["resourcepacks", "texturepacks"],
    "World/Data": ["datapacks", "saves"],
    Other: [],
  };
  let settingsLoadedPath: string | null = null;

  let confirmOpen = false;
  let confirmEntry: ChangeEntry | null = null;

  function showRollbackConfirm(entry: ChangeEntry) { confirmEntry = entry; confirmOpen = true; }

  async function doRollback() {
    if (!$projectPath || !confirmEntry) return;
    confirmOpen = false;
    loading = true; error = null;
    try {
      await invoke("rollback_history_file", { path: $projectPath, snapshotId: confirmEntry.snapshotId, relativePath: confirmEntry.path });
      message = `Rolled back ${confirmEntry.path}.`;
      await load(true);
    } catch(e) { error = String(e); }
    finally { loading = false; confirmEntry = null; }
  }

  async function loadHistorySettings() {
    if (!$projectPath || settingsLoadedPath === $projectPath) return;
    try {
      const settings: { tracked: Record<string, boolean> } = await invoke("get_history_settings", { path: $projectPath });
      tracked = { ...tracked, ...(settings.tracked ?? {}) };
      settingsLoadedPath = $projectPath;
    } catch {
      // Keep defaults if settings are missing or invalid.
    }
  }

  async function saveHistorySettings() {
    if (!$projectPath) return;
    try {
      await invoke("update_history_settings", { path: $projectPath, settings: { tracked } });
      message = "History tracking settings saved to .tuffbox/history.json.";
    } catch (e) {
      error = String(e);
    }
  }

  async function load(force = false) {
    if (!$projectPath) return;
    await loadHistorySettings();
    if (!force && lastLoadedPath === $projectPath && entries.length > 0) return;
    loading = true;
    error = null;
    message = null;
    try {
      let data: any[] = await invoke("list_project_change_history", { path: $projectPath }) as any[];
      entries = data.reverse();
      selectedId = entries[0]?.id ?? "";
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function openFullFile(entry: ChangeEntry) {
    if (!$projectPath || !entry.canOpen) return;
    loading = true;
    error = null;
    try {
      const result: { path: string; content: string } = await invoke("read_project_history_file", {
        path: $projectPath,
        relativePath: entry.path,
      });
      editorPath = result.path;
      editorContent = result.content;
      editorOriginal = result.content;
      editorOpen = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function saveEditor() {
    if (!$projectPath || !editorPath || editorContent === editorOriginal) return;
    saving = true;
    error = null;
    message = null;
    try {
      await invoke("write_config_file", {
        path: $projectPath,
        relativePath: editorPath,
        content: editorContent,
      });
      editorOriginal = editorContent;
      message = `Saved ${editorPath}. Change snapshot registered.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function registerSelectedRoots() {
    if (!$projectPath) return;
    loading = true;
    error = null;
    message = null;
    try {
      const roots = Object.entries(tracked).flatMap(([category, enabled]) => enabled ? rootsByCategory[category] ?? [] : []);
      const snapshot: any = await invoke("create_tracked_history_snapshot", { path: $projectPath, roots });
      message = `Registered ${snapshot.changedFiles?.length ?? 0} tracked files in snapshot ${snapshot.id}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function rollbackEntry(entry: ChangeEntry) {
    showRollbackConfirm(entry);
    return;
  }
  async function _legacy_rollback(entry: ChangeEntry) {
    if (!$projectPath || entry.kind !== "file_changed") return;
    loading = true;
    error = null;
    message = null;
    try {
      await invoke("rollback_history_file", {
        path: $projectPath,
        snapshotId: entry.snapshotId,
        relativePath: entry.path,
      });
      message = `Rolled back ${entry.path}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function toggleExpanded(entry: ChangeEntry) {
    expanded = { ...expanded, [entry.id]: !expanded[entry.id] };
    selectedId = entry.id;
  }

  function lineClass(line: string) {
    if (line.startsWith("+ ")) return "added";
    if (line.startsWith("- ")) return "removed";
    return "context";
  }

  $: categories = ["All", ...Array.from(new Set(entries.map((entry) => entry.category)))];
  $: visible = entries.filter((entry) => {
    const q = filter.toLowerCase();
    const matchesText =
      !q ||
      entry.path.toLowerCase().includes(q) ||
      entry.kind.toLowerCase().includes(q) ||
      entry.preview.toLowerCase().includes(q);
    const matchesCategory = categoryFilter === "All" || entry.category === categoryFilter;
    const matchesTracked = tracked[entry.category] ?? true;
    return matchesText && matchesCategory && matchesTracked;
  });
  $: grouped = visible.reduce<Record<string, ChangeEntry[]>>((acc, entry) => {
    acc[entry.category] = acc[entry.category] ?? [];
    acc[entry.category].push(entry);
    return acc;
  }, {});
  $: selected = entries.find((entry) => entry.id === selectedId) ?? visible[0] ?? null;
  $: editorDirty = editorContent !== editorOriginal;
  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
</script>

<div class="change-history">
  <div class="toolbar">
    <div class="title"><History size={18} /> Change history</div>
    <div class="toolbar-actions">
      <div class="search">
        <Search size={15} />
        <input bind:value={filter} placeholder="Search files, mods, configs..." />
      </div>
      <select bind:value={categoryFilter}>
        {#each categories as category}<option value={category}>{category}</option>{/each}
      </select>
      <button class="secondary" on:click={saveHistorySettings} disabled={!$projectPath || loading}>Save tracking settings</button>
      <button class="secondary" on:click={registerSelectedRoots} disabled={!$projectPath || loading}>Register selected folders</button>
      <button class="ghost" on:click={() => load(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  <div class="tracking-controls">
    {#each Object.keys(tracked) as key}
      <label><input type="checkbox" bind:checked={tracked[key]} /> {key}{#if key === "World/Data"}<small>saves off by default</small>{/if}</label>
    {/each}
  </div>

  {#if !$projectPath}
    <div class="empty">Open a project to view file and mod change history.</div>
  {:else if loading && entries.length === 0}
    <div class="empty">Loading history...</div>
  {:else if entries.length === 0}
    <div class="empty">No tracked changes yet. Edits, mod operations and snapshots will appear here.</div>
  {:else}
    <div class="history-layout">
      <aside class="change-tree">
        <div class="timeline-line" />
        {#each Object.entries(grouped) as [category, group]}
          <section>
            <h3>{category}</h3>
            {#each group as entry}
              <div class="timeline-item">
                <button
                  class="file-strip {entry.kind}"
                  on:click={() => {
                    const el = document.getElementById('change-' + entry.id);
                    if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
                  }}
                  title={entry.path}
                >
                  <span class="file-title">{entry.path}</span>
                  <small>{entry.kind.replace("_", " ")}</small>
                </button>
              </div>
            {/each}
          </section>
        {/each}
      </aside>

      <section class="change-preview">
        <div class="all-changes-list">
          {#each visible as entry (entry.id)}
            <div class="change-card" id="change-{entry.id}">
              <div class="preview-header">
                <div>
                  <span class="eyebrow">{entry.category} · {entry.kind.replace("_", " ")}</span>
                  <h2><FileText size={18} /> {entry.path}</h2>
                  <p>{entry.createdAt} · {entry.reason}</p>
                </div>
                <div class="preview-actions">
                  <button class="secondary" on:click={() => showRollbackConfirm(entry)} disabled={entry.kind !== "file_changed"}>
                    <RotateCcw size={16} /> Rollback
                  </button>
                  <button class="secondary" on:click={() => openFullFile(entry)} disabled={!entry.canOpen}>
                    <Maximize2 size={16} /> Open
                  </button>
                </div>
              </div>

              <div
                class="summary-card"
                role="button"
                tabindex="0"
                on:click={() => toggleExpanded(entry)}
                on:keydown={(e) => (e.key === "Enter" || e.key === " ") && toggleExpanded(entry)}
                style="cursor: pointer;"
              >
                <div style="display: flex; justify-content: space-between; align-items: center;">
                  <strong>{entry.operation}</strong>
                  <span class="chev">{#if expanded[entry.id]}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}</span>
                </div>
                {#if !expanded[entry.id]}
                  <pre class="mini-preview">{entry.preview || "No preview available."}</pre>
                {/if}
              </div>

              {#if expanded[entry.id]}
                <div class="diff-card">
                  <div class="diff-title">Inline diff preview</div>
                  <pre>
{#each (entry.diff || entry.preview || "No diff available.").split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                  </pre>
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>

{#if editorOpen}
  <div class="editor-backdrop">
    <div class="editor-modal">
      <div class="editor-head">
        <div>
          <span class="eyebrow">Built-in editor</span>
          <h2>{editorPath}</h2>
        </div>
        <div class="editor-actions">
          {#if editorDirty}<span class="dirty">Unsaved</span>{/if}
          <button on:click={saveEditor} disabled={!editorDirty || saving}>
            <Save size={16} /> {saving ? "Saving..." : "Save"}
          </button>
          <button class="icon-btn" on:click={() => (editorOpen = false)}><X size={18} /></button>
        </div>
      </div>
      <textarea bind:value={editorContent} spellcheck="false" />
    </div>
  </div>
{/if}

<style>
  .change-history { width: 100%; }
  .toolbar, .toolbar-actions, .title, .preview-header, .preview-header h2, .editor-head, .editor-actions, .tracking-controls, .preview-actions { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 14px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 800; }
  .toolbar-actions { gap: 10px; }
  .search { position: relative; display: flex; align-items: center; min-width: 280px; }
  .search :global(svg) { position: absolute; left: 12px; color: var(--text-muted); }
  .search input { width: 100%; padding-left: 36px; }
  .notice, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .notice { padding: 12px 14px; margin-bottom: 14px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .tracking-controls { flex-wrap: wrap; gap: 8px; margin-bottom: 14px; padding: 10px; border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); background: rgba(255,255,255,.018); }
  .tracking-controls label { display: flex; align-items: center; gap: 7px; color: var(--text-secondary); font-size: 12px; }
  .tracking-controls input { width: auto; }
  .tracking-controls small { color: var(--text-muted); }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  .empty.inner { margin: 16px; }
  .history-layout { display: grid; grid-template-columns: 340px minmax(0, 1fr); gap: 16px; min-height: 76vh; }
  .change-tree, .change-preview, .summary-card, .diff-card { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .change-tree { position: relative; padding: 14px 14px 14px 28px; overflow: auto; background: transparent; border-color: transparent; }
  .timeline-line { position: absolute; left: 18px; top: 20px; bottom: 20px; width: 2px; background: linear-gradient(180deg, rgba(27,217,106,.7), rgba(139,92,246,.25)); border-radius: 999px; }
  h3 { margin: 16px 6px 8px; color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  .timeline-item { position: relative; margin-bottom: 10px; }
  .timeline-item::before { content: ""; position: absolute; left: -15px; top: 23px; width: 15px; height: 2px; background: rgba(27,217,106,.5); }
  .timeline-item::after { content: ""; position: absolute; left: -19px; top: 18px; width: 10px; height: 10px; border-radius: 50%; background: var(--bg-secondary); border: 2px solid var(--accent-primary); }
  .file-strip { width: 100%; min-height: 54px; display: grid; grid-template-columns: 18px minmax(0,1fr); align-items: center; gap: 4px 8px; text-align: left; background: var(--bg-tertiary); color: var(--text-secondary); border: 1px solid transparent; border-radius: 12px; padding: 10px 12px; transform: none; }
  .file-strip:hover, .file-strip.selected { border-color: rgba(27, 217, 106, 0.34); background: rgba(27, 217, 106, 0.07); color: var(--text-primary); }
  .chev { grid-row: 1 / span 2; color: var(--text-muted); }
  .file-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 800; }
  .file-strip small { grid-column: 2; }
  .mini-diff { margin: 6px 0 0 26px; max-height: 150px; padding: 10px; border: 1px solid var(--border-color); border-radius: 10px; background: #09090b; color: #a1a1aa; }
  .file-strip small, .preview-header p, .eyebrow { color: var(--text-muted); font-size: 12px; }
  .change-preview { min-width: 0; padding: 16px; overflow-y: auto; max-height: 80vh; }
  .change-card { margin-bottom: 32px; padding-bottom: 32px; border-bottom: 1px solid var(--border-color); }
  .change-card:last-child { border-bottom: none; }
  .mini-preview { margin-top: 10px; max-height: 80px; overflow: hidden; opacity: 0.7; mask-image: linear-gradient(to bottom, black 50%, transparent 100%); }
  .preview-header { justify-content: space-between; gap: 16px; margin-bottom: 14px; }
  .preview-actions { gap: 10px; flex-wrap: wrap; justify-content: flex-end; }
  .preview-header h2 { gap: 10px; margin: 4px 0; font-size: 20px; }
  .eyebrow { color: var(--accent-primary); text-transform: uppercase; letter-spacing: .1em; font-weight: 900; }
  .summary-card, .diff-card { padding: 14px; margin-bottom: 14px; background: var(--bg-tertiary); }
  .summary-card strong, .diff-title { display: block; color: var(--text-secondary); margin-bottom: 10px; font-weight: 800; }
  pre { overflow: auto; white-space: pre-wrap; margin: 0; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; color: #d4d4d8; }
  .diff-card pre { max-height: 58vh; background: #09090b; border-radius: 12px; padding: 12px; }
  pre span { display: block; }
  .added { color: #86efac; background: rgba(27, 217, 106, 0.08); }
  .removed { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
  .context { color: #a1a1aa; }
  .editor-backdrop { position: fixed; inset: 0; z-index: 60; background: rgba(0,0,0,.68); backdrop-filter: blur(12px); display: flex; align-items: center; justify-content: center; padding: 24px; }
  .editor-modal { width: min(1500px, 96vw); height: min(900px, 92vh); background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: 24px; overflow: hidden; display: flex; flex-direction: column; box-shadow: 0 30px 110px rgba(0,0,0,.55); }
  .editor-head { justify-content: space-between; gap: 16px; padding: 18px 20px; border-bottom: 1px solid var(--border-color); }
  .editor-head h2 { margin: 3px 0 0; font-size: 18px; }
  .editor-actions { gap: 10px; }
  .dirty { color: var(--accent-warning); font-size: 12px; font-weight: 800; }
  .icon-btn { width: 36px; height: 36px; padding: 0; background: transparent; color: var(--text-muted); }
  textarea { flex: 1; width: 100%; resize: none; border: 0; outline: none; padding: 20px; background: #0b0b0d; color: #e5e7eb; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 13px; line-height: 1.65; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1000px) { .history-layout { grid-template-columns: 1fr; } .search { min-width: 0; } }
</style>
