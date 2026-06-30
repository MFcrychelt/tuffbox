<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { PlayCircle, RefreshCw, Terminal, TimerReset } from "lucide-svelte";
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
      await invoke("launch_profile", { path: $projectPath, profile: selectedProfile });
      message = `Started profile ${selectedProfile}. Watch latest.log below.`;
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
      if (log.includes("# Launch error:") || log.includes("Process exited")) {
        running = false;
      }
    } catch {
      // latest.log may not exist before first run.
    }
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
        <Terminal size={16} />
        Tail log
      </button>
      <button on:click={launch} disabled={!$projectPath || running || !selectedProfile}>
        <PlayCircle size={16} />
        {running ? "Running..." : "Run profile"}
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if !$projectPath}
    <div class="empty">Open a project to run test profiles.</div>
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
  .test-runs { max-width: 1400px; }
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
  .run-panel { overflow: hidden; }
  .run-header { justify-content: space-between; gap: 12px; padding: 16px 18px; border-bottom: 1px solid var(--border-color); }
  .run-header h2 { margin: 0 0 4px; }
  .status { gap: 8px; color: var(--text-muted); background: var(--bg-tertiary); border-radius: 999px; padding: 8px 12px; }
  .status.running { color: var(--accent-primary); }
  .log { min-height: 560px; max-height: 680px; overflow: auto; margin: 0; padding: 18px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; }
  .stop { margin: 12px; }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 920px) { .layout { grid-template-columns: 1fr; } }
</style>
