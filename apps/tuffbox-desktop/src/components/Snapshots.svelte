<script lang="ts">
  import {
    History, Plus, RefreshCw, RotateCcw, Calendar, GitCompare, FileText, Archive, Trash2,
    Search, ChevronDown, ChevronRight, ExternalLink, AlertTriangle, HardDrive, ShieldCheck,
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
    type SnapshotVsCurrent,
    type PruneResult,
  } from "../lib/api";
  import { historyFocusSnapshotId, ideStageRequest, projectPath } from "../lib/store";

  let snapshots = $state<Snapshot[]>([]);
  let loading = $state(false);
  let newName = $state("");
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let projectDir = $state<string | null>(null);
  let lastLoadedPath = $state<string | null>(null);
  let fromId = $state("");
  let toId = $state("");
  let diff = $state<SnapshotDiff | null>(null);
  let selectedDiffPath = $state("");
  let fileDiff = $state<SnapshotFileDiff | null>(null);
  let diffLoading = $state(false);

  let selectedId = $state("");
  let detail = $state<SnapshotDetail | null>(null);
  let detailLoading = $state(false);
  let search = $state("");
  let filterKind = $state<"all" | "auto" | "manual" | "crash">("all");
  let backupsOpen = $state(false);
  let compareOpen = $state(false);

  // Diff vs current (project now) panel for the selected snapshot.
  let vsDiff = $state<SnapshotVsCurrent | null>(null);
  let vsLoading = $state(false);
  let vsSelectedPath = $state("");
  let vsFileDiff = $state<SnapshotFileDiff | null>(null);
  let vsFileLoading = $state(false);

  // Disk cleanup / prune.
  let cleanupOpen = $state(false);
  let pruneDays = $state<number>(30);
  let pruneLoading = $state(false);
  let pruneResult = $state<PruneResult | null>(null);

  let confirmOpen = $state(false);
  let confirmTitle = $state("");
  let confirmMessage = $state("");
  let confirmDanger = $state(false);
  let confirmAction = $state<(() => void) | null>(null);

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

  let manifestDiff = $state<ManifestSnapshotDiff | null>(null);
  let manifestDiffLoading = $state(false);

  let backups = $state<BackupEntry[]>([]);
  let backupLoading = $state(false);
  let backupName = $state("");

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

  function kindOf(s: Snapshot): "auto" | "manual" | "crash" {
    if (isCrash(s)) return "crash";
    if (isAuto(s)) return "auto";
    return "manual";
  }

  const totalBytes = $derived(snapshots.reduce((acc, s) => acc + (s.sizeBytes || 0), 0));
  const autoCount = $derived(snapshots.filter(isAuto).length);
  const manualCount = $derived(snapshots.filter(isManual).length);
  const crashCount = $derived(snapshots.filter(isCrash).length);

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

  function dayKey(iso: string): string {
    const d = new Date(iso);
    return isNaN(d.getTime()) ? "Other" : `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
  }
  function dayLabel(iso: string): string {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return "Other";
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const start = new Date(d);
    start.setHours(0, 0, 0, 0);
    const diffDays = Math.round((today.getTime() - start.getTime()) / 86_400_000);
    if (diffDays === 0) return "Today";
    if (diffDays === 1) return "Yesterday";
    return d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" });
  }
  // Group filtered snapshots by local calendar day, newest day first.
  const timeline = $derived((() => {
    const groups = new Map<string, Snapshot[]>();
    for (const s of filtered) {
      const key = dayKey(s.createdAt);
      const arr = groups.get(key) ?? [];
      arr.push(s);
      groups.set(key, arr);
    }
    return Array.from(groups.entries())
      .sort((a, b) => (a[0] < b[0] ? 1 : -1))
      .map(([key, items]) => ({ key, label: dayLabel(items[0]?.createdAt ?? ""), items }));
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
      // Re-validate compare targets: a snapshot may have been deleted since
      // the last load, which would leave the compare panel pointing at a
      // non-existent id.
      const ids = new Set(snapshots.map((s) => s.id));
      if (fromId && !ids.has(fromId)) fromId = "";
      if (toId && !ids.has(toId)) toId = "";
      if (!fromId && !toId && snapshots.length >= 2) {
        fromId = snapshots[snapshots.length - 2].id;
        toId = snapshots[snapshots.length - 1].id;
      }
      if (selectedId && !snapshots.some((s) => s.id === selectedId)) {
        selectedId = "";
        detail = null;
      }
      const focusSnap = $historyFocusSnapshotId;
      if (focusSnap && snapshots.some((s) => s.id === focusSnap)) {
        historyFocusSnapshotId.set(null);
        await selectSnapshot(focusSnap);
      } else if (!selectedId && snapshots.length) {
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
    if (vsDiff) {
      vsDiff = null;
      vsFileDiff = null;
      vsSelectedPath = "";
    }
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

  async function loadVsCurrent(id: string) {
    const dir = await ensureProjectDir();
    if (!dir || !id) return;
    vsDiff = null;
    vsFileDiff = null;
    vsSelectedPath = "";
    vsLoading = true;
    error = null;
    try {
      vsDiff = await api.snapshots.diffVsCurrent(id, dir);
    } catch (e) {
      error = String(e);
      vsDiff = null;
    } finally {
      vsLoading = false;
    }
  }

  async function openVsFile(path: string) {
    const dir = await ensureProjectDir();
    if (!dir || !selectedId) return;
    vsSelectedPath = path;
    vsFileLoading = true;
    error = null;
    try {
      vsFileDiff = await api.snapshots.fileDiffVsCurrent(selectedId, path, dir);
    } catch (e) {
      error = String(e);
      vsFileDiff = null;
    } finally {
      vsFileLoading = false;
    }
  }

  const vsChangedFiles = $derived(vsDiff
    ? Array.from(new Set([...vsDiff.snapshotChangedFiles, ...vsDiff.snapshotGoneFiles, ...vsDiff.currentAddedFiles])).sort()
    : []);

  async function runPrune() {
    const dir = await ensureProjectDir();
    if (!dir || pruneDays < 1) return;
    const target = Math.max(1, Math.floor(pruneDays));
    const autoCountBefore = autoCount;
    showConfirm(
      "Clean up old auto-snapshots",
      `Delete automatic snapshots that are ${target} days old or older? Manual and crash-fix snapshots are kept. Up to ${autoCountBefore} auto snapshot(s) may qualify.`,
      async () => {
        pruneLoading = true;
        error = null;
        message = null;
        try {
          const res = await api.snapshots.pruneAuto(target, dir!);
          pruneResult = res;
          message = res.removedIds.length
            ? `Removed ${res.removedIds.length} old snapshot(s), freed ${formatBytes(res.totalBytes)}.`
            : "No automatic snapshots were old enough to remove.";
          await load(true);
          if (fromId && toId) {
            // Re-validate compare targets after deletion.
            const ids = new Set(snapshots.map((s) => s.id));
            if (!ids.has(fromId)) fromId = "";
            if (!ids.has(toId)) toId = "";
          }
        } catch (e) {
          error = String(e);
        } finally {
          pruneLoading = false;
        }
      },
      true,
    );
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
  $effect(() => {
    const focusSnap = $historyFocusSnapshotId;
    if (!focusSnap || !snapshots.length) return;
    if (!snapshots.some((s) => s.id === focusSnap)) return;
    historyFocusSnapshotId.set(null);
    void selectSnapshot(focusSnap);
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

    <div class="summary">
      <div class="summary-stat"><strong>{snapshots.length}</strong><span>Checkpoints</span></div>
      <div class="summary-stat auto"><strong>{autoCount}</strong><span>Auto</span></div>
      <div class="summary-stat manual"><strong>{manualCount}</strong><span>Manual</span></div>
      <div class="summary-stat crash"><strong>{crashCount}</strong><span>Crash fixes</span></div>
      <div class="summary-stat size" title={formatBytes(totalBytes)}>
        <HardDrive size={14} />
        <strong>{formatBytes(totalBytes)}</strong>
        <span>on disk</span>
      </div>
    </div>

    <div class="collapsible">
      <button type="button" class="collapse-toggle" onclick={() => (cleanupOpen = !cleanupOpen)}>
        {#if cleanupOpen}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}
        <ShieldCheck size={16} /> Disk cleanup
      </button>
      {#if cleanupOpen}
        <div class="cleanup-panel">
          <p class="muted">
            Delete automatic (launcher / AI / scan) snapshots that are older than the selected age.
            Manual and crash-fix snapshots are always kept.
          </p>
          <div class="cleanup-controls">
            <label>
              Older than
              <select value={pruneDays} onchange={(e) => { pruneDays = parseInt((e.currentTarget as HTMLSelectElement).value, 10) || 30; }}>
                <option value={14}>14 days</option>
                <option value={30}>30 days</option>
                <option value={60}>60 days</option>
                <option value={90}>90 days</option>
              </select>
            </label>
            <button class="ghost" onclick={runPrune} disabled={pruneLoading || autoCount === 0}>
              {pruneLoading ? "Cleaning..." : "Clean up old auto-snapshots"}
            </button>
            <span class="muted">({autoCount} auto snapshot(s))</span>
          </div>
          {#if pruneResult && pruneResult.removedIds.length > 0}
            <div class="notice success">
              Removed {pruneResult.removedIds.length} old snapshot(s), freed {formatBytes(pruneResult.totalBytes)}.
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="master-detail">
      <aside class="list-pane">
        {#each timeline as group}
          <div class="timeline-group">
            <div class="timeline-header">
              <span class="timeline-dot"></span>
              <span class="timeline-label">{group.label}</span>
              <span class="timeline-count">{group.items.length}</span>
            </div>
            {#each group.items as s (s.id)}
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
                  <span class="kind-badge" class:auto={kindOf(s) === "auto"} class:manual={kindOf(s) === "manual"} class:crash={kindOf(s) === "crash"}>{kindOf(s)}</span>
                  {#if s.sizeBytes}
                    <span class="size-badge"><HardDrive size={11} /> {formatBytes(s.sizeBytes)}</span>
                  {/if}
                  {#if s.tags?.length}
                    <span class="tags">
                      {#each s.tags as t}<span class="tag" class:crash-fix={t === "crash_fix"}>{t}</span>{/each}
                    </span>
                  {/if}
                </div>
              </button>
            {/each}
          </div>
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
              <button class="secondary" onclick={() => loadVsCurrent(s.id)} title="What changed in this project since this checkpoint?">
                <GitCompare size={14} /> Diff vs current
              </button>
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

          {#if vsDiff}
            <div class="block vs-panel">
              <h3>What changed since this checkpoint?</h3>
              {#if vsDiff.manifestCompared && vsDiff.manifestDiff}
                <h4>Manifest</h4>
                <pre class="manifest-diff-text">
{#each vsDiff.manifestDiff.split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                </pre>
              {:else if vsDiff.manifestCompared}
                <p class="muted">Manifest on disk matches this snapshot.</p>
              {/if}
              {#if vsChangedFiles.length > 0}
                <h4>Tracked files</h4>
                <div class="inline-diff-shell">
                  <aside class="diff-files">
                    {#each vsChangedFiles as path}
                      <button class:selected={vsSelectedPath === path} onclick={() => openVsFile(path)}>
                        <span>{path}</span>
                        {#if vsDiff.snapshotGoneFiles.includes(path)}<small class="removed-label">gone</small>{/if}
                        {#if vsDiff.currentAddedFiles.includes(path)}<small class="added-label">new</small>{/if}
                        {#if vsDiff.snapshotChangedFiles.includes(path)}<small>changed</small>{/if}
                      </button>
                    {/each}
                  </aside>
                  <section class="inline-diff">
                    {#if vsFileLoading}
                      <div class="muted">Loading file diff...</div>
                    {:else if vsFileDiff}
                      <div class="inline-diff-header">
                        <strong>{vsFileDiff.path}</strong>
                        <span>{vsFileDiff.fromExists ? "snapshot" : "snapshot missing"} → {vsFileDiff.toExists ? "on disk" : "missing on disk"}</span>
                      </div>
                      <pre>
{#each vsFileDiff.text.split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                      </pre>
                    {:else}
                      <div class="muted">Select a file above to view the inline diff against the current state.</div>
                    {/if}
                  </section>
                </div>
              {:else}
                <p class="muted">No tracked files recorded for this snapshot.</p>
              {/if}
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
  .snapshots {
    max-width: none;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    height: 100%;
    box-sizing: border-box;
  }
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; flex-shrink: 0; }
  .title, .actions, .row-meta, .detail-sub, .detail-actions, .backup-create, .search, .collapse-toggle {
    display: flex; align-items: center; gap: 10px;
  }
  .title { color: var(--text-secondary); font-weight: 600; }
  .actions input, .backup-create input, .search input { min-width: 180px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); border: 1px solid var(--border-color); display: flex; align-items: flex-start; gap: 8px; }
  .notice.error { color: var(--accent-danger); background: color-mix(in srgb, var(--accent-danger) 8%, transparent); border-color: color-mix(in srgb, var(--accent-danger) 28%, transparent); }
  .notice.success { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent); }
  .notice.warn { color: var(--accent-warning); background: color-mix(in srgb, var(--accent-warning) 8%, transparent); border-color: color-mix(in srgb, var(--accent-warning) 28%, transparent); font-size: 13px; }

  .filters { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; justify-content: space-between; }
  .search { flex: 1; min-width: 220px; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 0 10px; color: var(--text-muted); }
  .search input { flex: 1; border: 0; background: transparent; color: var(--text-primary); padding: 10px 0; outline: none; min-width: 0; }
  .chips { display: flex; gap: 6px; flex-wrap: wrap; }
  .chips button { background: var(--bg-secondary); border: 1px solid var(--border-color); color: var(--text-muted); padding: 6px 12px; font-size: 12px; transform: none; }
  .chips button.active { color: var(--text-primary); border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); }

  .master-detail {
    display: grid;
    grid-template-columns: minmax(280px, 360px) minmax(0, 1fr);
    gap: 14px;
    flex: 1;
    min-height: 0;
  }
  .list-pane, .detail-pane, .compare-panel, .diff-panel, .inline-diff-shell, .backup-section, .collapsible {
    background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg);
  }
  .list-pane { overflow: auto; min-height: 0; max-height: none; padding: 10px; display: flex; flex-direction: column; gap: 16px; scrollbar-gutter: stable; }
  .row { width: 100%; text-align: left; background: transparent; border: 1px solid transparent; border-radius: var(--border-radius-md); padding: 12px; color: var(--text-secondary); display: grid; gap: 6px; transform: none; }
  .row:hover, .row.selected { background: var(--bg-tertiary); border-color: color-mix(in srgb, var(--accent-primary) 28%, transparent); color: var(--text-primary); }
  .timeline-group { display: grid; gap: 6px; }
  .timeline-header { display: flex; align-items: center; gap: 8px; padding: 6px 6px 2px; position: sticky; top: 0; z-index: 2; background: var(--bg-secondary); }
  .timeline-dot { width: 8px; height: 8px; border-radius: 50%; background: color-mix(in srgb, var(--accent-primary) 55%, transparent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-primary) 14%, transparent); flex-shrink: 0; }
  .timeline-label { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-secondary); }
  .timeline-count { font-size: 10px; font-weight: 700; color: var(--text-muted); background: var(--bg-elevated); border-radius: 999px; padding: 1px 7px; }
  .row-top strong { font-size: 13px; color: var(--text-primary); }
  .op-badge { font-size: 10px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); color: var(--text-muted); font-family: ui-monospace, monospace; max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .preview { margin: 0; font-size: 12px; color: var(--text-muted); line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .row-meta { font-size: 11px; color: var(--text-muted); flex-wrap: wrap; }
  .tags { display: flex; gap: 4px; flex-wrap: wrap; }
  .kind-badge { font-size: 10px; font-weight: 700; text-transform: uppercase; padding: 1px 7px; border-radius: 999px; letter-spacing: 0.04em; }
  .kind-badge.auto { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 14%, transparent); }
  .kind-badge.manual { color: var(--text-secondary); background: var(--bg-elevated); }
  .kind-badge.crash { color: var(--accent-danger); background: color-mix(in srgb, var(--accent-danger) 14%, transparent); }
  .size-badge { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); }

  .summary { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 10px; flex-shrink: 0; }
  .summary-stat { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .summary-stat strong { font-size: 20px; color: var(--text-primary); line-height: 1; }
  .summary-stat span { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .summary-stat.size { flex-direction: row; align-items: center; gap: 8px; }
  .summary-stat.size strong { font-size: 15px; }
  @media (max-width: 760px) { .summary { grid-template-columns: repeat(3, 1fr); } }

  .cleanup-panel { padding: 0 14px 14px; display: grid; gap: 10px; border: 0; background: transparent; }
  .cleanup-controls { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; }
  .cleanup-controls label { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-secondary); }
  .cleanup-controls select { min-width: 120px; }
  .vs-panel h4 { margin: 12px 0 8px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-muted); }

  .detail-pane { padding: 18px; overflow: auto; min-height: 0; max-height: none; display: flex; flex-direction: column; gap: 14px; scrollbar-gutter: stable; }
  .detail-header { display: flex; justify-content: space-between; gap: 12px; flex-wrap: wrap; align-items: flex-start; }
  .detail-header h2 { margin: 0 0 6px; font-size: 18px; }
  .detail-actions { flex-wrap: wrap; }
  .badge { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 3px 8px; border-radius: 4px; font-family: ui-monospace, monospace; max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .tag { font-size: 11px; padding: 2px 8px; border-radius: 999px; background: var(--bg-elevated); color: var(--text-muted); }
  .tag.crash-fix { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 12%, transparent); }
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
  .diff-files button:hover, .diff-files button.selected { background: var(--bg-tertiary); border-color: color-mix(in srgb, var(--accent-primary) 28%, transparent); color: var(--text-primary); }
  .diff-files small { color: var(--text-muted); }
  .added-label { color: var(--accent-primary) !important; }
  .removed-label { color: var(--accent-danger) !important; }
  .manifest-diff-panel { margin: 0 14px 14px; padding: 14px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .manifest-diff-panel h3 { font-size: 13px; margin: 0 0 10px; color: var(--text-secondary); }
  .manifest-diff-stats { display: grid; gap: 6px; margin-bottom: 12px; }
  .diff-stat { display: flex; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: var(--border-radius-sm); font-size: 12px; background: var(--bg-secondary); border: 1px solid var(--border-color); }
  .diff-stat strong { color: var(--text-primary); }
  .diff-stat span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-stat.changed { border-color: rgba(245,158,11,.30); }
  .diff-stat.added { border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent); }
  .diff-stat.removed { border-color: rgba(239,68,68,.30); }
  .manifest-diff-text { margin: 0; padding: 12px; border-radius: 10px; background: var(--bg-elevated); color: var(--text-secondary); font-family: ui-monospace,monospace; font-size: 11px; line-height: 1.5; max-height: 360px; overflow: auto; white-space: pre-wrap; }
  .inline-diff { min-width: 0; }
  .inline-diff-header { display: flex; justify-content: space-between; gap: 12px; padding: 0 0 10px; color: var(--text-secondary); }
  .inline-diff-header span { color: var(--text-muted); font-size: 12px; }
  pre { overflow: auto; max-height: 420px; background: var(--bg-elevated); border-radius: var(--border-radius-md); padding: 12px; color: var(--text-secondary); font-size: 12px; line-height: 1.5; margin: 0; }
  pre span { display: block; white-space: pre-wrap; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
  pre span.added { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); }
  pre span.removed { color: var(--accent-danger); background: color-mix(in srgb, var(--accent-danger) 8%, transparent); }
  pre span.context { color: var(--text-muted); }

  .backup-section { padding: 0 14px 14px; display: grid; gap: 10px; border: 0; background: transparent; }
  .backup-list { display: grid; gap: 6px; }
  .backup-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .backup-info { display: grid; gap: 3px; flex: 1; }
  .backup-info strong { color: var(--text-primary); font-size: 13px; }
  .backup-info span { color: var(--text-muted); font-size: 11px; }
  .rollback { padding: 6px 10px; font-size: 12px; font-weight: 600; }
  .danger { color: var(--accent-danger); }
  .loading { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) {
    .master-detail, .inline-diff-shell { grid-template-columns: 1fr; }
    .list-pane, .detail-pane { max-height: none; }
    .diff-files { border-right: 0; padding-right: 0; }
  }
</style>
