<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    History, RefreshCw, Search, FileText, Maximize2, Save, X, RotateCcw,
    ChevronDown, ChevronRight, ScanSearch, Stethoscope, Sparkles, AlertTriangle,
  } from "@lucide/svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import {
    diagnoseFocusPaths,
    historyFocusEventId,
    historyFocusSnapshotId,
    ideStageRequest,
    projectPath,
  } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";

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
    tags?: string[];
    crashFingerprintKey?: string | null;
    planSource?: string | null;
    actor?: string;
    op?: string;
  };

  let entries = $state<ChangeEntry[]>([]);
  let selectedId = $state("");
  let filter = $state("");
  let categoryFilter = $state("All");
  let actorFilter = $state("All");
  let loading = $state(false);
  let scanning = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let lastLoadedPath = $state<string | null>(null);
  let explainText = $state<string | null>(null);

  let editorOpen = $state(false);
  let editorPath = $state("");
  let editorContent = $state("");
  let editorOriginal = $state("");
  let saving = $state(false);
  let expanded = $state<Record<string, boolean>>({});
  let tracked = $state<Record<string, boolean>>({
    Mods: true,
    Configs: true,
    Shaders: true,
    "Resource Packs": true,
    Resolutions: true,
    "World/Data": false,
    Other: true,
  });
  let focusedScan = $state(false);
  let settingsLoadedPath = $state<string | null>(null);

  const rootsByCategory: Record<string, string[]> = {
    Mods: ["mods"],
    Configs: ["config", "defaultconfigs", "kubejs", "scripts", "overrides", "options.txt", "servers.dat"],
    Shaders: ["shaderpacks", "shaders"],
    "Resource Packs": ["resourcepacks", "texturepacks"],
    Resolutions: [],
    "World/Data": ["datapacks", "saves"],
    Other: [],
  };

  let confirmOpen = $state(false);
  let confirmEntry = $state<ChangeEntry | null>(null);
  let visibleLimit = $state(40);
  const VISIBLE_STEP = 40;
  let prevFilterKey = $state("");

  function actorLabel(actor?: string) {
    switch ((actor ?? "").toLowerCase()) {
      case "scan": return "Disk";
      case "ai": return "AI";
      case "user": return "You";
      case "launcher": return "Launcher";
      default: return actor || "Launcher";
    }
  }

  function actorClass(actor?: string) {
    switch ((actor ?? "").toLowerCase()) {
      case "scan": return "disk";
      case "ai": return "ai";
      case "user": return "you";
      default: return "launcher";
    }
  }

  function dayKey(iso: string) {
    if (!iso) return "Unknown";
    return iso.slice(0, 10);
  }

  function showRollbackConfirm(entry: ChangeEntry) {
    confirmEntry = entry;
    confirmOpen = true;
  }

  async function doRollback() {
    if (!$projectPath || !confirmEntry) return;
    confirmOpen = false;
    loading = true;
    error = null;
    try {
      await invoke("rollback_history_file", {
        path: $projectPath,
        snapshotId: confirmEntry.snapshotId,
        relativePath: confirmEntry.path,
      });
      message = `Rolled back ${confirmEntry.path}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      confirmEntry = null;
    }
  }

  async function loadHistorySettings() {
    if (!$projectPath || settingsLoadedPath === $projectPath) return;
    try {
      const settings: { tracked: Record<string, boolean>; focusedScan?: boolean } =
        await invoke("get_history_settings", { path: $projectPath });
      tracked = { ...tracked, ...(settings.tracked ?? {}) };
      focusedScan = !!settings.focusedScan;
      settingsLoadedPath = $projectPath;
    } catch {
      // Keep defaults
    }
  }

  async function saveHistorySettings() {
    if (!$projectPath) return;
    try {
      await invoke("update_history_settings", {
        path: $projectPath,
        settings: { tracked, focusedScan },
      });
      message = "History settings saved.";
      try {
        window.dispatchEvent(new CustomEvent("tuffbox:history-settings-changed"));
      } catch {
        // ignore
      }
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
    try {
      const data: ChangeEntry[] = await invoke("list_project_change_history", { path: $projectPath });
      entries = data;
      selectedId = entries[0]?.id ?? "";
      lastLoadedPath = $projectPath;
      if ($historyFocusEventId) {
        selectedId = $historyFocusEventId;
        historyFocusEventId.set(null);
        setTimeout(() => {
          document.getElementById("change-" + selectedId)?.scrollIntoView({ behavior: "smooth", block: "center" });
        }, 50);
      } else if ($historyFocusSnapshotId) {
        const snapId = $historyFocusSnapshotId;
        historyFocusSnapshotId.set(null);
        const match = entries.find((e) => e.snapshotId === snapId);
        if (match) {
          selectedId = match.id;
          setTimeout(() => {
            document.getElementById("change-" + selectedId)?.scrollIntoView({ behavior: "smooth", block: "center" });
          }, 50);
        }
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function scanNow(silent = false) {
    if (!$projectPath || scanning) return;
    scanning = true;
    if (!silent) {
      error = null;
      message = null;
    }
    try {
      await saveHistorySettings();
      const res: {
        added: number;
        modified: number;
        removed: number;
        jarDrift: number;
      } = await invoke("scan_project_changes", { path: $projectPath });
      if (!silent) {
        message = `Scan: +${res.added} ~${res.modified} -${res.removed}` +
          (res.jarDrift ? ` · ${res.jarDrift} jar drift` : "");
      }
      await load(true);
    } catch (e) {
      if (!silent) error = String(e);
    } finally {
      scanning = false;
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
    try {
      await invoke("write_config_file", {
        path: $projectPath,
        relativePath: editorPath,
        content: editorContent,
      });
      editorOriginal = editorContent;
      message = `Saved ${editorPath}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function explainEntry(entry: ChangeEntry) {
    if (!$projectPath) return;
    explainText = null;
    try {
      const res: any = await invoke("explain_pack_change", {
        path: $projectPath,
        eventId: entry.id,
      });
      explainText = String(res.explanation ?? "");
      message = explainText;
    } catch (e) {
      // Legacy snapshot-derived entries may not be in events.jsonl — explain heuristically
      explainText = `${actorLabel(entry.actor)} · ${entry.kind}: ${entry.operation} — ${entry.path}`;
      message = explainText;
    }
  }

  function openDiagnose(entry: ChangeEntry) {
    const paths = entry.path && !entry.path.startsWith("crash://") ? [entry.path] : [];
    diagnoseFocusPaths.set(paths.length ? paths : null);
    ideStageRequest.set("diagnose");
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

  function canRollback(entry: ChangeEntry) {
    return entry.kind === "file_changed" && !!entry.snapshotId;
  }

  const categories = $derived(["All", ...Array.from(new Set(entries.map((e) => e.category)))]);
  const actors = $derived(["All", "launcher", "scan", "ai", "user"]);
  const visible = $derived(entries.filter((entry) => {
    const q = filter.toLowerCase();
    const matchesText =
      !q ||
      entry.path.toLowerCase().includes(q) ||
      entry.kind.toLowerCase().includes(q) ||
      entry.preview.toLowerCase().includes(q) ||
      (entry.operation || "").toLowerCase().includes(q);
    const matchesCategory = categoryFilter === "All" || entry.category === categoryFilter;
    const matchesTracked = tracked[entry.category] ?? true;
    const matchesActor = actorFilter === "All" || (entry.actor || "launcher") === actorFilter;
    return matchesText && matchesCategory && matchesTracked && matchesActor;
  }));
  const byDay = $derived(visible.reduce<Record<string, ChangeEntry[]>>((acc, entry) => {
    const key = dayKey(entry.createdAt);
    acc[key] = acc[key] ?? [];
    acc[key].push(entry);
    return acc;
  }, {}));
  const dayKeys = $derived(Object.keys(byDay).sort((a, b) => b.localeCompare(a)));
  const visibleSlice = $derived(visible.slice(0, visibleLimit));
  const filterKey = $derived(`${filter}|${categoryFilter}|${actorFilter}`);
  $effect(() => {
    if (filterKey !== prevFilterKey) {
      prevFilterKey = filterKey;
      visibleLimit = VISIBLE_STEP;
    }
  });
  const hasMoreVisible = $derived(visible.length > visibleLimit);
  const editorDirty = $derived(editorContent !== editorOriginal);
  $effect(() => {
    if ($projectPath && lastLoadedPath !== $projectPath) {
      load(true).then(() => scanNow(true));
    }
  });
</script>

<div class="change-history">
  <div class="toolbar">
    <div class="title"><History size={18} /> History · pack timeline</div>
    <div class="toolbar-actions">
      <div class="search">
        <Search size={15} />
        <input bind:value={filter} placeholder="Search files, mods, configs…" />
      </div>
      <select bind:value={categoryFilter}>
        {#each categories as category}<option value={category}>{category}</option>{/each}
      </select>
      <select bind:value={actorFilter} title="Actor filter">
        {#each actors as a}<option value={a}>{a === "All" ? "All actors" : actorLabel(a)}</option>{/each}
      </select>
      <button class="secondary" onclick={() => scanNow()} disabled={!$projectPath || scanning} title="Delta-scan disk vs baseline">
        <ScanSearch size={16} /> {scanning ? "Scanning…" : "Scan now"}
      </button>
      <button class="secondary" onclick={saveHistorySettings} disabled={!$projectPath || loading}>Save settings</button>
      <button class="ghost" onclick={() => load(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  <div class="tracking-controls">
    {#each Object.keys(tracked) as key}
      <label><input type="checkbox" bind:checked={tracked[key]} /> {key}{#if key === "World/Data"}<small>opt-in</small>{/if}</label>
    {/each}
    <label title="While IDE is open, rescan every 60s">
      <input type="checkbox" bind:checked={focusedScan} onchange={saveHistorySettings} /> Focused scan
    </label>
  </div>

  {#if !$projectPath}
    <EmptyState icon={History} title="No project selected" description="Open a project to view pack activity." />
  {:else if loading && entries.length === 0}
    <div class="empty">Loading history…</div>
  {:else if entries.length === 0}
    <EmptyState icon={History} title="No changes yet" description="Run Scan now to capture external edits, or edit via Tune / Content." />
  {:else}
    <div class="history-layout">
      <aside class="change-tree">
        <div class="timeline-line" />
        {#each dayKeys as day}
          <section>
            <h3>{day}</h3>
            {#each byDay[day] as entry (entry.id)}
              <div class="timeline-item">
                <button
                  class="file-strip {entry.kind}"
                  class:selected={selectedId === entry.id}
                  onclick={() => {
                    selectedId = entry.id;
                    document.getElementById("change-" + entry.id)?.scrollIntoView({ behavior: "smooth", block: "center" });
                  }}
                  title={entry.path || entry.operation}
                >
                  <span class="actor-pill {actorClass(entry.actor)}">{actorLabel(entry.actor)}</span>
                  <span class="file-title">{entry.operation || entry.path}</span>
                  <small>{entry.path || entry.kind}</small>
                </button>
              </div>
            {/each}
          </section>
        {/each}
      </aside>

      <section class="change-preview">
        <div class="all-changes-list">
          {#each visibleSlice as entry (entry.id)}
            <div class="change-card" id="change-{entry.id}" class:jar-drift={entry.kind === "jar_drift" || entry.tags?.includes("jar_drift")}>
              <div class="preview-header">
                <div>
                  <span class="eyebrow">{entry.category} · {entry.kind.replaceAll("_", " ")}</span>
                  <span class="actor-pill {actorClass(entry.actor)}">{actorLabel(entry.actor)}</span>
                  {#if entry.tags?.includes("crash_resolved") || entry.kind === "crash_resolved"}
                    <span class="crash-resolved-badge">resolved{#if entry.planSource} · {entry.planSource}{/if}</span>
                  {:else if entry.tags?.includes("crash_fix")}
                    <span class="crash-fix-badge">crash_fix{#if entry.planSource} · {entry.planSource}{/if}</span>
                  {/if}
                  {#if entry.kind === "jar_drift" || entry.tags?.includes("jar_drift")}
                    <span class="drift-badge"><AlertTriangle size={11} /> jar drift — import to manifest or remove</span>
                  {/if}
                  <h2><FileText size={18} /> {entry.path}</h2>
                  <p>{entry.createdAt} · {entry.reason}</p>
                </div>
                <div class="preview-actions">
                  <button class="secondary" onclick={() => explainEntry(entry)} title="Explain this change">
                    <Sparkles size={16} /> Explain
                  </button>
                  <button class="secondary" onclick={() => openDiagnose(entry)} title="Open Diagnose">
                    <Stethoscope size={16} /> Diagnose
                  </button>
                  <button class="secondary" onclick={() => showRollbackConfirm(entry)} disabled={!canRollback(entry)}>
                    <RotateCcw size={16} /> Rollback
                  </button>
                  <button class="secondary" onclick={() => openFullFile(entry)} disabled={!entry.canOpen}>
                    <Maximize2 size={16} /> Open
                  </button>
                </div>
              </div>

              <div
                class="summary-card"
                role="button"
                tabindex="0"
                onclick={() => toggleExpanded(entry)}
                onkeydown={(e) => (e.key === "Enter" || e.key === " ") && toggleExpanded(entry)}
              >
                <div class="summary-row">
                  <strong>{entry.operation}</strong>
                  <span class="chev">{#if expanded[entry.id]}<ChevronDown size={16} />{:else}<ChevronRight size={16} />{/if}</span>
                </div>
                {#if !expanded[entry.id]}
                  <pre class="mini-preview">{entry.preview || "No preview available."}</pre>
                {/if}
              </div>

              {#if expanded[entry.id]}
                <div class="diff-card">
                  <div class="diff-title">Details</div>
                  <pre>
{#each (entry.diff || entry.preview || "No diff available.").split("\n") as line}
<span class={lineClass(line)}>{line}</span>
{/each}
                  </pre>
                </div>
              {/if}
            </div>
          {/each}
          {#if hasMoreVisible}
            <div class="show-more-row">
              <button class="secondary" onclick={() => (visibleLimit += VISIBLE_STEP)}>
                Show more ({visible.length - visibleLimit} remaining)
              </button>
            </div>
          {/if}
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
          <button onclick={saveEditor} disabled={!editorDirty || saving}>
            <Save size={16} /> {saving ? "Saving…" : "Save"}
          </button>
          <button class="icon-btn" onclick={() => (editorOpen = false)}><X size={18} /></button>
        </div>
      </div>
      <textarea bind:value={editorContent} spellcheck="false" />
    </div>
  </div>
{/if}

{#if confirmOpen}
  <ConfirmDialog title="Rollback file?" message={`Restore ${confirmEntry?.path ?? "file"} from snapshot?`} danger={false}
    confirmLabel="Rollback" onconfirm={doRollback} oncancel={() => (confirmOpen = false, confirmEntry = null)} />
{/if}

<style>
  .change-history { width: 100%; }
  .toolbar, .toolbar-actions, .title, .preview-header, .preview-header h2, .editor-head, .editor-actions, .tracking-controls, .preview-actions, .summary-row { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 14px; flex-wrap: wrap; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 800; }
  .toolbar-actions { gap: 10px; flex-wrap: wrap; }
  .search {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 240px;
    padding: 0 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    color: var(--text-muted);
  }
  .search :global(svg) {
    flex-shrink: 0;
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    width: 100%;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    padding: 10px 0;
    outline: none;
  }
  .notice, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .notice { padding: 12px 14px; margin-bottom: 14px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .tracking-controls { flex-wrap: wrap; gap: 8px; margin-bottom: 14px; padding: 10px; border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); background: rgba(255,255,255,.018); }
  .tracking-controls label { display: flex; align-items: center; gap: 7px; color: var(--text-secondary); font-size: 12px; }
  .tracking-controls input { width: auto; }
  .tracking-controls small { color: var(--text-muted); }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  .history-layout { display: grid; grid-template-columns: 340px minmax(0, 1fr); gap: 16px; min-height: 76vh; }
  .change-tree, .change-preview, .summary-card, .diff-card { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .change-tree { position: relative; padding: 14px 14px 14px 28px; overflow: auto; background: transparent; border-color: transparent; }
  .timeline-line { position: absolute; left: 18px; top: 20px; bottom: 20px; width: 2px; background: linear-gradient(180deg, rgba(27,217,106,.7), rgba(139,92,246,.25)); border-radius: 999px; }
  h3 { margin: 16px 6px 8px; color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  .timeline-item { position: relative; margin-bottom: 10px; }
  .timeline-item::before { content: ""; position: absolute; left: -15px; top: 23px; width: 15px; height: 2px; background: rgba(27,217,106,.5); }
  .timeline-item::after { content: ""; position: absolute; left: -19px; top: 18px; width: 10px; height: 10px; border-radius: 50%; background: var(--bg-secondary); border: 2px solid var(--accent-primary); }
  .file-strip { width: 100%; min-height: 54px; display: flex; flex-direction: column; align-items: flex-start; justify-content: center; gap: 2px; text-align: left; background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid transparent; border-radius: var(--border-radius-md); padding: 10px 12px; transform: none; }
  .file-strip:hover, .file-strip.selected { border-color: rgba(27, 217, 106, 0.34); background: rgba(27, 217, 106, 0.07); }
  .file-title { display: block; width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 800; }
  .file-strip small { display: block; width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: 12px; }
  .actor-pill { display: inline-block; font-size: 10px; font-weight: 800; text-transform: uppercase; letter-spacing: .04em; padding: 1px 6px; border-radius: 999px; border: 1px solid var(--border-color); margin-right: 6px; }
  .actor-pill.launcher { color: #93c5fd; border-color: rgba(147,197,253,.35); }
  .actor-pill.disk { color: #fbbf24; border-color: rgba(251,191,36,.4); }
  .actor-pill.ai { color: #c4b5fd; border-color: rgba(196,181,253,.4); }
  .actor-pill.you { color: var(--accent-primary); border-color: rgba(27,217,106,.4); }
  .change-preview { min-width: 0; padding: 16px; overflow-y: auto; max-height: 80vh; }
  .change-card { margin-bottom: 28px; padding-bottom: 28px; border-bottom: 1px solid var(--border-color); }
  .change-card.jar-drift { border-color: rgba(245,158,11,.35); }
  .change-card:last-child { border-bottom: none; }
  .show-more-row { display: flex; justify-content: center; padding: 8px 0 16px; }
  .mini-preview { margin-top: 10px; max-height: 80px; overflow: hidden; opacity: 0.7; mask-image: linear-gradient(to bottom, black 50%, transparent 100%); }
  .preview-header { justify-content: space-between; gap: 16px; margin-bottom: 14px; flex-wrap: wrap; }
  .preview-actions { gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
  .preview-header h2 { gap: 10px; margin: 4px 0; font-size: 18px; }
  .preview-header p, .eyebrow { color: var(--text-muted); font-size: 12px; }
  .eyebrow { color: var(--accent-primary); text-transform: uppercase; letter-spacing: .1em; font-weight: 900; }
  .crash-fix-badge, .crash-resolved-badge, .drift-badge {
    display: inline-flex; align-items: center; gap: 4px; margin-left: 8px; padding: 2px 8px; border-radius: 999px; font-size: 11px;
  }
  .crash-fix-badge { background: rgba(245, 158, 11, 0.15); color: #fbbf24; border: 1px solid rgba(245, 158, 11, 0.3); }
  .crash-resolved-badge { background: rgba(27, 217, 106, 0.15); color: #1bd96a; border: 1px solid rgba(27, 217, 106, 0.35); }
  .drift-badge { background: rgba(245, 158, 11, 0.12); color: #fcd34d; border: 1px solid rgba(245, 158, 11, 0.35); }
  .summary-card, .diff-card { padding: 14px; margin-bottom: 14px; background: var(--bg-tertiary); cursor: pointer; }
  .summary-row { justify-content: space-between; }
  .summary-card strong, .diff-title { display: block; color: var(--text-secondary); margin-bottom: 10px; font-weight: 800; }
  pre { overflow: auto; white-space: pre-wrap; margin: 0; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; color: #d4d4d8; }
  .diff-card pre { max-height: 58vh; background: #09090b; border-radius: var(--border-radius-md); padding: 12px; }
  pre span { display: block; }
  .added { color: #86efac; background: rgba(27, 217, 106, 0.08); }
  .removed { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
  .context { color: #a1a1aa; }
  .editor-backdrop { position: fixed; inset: 0; z-index: 60; background: rgba(0,0,0,.68); backdrop-filter: blur(12px); display: flex; align-items: center; justify-content: center; padding: 24px; }
  .editor-modal { width: min(1500px, 96vw); height: min(900px, 92vh); background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-xl); overflow: hidden; display: flex; flex-direction: column; box-shadow: 0 30px 110px rgba(0,0,0,.55); }
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
