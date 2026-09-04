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
    type PackSource,
    type PackStateDiff,
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

  // Pack Diff: compare across snapshots / backups / other instances.
  type DiffSourceKind = "snapshot" | "backup" | "manifest";
  let fromKind = $state<DiffSourceKind>("snapshot");
  let toKind = $state<DiffSourceKind>("snapshot");
  let otherManifestPath = $state("");
  let backupFromId = $state("");
  let backupToId = $state("");
  let packDiff = $state<PackStateDiff | null>(null);
  let packDiffLoading = $state(false);

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

  function packSourceFor(kind: DiffSourceKind, side: "from" | "to"): PackSource | null {
    const dir = projectDir ?? "";
    if (side === "from") {
      if (kind === "snapshot")
        return fromId ? { type: "snapshot", projectDir: dir, snapshotId: fromId } : null;
      if (kind === "backup")
        return backupFromId ? { type: "backup", projectDir: dir, backupId: backupFromId } : null;
      return otherManifestPath ? { type: "manifest", path: otherManifestPath } : null;
    }
    if (kind === "snapshot")
      return toId ? { type: "snapshot", projectDir: dir, snapshotId: toId } : null;
    if (kind === "backup")
      return backupToId ? { type: "backup", projectDir: dir, backupId: backupToId } : null;
    return otherManifestPath ? { type: "manifest", path: otherManifestPath } : null;
  }

  async function runPackDiff() {
    const dir = await ensureProjectDir();
    if (!dir) return;
    const a = packSourceFor(fromKind, "from");
    const b = packSourceFor(toKind, "to");
    if (!a || !b) {
      error = "Pick both sources first.";
      return;
    }
    packDiffLoading = true;
    error = null;
    packDiff = null;
    try {
      packDiff = await api.packDiff.compare(a, b);
    } catch (e) {
      error = String(e);
    } finally {
      packDiffLoading = false;
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

<div class="snapshots flex flex-col gap-3.5 h-full min-h-0 w-full max-w-[1440px] mx-auto box-border">
  <!-- ── Toolbar ─────────────────────────────────────────────── -->
  <div class="flex justify-between items-center gap-4 flex-wrap shrink-0">
    <div class="grid gap-1">
      <div class="flex items-center gap-2.5 text-[var(--text-primary)] font-extrabold text-[16px]">
        <History size={19} class="text-[var(--accent-primary)]" />
        <span>Snapshots</span>
      </div>
      <p class="m-0 text-[var(--text-muted)] text-[13px]">Checkpoints of your pack — roll back to any saved state</p>
    </div>
    <div class="flex items-center gap-2.5 flex-wrap">
      <div class="flex items-center gap-2">
        <input
          class="min-w-[200px] max-w-[300px]"
          bind:value={newName}
          placeholder="Snapshot name"
          onkeydown={(e) => e.key === "Enter" && !loading && ($projectPath ? create() : null)}
        />
        <button onclick={create} disabled={!$projectPath || loading} title="Create a safety snapshot of the current state">
          <Plus size={16} />
          Snapshot
        </button>
      </div>
      <button class="ghost w-[38px] h-[38px] p-0 shrink-0 justify-center" onclick={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="px-3.5 py-3 rounded-[var(--border-radius-lg)] border text-[13px] leading-snug text-[#fecaca] bg-[rgba(239,68,68,0.08)] border-[rgba(239,68,68,0.28)]">{error}</div>{/if}
  {#if message}<div class="px-3.5 py-3 rounded-[var(--border-radius-lg)] border text-[13px] leading-snug text-[var(--accent-primary)] bg-[color-mix(in_srgb,var(--accent-primary)_8%,transparent)] border-[color-mix(in_srgb,var(--accent-primary)_25%,transparent)]">{message}</div>{/if}

  {#if loading && snapshots.length === 0}
    <div class="text-[var(--text-muted)] py-20 text-center text-[14px] bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)]">Loading snapshots…</div>
  {:else if !$projectPath}
    <EmptyState icon={History} title="No project selected" description="Open a project to manage snapshots." />
  {:else if snapshots.length === 0}
    <EmptyState icon={History} title="No snapshots yet" description="Create a snapshot to save the current state of your project." />
  {:else}
    <!-- ── Kind filter cards ───────────────────────────────────── -->
    <div class="grid grid-cols-4 max-[900px]:grid-cols-2 gap-2.5 shrink-0" role="group" aria-label="Filter snapshots">
      <button
        type="button"
        class="filter-card {filterKind === "all" ? "active" : ""}"
        onclick={() => (filterKind = "all")}
      >
        <Database size={16} />
        <span class="text-[19px] font-extrabold leading-none">{ stats.all }</span>
        <span>All</span>
      </button>
      <button
        type="button"
        class="filter-card {filterKind === "auto" ? "active" : ""}"
        onclick={() => (filterKind = "auto")}
      >
        <Zap size={16} />
        <span class="text-[19px] font-extrabold leading-none">{ stats.auto }</span>
        <span>Auto</span>
      </button>
      <button
        type="button"
        class="filter-card {filterKind === "manual" ? "active" : ""}"
        onclick={() => (filterKind = "manual")}
      >
        <Hand size={16} />
        <span class="text-[19px] font-extrabold leading-none">{ stats.manual }</span>
        <span>Manual</span>
      </button>
      <button
        type="button"
        class="filter-card {filterKind === "crash" ? "active" : ""}"
        onclick={() => (filterKind = "crash")}
      >
        <ShieldAlert size={16} />
        <span class="text-[19px] font-extrabold leading-none">{ stats.crash }</span>
        <span>Crash fix</span>
      </button>
    </div>

    <!-- ── Search ──────────────────────────────────────────────── -->
    <div class="flex gap-3 items-center shrink-0">
      <div class="flex-1 min-w-[240px] flex items-center gap-2 bg-[var(--bg-elevated)] border border-[var(--border-color)] rounded-[var(--border-radius-md)] px-2.5 text-[var(--text-muted)]">
        <Search size={15} />
        <input class="flex-1 border-0 bg-transparent text-[var(--text-primary)] py-2.5 outline-none min-w-0 text-[13px]" bind:value={search} placeholder="Search name, actions, tags…" />
      </div>
      <span class="text-[var(--text-muted)] text-[13px] whitespace-nowrap">{ filtered.length } of { snapshots.length }</span>
    </div>

    <!-- ── Master / detail ─────────────────────────────────────── -->
    <div class="grid grid-cols-[minmax(300px,380px)_minmax(0,1fr)] max-[900px]:grid-cols-1 gap-3.5 flex-1 min-h-0">
      <aside class="overflow-auto min-h-0 p-2 flex flex-col gap-1.5 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)] [scrollbar-gutter:stable]">
        {#each filtered as s (s.id)}
          {@const kind = kindOf(s)}
          <button
            type="button"
            class="w-full text-left rounded-[var(--border-radius-md)] border px-3 py-3 flex gap-2.5 cursor-pointer transition-colors duration-150 { selectedId === s.id
              ? kind === "crash"
                ? "bg-[rgba(245,158,11,0.08)] border-[rgba(245,158,11,0.35)] text-[var(--text-primary)] border-l-[3px] border-l-[#f59e0b]"
                : kind === "auto"
                  ? "bg-[rgba(147,197,253,0.07)] border-[rgba(147,197,253,0.35)] text-[var(--text-primary)] border-l-[3px] border-l-[#93c5fd]"
                  : "bg-[color-mix(in_srgb,var(--accent-primary)_8%,var(--bg-tertiary))] border-[color-mix(in_srgb,var(--accent-primary)_32%,var(--border-color))] text-[var(--text-primary)] border-l-[3px] border-l-[var(--accent-primary)]"
              : "border-transparent text-[var(--text-secondary)] hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]" }"
            onclick={() => selectSnapshot(s.id)}
          >
            <span class="w-2 h-2 rounded-full shrink-0 mt-[7px] {
              kind === "auto" ? "bg-[#93c5fd] shadow-[0_0_0_3px_rgba(147,197,253,0.16)]"
              : kind === "crash" ? "bg-[#f59e0b] shadow-[0_0_0_3px_rgba(245,158,11,0.16)]"
              : "bg-[var(--accent-primary)] shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent-primary)_16%,transparent)]"
            }" aria-hidden="true"></span>
            <div class="min-w-0 flex-1 grid gap-1.5">
              <div class="flex justify-between gap-2 items-start min-w-0">
                <strong class="text-[13.5px] text-[var(--text-primary)] max-w-full tb-truncate">{ s.name }</strong>
                <span class="text-[10.5px] px-1.5 py-0.5 rounded bg-[var(--bg-elevated)] text-[var(--text-muted)] font-mono max-w-[130px] shrink-0 tb-truncate">{ operationLabel(s) }</span>
              </div>
              <p class="m-0 text-[12.5px] text-[var(--text-muted)] leading-snug line-clamp-2">{ previewLine(s) }</p>
              <div class="flex items-center gap-2 text-[11.5px] text-[var(--text-muted)] flex-wrap">
                <span class="kind-tag { kind }">{ kindLabel(kind) }</span>
                <span class="inline-flex items-center gap-1"><Clock size={12} /> { formatDate(s.createdAt) }</span>
                {#if changedCount(s) > 0}
                  <span class="inline-flex items-center gap-1"><FileText size={12} /> { changedCount(s) }</span>
                {/if}
                {#if s.tags?.length}
                  <span class="flex gap-1 flex-wrap">
                    {#each s.tags as t}
                      <span class="tag" class:crash-fix={ t === "crash_fix" }>{ t }</span>
                    {/each}
                  </span>
                {/if}
              </div>
            </div>
          </button>
        {:else}
          <div class="text-[var(--text-muted)] text-[13px] p-6">No snapshots match filters.</div>
        {/each}
      </aside>

      <section class="p-[18px] overflow-auto min-h-0 flex flex-col gap-3.5 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)] [scrollbar-gutter:stable]">
        {#if detailLoading}
          <div class="flex flex-col gap-3">
            <span class="skeleton" style="width: 44%; height: 26px"></span>
            <span class="skeleton" style="width: 70%"></span>
            <span class="skeleton" style="width: 90%"></span>
            <span class="skeleton" style="height: 120px"></span>
          </div>
        {:else if detail}
          {@const s = detail.snapshot}
          <div class="flex justify-between gap-3.5 flex-wrap items-start">
            <div class="min-w-0">
              <div class="flex items-center gap-2.5 flex-wrap">
                <h2 class="m-0 text-[20px] leading-tight [overflow-wrap:anywhere] text-[var(--text-primary)]">{ s.name }</h2>
                <span class="kind-pill { detailKind }">{ kindLabel(detailKind) }</span>
              </div>
              <div class="flex items-center gap-2 flex-wrap mt-1.5">
                <span class="text-[12px] text-[var(--text-muted)] bg-[var(--bg-elevated)] px-2 py-1 rounded font-mono" title={ s.id }>{ shortId(s.id) }</span>
                <span class="text-[13px] text-[var(--text-secondary)] inline-flex items-center gap-1.5"><Clock size={13} /> { formatRelative(s.createdAt) }</span>
                {#if s.actor}<span class="actor-pill">{ s.actor }</span>{/if}
                {#if s.planSource}<span class="actor-pill plan">{ s.planSource }</span>{/if}
              </div>
            </div>
            <div class="flex items-center gap-2 flex-wrap">
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
            <div class="flex flex-wrap gap-1.5">
              {#each s.tags ?? [] as t}
                <span class="tag" class:crash-fix={ t === "crash_fix" }>{ t }</span>
              {/each}
              {#if s.crashFingerprintKey}
                <span class="tag font-mono max-w-[200px] overflow-hidden text-ellipsis whitespace-nowrap" title={ s.crashFingerprintKey }>{ s.crashFingerprintKey.slice(0, 28) }…</span>
              {/if}
              {#each s.matchedCaseIds ?? [] as cid}
                <span class="tag font-mono">{ cid }</span>
              {/each}
            </div>
          {/if}

          <p class="text-[14px] text-[var(--text-secondary)] m-0 leading-relaxed">{ s.reason }</p>

          {#if detail.manifestOnly}
            <div class="px-3.5 py-3 rounded-[var(--border-radius-lg)] border text-[13px] leading-snug inline-flex items-center gap-2 text-[#fcd34d] bg-[rgba(245,158,11,0.08)] border-[rgba(245,158,11,0.28)]">
              <AlertTriangle size={15} />
              <span><strong>Checkpoint without file copies.</strong> Rollback restores the manifest but not mod jars from this snapshot.</span>
            </div>
          {/if}

          {#if detail.humanExplanation}
            <div class="grid gap-2.5">
              <h3 class="flex items-center gap-1.5 m-0 text-[12.5px] uppercase tracking-wider text-[var(--text-muted)] font-bold"><Sparkles size={14} class="text-[var(--accent-primary)]" /> Explanation</h3>
              <p class="m-0 text-[14px] text-[var(--text-secondary)] leading-relaxed">{ detail.humanExplanation }</p>
            </div>
          {/if}

          <div class="grid gap-2.5">
            <h3 class="flex items-center gap-1.5 m-0 text-[12.5px] uppercase tracking-wider text-[var(--text-muted)] font-bold"><Zap size={14} class="text-[var(--accent-primary)]" /> Actions ({ (detail.actionsSummary ?? []).length })</h3>
            {#if (detail.actionsSummary ?? []).length > 0}
              <ul class="m-0 p-0 list-none grid gap-1.5">
                {#each detail.actionsSummary ?? [] as line}
                  <li class="flex items-start gap-2.5 text-[13.5px] leading-snug text-[var(--text-secondary)] px-2.5 py-2 bg-[var(--bg-tertiary)] border border-[var(--border-color)] rounded-[var(--border-radius-sm)]">
                    <span class="w-1.5 h-1.5 rounded-full bg-[var(--accent-primary)] shrink-0 mt-[7px]" aria-hidden="true"></span>{ line }
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="text-[13px] text-[var(--text-muted)] m-0">No action details recorded.</p>
            {/if}
          </div>

          <div class="grid gap-2">
            <h3 class="flex items-center gap-1.5 m-0 text-[12.5px] uppercase tracking-wider text-[var(--text-muted)] font-bold"><FolderOpen size={14} class="text-[var(--accent-primary)]" /> Changed files ({ (detail.changedFiles ?? []).length })</h3>
            {#if (detail.changedFiles ?? []).length > 0}
              <ul class="m-0 p-0 list-none grid gap-1">
                {#each detail.changedFiles ?? [] as f}
                  <li class="flex items-center gap-2.5 px-2.5 py-1.5 rounded-[var(--border-radius-sm)] bg-[var(--bg-tertiary)] border border-transparent hover:border-[var(--border-color)] min-w-0">
                    <span class="shrink-0 text-[10.5px] font-extrabold uppercase tracking-wide px-1.5 py-0.5 rounded bg-[var(--bg-elevated)] text-[var(--text-muted)]">{ f.category }</span>
                    <span class="text-[var(--text-secondary)] font-mono text-[12.5px] flex-1 min-w-0 tb-truncate">{ f.path }</span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="text-[13px] text-[var(--text-muted)] m-0">No tracked files copied into this snapshot.</p>
            {/if}
          </div>

          {#if detail.relatedEvents.length}
            <div class="grid gap-2.5">
              <h3 class="flex items-center gap-1.5 m-0 text-[12.5px] uppercase tracking-wider text-[var(--text-muted)] font-bold"><History size={14} class="text-[var(--accent-primary)]" /> Related activity ({ detail.relatedEvents.length })</h3>
              <ul class="m-0 p-0 list-none grid gap-1.5">
                {#each detail.relatedEvents as ev}
                  <li class="flex items-center gap-2.5 px-2.5 py-2 bg-[var(--bg-tertiary)] border border-[var(--border-color)] rounded-[var(--border-radius-sm)] min-w-0">
                    <span class="actor-pill">{ ev.actor }</span>
                    <span class="text-[13.5px] text-[var(--text-secondary)] min-w-0 [overflow-wrap:anywhere]">{ ev.summary }</span>
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

    <!-- ── Compare snapshots ───────────────────────────────────── -->
    <div class="overflow-hidden flex flex-col shrink-0 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)]">
      <button type="button" class="w-full flex items-center gap-2.5 bg-transparent border-0 text-[var(--text-secondary)] font-bold px-3.5 py-3 text-[13.5px] cursor-pointer rounded-none transition-colors duration-150 hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]" onclick={() => (compareOpen = !compareOpen)}>
        <span class="inline-flex text-[var(--text-muted)]">{#if compareOpen}<ChevronDown size={17} />{:else}<ChevronRight size={17} />{/if}</span>
        <GitCompare size={17} /> Compare snapshots
        <span class="flex-1"></span>
        {#if diff}
          <span class="text-[12px] font-bold text-[var(--text-muted)] bg-[var(--bg-elevated)] px-2 py-0.5 rounded-full">{ allDiffFiles.length } file{ allDiffFiles.length === 1 ? "" : "s" }</span>
        {/if}
      </button>
      {#if compareOpen}
        <div class="grid gap-3.5 px-3.5 pb-3.5">
          <div class="flex items-center gap-2.5 flex-wrap">
            <div class="grid gap-1 flex-1 min-w-[200px]">
              <label for="snap-from" class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">From</label>
              <select id="snap-from" class="w-full min-w-0" bind:value={fromId}>
                {#each snapshots as s}<option value={s.id}>{ snapshotSelectLabel(s) }</option>{/each}
              </select>
            </div>
            <button class="ghost w-[38px] h-[38px] p-0 justify-center shrink-0 border border-[var(--border-color)] bg-[var(--bg-elevated)] text-[var(--text-muted)] hover:text-[var(--accent-primary)] hover:border-[color-mix(in_srgb,var(--accent-primary)_40%,transparent)]" onclick={swapCompare} title="Swap direction" disabled={!fromId || !toId}>
              <ArrowRightLeft size={16} />
            </button>
            <div class="grid gap-1 flex-1 min-w-[200px]">
              <label for="snap-to" class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">To</label>
              <select id="snap-to" class="w-full min-w-0" bind:value={toId}>
                {#each snapshots as s}<option value={s.id}>{ snapshotSelectLabel(s) }</option>{/each}
              </select>
            </div>
            <div class="flex gap-2">
              <button class="secondary" onclick={compare} disabled={fromId === toId || !fromId || !toId}>
                Diff files
              </button>
              <button class="secondary" onclick={loadManifestDiff} disabled={fromId === toId || !fromId || !toId || manifestDiffLoading}>
                { manifestDiffLoading ? "Loading…" : "Diff manifest" }
              </button>
            </div>
          </div>

          <!-- ── Pack Diff: cross-source compare ─────────────────── -->
          <div class="grid gap-2.5 p-3.5 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)]">
            <h3 class="m-0 text-[14px] text-[var(--text-secondary)] font-bold">Compare packs</h3>
            <div class="grid gap-2" style="grid-template-columns: repeat(2, minmax(180px, 1fr));">
              <div class="grid gap-1">
                <label for="pd-from-kind" class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">From source</label>
                <select id="pd-from-kind" bind:value={fromKind}>
                  <option value="snapshot">Snapshot</option>
                  <option value="backup">Zip backup</option>
                  <option value="manifest">Other instance manifest</option>
                </select>
              </div>
              <div class="grid gap-1">
                <label for="pd-to-kind" class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">To source</label>
                <select id="pd-to-kind" bind:value={toKind}>
                  <option value="snapshot">Snapshot</option>
                  <option value="backup">Zip backup</option>
                  <option value="manifest">Other instance</option>
                </select>
              </div>
              {#if fromKind === "snapshot" || toKind === "snapshot"}
                <p class="m-0 col-span-2 text-[12px] text-[var(--text-muted)]">Snapshot side uses the From/To snapshot selects above.</p>
              {/if}
              {#if fromKind === "backup" || toKind === "backup"}
                <label class="grid gap-1">
                  <span class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">Backup ids</span>
                  <span class="flex gap-2">
                    <input class="flex-1 min-w-0" bind:value={backupFromId} placeholder="From backup id" />
                    <input class="flex-1 min-w-0" bind:value={backupToId} placeholder="To backup id" />
                  </span>
                </label>
              {/if}
              {#if fromKind === "manifest" || toKind === "manifest"}
                <label class="grid gap-1 col-span-2">
                  <span class="text-[11px] uppercase tracking-wider text-[var(--text-muted)] font-extrabold">Other instance manifest path</span>
                  <input bind:value={otherManifestPath} placeholder="U:/…/project.tuffbox.json" />
                </label>
              {/if}
            </div>
            <button class="secondary" onclick={runPackDiff} disabled={packDiffLoading}>
              { packDiffLoading ? "Comparing…" : "Compare packs" }
            </button>

            {#if packDiff}
              {@const r = packDiff.report}
              <div class="grid gap-1.5 mb-1.5">
                {#if r.mcA !== r.mcB}
                  <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(245,158,11,0.3)]">
                    <strong class="text-[var(--text-primary)]">MC version</strong><span class="text-[var(--text-muted)] tb-truncate">{ r.mcA || "—" } → { r.mcB || "—" }</span>
                  </div>
                {/if}
                <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[color-mix(in_srgb,var(--accent-primary)_30%,transparent)]">
                  <strong class="text-[var(--accent-primary)]">+{ r.addedMods.length } mods</strong><span class="text-[var(--text-muted)] tb-truncate">{ r.addedMods.map((m) => m.id).join(", ") }</span>
                </div>
                <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(239,68,68,0.3)]">
                  <strong class="text-[#fca5a5]">-{ r.removedMods.length } mods</strong><span class="text-[var(--text-muted)] tb-truncate">{ r.removedMods.map((m) => m.id).join(", ") }</span>
                </div>
                <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(147,197,253,0.35)]">
                  <strong class="text-[#93c5fd]">~{ r.updatedMods.length } updated</strong><span class="text-[var(--text-muted)] tb-truncate">{ r.updatedMods.map((u) => `${u.id} ${u.from.version}→${u.to.version}`).join(", ") }</span>
                </div>
              </div>
              {#if packDiff.configDiffs.length}
                <div class="grid gap-2">
                  {#each packDiff.configDiffs as cd (cd.path)}
                    <details>
                      <summary class="cursor-pointer text-[12.5px] text-[var(--text-secondary)] font-mono">{ cd.path }</summary>
                      <pre class="m-1.5 p-3 rounded-[var(--border-radius-sm)] bg-[#0d0d10] text-[#b4b4bc] font-mono text-[12.5px] leading-relaxed max-h-[360px] overflow-auto whitespace-pre-wrap">{ cd.diffText }</pre>
                    </details>
                  {/each}
                </div>
              {:else}
                <p class="m-0 text-[12.5px] text-[var(--text-muted)]">No config file changes.</p>
              {/if}
            {/if}
          </div>
          {#if manifestDiff}
            <div class="p-3.5 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)] grid gap-2.5">
              <h3 class="m-0 text-[14px] text-[var(--text-secondary)] font-bold">Manifest changes</h3>
              <div class="grid gap-1.5 mb-1.5">
                {#if manifestDiff.mcVersionChanged}
                  <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(245,158,11,0.3)]">
                    <strong class="text-[var(--text-primary)]">MC version</strong><span class="text-[var(--text-muted)] tb-truncate">{ manifestDiff.fromMcVersion } → { manifestDiff.toMcVersion }</span>
                  </div>
                {/if}
                {#if manifestDiff.loaderVersionChanged}
                  <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(245,158,11,0.3)]">
                    <strong class="text-[var(--text-primary)]">Loader</strong><span class="text-[var(--text-muted)] tb-truncate">{ manifestDiff.fromLoaderVersion } → { manifestDiff.toLoaderVersion }</span>
                  </div>
                {/if}
                {#if manifestDiff.addedMods?.length}
                  <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[color-mix(in_srgb,var(--accent-primary)_30%,transparent)]">
                    <strong class="text-[var(--accent-primary)]">+{ manifestDiff.addedMods.length } mods</strong><span class="text-[var(--text-muted)] tb-truncate">{ manifestDiff.addedMods.join(", ") }</span>
                  </div>
                {/if}
                {#if manifestDiff.removedMods?.length}
                  <div class="flex justify-between gap-2.5 px-2.5 py-2 rounded-[var(--border-radius-sm)] text-[13px] bg-[var(--bg-tertiary)] border border-[rgba(239,68,68,0.3)]">
                    <strong class="text-[#fca5a5]">-{ manifestDiff.removedMods.length } mods</strong><span class="text-[var(--text-muted)] tb-truncate">{ manifestDiff.removedMods.join(", ") }</span>
                  </div>
                {/if}
              </div>
              <pre class="m-0 p-3 rounded-[var(--border-radius-sm)] bg-[#0d0d10] text-[#b4b4bc] font-mono text-[12.5px] leading-relaxed max-h-[360px] overflow-auto whitespace-pre-wrap">{ manifestDiff.diffText || "No differences." }</pre>
            </div>
          {/if}
          {#if diff}
            <div class="grid grid-cols-3 gap-2.5">
              <div class="bg-[var(--bg-tertiary)] border border-[color-mix(in_srgb,var(--accent-primary)_35%,transparent)] rounded-[var(--border-radius-md)] px-3.5 py-3 flex flex-col gap-0.5">
                <strong class="text-[26px] leading-tight text-[var(--accent-primary)]">{ diff.addedFiles.length }</strong>
                <span class="text-[var(--text-muted)] text-[13px]">Added</span>
              </div>
              <div class="bg-[var(--bg-tertiary)] border border-[rgba(239,68,68,0.35)] rounded-[var(--border-radius-md)] px-3.5 py-3 flex flex-col gap-0.5">
                <strong class="text-[26px] leading-tight text-[#fca5a5]">{ diff.removedFiles.length }</strong>
                <span class="text-[var(--text-muted)] text-[13px]">Removed</span>
              </div>
              <div class="bg-[var(--bg-tertiary)] border border-[rgba(147,197,253,0.35)] rounded-[var(--border-radius-md)] px-3.5 py-3 flex flex-col gap-0.5">
                <strong class="text-[26px] leading-tight text-[#93c5fd]">{ diff.modifiedFiles.length }</strong>
                <span class="text-[var(--text-muted)] text-[13px]">Modified</span>
              </div>
            </div>
            {#if allDiffFiles.length > 0}
              <div class="grid grid-cols-[320px_minmax(0,1fr)] max-[900px]:grid-cols-1 gap-3.5 p-3.5 bg-[var(--bg-tertiary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)]">
                <aside class="min-w-0">
                  <h3 class="text-[var(--text-muted)] text-[12.5px] uppercase tracking-wider mb-2.5 flex items-center gap-2 font-bold"><FileText size={14} /> Changed files</h3>
                  {#each allDiffFiles as path}
                    <button class="w-full flex items-center justify-between gap-2 text-left bg-transparent text-[var(--text-secondary)] border rounded-[var(--border-radius-sm)] px-2.5 py-2 mb-1.5 text-[12.5px] cursor-pointer transition-colors duration-150 {
                      selectedDiffPath === path
                        ? "bg-[var(--bg-elevated)] border-[color-mix(in_srgb,var(--accent-primary)_30%,transparent)] text-[var(--text-primary)]"
                        : "border-transparent hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
                    }" onclick={() => openFileDiff(path)}>
                      <span class="tb-truncate">{ path }</span>
                      <span class="inline-flex gap-1 shrink-0">
                        {#if diff.addedFiles.includes(path)}<small class="diff-label added">added</small>{/if}
                        {#if diff.removedFiles.includes(path)}<small class="diff-label removed">removed</small>{/if}
                        {#if diff.modifiedFiles.includes(path)}<small class="diff-label modified">modified</small>{/if}
                      </span>
                    </button>
                  {/each}
                </aside>
                <section class="min-w-0 flex flex-col gap-2">
                  {#if diffLoading}
                    <div class="text-[var(--text-muted)] text-[13px]">Loading file diff…</div>
                  {:else if fileDiff}
                    <div class="flex justify-between items-center gap-3 text-[var(--text-secondary)]">
                      <strong class="text-[14px] tb-truncate">{ fileDiff.path }</strong>
                      <span class="text-[var(--text-muted)] text-[12.5px] shrink-0">{ fileDiff.fromExists ? "from exists" : "from missing" } → { fileDiff.toExists ? "to exists" : "to missing" }</span>
                    </div>
                    {#if fileDiff.text}
                      <pre class="overflow-auto max-h-[420px] bg-[#0d0d10] rounded-[var(--border-radius-md)] p-3 m-0 text-[12.5px] leading-relaxed">
{#each fileDiff.text.split("\n") as line}
<span class="block whitespace-pre-wrap font-mono { lineClass(line) }">{ line }</span>
{/each}
                      </pre>
                    {:else}
                      <div class="text-[var(--text-muted)] text-[13px] p-6">File looks identical — content unchanged.</div>
                    {/if}
                  {:else}
                    <div class="text-[var(--text-muted)] text-[13px]">Select a file to view inline diff.</div>
                  {/if}
                </section>
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    </div>

    <!-- ── Project backups ─────────────────────────────────────── -->
    <div class="overflow-hidden flex flex-col shrink-0 bg-[var(--bg-secondary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)]">
      <button type="button" class="w-full flex items-center gap-2.5 bg-transparent border-0 text-[var(--text-secondary)] font-bold px-3.5 py-3 text-[13.5px] cursor-pointer transition-colors duration-150 hover:bg-[var(--bg-tertiary)] hover:text-[var(--text-primary)]" onclick={() => (backupsOpen = !backupsOpen)}>
        <span class="inline-flex text-[var(--text-muted)]">{#if backupsOpen}<ChevronDown size={17} />{:else}<ChevronRight size={17} />{/if}</span>
        <Archive size={17} /> Project backups
        <span class="flex-1"></span>
        <span class="text-[12px] font-bold text-[var(--text-muted)] bg-[var(--bg-elevated)] px-2 py-0.5 rounded-full">{ backups.length } saved</span>
      </button>
      {#if backupsOpen}
        <div class="px-3.5 pb-3.5 grid gap-3">
          <div class="flex items-center gap-2 flex-wrap">
            <input class="flex-1 min-w-[200px]" bind:value={backupName} placeholder="Backup name" onkeydown={(e) => e.key === "Enter" && !loading && ($projectPath ? createBackup() : null)} />
            <button class="secondary" onclick={createBackup} disabled={!$projectPath || loading}>
              <Archive size={16} /> Create zip
            </button>
            <button class="ghost w-[38px] h-[38px] p-0 justify-center shrink-0" onclick={loadBackups} disabled={backupLoading} title="Refresh backups">
              <RefreshCw size={15} class={backupLoading ? "spin" : ""} />
            </button>
            <p class="m-0 w-full text-[13px] text-[var(--text-muted)]">Full zip of tracked files — restore to bring back an older pack state.</p>
          </div>
          {#if backups.length > 0}
            <div class="grid gap-1.5">
              {#each backups.slice(0, 12) as b}
                <div class="flex items-center gap-3 px-3 py-2.5 rounded-[var(--border-radius-md)] bg-[var(--bg-tertiary)] border border-[var(--border-color)] min-w-0 hover:border-[color-mix(in_srgb,var(--accent-primary)_25%,var(--border-color))]">
                  <div class="inline-flex items-center justify-center w-8 h-8 rounded-[var(--border-radius-sm)] bg-[var(--bg-elevated)] border border-[var(--border-color)] text-[var(--text-muted)] shrink-0"><Archive size={16} /></div>
                  <div class="grid gap-0.5 flex-1 min-w-0">
                    <strong class="text-[var(--text-primary)] text-[13.5px] tb-truncate">{ b.name }</strong>
                    <span class="text-[var(--text-muted)] text-[12.5px]">{ formatDate(b.createdAt) }{#if b.fileCount} · { b.fileCount } files{/if}</span>
                  </div>
                  <span class="text-[var(--text-muted)] text-[12.5px] font-mono shrink-0">{ formatBytes(b.sizeBytes) }</span>
                  <button class="ghost mini" onclick={() => restoreBackup(b.id)} title="Restore">
                    <RotateCcw size={15} />
                  </button>
                  <button class="ghost mini danger" onclick={() => deleteBackup(b.id)} title="Delete">
                    <Trash2 size={15} />
                  </button>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-[13px] text-[var(--text-muted)] m-0">No zip backups yet.</p>
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
  /* Layout comes from Tailwind utilities. Scoped styles only for
     pieces Tailwind can't express: shared app button skins used here
     (ghost/secondary/danger/mini are defined app-wide), the Ore-style
     kind tags, and skeletons. */

  /* Snapshots uses theme-token button skins, not the global Ore gray skin:
     secondary = tinted theme surface, ghost = quiet bordered pill. All
     colors come from CSS vars so every theme (light/sharp/minimal) reads
     correctly. Scoped to the component root class (not `section`) — the
     toolbar, Compare block and backups block are <div>s, and those buttons
     were falling back to the gray global skin. */
  :global(.snapshots) button.secondary {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-bottom-width: 2px;
    border-bottom-color: color-mix(in srgb, var(--border-color) 60%, transparent);
  }
  :global(.snapshots) button.secondary:hover:not(:disabled) {
    background: var(--bg-hover);
    border-color: color-mix(in srgb, var(--accent-primary) 30%, var(--border-color));
  }
  :global(.snapshots) button.secondary:active:not(:disabled) {
    filter: none;
    background: var(--bg-active);
  }
  :global(.snapshots) button.ghost:not(.mini) {
    background: var(--bg-secondary);
    color: var(--text-secondary);
    border: 1px solid var(--border-color);
    border-bottom-width: 2px;
    border-bottom-color: color-mix(in srgb, var(--border-color) 55%, transparent);
  }
  :global(.snapshots) button.ghost:not(.mini):hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--accent-primary) 30%, var(--border-color));
  }
  :global(.snapshots) button.ghost:not(.mini):active:not(:disabled) {
    filter: none;
    background: var(--bg-active);
  }
  :global(.snapshots) button.ghost:disabled {
    opacity: 0.55;
  }

  /* Kind filter cards: flat quiet tiles, accent fill when active — the bare
     button here otherwise inherits the gray Ore skin (3px bottom edge). */
  .filter-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--motion-fast) ease, border-color var(--motion-fast) ease, color var(--motion-fast) ease;
  }
  .filter-card:hover:not(.active) {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--accent-primary) 25%, var(--border-color));
  }
  .filter-card.active {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: var(--on-accent, #fff);
  }

  /* Rollback / Delete detail actions: soft tinted pills, not gray blocks. */
  .rollback {
    color: var(--accent-primary);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 35%, transparent);
  }
  .rollback:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
  }
  .ghost.danger {
    color: #fca5a5;
    border: 1px solid rgba(239, 68, 68, 0.35);
  }
  .ghost.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: #fecaca;
  }

  .kind-tag {
    font-size: 10.5px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 8px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    color: var(--text-muted);
  }
  .kind-tag.auto { color: #93c5fd; border-color: rgba(147, 197, 253, 0.4); }
  .kind-tag.manual { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); }
  .kind-tag.crash { color: #fbbf24; border-color: rgba(251, 191, 36, 0.4); }

  .actor-pill { font-size: 11px; font-weight: 800; text-transform: uppercase; letter-spacing: 0.04em; padding: 2px 9px; border-radius: 999px; border: 1px solid rgba(147, 197, 253, 0.4); color: #93c5fd; background: rgba(147, 197, 253, 0.07); }
  .actor-pill.plan { color: #c4b5fd; border-color: rgba(196, 181, 253, 0.35); background: rgba(196, 181, 253, 0.07); }

  .tag { font-size: 11.5px; padding: 2px 9px; border-radius: 999px; background: var(--bg-elevated); color: var(--text-secondary); }
  .tag.crash-fix { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 12%, transparent); }

  .diff-label { color: var(--text-muted); font-size: 9.5px; font-weight: 800; text-transform: uppercase; letter-spacing: 0.03em; padding: 1px 6px; border-radius: 4px; background: var(--bg-secondary); border: 1px solid var(--border-color); }
  .diff-label.added { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); }
  .diff-label.removed { color: #fca5a5; border-color: rgba(239, 68, 68, 0.35); }
  .diff-label.modified { color: #93c5fd; border-color: rgba(147, 197, 253, 0.35); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
