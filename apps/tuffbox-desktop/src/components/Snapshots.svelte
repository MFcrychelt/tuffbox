<script lang="ts">
  import {
    History, Plus, RefreshCw, RotateCcw, GitCompare, FileText, Archive, Trash2,
    Search, ChevronDown, ChevronRight, ExternalLink, AlertTriangle, Sparkles, FolderOpen,
    ArrowRightLeft, Clock, Zap, Hand, ShieldAlert, Database,
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
    showConfirm(
      "Delete backup",
      "Delete this backup permanently? This cannot be undone.",
      async () => {
        try {
          await api.backups.delete(id, $projectPath!);
          await loadBackups();
        } catch (e) {
          error = String(e);
        }
      },
      true,
    );
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

  function formatRelative(iso: string) {
    try {
      const t = new Date(iso).getTime();
      const diffMs = Date.now() - t;
      const min = 60_000;
      if (diffMs < min) return "just now";
      if (diffMs < 60 * min) return `${Math.floor(diffMs / min)}m ago`;
      if (diffMs < 24 * 60 * min) return `${Math.floor(diffMs / (60 * min))}h ago`;
      if (diffMs < 7 * 24 * 60 * min) return `${Math.floor(diffMs / (24 * 60 * min))}d ago`;
      return new Date(iso).toLocaleDateString();
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

  function kindLabel(kind: string): string {
    if (kind === "crash") return "Crash fix";
    if (kind === "auto") return "Auto";
    return "Manual";
  }

  /** Short prefixed id for tight badges ("bf3a…c21"). */
  function shortId(id: string) {
    if (id.length <= 9) return id;
    return `${id.slice(0, 4)}…${id.slice(-3)}`;
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

  const stats = $derived({
    all: snapshots.length,
    auto: snapshots.filter(isAuto).length,
    manual: snapshots.filter(isManual).length,
    crash: snapshots.filter(isCrash).length,
  });

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

  let snapshotRequestGen = 0;

  async function selectSnapshot(id: string) {
    selectedId = id;
    const dir = await ensureProjectDir();
    if (!dir) return;
    const generation = ++snapshotRequestGen;
    detailLoading = true;
    error = null;
    try {
      const result = await api.snapshots.detail(id, dir);
      // Ignore stale responses if the user clicked another snapshot meanwhile.
      if (generation !== snapshotRequestGen) return;
      detail = result;
    } catch (e) {
      if (generation !== snapshotRequestGen) return;
      error = String(e);
      detail = null;
    } finally {
      if (generation === snapshotRequestGen) detailLoading = false;
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

  function swapCompare() {
    const tmp = fromId;
    fromId = toId;
    toId = tmp;
    void compare();
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

  /** Friendly snapshot label for select dropdowns. */
  function snapshotSelectLabel(s: Snapshot) {
    return `${s.name} · ${formatRelative(s.createdAt)}`;
  }

  function changedCount(s: Snapshot) {
    return s.changedFiles?.length ?? 0;
  }

  const allDiffFiles = $derived(diff
    ? Array.from(new Set([...diff.addedFiles, ...diff.removedFiles, ...diff.modifiedFiles])).sort()
    : []);
  const detailKind = $derived(detail ? kindOf(detail.snapshot) : "manual");
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
    <div class="title-block">
      <div class="title"><History size={18} /> <span>Snapshots</span></div>
      <p class="subtitle">Checkpoints of your pack — roll back to any saved state</p>
    </div>
    <div class="actions">
      <div class="create-group">
        <input
          bind:value={newName}
          placeholder="Snapshot name"
          onkeydown={(e) => e.key === "Enter" && !loading && ($projectPath ? create() : null)}
        />
        <button onclick={create} disabled={!$projectPath || loading} title="Create a safety snapshot of the current state">
          <Plus size={16} />
          Snapshot
        </button>
      </div>
      <button class="ghost icon-btn" onclick={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if loading && snapshots.length === 0}
    <div class="loading">Loading snapshots…</div>
  {:else if !$projectPath}
    <EmptyState icon={History} title="No project selected" description="Open a project to manage snapshots." />
  {:else if snapshots.length === 0}
    <EmptyState icon={History} title="No snapshots yet" description="Create a snapshot to save the current state of your project." />
  {:else}
    <div class="kind-bar" role="group" aria-label="Filter snapshots">
      <button class="kind-btn" class:active={filterKind === "all"} onclick={() => (filterKind = "all")}>
        <Database size={14} />
        <span class="k-count">{stats.all}</span>
        <span class="k-label">All</span>
      </button>
      <button class="kind-btn auto" class:active={filterKind === "auto"} onclick={() => (filterKind = "auto")}>
        <Zap size={14} />
        <span class="k-count">{stats.auto}</span>
        <span class="k-label">Auto</span>
      </button>
      <button class="kind-btn manual" class:active={filterKind === "manual"} onclick={() => (filterKind = "manual")}>
        <Hand size={14} />
        <span class="k-count">{stats.manual}</span>
        <span class="k-label">Manual</span>
      </button>
      <button class="kind-btn crash" class:active={filterKind === "crash"} onclick={() => (filterKind = "crash")}>
        <ShieldAlert size={14} />
        <span class="k-count">{stats.crash}</span>
        <span class="k-label">Crash fix</span>
      </button>
    </div>

    <div class="filters">
      <div class="search">
        <Search size={14} />
        <input bind:value={search} placeholder="Search name, actions, tags…" />
      </div>
      <span class="match-count">{filtered.length} of {snapshots.length}</span>
    </div>

    <div class="master-detail">
      <aside class="list-pane">
        {#each filtered as s (s.id)}
          {@const kind = kindOf(s)}
          <button
            type="button"
            class="row {kind}"
            class:selected={selectedId === s.id}
            onclick={() => selectSnapshot(s.id)}
          >
            <span class="kind-dot {kind}" aria-hidden="true"></span>
            <div class="row-body">
              <div class="row-top">
                <strong class="tb-truncate">{s.name}</strong>
                <span class="op-badge tb-truncate">{operationLabel(s)}</span>
              </div>
              <p class="preview">{previewLine(s)}</p>
              <div class="row-meta">
                <span class="kind-tag {kind}">{kindLabel(kind)}</span>
                <span><Clock size={12} /> {formatDate(s.createdAt)}</span>
                {#if changedCount(s) > 0 || s.tags?.length}
                  <span class="row-files">
                    {#if changedCount(s)}<FileText size={12} /> {changedCount(s)}{/if}
                  </span>
                {/if}
                {#if s.tags?.length}
                  <span class="tags">
                    {#each s.tags as t}
                      <span class="tag" class:crash-fix={t === "crash_fix"}>{t}</span>
                    {/each}
                  </span>
                {/if}
              </div>
            </div>
          </button>
        {:else}
          <div class="muted pad">No snapshots match filters.</div>
        {/each}
      </aside>

      <section class="detail-pane">
        {#if detailLoading}
          <div class="detail-skeleton">
            <span class="skeleton skeleton-round" style="width: 44%; height: 26px"></span>
            <span class="skeleton skeleton-line" style="width: 70%"></span>
            <span class="skeleton skeleton-line" style="width: 90%"></span>
            <span class="skeleton skeleton-block" style="height: 120px"></span>
          </div>
        {:else if detail}
          {@const s = detail.snapshot}
          <div class="detail-header">
            <div class="detail-heading">
              <div class="title-line">
                <h2>{s.name}</h2>
                <span class="kind-pill {detailKind}">{kindLabel(detailKind)}</span>
              </div>
              <div class="detail-sub">
                <span class="badge mono" title={s.id}>{shortId(s.id)}</span>
                <span class="muted"><Clock size={12} /> {formatRelative(s.createdAt)}</span>
                {#if s.actor}<span class="actor-pill">{s.actor}</span>{/if}
                {#if s.planSource}<span class="actor-pill plan">{s.planSource}</span>{/if}
              </div>
            </div>
            <div class="detail-actions">
              <button class="secondary" onclick={() => compareWithPrevious(s.id)} title="Compare with previous">
                <GitCompare size={14} /> Compare prev
              </button>
              <button class="secondary" onclick={() => openInHistory(s.id)} title="Open in History">
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
              <h3><Sparkles size={13} /> Explanation</h3>
              <p class="block-text">{detail.humanExplanation}</p>
            </div>
          {/if}

          <div class="block">
            <h3><Zap size={13} /> Actions ({(detail.actionsSummary ?? []).length})</h3>
            {#if (detail.actionsSummary ?? []).length > 0}
              <ul class="action-list">
                {#each detail.actionsSummary ?? [] as line}
                  <li><span class="bullet" aria-hidden="true"></span>{line}</li>
                {/each}
              </ul>
            {:else}
              <p class="muted">No action details recorded.</p>
            {/if}
          </div>

          <div class="block">
            <h3><FolderOpen size={13} /> Changed files ({(detail.changedFiles ?? []).length})</h3>
            {#if (detail.changedFiles ?? []).length > 0}
              <ul class="file-list">
                {#each detail.changedFiles ?? [] as f}
                  <li>
                    <span class="cat">{f.category}</span>
                    <span class="file-path tb-truncate">{f.path}</span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="muted">No tracked files copied into this snapshot.</p>
            {/if}
          </div>

          {#if detail.relatedEvents.length}
            <div class="block">
              <h3><History size={13} /> Related activity ({detail.relatedEvents.length})</h3>
              <ul class="event-list">
                {#each detail.relatedEvents as ev}
                  <li>
                    <span class="actor-pill">{ev.actor}</span>
                    <span class="event-summary">{ev.summary}</span>
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
        <span class="chev">{#if compareOpen}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}</span>
        <GitCompare size={16} /> Compare snapshots
        <span class="toggle-spacer"></span>
        <span class="n">{diff ? `${allDiffFiles.length} file${allDiffFiles.length === 1 ? "" : "s"}` : ""}</span>
      </button>
      {#if compareOpen}
        <div class="compare-panel">
          <div class="compare-pick">
            <div class="pick-field">
              <label for="snap-from">From</label>
              <select id="snap-from" bind:value={fromId}>
                {#each snapshots as s}<option value={s.id}>{snapshotSelectLabel(s)}</option>{/each}
              </select>
            </div>
            <button class="ghost icon-btn swap" onclick={swapCompare} title="Swap direction" disabled={!fromId || !toId}>
              <ArrowRightLeft size={16} />
            </button>
            <div class="pick-field">
              <label for="snap-to">To</label>
              <select id="snap-to" bind:value={toId}>
                {#each snapshots as s}<option value={s.id}>{snapshotSelectLabel(s)}</option>{/each}
              </select>
            </div>
            <div class="pick-actions">
              <button class="secondary" onclick={compare} disabled={fromId === toId || !fromId || !toId}>
                Diff files
              </button>
              <button class="secondary" onclick={loadManifestDiff} disabled={fromId === toId || !fromId || !toId || manifestDiffLoading}>
                {manifestDiffLoading ? "Loading…" : "Diff manifest"}
              </button>
            </div>
          </div>
          {#if manifestDiff}
            <div class="manifest-diff-panel">
              <div class="section-head"><h3>Manifest changes</h3></div>
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
              <div class="diff-stat-card added"><strong>{diff.addedFiles.length}</strong><span>Added</span></div>
              <div class="diff-stat-card removed"><strong>{diff.removedFiles.length}</strong><span>Removed</span></div>
              <div class="diff-stat-card modified"><strong>{diff.modifiedFiles.length}</strong><span>Modified</span></div>
            </div>
            {#if allDiffFiles.length > 0}
              <div class="inline-diff-shell">
                <aside class="diff-files">
                  <h3><FileText size={14} /> Changed files</h3>
                  {#each allDiffFiles as path}
                    <button class:selected={selectedDiffPath === path} onclick={() => openFileDiff(path)}>
                      <span class="tb-truncate">{path}</span>
                      <span class="labels">
                        {#if diff.addedFiles.includes(path)}<small class="added-label">added</small>{/if}
                        {#if diff.removedFiles.includes(path)}<small class="removed-label">removed</small>{/if}
                        {#if diff.modifiedFiles.includes(path)}<small class="modified-label">modified</small>{/if}
                      </span>
                    </button>
                  {/each}
                </aside>
                <section class="inline-diff">
                  {#if diffLoading}
                    <div class="muted">Loading file diff…</div>
                  {:else if fileDiff}
                    <div class="inline-diff-header">
                      <strong class="tb-truncate">{fileDiff.path}</strong>
                      <span>{fileDiff.fromExists ? "from exists" : "from missing"} → {fileDiff.toExists ? "to exists" : "to missing"}</span>
                    </div>
                    {#if fileDiff.text}
                      <pre class="code-diff">
{#each fileDiff.text.split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                      </pre>
                    {:else}
                      <div class="muted pad">File looks identical — content unchanged.</div>
                    {/if}
                  {:else}
                    <div class="muted">Select a file to view inline diff.</div>
                  {/if}
                </section>
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    </div>

    <div class="collapsible">
      <button type="button" class="collapse-toggle" onclick={() => (backupsOpen = !backupsOpen)}>
        <span class="chev">{#if backupsOpen}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}</span>
        <Archive size={16} /> Project backups
        <span class="toggle-spacer"></span>
        <span class="n">{backups.length} saved</span>
      </button>
      {#if backupsOpen}
        <div class="backup-section">
          <div class="backup-create">
            <input bind:value={backupName} placeholder="Backup name" onkeydown={(e) => e.key === "Enter" && !loading && ($projectPath ? createBackup() : null)} />
            <button class="secondary" onclick={createBackup} disabled={!$projectPath || loading}>
              <Archive size={16} /> Create zip
            </button>
            <button class="ghost icon-btn" onclick={loadBackups} disabled={backupLoading} title="Refresh backups">
              <RefreshCw size={14} class={backupLoading ? "spin" : ""} />
            </button>
            <p class="backup-hint">Full zip of tracked files — restore to bring back an older pack state.</p>
          </div>
          {#if backups.length > 0}
            <div class="backup-list">
              {#each backups.slice(0, 12) as b}
                <div class="backup-row">
                  <div class="backup-icon"><Archive size={15} /></div>
                  <div class="backup-info">
                    <strong class="tb-truncate">{b.name}</strong>
                    <span>{formatDate(b.createdAt)}{#if b.fileCount} · {b.fileCount} files{/if}</span>
                  </div>
                  <span class="backup-size">{formatBytes(b.sizeBytes)}</span>
                  <button class="ghost mini" onclick={() => restoreBackup(b.id)} title="Restore">
                    <RotateCcw size={14} />
                  </button>
                  <button class="ghost mini danger" onclick={() => deleteBackup(b.id)} title="Delete">
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
    padding: 2px;
  }

  /* ── Toolbar ─────────────────────────────────────────────── */
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; flex-shrink: 0; }
  .title-block { display: grid; gap: 3px; }
  .title { display: flex; align-items: center; gap: 10px; color: var(--text-primary); font-weight: 800; font-size: 15px; }
  .title :global(svg) { color: var(--accent-primary); }
  .subtitle { margin: 0; color: var(--text-muted); font-size: 12px; }

  .actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .create-group { display: flex; align-items: center; gap: 8px; }
  .create-group input { min-width: 180px; }
  .icon-btn { width: 38px; height: 38px; padding: 0; flex-shrink: 0; justify-content: center; }

  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); border: 1px solid var(--border-color); display: flex; align-items: flex-start; gap: 8px; font-size: 13px; line-height: 1.4; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent); }
  .notice.warn { color: #fcd34d; background: rgba(245, 158, 11, 0.08); border-color: rgba(245, 158, 11, 0.28); font-size: 13px; }

  /* ── Kind filter bar ─────────────────────────────────────── */
  .kind-bar {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    flex-shrink: 0;
  }
  .kind-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 10px 14px;
    /* Task #67: buttons had no rounding/borders — flat slabs. Ore UI keys,
       theme-aware via tokens so light themes stay readable. */
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-bottom-color: color-mix(in srgb, var(--text-primary) 14%, var(--border-color));
    color: var(--text-muted);
    font-weight: 600;
    font-size: 12px;
    cursor: pointer;
    transition: background var(--motion-fast) var(--motion-ease),
      border-color var(--motion-fast) var(--motion-ease), color var(--motion-fast) var(--motion-ease);
  }
  .kind-btn:hover {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }
  .kind-btn:active {
    background: color-mix(in srgb, var(--text-primary) 6%, var(--bg-tertiary));
  }
  .kind-btn :global(svg) { color: var(--text-muted); flex-shrink: 0; }
  .k-count { font-size: 18px; font-weight: 800; color: var(--text-primary); line-height: 1; }
  .k-label { color: var(--text-muted); }
  .kind-btn.active {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    border-bottom-color: color-mix(in srgb, var(--accent-primary) 70%, #000);
  }
  .kind-btn.active .k-label { color: #ffffff; }
  .kind-btn.active .k-count { color: #ffffff; }
  .kind-btn.active :global(svg) { color: #ffffff; }
  /* Active kind buttons inherit the shared accent styling above; per-kind
     variants only need white text on the filled accent background. */
  .kind-btn.active :global(svg),
  .kind-btn.active .k-count { color: #ffffff; }

  /* ── Filters ─────────────────────────────────────────────── */
  .filters { display: flex; gap: 12px; align-items: center; flex-shrink: 0; }
  .search { flex: 1; min-width: 220px; display: flex; align-items: center; gap: 8px; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 0 10px; color: var(--text-muted); }
  .search input { flex: 1; border: 0; background: transparent; color: var(--text-primary); padding: 10px 0; outline: none; min-width: 0; }
  .match-count { color: var(--text-muted); font-size: 12px; white-space: nowrap; }

  /* ── Master / detail ─────────────────────────────────────── */
  .master-detail {
    display: grid;
    grid-template-columns: minmax(300px, 360px) minmax(0, 1fr);
    gap: 14px;
    flex: 1;
    min-height: 0;
  }
  .list-pane, .detail-pane, .collapsible, .manifest-diff-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }
  .list-pane { overflow: auto; min-height: 0; padding: 8px; display: flex; flex-direction: column; gap: 6px; scrollbar-gutter: stable; }

  .row {
    width: 100%;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--border-radius-md);
    padding: 12px 12px 12px 10px;
    color: var(--text-secondary);
    display: flex;
    gap: 10px;
    transform: none;
    position: relative;
  }
  .row:hover { background: var(--bg-tertiary); color: var(--text-primary); }
  .row.selected {
    background: color-mix(in srgb, var(--accent-primary) 7%, var(--bg-tertiary));
    border-color: color-mix(in srgb, var(--accent-primary) 30%, var(--border-color));
    color: var(--text-primary);
    box-shadow: inset 3px 0 0 var(--accent-primary), var(--shadow-sm);
  }
  .row.selected.crash { background: rgba(245,158,11,.07); border-color: rgba(245,158,11,.32); box-shadow: inset 3px 0 0 #f59e0b, var(--shadow-sm); }
  .row.selected.auto { box-shadow: inset 3px 0 0 #93c5fd, var(--shadow-sm); border-color: rgba(147,197,253,.32); background: rgba(147,197,253,.06); }

  .kind-dot { width: 8px; height: 8px; border-radius: 999px; flex-shrink: 0; align-self: flex-start; margin-top: 5px; background: var(--bg-active); box-shadow: 0 0 0 3px color-mix(in srgb, var(--bg-active) 30%, transparent); }
  .kind-dot.auto { background: #93c5fd; box-shadow: 0 0 0 3px rgba(147,197,253,.16); }
  .kind-dot.manual { background: var(--accent-primary); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-primary) 16%, transparent); }
  .kind-dot.crash { background: #f59e0b; box-shadow: 0 0 0 3px rgba(245,158,11,.16); }

  .row-body { min-width: 0; flex: 1; display: grid; gap: 6px; }
  .row-top { display: flex; justify-content: space-between; gap: 8px; align-items: flex-start; min-width: 0; }
  .row-top strong { font-size: 13px; color: var(--text-primary); max-width: 100%; }
  .op-badge { font-size: 10px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); color: var(--text-muted); font-family: ui-monospace, monospace; max-width: 130px; flex-shrink: 0; margin-top: 1px; }
  .preview { margin: 0; font-size: 12px; color: var(--text-muted); line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .row-meta { display: flex; align-items: center; gap: 8px; font-size: 11px; color: var(--text-muted); flex-wrap: wrap; }
  .row-meta :global(svg) { color: var(--text-muted); }
  .row-files { display: inline-flex; align-items: center; gap: 4px; }
  .tags { display: flex; gap: 4px; flex-wrap: wrap; }

  .kind-tag { font-size: 10px; font-weight: 800; text-transform: uppercase; letter-spacing: .04em; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--border-color); color: var(--text-muted); }
  .kind-tag.auto { color: #93c5fd; border-color: rgba(147,197,253,.4); }
  .kind-tag.manual { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); }
  .kind-tag.crash { color: #fbbf24; border-color: rgba(251,191,36,.4); }

  /* ── Detail pane ─────────────────────────────────────────── */
  .detail-pane { padding: 18px; overflow: auto; min-height: 0; display: flex; flex-direction: column; gap: 14px; scrollbar-gutter: stable; }
  .detail-header { display: flex; justify-content: space-between; gap: 14px; flex-wrap: wrap; align-items: flex-start; }
  .detail-heading { min-width: 0; }
  .title-line { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .detail-header h2 { margin: 0; font-size: 19px; line-height: 1.25; overflow-wrap: anywhere; }
  .detail-sub { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 6px; }
  .detail-sub .muted { display: inline-flex; align-items: center; gap: 5px; }
  .detail-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .detail-actions .secondary, .detail-actions .ghost { padding: 7px 12px; font-size: 12px; font-weight: 700; }

  .kind-pill { font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: .05em; padding: 3px 10px; border-radius: 999px; border: 1px solid var(--border-color); color: var(--text-muted); flex-shrink: 0; }
  .kind-pill.auto { color: #93c5fd; border-color: rgba(147,197,253,.4); background: rgba(147,197,253,.08); }
  .kind-pill.manual { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); background: color-mix(in srgb, var(--accent-primary) 10%, transparent); }
  .kind-pill.crash { color: #fbbf24; border-color: rgba(251,191,36,.4); background: rgba(251,191,36,.08); }

  .badge { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 3px 8px; border-radius: 4px; }
  .badge.mono { font-family: ui-monospace, monospace; }
  .actor-pill { font-size: 10px; font-weight: 800; text-transform: uppercase; letter-spacing: .04em; padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border-color); color: #93c5fd; background: rgba(147,197,253,.07); }
  .actor-pill.plan { color: #c4b5fd; border-color: rgba(196,181,253,.35); background: rgba(196,181,253,.07); }

  .tag-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .tag { font-size: 11px; padding: 2px 8px; border-radius: 999px; background: var(--bg-elevated); color: var(--text-muted); }
  .tag.crash-fix { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 12%, transparent); }
  .tag.mono { font-family: ui-monospace, monospace; max-width: 180px; overflow: hidden; text-overflow: ellipsis; }

  .reason { color: var(--text-secondary); font-size: 13px; margin: 0; line-height: 1.5; }

  .block { display: grid; gap: 10px; }
  .block h3 { display: flex; align-items: center; gap: 7px; margin: 0; font-size: 12px; text-transform: uppercase; letter-spacing: 0.07em; color: var(--text-muted); }
  .block h3 :global(svg) { color: color-mix(in srgb, var(--accent-primary) 70%, var(--text-muted)); }
  .block-text { margin: 0; color: var(--text-secondary); font-size: 13px; line-height: 1.5; }

  .action-list { margin: 0; padding: 0; list-style: none; display: grid; gap: 6px; }
  .action-list li { display: flex; align-items: flex-start; gap: 10px; color: var(--text-secondary); font-size: 13px; line-height: 1.45; padding: 8px 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-sm); }
  .bullet { width: 6px; height: 6px; border-radius: 999px; background: var(--accent-primary); flex-shrink: 0; margin-top: 6px; }

  .file-list { margin: 0; padding: 0; list-style: none; display: grid; gap: 4px; }
  .file-list li { display: flex; align-items: center; gap: 10px; padding: 6px 10px; border-radius: var(--border-radius-sm); background: var(--bg-tertiary); border: 1px solid transparent; min-width: 0; }
  .file-list li:hover { border-color: var(--border-color); }
  .cat { flex-shrink: 0; font-size: 10px; font-weight: 800; text-transform: uppercase; letter-spacing: .04em; padding: 2px 7px; border-radius: 4px; background: var(--bg-elevated); color: var(--text-muted); }
  .file-path { color: var(--text-secondary); font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; flex: 1; min-width: 0; }

  .event-list { margin: 0; padding: 0; list-style: none; display: grid; gap: 6px; }
  .event-list li { display: flex; align-items: center; gap: 10px; padding: 8px 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-sm); min-width: 0; }
  .event-summary { color: var(--text-secondary); font-size: 13px; min-width: 0; overflow-wrap: anywhere; }

  .detail-skeleton { display: flex; flex-direction: column; gap: 12px; }

  /* ── Collapsibles ────────────────────────────────────────── */
  .collapsible { overflow: hidden; display: flex; flex-direction: column; flex-shrink: 0; }
  .collapse-toggle { width: 100%; justify-content: flex-start; align-items: center; gap: 10px; background: transparent; border: 0; color: var(--text-secondary); font-weight: 700; padding: 13px 14px; transform: none; border-radius: 0; font-size: 13px; }
  .collapse-toggle:hover { background: var(--bg-tertiary); color: var(--text-primary); }
  .collapse-toggle :global(svg) { flex-shrink: 0; }
  .chev { display: inline-flex; color: var(--text-muted); }
  .toggle-spacer { flex: 1; }
  .collapse-toggle .n { font-size: 11px; font-weight: 700; color: var(--text-muted); background: var(--bg-elevated); padding: 2px 8px; border-radius: 999px; }

  /* ── Compare ─────────────────────────────────────────────── */
  .compare-panel { display: grid; gap: 14px; padding: 0 14px 14px; }
  .compare-pick { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .pick-field { display: grid; gap: 4px; flex: 1; min-width: 180px; }
  .pick-field label { font-size: 10px; text-transform: uppercase; letter-spacing: .06em; color: var(--text-muted); font-weight: 800; }
  .pick-field select { width: 100%; min-width: 0; }
  .swap { border: 1px solid var(--border-color); background: var(--bg-elevated); color: var(--text-muted); }
  .swap:hover { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); }
  .pick-actions { display: flex; gap: 8px; }

  .diff-panel { display: grid; grid-template-columns: repeat(3, minmax(110px, 1fr)); gap: 10px; }
  .diff-stat-card { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 12px 14px; display: flex; flex-direction: column; gap: 2px; }
  .diff-stat-card strong { font-size: 26px; line-height: 1.1; color: var(--text-primary); }
  .diff-stat-card span { color: var(--text-muted); font-size: 12px; }
  .diff-stat-card.added { border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); background: color-mix(in srgb, var(--accent-primary) 6%, var(--bg-tertiary)); }
  .diff-stat-card.added strong { color: var(--accent-primary); }
  .diff-stat-card.removed { border-color: rgba(239,68,68,.35); background: rgba(239,68,68,.06); }
  .diff-stat-card.removed strong { color: #fca5a5; }
  .diff-stat-card.modified { border-color: rgba(147,197,253,.35); background: rgba(147,197,253,.06); }
  .diff-stat-card.modified strong { color: #93c5fd; }

  .inline-diff-shell { display: grid; grid-template-columns: 310px minmax(0, 1fr); gap: 14px; padding: 14px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .diff-files { min-width: 0; }
  .diff-files h3 { color: var(--text-muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 10px; display: flex; align-items: center; gap: 8px; }
  .diff-files button { width: 100%; justify-content: space-between; text-align: left; background: transparent; color: var(--text-secondary); border: 1px solid transparent; border-radius: var(--border-radius-sm); padding: 9px 10px; margin-bottom: 5px; transform: none; gap: 8px; font-size: 12px; align-items: center; }
  .diff-files button:hover, .diff-files button.selected { background: var(--bg-elevated); border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent); color: var(--text-primary); }
  .diff-files .labels { display: inline-flex; gap: 4px; flex-shrink: 0; }
  .diff-files small { color: var(--text-muted); font-size: 9px; font-weight: 800; text-transform: uppercase; letter-spacing: .03em; padding: 1px 5px; border-radius: 4px; background: var(--bg-secondary); border: 1px solid var(--border-color); }
  .added-label { color: var(--accent-primary) !important; border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent) !important; }
  .removed-label { color: #fca5a5 !important; border-color: rgba(239,68,68,.35) !important; }
  .modified-label { color: #93c5fd !important; border-color: rgba(147,197,253,.35) !important; }

  .inline-diff { min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  .inline-diff-header { display: flex; justify-content: space-between; align-items: center; gap: 12px; color: var(--text-secondary); }
  .inline-diff-header strong { font-size: 13px; }
  .inline-diff-header span { color: var(--text-muted); font-size: 12px; flex-shrink: 0; }

  .manifest-diff-panel { padding: 14px; }
  .section-head h3 { margin: 0 0 10px; font-size: 13px; color: var(--text-secondary); }
  .manifest-diff-stats { display: grid; gap: 6px; margin-bottom: 12px; }
  .diff-stat { display: flex; justify-content: space-between; gap: 10px; padding: 8px 10px; border-radius: var(--border-radius-sm); font-size: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .diff-stat strong { color: var(--text-primary); }
  .diff-stat span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-stat.changed { border-color: rgba(245,158,11,.30); }
  .diff-stat.added { border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent); }
  .diff-stat.removed { border-color: rgba(239,68,68,.30); }
  .manifest-diff-text { margin: 0; padding: 12px; border-radius: var(--border-radius-sm); background: #0d0d10; color: #a1a1aa; font-family: ui-monospace,monospace; font-size: 11px; line-height: 1.5; max-height: 360px; overflow: auto; white-space: pre-wrap; }

  .code-diff { overflow: auto; max-height: 420px; background: #0d0d10; border-radius: var(--border-radius-md); padding: 12px; margin: 0; font-size: 12px; line-height: 1.5; }
  .code-diff span { display: block; white-space: pre-wrap; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
  .code-diff span.added { color: #86efac; background: color-mix(in srgb, var(--accent-primary) 8%, transparent); }
  .code-diff span.removed { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
  .code-diff span.context { color: #a1a1aa; }

  /* ── Backups ─────────────────────────────────────────────── */
  .backup-section { padding: 0 14px 14px; display: grid; gap: 12px; }
  .backup-create { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .backup-create input { flex: 1; min-width: 180px; }
  .backup-hint { margin: 0; width: 100%; color: var(--text-muted); font-size: 11px; }
  .backup-list { display: grid; gap: 6px; }
  .backup-row { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: var(--border-radius-md); background: var(--bg-tertiary); border: 1px solid var(--border-color); min-width: 0; }
  .backup-row:hover { border-color: color-mix(in srgb, var(--accent-primary) 25%, var(--border-color)); }
  .backup-icon { display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 30px; border-radius: var(--border-radius-sm); background: var(--bg-elevated); border: 1px solid var(--border-color); color: var(--text-muted); flex-shrink: 0; }
  .backup-info { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .backup-info strong { color: var(--text-primary); font-size: 13px; }
  .backup-info span { color: var(--text-muted); font-size: 11px; }
  .backup-size { color: var(--text-muted); font-size: 11px; font-family: ui-monospace, monospace; flex-shrink: 0; }

  .rollback { padding: 6px 10px; font-size: 12px; font-weight: 700; }
  .danger { color: #fca5a5; }
  .mini { width: 30px; height: 30px; padding: 0; justify-content: center; flex-shrink: 0; }
  .pad { padding: 24px; }
  .muted { color: var(--text-muted); font-size: 12px; }
  .loading { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) {
    .master-detail, .inline-diff-shell { grid-template-columns: 1fr; }
    .kind-bar { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .pick-field { min-width: 140px; }
  }
</style>