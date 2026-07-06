<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { History, Plus, RefreshCw, RotateCcw, Calendar, GitCompare, FileText, Archive, Trash2 } from "lucide-svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
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

  type FileDiff = {
    path: string;
    fromExists: boolean;
    toExists: boolean;
    text: string;
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
  let selectedDiffPath = "";
  let fileDiff: FileDiff | null = null;
  let diffLoading = false;

  // Confirm dialog state
  let confirmOpen = false;
  let confirmTitle = "";
  let confirmMessage = "";
  let confirmDanger = false;
  let confirmAction: (() => void) | null = null;

  function showConfirm(title: string, message: string, action: () => void, danger = false) {
    confirmTitle = title; confirmMessage = message; confirmAction = action; confirmDanger = danger; confirmOpen = true;
  }

  function handleConfirm() {
    if (confirmAction) confirmAction();
    confirmOpen = false; confirmAction = null;
  }
  let manifestDiff: any = null;
  let manifestDiffLoading = false;

  // Backups
  let backups: any[] = [];
  let backupLoading = false;
  let backupName = "";

  async function loadBackups() {
    if (!$projectPath) return;
    backupLoading = true;
    try { backups = await invoke("list_backups", { path: $projectPath }); }
    catch { backups = []; }
    finally { backupLoading = false; }
  }

  async function createBackup() {
    if (!$projectPath) return;
    loading = true;
    try {
      await invoke("create_project_backup", { path: $projectPath, name: backupName || null });
      backupName = "";
      await loadBackups();
      message = "Backup created.";
    } catch (e) { error = String(e); }
    finally { loading = false; }
  }

  async function deleteBackup(id: string) {
    if (!$projectPath) return;
    await invoke("delete_backup", { path: $projectPath, backupId: id });
    await loadBackups();
  }

  async function restoreBackup(id: string) {
    if (!$projectPath) return;
    showConfirm("Restore backup", "Restore this backup? A safety snapshot will be created first.", async () => {
    loading = true; error = null;
    try {
      await invoke("restore_backup", { path: $projectPath, backupId: id });
      message = "Backup restored. A safety snapshot was created.";
      await load(true);
    } catch (e) { error = String(e); }
    finally { loading = false; }
    }, true);
  }

  function formatBytes(b: number) {
    if (b < 1024) return b + " B";
    if (b < 1048576) return (b/1024).toFixed(1) + " KB";
    if (b < 1073741824) return (b/1048576).toFixed(1) + " MB";
    return (b/1073741824).toFixed(1) + " GB";
  }

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
    showConfirm("Rollback snapshot", `Rollback project to snapshot ${id}? This will restore manifest and changed files.`, async () => {
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
    });
  }

  async function compare() {
    if (!$projectPath || !fromId || !toId || fromId === toId) return;
    error = null;
    message = null;
    fileDiff = null;
    selectedDiffPath = "";
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      const nextDiff: SnapshotDiff = await invoke("diff_snapshots", { projectDir: dir, from: fromId, to: toId });
      diff = nextDiff;
      const files = Array.from(new Set([...nextDiff.addedFiles, ...nextDiff.removedFiles, ...nextDiff.modifiedFiles])).sort();
      selectedDiffPath = files[0] ?? "";
      if (selectedDiffPath) await openFileDiff(selectedDiffPath);
    } catch (e) {
      error = String(e);
    }
  }

  async function loadManifestDiff() {
    const dir = await ensureProjectDir();
    if (!dir || !fromId || !toId) return;
    manifestDiffLoading = true;
    error = null;
    try {
      manifestDiff = await invoke("diff_manifest_snapshots", {
        projectDir: dir,
        fromId,
        toId,
      });
    } catch (e) {
      error = String(e);
    } finally {
      manifestDiffLoading = false;
    }
  }

  async function openFileDiff(path: string) {
    const dir = await ensureProjectDir();
    if (!dir || !fromId || !toId) return;
    selectedDiffPath = path;
    diffLoading = true;
    error = null;
    try {
      fileDiff = await invoke("get_snapshot_file_diff", {
        projectDir: dir,
        from: fromId,
        to: toId,
        relativePath: path,
      });
    } catch (e) {
      error = String(e);
    } finally {
      diffLoading = false;
    }
  }

  function lineClass(line: string) {
    if (line.startsWith("+ ")) return "added";
    if (line.startsWith("- ")) return "removed";
    return "context";
  }

  $: allDiffFiles = diff
    ? Array.from(new Set([...diff.addedFiles, ...diff.removedFiles, ...diff.modifiedFiles])).sort()
    : [];
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
      <input bind:value={backupName} placeholder="Backup name" />
      <button on:click={createBackup} disabled={!$projectPath || loading}>
        <Archive size={16} /> Backup
      </button>
      <button on:click={create} disabled={!$projectPath || loading}>
        <Plus size={16} />
        Create
      </button>
      <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
      <button class="secondary" on:click={loadBackups} disabled={!$projectPath || backupLoading} title="Backups">
        <Archive size={16} /> Backups
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
        <button class="secondary" on:click={compare} disabled={fromId === toId}>Diff files</button>
        <button class="secondary" on:click={loadManifestDiff} disabled={fromId === toId || manifestDiffLoading}>
          {manifestDiffLoading ? 'Loading...' : 'Diff manifest'}
        </button>
      </div>
      {#if manifestDiff}
        <div class="manifest-diff-panel">
          <h3>Manifest changes</h3>
          <div class="manifest-diff-stats">
            {#if manifestDiff.mcVersionChanged}
              <div class="diff-stat changed"><strong>MC version</strong><span>{manifestDiff.fromMcVersion} → {manifestDiff.toMcVersion}</span></div>
            {/if}
            {#if manifestDiff.loaderVersionChanged}
              <div class="diff-stat changed"><strong>Loader</strong><span>{manifestDiff.fromLoaderVersion} → {manifestDiff.toLoaderVersion}</span></div>
            {/if}
            {#if manifestDiff.addedMods?.length}
              <div class="diff-stat added"><strong>+{manifestDiff.addedMods.length} mods</strong><span>{manifestDiff.addedMods.join(", ")}</span></div>
            {/if}
            {#if manifestDiff.removedMods?.length}
              <div class="diff-stat removed"><strong>-{manifestDiff.removedMods.length} mods</strong><span>{manifestDiff.removedMods.join(", ")}</span></div>
            {/if}
          </div>
          <pre class="manifest-diff-text">{manifestDiff.diffText || 'No differences.'}</pre>
        </div>
      {/if}
      {#if diff}
        <div class="diff-panel">
          <div><strong>{diff.addedFiles.length}</strong><span>Added</span></div>
          <div><strong>{diff.removedFiles.length}</strong><span>Removed</span></div>
          <div><strong>{diff.modifiedFiles.length}</strong><span>Modified by content</span></div>
        </div>

        {#if allDiffFiles.length > 0}
          <div class="inline-diff-shell">
            <aside class="diff-files">
              <h3><FileText size={14} /> Changed files</h3>
              {#each allDiffFiles as path}
                <button class:selected={selectedDiffPath === path} on:click={() => openFileDiff(path)}>
                  <span>{path}</span>
                  {#if diff.addedFiles.includes(path)}<small class="added-label">added</small>{/if}
                  {#if diff.removedFiles.includes(path)}<small class="removed-label">removed</small>{/if}
                  {#if diff.modifiedFiles.includes(path)}<small>modified</small>{/if}
                </button>
              {/each}
            </aside>
            <section class="inline-diff">
              {#if diffLoading}
                <div class="muted">Loading file diff...</div>
              {:else if fileDiff}
                <div class="inline-diff-header">
                  <strong>{fileDiff.path}</strong>
                  <span>{fileDiff.fromExists ? "from exists" : "from missing"} → {fileDiff.toExists ? "to exists" : "to missing"}</span>
                </div>
                <pre>
{#each fileDiff.text.split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                </pre>
              {:else}
                <div class="muted">Select a file to view inline diff.</div>
              {/if}
            </section>
          </div>
        {/if}
      {/if}
    {/if}

    {#if backups.length > 0}
      <div class="backup-section">
        <h3>Project backups ({backups.length})</h3>
        <div class="backup-list">
          {#each backups.slice(0, 6) as b}
            <div class="backup-row">
              <div class="backup-info">
                <strong>{b.name}</strong>
                <span>{b.createdAt} · {formatBytes(b.sizeBytes)}</span>
              </div>
              <button class="ghost mini" on:click={() => restoreBackup(b.id)} title="Restore">
                <RotateCcw size={14} />
              </button>
              <button class="ghost mini danger" on:click={() => deleteBackup(b.id)}>
                <Trash2 size={14} />
              </button>
            </div>
          {/each}
        </div>
      </div>
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
{#if confirmOpen}
    <ConfirmDialog title={confirmTitle} message={confirmMessage} danger={confirmDanger}
      on:confirm={handleConfirm} on:cancel={() => (confirmOpen = false, confirmAction = null)} />
  {/if}
</div>

<style>
  .snapshots { max-width: none; width: 100%; }
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin-bottom: 20px; flex-wrap: wrap; }
  .title, .actions, .date, .compare-title, .diff-files h3 { display: flex; align-items: center; gap: 10px; }
  .title { color: var(--text-secondary); font-weight: 600; }
  .actions { gap: 10px; }
  .actions input { min-width: 180px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .compare-panel, .diff-panel, .inline-diff-shell { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; margin-bottom: 16px; }
  .compare-panel { display: grid; grid-template-columns: auto 1fr 1fr auto; gap: 10px; align-items: center; }
  .compare-title { color: var(--text-secondary); font-weight: 700; }
  .diff-panel { display: grid; grid-template-columns: repeat(3, minmax(120px, 1fr)); gap: 12px; }
  .diff-panel div { background: var(--bg-tertiary); border-radius: 12px; padding: 12px; display: flex; flex-direction: column; gap: 4px; }
  .diff-panel strong { font-size: 24px; color: var(--text-primary); }
  .diff-panel span, .changed, .muted { color: var(--text-muted); font-size: 12px; }
  .inline-diff-shell { display: grid; grid-template-columns: 310px minmax(0, 1fr); gap: 14px; }
  .diff-files { border-right: 1px solid var(--border-color); padding-right: 14px; }
  .diff-files h3 { color: var(--text-muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 10px; }
  .diff-files button { width: 100%; justify-content: space-between; text-align: left; background: transparent; color: var(--text-secondary); border: 1px solid transparent; padding: 9px 10px; margin-bottom: 5px; transform: none; }
  .diff-files button:hover, .diff-files button.selected { background: var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.28); color: var(--text-primary); }
  .diff-files small { color: var(--text-muted); }
  .added-label { color: var(--accent-primary) !important; }
  .removed-label { color: #fca5a5 !important; }
  .manifest-diff-panel { margin-top: 14px; padding: 14px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .manifest-diff-panel h3 { font-size: 13px; margin: 0 0 10px; color: var(--text-secondary); }
  .manifest-diff-stats { display: grid; gap: 6px; margin-bottom: 12px; }
  .diff-stat { display: flex; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: 8px; font-size: 12px; background: var(--bg-secondary); border: 1px solid var(--border-color); }
  .diff-stat strong { color: var(--text-primary); }
  .diff-stat span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-stat.changed { border-color: rgba(245,158,11,.30); }
  .diff-stat.added { border-color: rgba(27,217,106,.30); }
  .diff-stat.removed { border-color: rgba(239,68,68,.30); }
  .manifest-diff-text { margin: 0; padding: 12px; border-radius: 10px; background: #0d0d10; color: #a1a1aa; font-family: ui-monospace,monospace; font-size: 11px; line-height: 1.5; max-height: 360px; overflow: auto; white-space: pre-wrap; }
  .inline-diff { min-width: 0; }
  .inline-diff-header { display: flex; justify-content: space-between; gap: 12px; padding: 0 0 10px; color: var(--text-secondary); }
  .inline-diff-header span { color: var(--text-muted); font-size: 12px; }
  pre { overflow: auto; max-height: 620px; background: #0d0d10; border-radius: 12px; padding: 12px; color: var(--text-secondary); font-size: 12px; line-height: 1.5; margin: 0; }
  pre span { display: block; white-space: pre-wrap; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
  pre span.added { color: #86efac; background: rgba(27, 217, 106, 0.08); }
  pre span.removed { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
  pre span.context { color: #a1a1aa; }
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
  .backup-section { margin-bottom: 18px; padding: 14px; border: 1px solid rgba(139,92,246,.25); border-radius: var(--border-radius-lg); background: rgba(139,92,246,.03); }
  .backup-section h3 { color: var(--text-secondary); font-size: 14px; margin: 0 0 10px; }
  .backup-list { display: grid; gap: 6px; }
  .backup-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .backup-info { display: grid; gap: 3px; }
  .backup-info strong { color: var(--text-primary); font-size: 13px; }
  .backup-info span { color: var(--text-muted); font-size: 11px; }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) { .compare-panel, .inline-diff-shell { grid-template-columns: 1fr; } .diff-files { border-right: 0; padding-right: 0; } }
</style>
