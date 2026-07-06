<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { PlayCircle, RefreshCw, Terminal, TimerReset, CheckCircle2, AlertTriangle, XCircle, Shield, Server, FileText } from "lucide-svelte";
  import { onDestroy } from "svelte";
  import { projectPath } from "../lib/store";

  type Profile = {
    id: string;
    name: string;
    side: string;
    memoryMb?: number | null;
    jvmArgs: string[];
  };

  let profiles: Profile[] = [];
  let selectedProfile = "client";
  let log = "";
  let running = false;
  let loading = false;
  let error: string | null = null;
  let message: string | null = null;
  let startedAt: number | null = null;
  let lastLoadedPath: string | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let validationReport: any = null;
  let validationLoading = false;
  let validationError: string | null = null;

  // Launch stats
  let launchStats: any = null;
  let statsLoading = false;

  async function loadStats() {
    if (!$projectPath) return;
    statsLoading = true;
    try {
      launchStats = await invoke("get_launch_stats", { path: $projectPath });
    } catch { launchStats = null; }
    finally { statsLoading = false; }
  }

  async function runValidation() {
    if (!$projectPath) return;
    validationLoading = true;
    validationError = null;
    try {
      validationReport = await invoke("run_project_validation", { path: $projectPath });
    } catch (e) {
      validationError = String(e);
    } finally {
      validationLoading = false;
    }
  }
  let runs: { id: string; profile: string; startedAt: string; status: string; logPath: string; durationSeconds?: number | null }[] = [];
  let capturedRunIds: Record<string, boolean> = {};

  async function loadProfiles(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && profiles.length > 0) return;
    loading = true;
    error = null;
    try {
      profiles = await invoke("list_profiles", { path: $projectPath });
      selectedProfile = profiles.find((p) => p.id === selectedProfile)?.id ?? profiles[0]?.id ?? "client";
      lastLoadedPath = $projectPath;
      await refreshLog();
      await loadRuns();
      await loadStats();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function launch() {
    if (!$projectPath || !selectedProfile) return;
    running = true;
    startedAt = Date.now();
    error = null;
    message = null;
    log = "";
    try {
      await invoke("record_launch", { path: $projectPath });
      await invoke("launch_profile", { path: $projectPath, profile: selectedProfile });
      await loadStats();
      message = `Started profile ${selectedProfile}. Watch latest.log below.`;
      await loadRuns();
      startPolling();
    } catch (e) {
      error = String(e);
      running = false;
    }
  }

  async function refreshLog() {
    if (!$projectPath) return;
    try {
      log = await invoke("get_launch_log", { path: $projectPath });
      if (log.includes("# Launch error:") || log.includes("Process exited") || log.includes("Stopping!")) {
        const wasRunning = running;
        running = false;
        await loadRuns();
        if (wasRunning && runs[0] && !capturedRunIds[runs[0].id]) {
          await captureRunLogs(runs[0], true);
        }
      }
    } catch {
      // latest.log may not exist before first run.
    }
  }

  async function loadRuns() {
    if (!$projectPath) return;
    try {
      runs = await invoke("list_test_runs", { path: $projectPath });
    } catch {
      runs = [];
    }
  }

  async function captureRunLogs(run: { id: string }, silent = false) {
    if (!$projectPath) return;
    try {
      const dir: string = await invoke("capture_test_run_logs", { path: $projectPath, runId: run.id });
      capturedRunIds = { ...capturedRunIds, [run.id]: true };
      if (!silent) message = `Captured logs to ${dir}`;
    } catch (e) {
      if (!silent) error = String(e);
    }
  }

  function formatRunTime(value: string) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds)) return value;
    return new Date(seconds * 1000).toLocaleString();
  }

  function startPolling() {
    if (timer) clearInterval(timer);
    timer = setInterval(refreshLog, 1000);
  }

  function stopPolling() {
    if (timer) clearInterval(timer);
    timer = null;
    running = false;
  }

  $: selected = profiles.find((p) => p.id === selectedProfile);
  $: elapsed = startedAt ? Math.floor((Date.now() - startedAt) / 1000) : 0;
  $: if ($projectPath && lastLoadedPath !== $projectPath) loadProfiles(true);

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });
</script>

<div class="test-runs">
  <div class="toolbar">
    <div class="title"><PlayCircle size={18} /> Test runs</div>
    <div class="actions">
      <button class="ghost" on:click={() => loadProfiles(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        Refresh
      </button>
      <button class="secondary" on:click={refreshLog} disabled={!$projectPath}>
        <Terminal size={16} /> Tail log
      </button>
      <button class="secondary" on:click={runValidation} disabled={!$projectPath || validationLoading}>
        <Shield size={16} />
        {validationLoading ? "Checking..." : "Validate project"}
      </button>
      <button class="ghost" on:click={loadStats} disabled={!$projectPath} title="Refresh stats">
        <RefreshCw size={16} />
      </button>
      <button on:click={launch} disabled={!$projectPath || running || !selectedProfile}>
        <PlayCircle size={16} />
        {running ? "Running..." : "Run profile"}
      </button>
      <button class="secondary" on:click={async () => {
        if (!$projectPath) return;
        try {
          const props = await invoke("generate_server_properties", { path: $projectPath });
          message = "server.properties generated.";
        } catch(e) { error = String(e); }
      }} disabled={!$projectPath} title="Generate server.properties">
        <FileText size={16} />
      </button>
      <button class="secondary" on:click={async () => {
        if (!$projectPath) return;
        running = true; startedAt = Date.now(); error = null; message = null; log = "";
        try {
          await invoke("record_launch", { path: $projectPath });
          await invoke("launch_server", { path: $projectPath });
          message = "Server started. Watch log below.";
          await loadStats();
          startPolling();
        } catch(e) { error = String(e); running = false; }
      }} disabled={!$projectPath || running} title="Launch server">
        <Server size={16} /> Server
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if !$projectPath}
    <div class="empty">Open a project to run test profiles.</div>
  {:else if validationReport}
    <div class="validation-report">
      <div class="val-header">
        <h3><Shield size={18} /> Project health check</h3>
        {#if validationReport.passed}
          <span class="val-passed"><CheckCircle2 size={16} /> All checks passed</span>
        {:else}
          <span class="val-failed"><XCircle size={16} /> Issues found</span>
        {/if}
      </div>
      <div class="val-stats">
        <div class="val-stat"><strong>{validationReport.totalMods}</strong><span>mods</span></div>
        <div class="val-stat" class:danger={validationReport.graphErrors > 0}>
          <strong>{validationReport.graphErrors}</strong><span>graph errors</span>
        </div>
        <div class="val-stat" class:warning={validationReport.graphWarnings > 0}>
          <strong>{validationReport.graphWarnings}</strong><span>graph warnings</span>
        </div>
        <div class="val-stat" class:danger={validationReport.jsonErrors?.length > 0}>
          <strong>{validationReport.jsonErrors?.length ?? 0}</strong><span>JSON errors</span>
        </div>
        <div class="val-stat" class:danger={validationReport.circularDeps?.length > 0}>
          <strong>{validationReport.circularDeps?.length ?? 0}</strong><span>circular deps</span>
        </div>
      </div>
      {#if validationReport.graphErrorList?.length > 0}
        <div class="val-section">
          <h4><XCircle size={14} /> Graph errors</h4>
          {#each validationReport.graphErrorList as err}
            <div class="val-item error"><code>{err.code}</code> {err.message}</div>
          {/each}
        </div>
      {/if}
      {#if validationReport.jsonErrors?.length > 0}
        <div class="val-section">
          <h4><AlertTriangle size={14} /> Broken JSON configs</h4>
          {#each validationReport.jsonErrors as je}
            <div class="val-item warning"><code>{je.path}</code> {je.error}</div>
          {/each}
        </div>
      {/if}
      {#if validationReport.circularDeps?.length > 0}
        <div class="val-section">
          <h4><AlertTriangle size={14} /> Circular dependencies</h4>
          {#each validationReport.circularDeps as pair}
            <div class="val-item warning">{pair[0]} ⇄ {pair[1]}</div>
          {/each}
        </div>
      {/if}
      <button class="ghost" on:click={() => (validationReport = null)}>Close report</button>
    </div>
  {:else}
    <div class="layout">
      <aside class="profiles">
        <h2>Profiles</h2>
        {#if profiles.length === 0}
          <div class="muted">No profiles found.</div>
        {:else}
          {#each profiles as profile}
            <button
              class="profile-card"
              class:selected={selectedProfile === profile.id}
              on:click={() => (selectedProfile = profile.id)}
            >
              <strong>{profile.name}</strong>
              <span>{profile.id} · {profile.side}</span>
              <small>{profile.memoryMb ?? 4096} MB · {profile.jvmArgs.length} JVM args</small>
            </button>
          {/each}
        {/if}

        {#if launchStats}
          <div class="launch-stats-card">
            <h3>Launch stats</h3>
            <div class="ls-row"><span>Total launches</span><strong>{launchStats.totalLaunches}</strong></div>
            <div class="ls-row"><span>Total crashes</span><strong class:danger={launchStats.totalCrashes > 0}>{launchStats.totalCrashes}</strong></div>
            {#if launchStats.lastLaunch}<div class="ls-row"><span>Last launch</span><span>{launchStats.lastLaunch}</span></div>{/if}
          </div>
        {/if}

        <h2 class="history-title">Run history</h2>
        {#if runs.length === 0}
          <div class="muted">No test runs recorded yet.</div>
        {:else}
          <div class="run-history">
            {#each runs.slice(0, 10) as run}
              <div class="run-row {run.status}">
                <strong>{run.profile}</strong>
                <span>{formatRunTime(run.startedAt)}</span>
                <small>{run.status}{run.durationSeconds ? ` · ${run.durationSeconds}s` : ""}</small>
                <button class="ghost mini" on:click={() => captureRunLogs(run)}>{capturedRunIds[run.id] ? "Logs captured" : "Capture logs"}</button>
              </div>
            {/each}
          </div>
        {/if}
      </aside>

      <section class="run-panel">
        <div class="run-header">
          <div>
            <h2>{selected?.name ?? "Select profile"}</h2>
            <p>{selected ? `${selected.side} profile · ${selected.memoryMb ?? 4096} MB` : "Choose a profile to run"}</p>
          </div>
          <div class="status" class:running>
            <TimerReset size={16} />
            {running ? `${elapsed}s` : "idle"}
          </div>
        </div>

        <pre class="log">{log || "latest.log will appear here after the first run."}</pre>

        {#if running}
          <button class="secondary stop" on:click={stopPolling}>Stop watching log</button>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .test-runs { max-width: none; width: 100%; }
  .toolbar, .actions, .title, .run-header, .status { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 10px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .layout { display: grid; grid-template-columns: 280px minmax(0, 1fr); gap: 16px; }
  .profiles, .run-panel, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .profiles { padding: 16px; }
  .profiles h2 { font-size: 15px; margin-bottom: 12px; }
  .profile-card { width: 100%; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; background: var(--bg-tertiary); color: var(--text-secondary); border: 1px solid var(--border-color); padding: 12px; margin-bottom: 8px; text-align: left; }
  .profile-card:hover, .profile-card.selected { transform: none; border-color: rgba(27, 217, 106, 0.4); background: rgba(27, 217, 106, 0.08); }
  .profile-card strong { color: var(--text-primary); }
  .profile-card span, .profile-card small, .muted, .run-header p { color: var(--text-muted); }
  .history-title { margin-top: 22px; }
  .run-history { display: grid; gap: 8px; }
  .run-row { display: grid; gap: 3px; padding: 10px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .run-row strong { color: var(--text-primary); }
  .run-row span, .run-row small { color: var(--text-muted); font-size: 12px; }
  .run-row.failed { border-color: rgba(239, 68, 68, .35); }
  .run-row.finished { border-color: rgba(27, 217, 106, .28); }
  .run-row.started { border-color: rgba(245, 158, 11, .28); }
  .mini { padding: 5px 8px; font-size: 11px; justify-self: start; }
  .run-panel { overflow: hidden; }
  .run-header { justify-content: space-between; gap: 12px; padding: 16px 18px; border-bottom: 1px solid var(--border-color); }
  .run-header h2 { margin: 0 0 4px; }
  .status { gap: 8px; color: var(--text-muted); background: var(--bg-tertiary); border-radius: 999px; padding: 8px 12px; }
  .status.running { color: var(--accent-primary); }
  .log { min-height: 560px; max-height: 680px; overflow: auto; margin: 0; padding: 18px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; }
  .stop { margin: 12px; }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  .launch-stats-card { padding: 12px; border: 1px solid var(--border-color); border-radius: 12px; background: var(--bg-tertiary); margin-bottom: 14px; display: grid; gap: 6px; }
  .launch-stats-card h3 { color: var(--text-secondary); font-size: 12px; margin: 0 0 4px; text-transform: uppercase; letter-spacing: .04em; }
  .ls-row { display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .ls-row span { color: var(--text-muted); }
  .ls-row strong { color: var(--text-primary); font-size: 16px; }
  .ls-row strong.danger { color: #fca5a5; }
  .ls-row span:last-child { font-size: 10px; color: var(--text-muted); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 920px) { .layout { grid-template-columns: 1fr; } }

  .validation-report { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 18px; margin-bottom: 16px; }
  .val-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
  .val-header h3 { display: flex; align-items: center; gap: 8px; font-size: 16px; color: var(--text-primary); }
  .val-passed { display: flex; align-items: center; gap: 6px; color: var(--accent-primary); font-weight: 700; font-size: 13px; }
  .val-failed { display: flex; align-items: center; gap: 6px; color: #fca5a5; font-weight: 700; font-size: 13px; }
  .val-stats { display: grid; grid-template-columns: repeat(5, minmax(80px, 1fr)); gap: 10px; margin-bottom: 16px; }
  .val-stat { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 12px; padding: 10px; display: grid; gap: 3px; text-align: center; }
  .val-stat strong { font-size: 22px; color: var(--text-primary); }
  .val-stat span { font-size: 11px; color: var(--text-muted); }
  .val-stat.danger { border-color: rgba(239,68,68,.35); background: rgba(239,68,68,.06); }
  .val-stat.danger strong { color: #fca5a5; }
  .val-stat.warning { border-color: rgba(245,158,11,.35); background: rgba(245,158,11,.06); }
  .val-stat.warning strong { color: #fbbf24; }
  .val-section { margin-bottom: 14px; }
  .val-section h4 { display: flex; align-items: center; gap: 6px; color: var(--text-secondary); font-size: 12px; text-transform: uppercase; letter-spacing: .04em; margin-bottom: 6px; }
  .val-item { padding: 7px 10px; border-radius: 8px; font-size: 12px; border: 1px solid var(--border-color); margin-bottom: 4px; display: flex; gap: 6px; align-items: baseline; }
  .val-item.error { border-color: rgba(239,68,68,.3); color: #fca5a5; background: rgba(239,68,68,.04); }
  .val-item.warning { border-color: rgba(245,158,11,.3); color: #fde68a; background: rgba(245,158,11,.04); }
  .val-item code { font-size: 11px; color: var(--accent-primary); }
</style>
