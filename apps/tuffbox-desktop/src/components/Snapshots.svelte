<script lang="ts">
  import {
    History, Plus, RefreshCw, RotateCcw, Calendar, GitCompare, FileText, Archive, Trash2,
    Search, ChevronDown, ChevronRight, ExternalLink, AlertTriangle,
  } from "@lucide/svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import EmptyState from "./EmptyState.svelte";
  import {
    api,
    type BackupEntry,
    type ManifestSnapshotDiff,
    type Snapshot,
    type SnapshotDetail,
    type SnapshotDiff,
    type SnapshotFileDiff,
  } from "../lib/api";
  import { historyFocusSnapshotId, ideStageRequest, projectPath } from "../lib/store";

  let snapshots: Snapshot[] = [];
  let loading = $state(false);
  let newName = "";
  let error: string | null = null;
  let message: string | null = null;
  let projectDir: string | null = null;
  let lastLoadedPath: string | null = null;
  let fromId = "";
  let toId = "";
  let diff = $state<SnapshotDiff | null>(null);
  let selectedDiffPath = "";
  let fileDiff: SnapshotFileDiff | null = null;
  let diffLoading = $state(false);

  let selectedId = "";
  let detail: SnapshotDetail | null = null;
  let detailLoading = $state(false);
  let search = "";
  let filterKind = $state<"all" | "auto" | "manual" | "crash">("all");
  let backupsOpen = $state(false);
  let compareOpen = $state(false);

  let confirmOpen = $state(false);
  let confirmTitle = "";
  let confirmMessage = "";
  let confirmDanger = $state(false);
  let confirmAction: (() => void) | null = null;

  function showConfirm(title: string, message: string, action: () => void, danger = false) {
    confirmTitle = title;
    confirmMessage = message;
    confirmAction = action;
    confirmDanger = danger;
    confirmOpen = true;
  }

  function handleConfirm() {
    if (confirmAction) confirmAction();
    confirmOpen = false;
    confirmAction = null;
  }

  let manifestDiff: ManifestSnapshotDiff | null = null;
  let manifestDiffLoading = $state(false);

  let backups: BackupEntry[] = [];
  let backupLoading = $state(false);
  let backupName = "";

  async function ensureProjectDir() {
    if (!$projectPath) return null;
    if (!projectDir || lastLoadedPath !== $projectPath) {
      projectDir = await api.project.getDir($projectPath);
    }
    return projectDir;
  }

  async function loadBackups() {
    if (!$projectPath) return;
    backupLoading = true;
    try {
      backups = await api.backups.list($projectPath);
    } catch {
      backups = [];
    } finally {
      backupLoading = false;
    }
  }

  async function createBackup() {
    if (!$projectPath) return;
    loading = true;
    try {
      await api.backups.create(backupName || null, $projectPath);
      backupName = "";
      await loadBackups();
      message = "Backup created.";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function deleteBackup(id: string) {
    if (!$projectPath) return;
    await api.backups.delete(id, $projectPath);
    await loadBackups();
  }

  async function restoreBackup(id: string) {
    if (!$projectPath) return;
    showConfirm(
      "Restore backup",
      "Restore this backup? A safety snapshot will be created first.",
      async () => {
        loading = true;
        error = null;
        try {
          await api.backups.restore(id, $projectPath!);
          message = "Backup restored. A safety snapshot was created.";
          await load(true);
        } catch (e) {
          error = String(e);
        } finally {
          loading = false;
        }
      },
      true,
    );
  }

  function formatBytes(b: number) {
    if (b < 1024) return b + " B";
    if (b < 1048576) return (b / 1024).toFixed(1) + " KB";
    if (b < 1073741824) return (b / 1048576).toFixed(1) + " MB";
    return (b / 1073741824).toFixed(1) + " GB";
  }

  function formatDate(iso: string) {
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }

  function operationLabel(s: Snapshot): string {
    if (s.operation) return s.operation;
    if (s.name?.startsWith("auto-before-")) return s.name.slice("auto-before-".length);
    if (s.tags?.includes("crash_fix")) return "crash_fix";
    return s.name || "snapshot";
  }

  function isCrash(s: Snapshot) {
    return !!s.tags?.includes("crash_fix") || operationLabel(s).includes("crash");
  }

  function isAuto(s: Snapshot) {
    return s.name?.startsWith("auto-") || s.actor === "launcher" || s.actor === "ai" || s.actor === "scan";
  }

  function isManual(s: Snapshot) {
    return s.actor === "user" || s.operation === "manual" || (!isAuto(s) && !isCrash(s));
  }

  function previewLine(s: Snapshot): string {
    const summary = s.actionsSummary?.filter(Boolean) ?? [];
    if (summary.length) return summary.slice(0, 2).join(" · ");
    return s.reason || "No action details";
  }

  const filtered = $derived((() => {
    const q = search.trim().toLowerCase();
    let list = [...snapshots].reverse();
    if (filterKind === "auto") list = list.filter(isAuto);
    else if (filterKind === "manual") list = list.filter(isManual);
    else if (filterKind === "crash") list = list.filter(isCrash);
    if (q) {
      list = list.filter((s) => {
        const hay = [
          s.name,
          s.reason,
          s.id,
          operationLabel(s),
          ...(s.actionsSummary ?? []),
          ...(s.tags ?? []),
        ]
          .join(" ")
          .toLowerCase();
        return hay.includes(q);
      });
    }
    return list;
  })());

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && snapshots.length > 0) return;
    loading = true;
    error = null;
    try {
      const dir = await ensureProjectDir();
      if (!dir) return;
      snapshots = await api.snapshots.list(dir);
      lastLoadedPath = $projectPath;
      if (snapshots.length >= 2) {
        fromId ||= snapshots[snapshots.length - 2].id;
        toId ||= snapshots[snapshots.length - 1].id;
      }
      if (selectedId && !snapshots.some((s) => s.id === selectedId)) {
        selectedId = "";
        detail = null;
      }
      if (!selectedId && snapshots.length) {
        await selectSnapshot(snapshots[snapshots.length - 1].id);
      } else if (selectedId) {
        await selectSnapshot(selectedId);
      }
      await loadBackups();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function selectSnapshot(id: string) {
    selectedId = id;
    const dir = await ensureProjectDir();
    if (!dir) return;
    detailLoading = true;
    error = null;
    try {
      detail = await api.snapshots.detail(id, dir);
    } catch (e) {
      error = String(e);
      detail = null;
    } finally {
      detailLoading = false;
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
      const snap = await api.snapshots.create(newName || "manual", "Created from UI", dir);
      newName = "";
      await load(true);
      await selectSnapshot(snap.id);
      message = "Snapshot created.";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function rollback(id: string) {
    if (!$projectPath) return;
    const s = snapshots.find((x) => x.id === id);
    const manifestOnly = detail?.snapshot.id === id ? detail.manifestOnly : !(s?.changedFiles?.length);
    const warn = manifestOnly
      ? "\n\nThis snapshot has no tracked file copies — rollback restores manifest/lockfile only. Mod jars on disk may differ; check History."
      : "";
    showConfirm(
      "Rollback snapshot",
      `Rollback project to snapshot ${id}? This will restore manifest and changed files.${warn}`,
      async () => {
        loading = true;
        error = null;
        message = null;
        try {
          const dir = await ensureProjectDir();
          if (!dir) return;
          await api.snapshots.rollback(id, dir);
          message = `Rolled back to ${id}.`;
          await load(true);
        } catch (e) {
          error = String(e);
        } finally {
          loading = false;
        }
      },
      true,
    );
  }

  async function removeSnapshot(id: string) {
    showConfirm(
      "Delete snapshot",
      `Permanently delete snapshot ${id}? This cannot be undone.`,
      async () => {
        loading = true;
        error = null;
        try {
          const dir = await ensureProjectDir();
          if (!dir) return;
          await api.snapshots.delete(id, dir);
          if (selectedId === id) {
            selectedId = "";
            detail = null;
          }
          message = "Snapshot deleted.";
          await load(true);
        } catch (e) {
          error = String(e);
        } finally {
          loading = false;
        }
      },
      true,
    );
  }

  function compareWithPrevious(id: string) {
    const idx = snapshots.findIndex((s) => s.id === id);
    if (idx <= 0) {
      error = "No previous snapshot to compare with.";
      return;
    }
    fromId = snapshots[idx - 1].id;
    toId = id;
    compareOpen = true;
    compare();
  }

  function openInHistory(id: string) {
    historyFocusSnapshotId.set(id);
    ideStageRequest.set("history");
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
      diff = await api.snapshots.diff(fromId, toId, dir);
      selectedDiffPath = "";
      fileDiff = null;
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
      manifestDiff = await api.snapshots.diffManifest(fromId, toId, dir);
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
      fileDiff = await api.snapshots.fileDiff(fromId, toId, path, dir);
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

  const allDiffFiles = $derived(diff
    ? Array.from(new Set([...diff.addedFiles, ...diff.removedFiles, ...diff.modifiedFiles])).sort()
    : []);
  $effect(() => {
    if ($projectPath && lastLoadedPath !== $projectPath) load(true);
  });
</script>

<div class="snapshots">
  <div class="toolbar">
    <div class="title">
      <History size={18} />
      <span>Snapshots</span>
    </div>
    <div class="actions">
      <input bind:value={newName} placeholder="Snapshot name" />
      <button onclick={create} disabled={!$projectPath || loading}>
        <Plus size={16} />
        Create
      </button>
      <button class="ghost" onclick={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if loading && snapshots.length === 0}
    <div class="loading">Loading snapshots...</div>
  {:else if !$projectPath}
    <EmptyState icon={History} title="No project selected" description="Open a project to manage snapshots." />
  {:else if snapshots.length === 0}
    <EmptyState icon={History} title="No snapshots yet" description="Create a snapshot to save the current state of your project." />
  {:else}
    <div class="filters">
      <div class="search">
        <Search size={14} />
        <input bind:value={search} placeholder="Search name, actions, tags…" />
      </div>
      <div class="chips">
        <button class:active={filterKind === "all"} onclick={() => (filterKind = "all")}>All</button>
        <button class:active={filterKind === "auto"} onclick={() => (filterKind = "auto")}>Auto</button>
        <button class:active={filterKind === "manual"} onclick={() => (filterKind = "manual")}>Manual</button>
        <button class:active={filterKind === "crash"} onclick={() => (filterKind = "crash")}>Crash fix</button>
      </div>
    </div>

    <div class="master-detail">
      <aside class="list-pane">
        {#each filtered as s (s.id)}
          <button
            type="button"
            class="row"
            class:selected={selectedId === s.id}
            onclick={() => selectSnapshot(s.id)}
          >
            <div class="row-top">
              <strong>{s.name}</strong>
              <span class="op-badge">{operationLabel(s)}</span>
            </div>
            <p class="preview">{previewLine(s)}</p>
            <div class="row-meta">
              <span><Calendar size={12} /> {formatDate(s.createdAt)}</span>
              {#if s.tags?.length}
                <span class="tags">
                  {#each s.tags as t}<span class="tag" class:crash-fix={t === "crash_fix"}>{t}</span>{/each}
                </span>
              {/if}
            </div>
          </button>
        {:else}
          <div class="muted pad">No snapshots match filters.</div>
        {/each}
      </aside>

      <section class="detail-pane">
        {#if detailLoading}
          <div class="muted pad">Loading details…</div>
        {:else if detail}
          {@const s = detail.snapshot}
          <div class="detail-header">
            <div>
              <h2>{s.name}</h2>
              <div class="detail-sub">
                <span class="badge">{s.id}</span>
                <span class="muted">{formatDate(s.createdAt)}</span>
                {#if s.actor}<span class="tag">{s.actor}</span>{/if}
                {#if s.planSource}<span class="tag">{s.planSource}</span>{/if}
              </div>
            </div>
            <div class="detail-actions">
              <button class="secondary" onclick={() => compareWithPrevious(s.id)} title="Compare with previous">
                <GitCompare size={14} /> Compare prev
              </button>
              <button class="secondary" onclick={() => openInHistory(s.id)}>
                <ExternalLink size={14} /> History
              </button>
              <button class="ghost rollback" onclick={() => rollback(s.id)}>
                <RotateCcw size={14} /> Rollback
              </button>
              <button class="ghost danger" onclick={() => removeSnapshot(s.id)}>
                <Trash2 size={14} /> Delete
              </button>
            </div>
          </div>

          {#if s.tags?.length || s.crashFingerprintKey || s.matchedCaseIds?.length}
            <div class="tag-row">
              {#each s.tags ?? [] as t}
                <span class="tag" class:crash-fix={t === "crash_fix"}>{t}</span>
              {/each}
              {#if s.crashFingerprintKey}
                <span class="tag mono" title={s.crashFingerprintKey}>{s.crashFingerprintKey.slice(0, 28)}…</span>
              {/if}
              {#each s.matchedCaseIds ?? [] as cid}
                <span class="tag mono">{cid}</span>
              {/each}
            </div>
          {/if}

          <p class="reason">{s.reason}</p>

          {#if detail.manifestOnly}
            <div class="notice warn">
              <AlertTriangle size={14} />
              Manifest/lockfile checkpoint — no tracked file copies. Rollback will not restore mod jars from this snapshot.
            </div>
          {/if}

          {#if detail.humanExplanation}
            <div class="block">
              <h3>Explanation</h3>
              <p>{detail.humanExplanation}</p>
            </div>
          {/if}

          <div class="block">
            <h3>Actions ({detail.actionsSummary.length})</h3>
            {#if detail.actionsSummary.length}
              <ul class="action-list">
                {#each detail.actionsSummary as line}
                  <li>{line}</li>
                {/each}
              </ul>
            {:else}
              <p class="muted">No action details recorded.</p>
            {/if}
          </div>

          <div class="block">
            <h3>Changed files ({detail.changedFiles.length})</h3>
            {#if detail.changedFiles.length}
              <ul class="file-list">
                {#each detail.changedFiles as f}
                  <li><span class="cat">{f.category}</span> {f.path}</li>
                {/each}
              </ul>
            {:else}
              <p class="muted">No tracked files copied into this snapshot.</p>
            {/if}
          </div>

          {#if detail.relatedEvents.length}
            <div class="block">
              <h3>Related activity ({detail.relatedEvents.length})</h3>
              <ul class="event-list">
                {#each detail.relatedEvents as ev}
                  <li>
                    <span class="tag">{ev.actor}</span>
                    <span>{ev.summary}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        {:else}
          <EmptyState icon={History} title="Select a snapshot" description="Pick a checkpoint on the left to see actions, files, and rollback options." />
        {/if}
      </section>
    </div>

    <div class="collapsible">
      <button type="button" class="collapse-toggle" onclick={() => (compareOpen = !compareOpen)}>
        {#if compareOpen}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}
        <GitCompare size={16} /> Compare snapshots
      </button>
      {#if compareOpen}
        <div class="compare-panel">
          <select bind:value={fromId}>
            {#each snapshots as s}<option value={s.id}>{s.name} · {s.id}</option>{/each}
          </select>
          <select bind:value={toId}>
            {#each snapshots as s}<option value={s.id}>{s.name} · {s.id}</option>{/each}
          </select>
          <button class="secondary" onclick={compare} disabled={fromId === toId}>Diff files</button>
          <button class="secondary" onclick={loadManifestDiff} disabled={fromId === toId || manifestDiffLoading}>
            {manifestDiffLoading ? "Loading..." : "Diff manifest"}
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
            <pre class="manifest-diff-text">{manifestDiff.diffText || "No differences."}</pre>
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
                  <button class:selected={selectedDiffPath === path} onclick={() => openFileDiff(path)}>
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
    </div>

    <div class="collapsible">
      <button type="button" class="collapse-toggle" onclick={() => (backupsOpen = !backupsOpen)}>
        {#if backupsOpen}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}
        <Archive size={16} /> Project backups ({backups.length})
      </button>
      {#if backupsOpen}
        <div class="backup-section">
          <div class="backup-create">
            <input bind:value={backupName} placeholder="Backup name" />
            <button onclick={createBackup} disabled={!$projectPath || loading}>
              <Archive size={16} /> Backup
            </button>
            <button class="ghost" onclick={loadBackups} disabled={backupLoading}>
              <RefreshCw size={14} class={backupLoading ? "spin" : ""} />
            </button>
          </div>
          {#if backups.length > 0}
            <div class="backup-list">
              {#each backups.slice(0, 12) as b}
                <div class="backup-row">
                  <div class="backup-info">
                    <strong>{b.name}</strong>
                    <span>{formatDate(b.createdAt)} · {formatBytes(b.sizeBytes)}</span>
                  </div>
                  <button class="ghost mini" onclick={() => restoreBackup(b.id)} title="Restore">
                    <RotateCcw size={14} />
                  </button>
                  <button class="ghost mini danger" onclick={() => deleteBackup(b.id)}>
                    <Trash2 size={14} />
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="muted">No zip backups yet.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if confirmOpen}
    <ConfirmDialog
      title={confirmTitle}
      message={confirmMessage}
      danger={confirmDanger}
      onconfirm={handleConfirm}
      oncancel={() => ((confirmOpen = false), (confirmAction = null))}
    />
  {/if}
</div>

<style>
  .snapshots { max-width: none; width: 100%; display: flex; flex-direction: column; gap: 14px; }
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .title, .actions, .row-meta, .detail-sub, .detail-actions, .backup-create, .search, .collapse-toggle {
    display: flex; align-items: center; gap: 10px;
  }
  .title { color: var(--text-secondary); font-weight: 600; }
  .actions input, .backup-create input, .search input { min-width: 180px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); border: 1px solid var(--border-color); display: flex; align-items: flex-start; gap: 8px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .notice.warn { color: #fcd34d; background: rgba(245, 158, 11, 0.08); border-color: rgba(245, 158, 11, 0.28); font-size: 13px; }

  .filters { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; justify-content: space-between; }
  .search { flex: 1; min-width: 220px; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 0 10px; color: var(--text-muted); }
  .search input { flex: 1; border: 0; background: transparent; color: var(--text-primary); padding: 10px 0; outline: none; min-width: 0; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chips button { background: var(--bg-secondary); border: 1px solid var(--border-color); color: var(--text-muted); padding: 6px 12px; font-size: 12px; transform: none; }
  .chips button.active { color: var(--text-primary); border-color: rgba(27, 217, 106, 0.35); background: rgba(27, 217, 106, 0.08); }

  .master-detail { display: grid; grid-template-columns: minmax(280px, 360px) minmax(0, 1fr); gap: 14px; min-height: 420px; }
  .list-pane, .detail-pane, .compare-panel, .diff-panel, .inline-diff-shell, .backup-section, .collapsible {
    background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg);
  }
  .list-pane { overflow: auto; max-height: 70vh; padding: 8px; display: flex; flex-direction: column; gap: 6px; }
  .row { width: 100%; text-align: left; background: transparent; border: 1px solid transparent; border-radius: var(--border-radius-md); padding: 12px; color: var(--text-secondary); display: grid; gap: 6px; transform: none; }
  .row:hover, .row.selected { background: var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.28); color: var(--text-primary); }
  .row-top { display: flex; justify-content: space-between; gap: 8px; align-items: flex-start; }
  .row-top strong { font-size: 13px; color: var(--text-primary); }
  .op-badge { font-size: 10px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); color: var(--text-muted); font-family: ui-monospace, monospace; max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .preview { margin: 0; font-size: 12px; color: var(--text-muted); line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .row-meta { font-size: 11px; color: var(--text-muted); flex-wrap: wrap; }
  .tags { display: flex; gap: 4px; flex-wrap: wrap; }

  .detail-pane { padding: 18px; overflow: auto; max-height: 70vh; display: flex; flex-direction: column; gap: 14px; }
  .detail-header { display: flex; justify-content: space-between; gap: 12px; flex-wrap: wrap; align-items: flex-start; }
  .detail-header h2 { margin: 0 0 6px; font-size: 18px; }
  .detail-actions { flex-wrap: wrap; }
  .badge { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 3px 8px; border-radius: 4px; font-family: ui-monospace, monospace; max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .tag { font-size: 11px; padding: 2px 8px; border-radius: 999px; background: var(--bg-elevated); color: var(--text-muted); }
  .tag.crash-fix { color: var(--accent-primary); background: rgba(27, 217, 106, 0.12); }
  .tag.mono { font-family: ui-monospace, monospace; max-width: 180px; overflow: hidden; text-overflow: ellipsis; }
  .tag-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .reason { color: var(--text-secondary); font-size: 13px; margin: 0; }
  .block h3 { margin: 0 0 8px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-muted); }
  .action-list, .file-list, .event-list { margin: 0; padding-left: 18px; display: grid; gap: 6px; color: var(--text-secondary); font-size: 13px; }
  .file-list .cat { color: var(--text-muted); font-size: 11px; margin-right: 6px; }
  .event-list li { display: flex; gap: 8px; align-items: flex-start; }

  .collapsible { overflow: hidden; }
  .collapse-toggle { width: 100%; justify-content: flex-start; background: transparent; border: 0; color: var(--text-secondary); font-weight: 600; padding: 12px 14px; transform: none; }
  .compare-panel { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; padding: 0 14px 14px; }
  .compare-panel select { flex: 1; min-width: 180px; }
  .diff-panel { display: grid; grid-template-columns: repeat(3, minmax(120px, 1fr)); gap: 12px; margin: 0 14px 14px; padding: 0; border: 0; background: transparent; }
  .diff-panel div { background: var(--bg-tertiary); border-radius: var(--border-radius-md); padding: 12px; display: flex; flex-direction: column; gap: 4px; border: 1px solid var(--border-color); }
  .diff-panel strong { font-size: 24px; color: var(--text-primary); }
  .diff-panel span, .muted { color: var(--text-muted); font-size: 12px; }
  .pad { padding: 24px; }
  .inline-diff-shell { display: grid; grid-template-columns: 310px minmax(0, 1fr); gap: 14px; margin: 0 14px 14px; padding: 14px; }
  .diff-files { border-right: 1px solid var(--border-color); padding-right: 14px; }
  .diff-files h3 { color: var(--text-muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 10px; display: flex; align-items: center; gap: 8px; }
  .diff-files button { width: 100%; justify-content: space-between; text-align: left; background: transparent; color: var(--text-secondary); border: 1px solid transparent; padding: 9px 10px; margin-bottom: 5px; transform: none; }
  .diff-files button:hover, .diff-files button.selected { background: var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.28); color: var(--text-primary); }
  .diff-files small { color: var(--text-muted); }
  .added-label { color: var(--accent-primary) !important; }
  .removed-label { color: #fca5a5 !important; }
  .manifest-diff-panel { margin: 0 14px 14px; padding: 14px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .manifest-diff-panel h3 { font-size: 13px; margin: 0 0 10px; color: var(--text-secondary); }
  .manifest-diff-stats { display: grid; gap: 6px; margin-bottom: 12px; }
  .diff-stat { display: flex; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: var(--border-radius-sm); font-size: 12px; background: var(--bg-secondary); border: 1px solid var(--border-color); }
  .diff-stat strong { color: var(--text-primary); }
  .diff-stat span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-stat.changed { border-color: rgba(245,158,11,.30); }
  .diff-stat.added { border-color: rgba(27,217,106,.30); }
  .diff-stat.removed { border-color: rgba(239,68,68,.30); }
  .manifest-diff-text { margin: 0; padding: 12px; border-radius: 10px; background: #0d0d10; color: #a1a1aa; font-family: ui-monospace,monospace; font-size: 11px; line-height: 1.5; max-height: 360px; overflow: auto; white-space: pre-wrap; }
  .inline-diff { min-width: 0; }
  .inline-diff-header { display: flex; justify-content: space-between; gap: 12px; padding: 0 0 10px; color: var(--text-secondary); }
  .inline-diff-header span { color: var(--text-muted); font-size: 12px; }
  pre { overflow: auto; max-height: 420px; background: #0d0d10; border-radius: var(--border-radius-md); padding: 12px; color: var(--text-secondary); font-size: 12px; line-height: 1.5; margin: 0; }
  pre span { display: block; white-space: pre-wrap; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
  pre span.added { color: #86efac; background: rgba(27, 217, 106, 0.08); }
  pre span.removed { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
  pre span.context { color: #a1a1aa; }

  .backup-section { padding: 0 14px 14px; display: grid; gap: 10px; border: 0; background: transparent; }
  .backup-list { display: grid; gap: 6px; }
  .backup-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .backup-info { display: grid; gap: 3px; flex: 1; }
  .backup-info strong { color: var(--text-primary); font-size: 13px; }
  .backup-info span { color: var(--text-muted); font-size: 11px; }
  .rollback { padding: 6px 10px; font-size: 12px; font-weight: 600; }
  .danger { color: #fca5a5; }
  .loading { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) {
    .master-detail, .inline-diff-shell { grid-template-columns: 1fr; }
    .list-pane, .detail-pane { max-height: none; }
    .diff-files { border-right: 0; padding-right: 0; }
  }
</style>
