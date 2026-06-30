<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { History, Plus, RefreshCw, RotateCcw, Calendar, GitCompare } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type Snapshot = {
    id: string;
    name: string;
    createdAt: string;
    reason: string;
    changedFiles: string[];
  };

  type SnapshotDiff = {
    addedFiles: string[];
    removedFiles: string[];
    modifiedFiles: string[];
  };

  let snapshots: Snapshot[] = [];
  let loading = false;
  let newName = "";
  let error: string | null = null;
  let message: string | null = null;
  let projectDir: string | null = null;
  let lastLoadedPath: string | null = null;
  let fromId = "";
  let toId = "";
  let diff: SnapshotDiff | null = null;

  async function ensureProjectDir() {
    if (!$projectPath) return null;
    if (!projectDir || lastLoadedPath !== $projectPath) {
      projectDir = await invoke("get_project_dir", { path: $projectPath });
    }
    return projectDir;
  }

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && snapshots.length > 0) return;
    loading = true;
    error = null;
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      snapshots = await invoke("list_snapshots", { projectDir: dir });
      lastLoadedPath = $projectPath;
      if (snapshots.length >= 2) {
        fromId ||= snapshots[snapshots.length - 2].id;
        toId ||= snapshots[snapshots.length - 1].id;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function create() {
    if (!$projectPath) return;
    loading = true;
    error = null;
    message = null;
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      await invoke("create_snapshot", {
        projectDir: dir,
        name: newName || "manual",
        reason: "Created from UI",
      });
      newName = "";
      await load(true);
      message = "Snapshot created.";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function rollback(id: string) {
    if (!$projectPath) return;
    const ok = window.confirm(`Rollback project to snapshot ${id}? This will restore saved manifest/changed files.`);
    if (!ok) return;
    loading = true;
    error = null;
    message = null;
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      await invoke("rollback_snapshot", { projectDir: dir, id });
      message = `Rolled back to ${id}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function compare() {
    if (!$projectPath || !fromId || !toId || fromId === toId) return;
    error = null;
    message = null;
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      diff = await invoke("diff_snapshots", { projectDir: dir, from: fromId, to: toId });
    } catch (e) {
      error = String(e);
    }
  }

  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
</script>

<div class="snapshots">
  <div class="toolbar">
    <div class="title">
      <History size={18} />
      <span>Snapshots</span>
    </div>
    <div class="actions">
      <input bind:value={newName} placeholder="Snapshot name" />
      <button on:click={create} disabled={!$projectPath || loading}>
        <Plus size={16} />
        Create
      </button>
      <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if loading && snapshots.length === 0}
    <div class="loading">Loading snapshots...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to manage snapshots.</div>
  {:else if snapshots.length === 0}
    <div class="empty">No snapshots yet.</div>
  {:else}
    {#if snapshots.length >= 2}
      <div class="compare-panel">
        <div class="compare-title"><GitCompare size={16} /> Compare snapshots</div>
        <select bind:value={fromId}>
          {#each snapshots as s}<option value={s.id}>{s.name} · {s.id}</option>{/each}
        </select>
        <select bind:value={toId}>
          {#each snapshots as s}<option value={s.id}>{s.name} · {s.id}</option>{/each}
        </select>
        <button class="secondary" on:click={compare} disabled={fromId === toId}>Diff</button>
      </div>
      {#if diff}
        <div class="diff-panel">
          <div><strong>{diff.addedFiles.length}</strong><span>Added</span></div>
          <div><strong>{diff.removedFiles.length}</strong><span>Removed</span></div>
          <div><strong>{diff.modifiedFiles.length}</strong><span>Modified by content</span></div>
          {#if diff.addedFiles.length || diff.removedFiles.length || diff.modifiedFiles.length}
            <pre>{JSON.stringify(diff, null, 2)}</pre>
          {/if}
        </div>
      {/if}
    {/if}

    <div class="grid">
      {#each snapshots as s}
        <div class="card">
          <div class="card-header">
            <h3>{s.name}</h3>
            <span class="badge">{s.id}</span>
          </div>
          <p class="reason">{s.reason}</p>
          <div class="changed">{s.changedFiles?.length ?? 0} tracked changed files</div>
          <div class="card-footer">
            <div class="date">
              <Calendar size={14} />
              <span>{s.createdAt}</span>
            </div>
            <button class="ghost rollback" on:click={() => rollback(s.id)}>
              <RotateCcw size={14} />
              Rollback
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .snapshots { max-width: 1200px; }
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin-bottom: 20px; flex-wrap: wrap; }
  .title, .actions, .date, .compare-title { display: flex; align-items: center; gap: 10px; }
  .title { color: var(--text-secondary); font-weight: 600; }
  .actions { gap: 10px; }
  .actions input { min-width: 180px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .compare-panel, .diff-panel { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; margin-bottom: 16px; }
  .compare-panel { display: grid; grid-template-columns: auto 1fr 1fr auto; gap: 10px; align-items: center; }
  .compare-title { color: var(--text-secondary); font-weight: 700; }
  .diff-panel { display: grid; grid-template-columns: repeat(3, minmax(120px, 1fr)); gap: 12px; }
  .diff-panel div { background: var(--bg-tertiary); border-radius: 12px; padding: 12px; display: flex; flex-direction: column; gap: 4px; }
  .diff-panel strong { font-size: 24px; color: var(--text-primary); }
  .diff-panel span, .changed { color: var(--text-muted); font-size: 12px; }
  pre { grid-column: 1 / -1; overflow: auto; background: #0d0d10; border-radius: 12px; padding: 12px; color: var(--text-secondary); font-size: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px; }
  .card { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 20px; display: flex; flex-direction: column; gap: 12px; transition: transform 0.15s ease; }
  .card:hover { transform: translateY(-2px); }
  .card-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .card-header h3 { font-size: 16px; font-weight: 700; }
  .badge { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 3px 8px; border-radius: 4px; font-family: ui-monospace, monospace; max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
  .reason { color: var(--text-secondary); font-size: 13px; flex: 1; }
  .card-footer { display: flex; justify-content: space-between; align-items: center; padding-top: 12px; border-top: 1px solid var(--border-color); }
  .date { font-size: 12px; color: var(--text-muted); }
  .rollback { padding: 6px 10px; font-size: 12px; font-weight: 600; }
  .empty, .loading { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) { .compare-panel { grid-template-columns: 1fr; } }
</style>
