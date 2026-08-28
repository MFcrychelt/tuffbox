<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    PlayCircle, RefreshCw, Terminal, TimerReset, XCircle,
    Shield, Server, Square, Stethoscope, Zap,
    Camera, FolderOpen,
  } from "@lucide/svelte";
  import { onDestroy, onMount, tick } from "svelte";
  import { ideStageRequest, openLaunchLog, projectPath, projectInfo } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import TestHardwareCard from "./test/TestHardwareCard.svelte";
  import TestLoadChart from "./test/TestLoadChart.svelte";
  import { launchWithFeedback } from "../lib/launch";
  import type { TestRunRecord } from "../lib/api";
  import { gb1, peaksFromSamples, pushLoadSample, type LoadSample } from "../lib/testLoad";

  type Profile = {
    id: string;
    name: string;
    side: string;
    memoryMb?: number | null;
    jvmArgs: string[];
  };

  type LiveDebugStats = {
    hostCpuPercent: number;
    hostMemoryUsedMb: number;
    hostMemoryTotalMb: number;
    instance: null | {
      pid: number;
      profile: string;
      startedAt: number;
      cpuPercent: number;
      memoryMb: number;
      virtualMemoryMb: number;
    };
  };

  type Verdict = "pass" | "fail" | "timedOut" | "crashed";
  type LivePhase = "idle" | "launching" | "bootstrapping" | Verdict;

  type MatrixRow = {
    profile: string;
    verdict: Verdict | "skipped" | "running";
    durationSeconds: number | null;
    reason?: string;
  };

  const PASS_SIGNALS = [
    "Sound engine started",
    "Reloading ResourceManager",
    "Done (",
    "Done loading",
    "Minecraft has been loaded",
    "Started serving on",
    "For help, type \"help\"",
    "Preparing spawn area: 100%",
  ];

  const FAIL_SIGNALS = [
    "# Launch error:",
    "Exception in thread",
    "Minecraft has crashed",
    "---- Minecraft Crash Report ----",
    "Failed to start the minecraft server",
    "A fatal error has been detected",
  ];

  const CLIENT_4G_MEMORY_MB = 4096;
  const DEFAULT_TIMEOUT_S = 180;
  const LOG_TAIL_LINES = 500;

  let documentVisible = $state(true);

  let profiles = $state<Profile[]>([]);
  let selectedProfile = $state("client");
  let log = "";
  let running = $state(false);
  let watching = $state(false);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let startedAt: number | null = null;
  let lastLoadedPath: string | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;
  let now = $state(Date.now())
  let validationReport = $state<any>(null);
  let validationLoading = $state(false);
  let validationError = $state<string | null>(null);
  let autoScroll = $state(true);
  let logEl = $state<HTMLPreElement | null>(null);
  let live = $state<LiveDebugStats | null>(null);
  let killing = $state(false);
  let launchStats = $state<any>(null);
  let forceRun = $state(false);
  let autoSnapshot = $state(false);
  let levelSeed = $state("");
  let onlineModeOff = $state(true);
  let timeoutSeconds = $state(DEFAULT_TIMEOUT_S);
  let livePhase = $state<LivePhase>("idle");
  let verdictReason: string | null = null;
  let startupSeconds = $state<number | null>(null);
  let activeRunId: string | null = null;
  let finalizeInFlight = $state(false);
  let sawProcess = $state(false);
  let historyFilter = $state<"all" | "pass" | "fail" | "crashed">("all");
  let worlds = $state<{ name: string }[]>([]);
  let quickPlayWorld = $state("");
  let matrixDetailsOpen = $state(false);
  let matrixIds = $state<Record<string, boolean>>({});
  let matrixRunning = $state(false);
  let matrixStopOnFail = $state(true);
  let matrixSummary = $state<MatrixRow[]>([]);
  let matrixAbort = $state(false);
  let serverDir = $state("");
  let activeLogRoot = $state<string | null>(null);

  let runs: TestRunRecord[] = [];
  let capturedRunIds = $state<Record<string, boolean>>({});

  let loadSamples = $state<LoadSample[]>([]);
  let activeXmxMb = $state(4096);
  const potatoPc = typeof document !== "undefined"
    && document.documentElement.classList.contains("potato-pc");

  const selected = $derived(profiles.find((p) => p.id === selectedProfile));
  const elapsed = $derived(startedAt ? Math.floor((now - startedAt) / 1000) : 0);
  const validationCritical = $derived(!!validationReport && (
    !validationReport.passed
    || (validationReport.graphErrors ?? 0) > 0
    || (validationReport.jsonErrors?.length ?? 0) > 0
  ));
  const validationBadge = $derived(!validationReport
    ? null
    : validationReport.passed
      ? { ok: true, label: "OK" }
      : {
          ok: false,
          label: `${(validationReport.graphErrors ?? 0) + (validationReport.jsonErrors?.length ?? 0)} errors`,
        });
  const filteredRuns = $derived(runs.filter((r) => {
    if (historyFilter === "all") return true;
    const s = normalizeStatus(r.status);
    if (historyFilter === "pass") return s === "pass" || s === "finished";
    if (historyFilter === "fail") return s === "fail" || s === "failed" || s === "timedOut";
    if (historyFilter === "crashed") return s === "crashed";
    return true;
  }));
  const statusLabel = $derived((() => {
    switch (livePhase) {
      case "launching": return "Launching…";
      case "bootstrapping": return `Bootstrapping… ${elapsed}s`;
      case "pass": return `Pass (${startupSeconds ?? elapsed}s)`;
      case "fail": return "Fail";
      case "timedOut": return "TimedOut";
      case "crashed": return "Crashed";
      default: return live?.instance || running ? `${elapsed}s` : "idle";
    }
  })());
  $effect(() => {
    if ($projectPath && lastLoadedPath !== $projectPath) loadProfiles(true);
  });
  const displayLog = $derived(tailLogLines(log, LOG_TAIL_LINES));
  const logLineCount = $derived(log ? log.split("\n").length : 0);
  const logTruncated = $derived(logLineCount > LOG_TAIL_LINES);

  function tailLogLines(text: string, maxLines: number): string {
    if (!text) return "";
    const lines = text.split("\n");
    if (lines.length <= maxLines) return text;
    const omitted = lines.length - maxLines;
    return `… (${omitted} earlier lines omitted)\n${lines.slice(-maxLines).join("\n")}`;
  }

  function normalizeStatus(s: string) {
    return s;
  }

  function logSliceForCurrentRun(text: string): string {
    const marker = "# TuffBox launching";
    const idx = text.lastIndexOf(marker);
    if (idx >= 0) return text.slice(idx);
    return text;
  }

  function detectPass(text: string): string | null {
    const slice = logSliceForCurrentRun(text);
    for (const sig of PASS_SIGNALS) {
      if (slice.includes(sig)) return sig;
    }
    return null;
  }

  function detectFail(text: string): string | null {
    const slice = logSliceForCurrentRun(text);
    for (const sig of FAIL_SIGNALS) {
      if (slice.includes(sig)) return sig;
    }
    // Early Stopping! before any pass is fail-ish
    if (slice.includes("Stopping!") && !detectPass(text)) return "Stopping!";
    return null;
  }

  function formatRunTime(value: string) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds)) return value;
    return new Date(seconds * 1000).toLocaleString();
  }

  function verdictClass(status: string) {
    const s = status.toLowerCase();
    if (s === "pass" || s === "finished") return "pass";
    if (s === "crashed") return "crashed";
    if (s === "timedout" || s === "timeout") return "timedOut";
    if (s === "fail" || s === "failed") return "fail";
    if (s === "started") return "started";
    return status;
  }

  function verdictLabel(status: string) {
    const s = status.toLowerCase();
    if (s === "finished") return "Pass*";
    if (s === "failed") return "Fail";
    if (s === "timedout") return "TimedOut";
    if (s === "pass") return "Pass";
    if (s === "fail") return "Fail";
    if (s === "crashed") return "Crashed";
    if (s === "started") return "Running";
    return status;
  }

  async function loadStats() {
    if (!$projectPath) return;
    try {
      launchStats = await invoke("get_launch_stats", { path: $projectPath });
    } catch {
      launchStats = null;
    }
  }

  async function loadWorlds() {
    if (!$projectPath) return;
    try {
      worlds = await invoke("list_worlds", { path: $projectPath });
      if (!quickPlayWorld && worlds[0]) quickPlayWorld = worlds[0].name;
    } catch {
      worlds = [];
    }
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

  async function loadProfiles(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && profiles.length > 0) return;
    loading = true;
    error = null;
    try {
      profiles = await invoke("list_profiles", { path: $projectPath });
      selectedProfile = profiles.find((p) => p.id === selectedProfile)?.id ?? profiles[0]?.id ?? "client";
      const defaults: Record<string, boolean> = {};
      for (const p of profiles) {
        defaults[p.id] = /client|server|low/i.test(p.id) || p.id === selectedProfile;
      }
      if (Object.keys(matrixIds).length === 0) matrixIds = defaults;
      lastLoadedPath = $projectPath;
      if (!serverDir.trim()) serverDir = defaultServerDir();
      await refreshLog();
      await loadRuns();
      await loadStats();
      await loadWorlds();
      await refreshLive();
      if (live?.instance) {
        running = true;
        sawProcess = true;
        livePhase = "bootstrapping";
        startedAt = (live.instance.startedAt || 0) * 1000 || Date.now();
        startPolling();
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadRuns() {
    if (!$projectPath) return;
    try {
      runs = await invoke("list_test_runs", { path: $projectPath });
      for (const r of runs) {
        if ((r.capturedPaths?.length ?? 0) > 0) {
          capturedRunIds = { ...capturedRunIds, [r.id]: true };
        }
      }
    } catch {
      runs = [];
    }
  }

  function projectDir(): string {
    if (!$projectPath) return "";
    return $projectPath.replace(/[/\\][^/\\]+$/, "");
  }

  function defaultServerDir(): string {
    const dir = projectDir();
    const name = ($projectInfo?.id || $projectInfo?.name || "modpack")
      .toString()
      .replace(/[<>:"/\\|?*]+/g, "-")
      .trim() || "modpack";
    return dir ? `${dir}\\${name}-server` : `${name}-server`;
  }

  async function ensureServerDir(): Promise<string | null> {
    if (!$projectPath) return null;
    if (!serverDir.trim()) serverDir = defaultServerDir();
    const picked = await openDialog({
      directory: true,
      multiple: false,
      title: "Choose folder for the server instance",
      defaultPath: serverDir.trim() || projectDir() || undefined,
    });
    if (picked == null) return null;
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return null;
    serverDir = path;
    return path;
  }

  async function maybeSnapshot(label: string) {
    if (!autoSnapshot || !$projectPath) return;
    const dir = projectDir();
    try {
      await invoke("create_snapshot", {
        projectDir: dir,
        name: `pre-test-${Date.now()}`,
        reason: `Auto snapshot before ${label}`,
      });
      message = `Snapshot taken before ${label}.`;
    } catch (e) {
      error = `Snapshot failed: ${e}`;
    }
  }

  function canLaunch(): boolean {
    if (!$projectPath || running || matrixRunning) return false;
    if (validationCritical && !forceRun) {
      error = "Validation has critical issues. Enable Force run to launch anyway.";
      return false;
    }
    return true;
  }

  async function beginRun(opts: {
    profile: string;
    label: string;
    memoryMbOverride?: number | null;
    quickPlayType?: string | null;
    quickPlayValue?: string | null;
    prepareServer?: boolean;
    serverDir?: string | null;
    openServerConsole?: boolean;
  }): Promise<boolean> {
    if (!canLaunch()) return false;
    running = true;
    sawProcess = false;
    finalizeInFlight = false;
    startedAt = Date.now();
    loadSamples = [];
    const profile = profiles.find((p) => p.id === opts.profile);
    activeXmxMb = opts.memoryMbOverride ?? profile?.memoryMb ?? 4096;
    error = null;
    message = null;
    log = "";
    livePhase = "launching";
    verdictReason = null;
    startupSeconds = null;
    activeRunId = null;
    activeLogRoot = opts.serverDir
      ? `${opts.serverDir.replace(/[/\\]+$/, "")}\\tuffbox.project.json`
      : $projectPath;
    try {
      await maybeSnapshot(opts.label);
      if (opts.prepareServer && opts.serverDir) {
        await invoke("generate_server_properties", {
          path: $projectPath,
          levelSeed: levelSeed.trim() || null,
          onlineMode: onlineModeOff ? false : true,
          targetDir: opts.serverDir,
        });
      }
      await invoke("record_launch", { path: $projectPath });
      const res = await launchWithFeedback(
        {
          path: $projectPath!,
          profile: opts.profile,
          memoryMbOverride: opts.memoryMbOverride ?? null,
          quickPlayType: opts.quickPlayType ?? null,
          quickPlayValue: opts.quickPlayValue ?? null,
          serverDir: opts.serverDir ?? null,
          levelSeed: levelSeed.trim() || null,
          onlineMode: onlineModeOff ? false : true,
        },
        {
          openLog: false,
          logPath: activeLogRoot,
          logTitle: opts.openServerConsole ? "Server console" : null,
        },
      );
      if (!res) {
        running = false;
        livePhase = "idle";
        activeLogRoot = null;
        return false;
      }
      await loadStats();
      await loadRuns();
      activeRunId = runs[0]?.id ?? null;
      livePhase = "bootstrapping";
      message = opts.serverDir
        ? `${opts.label} started — server folder: ${opts.serverDir}`
        : `${opts.label} started — watching latest.log.`;
      startPolling();
      return true;
    } catch (e) {
      error = String(e);
      running = false;
      livePhase = "fail";
      verdictReason = String(e);
      activeLogRoot = null;
      return false;
    }
  }

  async function smokeClient() {
    activeLogRoot = $projectPath;
    await beginRun({ profile: selectedProfile, label: "Smoke client", openServerConsole: false });
  }

  async function runServer() {
    const dir = await ensureServerDir();
    if (!dir) {
      message = "Server folder selection cancelled.";
      return;
    }
    await beginRun({
      profile: "server",
      label: "Run server",
      prepareServer: true,
      serverDir: dir,
      openServerConsole: true,
    });
  }

  async function runClient4Ram() {
    const client = profiles.find((p) => p.id === "client")
      ?? profiles.find((p) => String(p.side).toLowerCase() !== "server")
      ?? profiles.find((p) => p.id === selectedProfile);
    if (!client) {
      error = "No client profile found.";
      return;
    }
    await beginRun({
      profile: client.id,
      label: "Run client 4 RAM",
      memoryMbOverride: CLIENT_4G_MEMORY_MB,
      openServerConsole: false,
    });
  }

  async function quickPlay() {
    if (!quickPlayWorld) {
      error = "Pick a world in saves/ for Quick Play.";
      return;
    }
    await beginRun({
      profile: selectedProfile,
      label: `Quick Play · ${quickPlayWorld}`,
      quickPlayType: "singleplayer",
      quickPlayValue: quickPlayWorld,
    });
  }

  async function finalizeActive(verdict: Verdict, reason: string, kill = false) {
    if (finalizeInFlight) return;
    finalizeInFlight = true;
    livePhase = verdict;
    verdictReason = reason;
    if (verdict === "pass") startupSeconds = elapsed;
    const duration = elapsed;
    const runId = activeRunId;
    const shouldKill = kill || matrixRunning;
    if (shouldKill && $projectPath) {
      try {
        await invoke("kill_running_instance", { instanceId: $projectPath });
        live = live ? { ...live, instance: null } : null;
      } catch {
        // ignore
      }
    }
    running = false;
    if ($projectPath && runId) {
      try {
        const peaks = peaksFromSamples(loadSamples);
        await invoke("finalize_test_run", {
          path: $projectPath,
          runId,
          status: verdict,
          durationSeconds: duration,
          verdictReason: reason,
          peakProcMb: peaks ? Math.round(peaks.peakProcMb) : null,
          peakHostMb: peaks ? Math.round(peaks.peakHostMb) : null,
          hostTotalMb: peaks
            ? Math.round(peaks.lastHostTotalMb)
            : (live?.hostMemoryTotalMb ?? null),
          xmxMb: Math.round(activeXmxMb),
        });
      } catch {
        // ignore
      }
      try {
        await captureRunLogs({ id: runId }, true);
      } catch {
        // ignore
      }
    }
    await loadRuns();
    await loadStats();
  }

  async function evaluateLogAndLifecycle() {
    if (!running || finalizeInFlight) return;
    if (livePhase === "launching" && (live?.instance || log.length > 40)) {
      livePhase = "bootstrapping";
    }
    if (live?.instance) sawProcess = true;

    const fail = detectFail(log);
    if (fail && livePhase !== "pass") {
      // Prefer crash check after exit; while alive treat as fail signal
      if (!live?.instance && sawProcess) {
        let crashed = false;
        try {
          crashed = await invoke<boolean>("has_crashed", { path: $projectPath });
        } catch {
          crashed = false;
        }
        await finalizeActive(crashed ? "crashed" : "fail", fail);
        return;
      }
      if (fail.includes("Launch error") || fail.includes("Crash Report") || fail.includes("fatal")) {
        await finalizeActive("fail", fail);
        return;
      }
    }

    const pass = detectPass(log);
    if (pass && livePhase !== "pass") {
      await finalizeActive("pass", pass);
      return;
    }

    if (timeoutSeconds > 0 && elapsed >= timeoutSeconds && livePhase === "bootstrapping") {
      await finalizeActive("timedOut", `No pass signal within ${timeoutSeconds}s`, true);
      return;
    }

    // Process exited without pass
    if (sawProcess && !live?.instance && watching && livePhase === "bootstrapping") {
      let crashed = false;
      try {
        crashed = await invoke<boolean>("has_crashed", { path: $projectPath });
      } catch {
        crashed = false;
      }
      if (crashed) {
        await finalizeActive("crashed", "Crash report detected after exit");
      } else if (fail) {
        await finalizeActive("fail", fail);
      } else if (log.includes("Process exited") || log.includes("Stopping!")) {
        await finalizeActive("fail", "Process exited before pass signal");
      }
    }
  }

  async function refreshLog() {
    if (!$projectPath) return;
    try {
      log = await invoke("get_launch_log", { path: activeLogRoot || $projectPath });
      if (autoScroll && logEl) {
        await tick();
        logEl.scrollTop = logEl.scrollHeight;
      }
      await evaluateLogAndLifecycle();
    } catch {
      // latest.log may not exist before first run.
    }
  }

  async function refreshLive() {
    if (!$projectPath) return;
    try {
      live = await invoke("get_live_debug_stats", { instanceId: $projectPath });
      if (live?.instance) {
        running = true;
        sawProcess = true;
        if (!startedAt) startedAt = (live.instance.startedAt || 0) * 1000 || Date.now();
        if (livePhase === "idle" || livePhase === "launching") livePhase = "bootstrapping";
      }
    } catch {
      // ignore sampler failures
    }
  }

  async function captureRunLogs(run: { id: string }, silent = false) {
    if (!$projectPath) return;
    try {
      const dir: string = await invoke("capture_test_run_logs", { path: $projectPath, runId: run.id });
      capturedRunIds = { ...capturedRunIds, [run.id]: true };
      if (!silent) message = `Captured logs to ${dir}`;
      await loadRuns();
    } catch (e) {
      if (!silent) error = String(e);
    }
  }

  async function openRunLogs(run: TestRunRecord) {
    if (!$projectPath) return;
    const captureDir = `${projectDir()}/.tuffbox/test-runs/${run.id}`.replace(/\//g, "\\");
    try {
      if (!capturedRunIds[run.id]) await captureRunLogs(run, true);
      await openShell(captureDir);
    } catch {
      try {
        await openShell(run.logPath);
      } catch (e) {
        error = String(e);
      }
    }
  }

  function openDiagnose() {
    ideStageRequest.set("diagnose");
  }

  async function reRun(run: TestRunRecord) {
    selectedProfile = run.profile;
    if (run.profile === "server") await runServer();
    else await beginRun({ profile: run.profile, label: `Re-run · ${run.profile}`, openServerConsole: false });
  }

  async function killInstance() {
    if (!$projectPath || killing) return;
    killing = true;
    error = null;
    try {
      message = await invoke("kill_running_instance", { instanceId: $projectPath });
      live = live ? { ...live, instance: null } : null;
      if (running && !finalizeInFlight && livePhase === "bootstrapping") {
        await finalizeActive("fail", "Killed by user");
      } else {
        running = false;
        if (livePhase === "launching" || livePhase === "bootstrapping") livePhase = "idle";
      }
      await refreshLog();
      await loadRuns();
    } catch (e) {
      error = String(e);
    } finally {
      killing = false;
    }
  }

  async function runMatrix() {
    if (!$projectPath || running || matrixRunning) return;
    if (validationCritical && !forceRun) {
      error = "Validation has critical issues. Enable Force run to launch matrix.";
      return;
    }
    const queue = profiles.filter((p) => matrixIds[p.id]);
    if (queue.length === 0) {
      error = "Select at least one profile for the matrix.";
      return;
    }
    matrixRunning = true;
    matrixDetailsOpen = true;
    matrixAbort = false;
    matrixSummary = queue.map((p) => ({
      profile: p.id,
      verdict: "running",
      durationSeconds: null,
    }));
    message = `Matrix: ${queue.length} profile(s), sequential.`;

    for (let i = 0; i < queue.length; i++) {
      if (matrixAbort) {
        for (let j = i; j < queue.length; j++) {
          matrixSummary[j] = { ...matrixSummary[j], verdict: "skipped", reason: "Aborted" };
        }
        break;
      }
      const profile = queue[i];
      matrixSummary[i] = { ...matrixSummary[i], verdict: "running" };
      const isServer = profile.id === "server" || String(profile.side).toLowerCase() === "server";
      let stagedServerDir: string | null = null;
      if (isServer) {
        stagedServerDir = serverDir.trim() || defaultServerDir();
        serverDir = stagedServerDir;
      }
      const ok = await beginRun({
        profile: profile.id,
        label: `Matrix · ${profile.name}`,
        prepareServer: isServer,
        serverDir: stagedServerDir,
        openServerConsole: isServer,
        memoryMbOverride: null,
      });
      if (!ok) {
        matrixSummary[i] = {
          profile: profile.id,
          verdict: "fail",
          durationSeconds: elapsed,
          reason: error ?? "Launch failed",
        };
        if (matrixStopOnFail) {
          for (let j = i + 1; j < queue.length; j++) {
            matrixSummary[j] = { ...matrixSummary[j], verdict: "skipped", reason: "Stop on fail" };
          }
          break;
        }
        continue;
      }

      // Wait until current run finalizes
      while (running && !matrixAbort) {
        await new Promise((r) => setTimeout(r, 500));
      }
      const verdict = (livePhase === "pass" || livePhase === "fail" || livePhase === "timedOut" || livePhase === "crashed")
        ? livePhase
        : "fail";
      matrixSummary[i] = {
        profile: profile.id,
        verdict,
        durationSeconds: startupSeconds ?? elapsed,
        reason: verdictReason ?? undefined,
      };
      // Brief pause between JVMs
      await new Promise((r) => setTimeout(r, 800));
      if (matrixStopOnFail && verdict !== "pass") {
        for (let j = i + 1; j < queue.length; j++) {
          matrixSummary[j] = { ...matrixSummary[j], verdict: "skipped", reason: "Stop on fail" };
        }
        break;
      }
    }

    matrixRunning = false;
    message = "Matrix finished.";
  }

  function stopMatrix() {
    matrixAbort = true;
  }

  function onSecondaryToggle(e: Event) {
    const el = e.currentTarget as HTMLDetailsElement;
    if (!el?.open) return;
    // Expand downward into scroll space instead of compressing the log above.
    requestAnimationFrame(() => {
      el.scrollIntoView({ block: "nearest", behavior: "smooth" });
    });
  }

  function startPolling() {
    watching = true;
    now = Date.now();
    syncPollingTimer();
  }

  function syncPollingTimer() {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    if (watching && documentVisible) {
      timer = setInterval(pollTick, 1000);
      pollTick();
    }
  }

  function pollTick() {
    if (!watching || !documentVisible) return;
    now = Date.now();
    refreshLog();
    void refreshLive().then(() => {
      if (!watching || startedAt == null || !live) return;
      pushLoadSample(loadSamples, {
        tSec: (Date.now() - startedAt) / 1000,
        hostUsedMb: live.hostMemoryUsedMb,
        hostTotalMb: live.hostMemoryTotalMb,
        procRssMb: live.instance?.memoryMb ?? 0,
        hostCpuPct: live.hostCpuPercent,
      });
    });
  }

  function stopWatching() {
    watching = false;
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
  }

  function onVisibilityChange() {
    documentVisible = document.visibilityState === "visible";
    syncPollingTimer();
  }

  onMount(() => {
    documentVisible = document.visibilityState === "visible";
    document.addEventListener("visibilitychange", onVisibilityChange);
  });

  onDestroy(() => {
    document.removeEventListener("visibilitychange", onVisibilityChange);
    if (timer) clearInterval(timer);
  });
</script>

<div class="test-runs">
  <div class="toolbar">
    <div class="title"><PlayCircle size={18} /> <span>Test · launch lab</span></div>
    <div class="toolbar-actions">
      <button class="ghost" onclick={() => loadProfiles(true)} disabled={!$projectPath || loading} title="Reload profiles">
        <RefreshCw size={14} class={loading ? "spin" : ""} />
        Profiles
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}
  {#if validationError}<div class="notice error">{validationError}</div>{/if}

  {#if !$projectPath}
    <EmptyState icon={PlayCircle} title="No project selected" description="Open a project to run test profiles." />
  {:else}
    <div class="terminal-body">
      <div class="status-strip">
        <div class="status" class:running={!!live?.instance || running} class:pass={livePhase === "pass"} class:fail={livePhase === "fail" || livePhase === "crashed" || livePhase === "timedOut"}>
          <TimerReset size={16} />
          {statusLabel}
        </div>
        {#if live?.instance}
          <button class="danger" onclick={killInstance} disabled={killing} title="Kill game/server process">
            <Square size={16} />
            {killing ? "Stopping…" : "Kill"}
          </button>
        {/if}
      </div>
      <div class="launch-bar">
        <label class="profile-select">
          Profile
          <select bind:value={selectedProfile} disabled={running || matrixRunning}>
            {#each profiles as p (p.id)}
              <option value={p.id}>{p.name} ({p.id})</option>
            {/each}
          </select>
        </label>
        <div class="launch-actions">
          <button class="preset primary" onclick={smokeClient} disabled={running || matrixRunning || !selectedProfile}>
            <PlayCircle size={16} /> Smoke client
          </button>
          <button class="preset" onclick={runServer} disabled={running || matrixRunning} title="Stage both+server mods into a folder and open Server console">
            <Server size={16} /> Run server
          </button>
          <button class="preset" onclick={runClient4Ram} disabled={running || matrixRunning} title={`Launch client with ${CLIENT_4G_MEMORY_MB} MB RAM`}>
            <Zap size={16} /> Run client 4 RAM
          </button>
        </div>
      </div>

      <div class="work">
        <div class="load-panel">
          <h3>Load</h3>
          <TestLoadChart samples={loadSamples} xmxMb={activeXmxMb} potato={potatoPc} />
          <TestHardwareCard samples={loadSamples} xmxMb={activeXmxMb} />
        </div>
        <div class="log-panel">
          <div class="log-tools">
            <label class="auto-scroll">
              <input type="checkbox" bind:checked={autoScroll} /> Auto-scroll
            </label>
            <div class="log-tools-right">
              {#if activeLogRoot && activeLogRoot !== $projectPath}
                <span class="log-trunc-hint">Server console</span>
                <button class="ghost mini" onclick={() => activeLogRoot && openLaunchLog(activeLogRoot, "Server console")}>
                  <Terminal size={12} /> Open server console
                </button>
              {/if}
              {#if logTruncated}
                <span class="log-trunc-hint">Showing last {LOG_TAIL_LINES} of {logLineCount} lines</span>
              {/if}
              {#if !documentVisible && watching}
                <span class="log-paused-hint">Poll paused (tab hidden)</span>
              {/if}
              <button class="ghost mini" onclick={openDiagnose}><Stethoscope size={12} /> Open in Diagnose</button>
              {#if watching}
                <button class="ghost mini" onclick={stopWatching}>Stop watching</button>
              {:else if running || live?.instance}
                <button class="ghost mini" onclick={startPolling}>Watch log</button>
              {/if}
            </div>
          </div>
          <pre class="log" bind:this={logEl}>{displayLog || "latest.log will appear here after the first run."}</pre>
        </div>
      </div>

      <details class="secondary-panel" ontoggle={onSecondaryToggle}>
        <summary>Options</summary>
        <div class="secondary-body">
          <div class="preflight">
            <button class="ghost" onclick={() => loadProfiles(true)} disabled={!$projectPath || loading}>
              <RefreshCw size={16} class={loading ? "spin" : ""} />
              Refresh
            </button>
            <button class="secondary" onclick={runValidation} disabled={!$projectPath || validationLoading}>
              <Shield size={16} />
              {validationLoading ? "Checking…" : "Validate"}
            </button>
            {#if validationBadge}
              <span class="val-badge" class:ok={validationBadge.ok} class:bad={!validationBadge.ok}>
                {validationBadge.label}
              </span>
            {/if}
            <label class="chk" title="Allow launch even when validation has errors">
              <input type="checkbox" bind:checked={forceRun} /> Force run
            </label>
            <label class="chk" title="Create a snapshot before smoke / dry run">
              <input type="checkbox" bind:checked={autoSnapshot} />
              <Camera size={12} /> Auto-snapshot
            </label>
            <label class="timeout">
              Timeout
              <input type="number" min="30" max="900" bind:value={timeoutSeconds} /> s
            </label>
            {#if selected}
              <span class="hint">
                {selected.name} · {selected.side} · {selected.memoryMb ?? 4096} MB
              </span>
            {/if}
            {#if startupSeconds != null && livePhase === "pass"}
              <span class="metric">Startup {startupSeconds}s</span>
            {/if}
          </div>
          <div class="opts-row">
            <label>
              Quick Play world
              <select bind:value={quickPlayWorld}>
                {#if worlds.length === 0}
                  <option value="">No worlds in saves/</option>
                {:else}
                  {#each worlds as w (w.name)}
                    <option value={w.name}>{w.name}</option>
                  {/each}
                {/if}
              </select>
            </label>
            <button class="secondary" onclick={quickPlay} disabled={running || matrixRunning || !quickPlayWorld}>
              Launch Quick Play
            </button>
          </div>
          <div class="opts-row">
            <label>
              Server folder
              <input type="text" placeholder="Where the server instance will be staged" bind:value={serverDir} />
            </label>
            <button class="secondary" onclick={async () => { await ensureServerDir(); }} disabled={!$projectPath}>
              <FolderOpen size={14} /> Browse…
            </button>
            <button class="ghost" onclick={() => (serverDir = defaultServerDir())} disabled={!$projectPath}>
              Default
            </button>
          </div>
          <div class="opts-row">
            <label>
              level-seed
              <input type="text" placeholder="optional" bind:value={levelSeed} />
            </label>
            <label class="chk">
              <input type="checkbox" bind:checked={onlineModeOff} /> online-mode=false
            </label>
            <button class="ghost" onclick={async () => {
              try {
                const dir = serverDir.trim() || defaultServerDir();
                await invoke("generate_server_properties", {
                  path: $projectPath,
                  levelSeed: levelSeed.trim() || null,
                  onlineMode: onlineModeOff ? false : true,
                  targetDir: dir,
                });
                message = `server.properties written to ${dir}`;
              } catch (e) { error = String(e); }
            }}>Write server.properties</button>
          </div>
          {#if validationReport && !validationReport.passed}
            <div class="validation-report compact">
              <div class="val-header">
                <h3><Shield size={16} /> Validation</h3>
                <span class="val-failed"><XCircle size={14} /> Issues — use Force run to launch</span>
              </div>
              <div class="val-stats">
                <div class="val-stat" class:danger={validationReport.graphErrors > 0}>
                  <strong>{validationReport.graphErrors}</strong><span>graph</span>
                </div>
                <div class="val-stat" class:danger={validationReport.jsonErrors?.length > 0}>
                  <strong>{validationReport.jsonErrors?.length ?? 0}</strong><span>JSON</span>
                </div>
                <div class="val-stat" class:danger={validationReport.circularDeps?.length > 0}>
                  <strong>{validationReport.circularDeps?.length ?? 0}</strong><span>cycles</span>
                </div>
              </div>
              <button class="ghost" onclick={() => (validationReport = null)}>Hide</button>
            </div>
          {/if}
        </div>
      </details>

      <details class="secondary-panel" bind:open={matrixDetailsOpen} ontoggle={onSecondaryToggle}>
        <summary>Profile matrix</summary>
        <div class="secondary-body">
          <div class="matrix-panel">
            <div class="matrix-head">
              <label class="chk"><input type="checkbox" bind:checked={matrixStopOnFail} /> Stop on fail</label>
              <button class="secondary" onclick={runMatrix} disabled={running || matrixRunning}>Run matrix</button>
              {#if matrixRunning}
                <button class="danger" onclick={stopMatrix}>Stop queue</button>
              {/if}
            </div>
            <div class="matrix-checks">
              {#each profiles as p (p.id)}
                <label class="chk">
                  <input type="checkbox" bind:checked={matrixIds[p.id]} />
                  {p.name} <small>({p.id})</small>
                </label>
              {/each}
            </div>
            {#if matrixSummary.length > 0}
              <table class="matrix-table">
                <thead><tr><th>Profile</th><th>Verdict</th><th>Time</th><th>Reason</th></tr></thead>
                <tbody>
                  {#each matrixSummary as row, i (row.profile + '-' + i)}
                    <tr class={row.verdict}>
                      <td>{row.profile}</td>
                      <td><span class="vbadge {row.verdict}">{row.verdict}</span></td>
                      <td>{row.durationSeconds != null ? `${row.durationSeconds}s` : "—"}</td>
                      <td class="muted">{row.reason ?? ""}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        </div>
      </details>

      <details class="secondary-panel" ontoggle={onSecondaryToggle}>
        <summary>Profiles &amp; run history</summary>
        <div class="secondary-body profiles-panel">
          {#if profiles.length === 0}
            <div class="muted">No profiles found.</div>
          {:else}
            <div class="profile-grid">
              {#each profiles as profile (profile.id)}
                <button
                  class="profile-card"
                  class:selected={selectedProfile === profile.id}
                  onclick={() => (selectedProfile = profile.id)}
                >
                  <strong>{profile.name}</strong>
                  <span>{profile.id} · {profile.side}</span>
                  <small>{profile.memoryMb ?? 4096} MB · {profile.jvmArgs.length} JVM args</small>
                </button>
              {/each}
            </div>
          {/if}

          {#if launchStats}
            <div class="launch-stats-card">
              <h3>Launch stats</h3>
              <div class="ls-row"><span>Total launches</span><strong>{launchStats.totalLaunches}</strong></div>
              <div class="ls-row"><span>Total crashes</span><strong class:danger={launchStats.totalCrashes > 0}>{launchStats.totalCrashes}</strong></div>
              {#if launchStats.lastLaunch}<div class="ls-row"><span>Last launch</span><span>{launchStats.lastLaunch}</span></div>{/if}
            </div>
          {/if}

          <div class="history-head">
            <h2>Run history</h2>
            <div class="filters">
              <button class="ghost mini" class:active={historyFilter === "all"} onclick={() => (historyFilter = "all")}>All</button>
              <button class="ghost mini" class:active={historyFilter === "pass"} onclick={() => (historyFilter = "pass")}>Pass</button>
              <button class="ghost mini" class:active={historyFilter === "fail"} onclick={() => (historyFilter = "fail")}>Fail</button>
              <button class="ghost mini" class:active={historyFilter === "crashed"} onclick={() => (historyFilter = "crashed")}>Crashed</button>
            </div>
          </div>
          {#if filteredRuns.length === 0}
            <div class="muted">No test runs recorded yet.</div>
          {:else}
            <div class="run-history">
              {#each filteredRuns.slice(0, 12) as run (run.id)}
                <div class="run-row {verdictClass(run.status)}">
                  <div class="run-top">
                    <strong>{run.profile}</strong>
                    <span class="vbadge {verdictClass(run.status)}">{verdictLabel(run.status)}</span>
                  </div>
                  <span>{formatRunTime(run.startedAt)}</span>
                  <small>
                    {run.durationSeconds != null ? `${run.durationSeconds}s` : "—"}
                    {#if run.peakProcMb}
                      · <span
                        class="peak-badge"
                        title={run.recommendedRamGb
                          ? `Players need ${run.recommendedRamGb} GB RAM`
                          : undefined}
                      >{gb1(run.peakProcMb)} GB peak</span>
                    {/if}
                    {#if run.verdictReason} · {run.verdictReason}{/if}
                  </small>
                  <div class="run-actions">
                    <button class="ghost mini" onclick={() => openRunLogs(run)}>Open logs</button>
                    {#if !capturedRunIds[run.id]}
                      <button class="ghost mini" onclick={() => captureRunLogs(run)}>Capture</button>
                    {/if}
                    <button class="ghost mini" onclick={openDiagnose} title="Open Diagnose stage">
                      <Stethoscope size={12} /> Diagnose
                    </button>
                    <button class="ghost mini" onclick={() => reRun(run)} disabled={running || matrixRunning}>Re-run</button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </details>
    </div>
  {/if}
</div>

<style>
  .test-runs {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    max-width: none;
    width: 100%;
  }
  .toolbar, .title, .status, .status-strip, .log-tools, .preflight, .opts-row, .matrix-head, .run-top, .run-actions, .history-head, .filters, .log-tools-right, .launch-bar, .launch-actions { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 8px; flex-shrink: 0; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .terminal-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow-x: hidden;
    overflow-y: auto;
    gap: 8px;
  }
  .status-strip { flex-shrink: 0; gap: 10px; flex-wrap: wrap; }
  .launch-bar {
    flex-shrink: 0;
    gap: 12px;
    flex-wrap: wrap;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }
  .profile-select {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .profile-select select { min-width: 180px; }
  .launch-actions { gap: 8px; flex-wrap: wrap; flex: 1; }
  .preset { display: inline-flex; align-items: center; gap: 8px; }
  /* Task #65: preset buttons get the Ore UI key treatment — flat fill,
     darker bottom edge, amethyst primary. */
  .preset {
    background: #39393b;
    border-color: #39393b;
    border-bottom-color: #232425;
  }
  .preset:hover:not(:disabled) { background: #47484a; color: var(--text-primary); }
  .preset:active:not(:disabled) {
    background: #2a2b2c;
    border-bottom-width: 1px;
    transform: translateY(1px);
    filter: none;
  }
  .preset.primary {
    background: #491ac0;
    border-color: #491ac0;
    border-bottom-color: #32127f;
    color: #ffffff;
    font-weight: 700;
  }
  .preset.primary:hover:not(:disabled) {
    background: #5c2dd5;
    border-color: #5c2dd5;
    border-bottom-color: #3f1a96;
    color: #ffffff;
  }
  .work {
    flex: 1 1 auto;
    min-height: 0;
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(280px, 0.9fr) minmax(0, 1.1fr);
    gap: 8px;
  }
  .load-panel,
  .log-panel {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }
  .load-panel {
    padding: 10px 12px;
    gap: 8px;
  }
  .load-panel h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .secondary-panel {
    flex: 0 0 auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: hidden;
  }
  .secondary-panel summary {
    cursor: pointer;
    padding: 10px 14px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
    user-select: none;
    list-style: none;
    transition: color var(--motion-fast) var(--motion-ease), background var(--motion-fast) var(--motion-ease);
  }
  .secondary-panel summary:hover { color: var(--text-primary); background: rgba(255, 255, 255, 0.03); }
  .secondary-panel summary::-webkit-details-marker { display: none; }
  .secondary-panel[open] summary { border-bottom: 1px solid var(--border-color); }
  .secondary-body { padding: 12px 14px; }
  .profiles-panel { max-height: 360px; overflow: auto; }
  .profile-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
    margin-bottom: 12px;
  }
  .preflight { gap: 14px; flex-wrap: wrap; margin-bottom: 10px; color: var(--text-muted); font-size: 12px; }
  .chk { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
  .chk input { width: auto; }
  .timeout { display: inline-flex; align-items: center; gap: 6px; }
  .timeout input { width: 72px; }
  .hint { color: var(--text-secondary); }
  .metric { color: var(--accent-primary); font-weight: 700; }
  .val-badge { font-size: 11px; font-weight: 700; padding: 4px 8px; border-radius: 999px; border: 1px solid var(--border-color); }
  .val-badge.ok { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); }
  .val-badge.bad { color: #fca5a5; border-color: rgba(239, 68, 68, 0.35); background: rgba(239, 68, 68, 0.08); }
  .opts-row { gap: 12px; flex-wrap: wrap; margin-bottom: 10px; padding: 10px 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .opts-row label { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--text-muted); }
  .opts-row input[type="text"], .opts-row select { min-width: 180px; }
  .matrix-panel { display: grid; gap: 10px; }
  .matrix-head { gap: 12px; flex-wrap: wrap; }
  .matrix-checks { display: flex; flex-wrap: wrap; gap: 10px 16px; }
  .matrix-table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .matrix-table th, .matrix-table td { text-align: left; padding: 6px 8px; border-bottom: 1px solid var(--border-color); }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 8px; border: 1px solid var(--border-color); flex-shrink: 0; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent); }
  .profile-card { width: 100%; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; background: var(--bg-tertiary); color: var(--text-secondary); border: 1px solid var(--border-color); padding: 12px; text-align: left; }
  .profile-card:hover, .profile-card.selected { transform: none; border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); }
  .profile-card strong { color: var(--text-primary); }
  .profile-card span, .profile-card small, .muted { color: var(--text-muted); }
  .history-head { margin-top: 12px; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .history-head h2 { margin: 0; font-size: 15px; }
  .filters { gap: 4px; flex-wrap: wrap; }
  .filters .active { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); }
  .run-history { display: grid; gap: 8px; margin-top: 10px; }
  .run-row { display: grid; gap: 3px; padding: 10px; border-radius: var(--border-radius-md); background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .run-row strong { color: var(--text-primary); }
  .run-row span, .run-row small { color: var(--text-muted); font-size: 12px; }
  .run-top { justify-content: space-between; gap: 8px; }
  .run-actions { gap: 4px; flex-wrap: wrap; margin-top: 4px; }
  .run-row.fail, .run-row.failed { border-color: rgba(239, 68, 68, .35); }
  .run-row.pass, .run-row.finished { border-color: color-mix(in srgb, var(--accent-primary) 28%, transparent); }
  .run-row.crashed { border-color: rgba(248, 113, 113, .55); }
  .run-row.timedOut { border-color: rgba(245, 158, 11, .45); }
  .run-row.started { border-color: rgba(245, 158, 11, .28); }
  .vbadge { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: .04em; padding: 2px 7px; border-radius: 999px; border: 1px solid var(--border-color); }
  .vbadge.pass, .vbadge.finished { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); }
  .vbadge.fail, .vbadge.failed { color: #fca5a5; border-color: rgba(239, 68, 68, .4); }
  .vbadge.crashed { color: #fecaca; background: rgba(239, 68, 68, .12); }
  .vbadge.timedOut { color: #fbbf24; border-color: rgba(245, 158, 11, .4); }
  .vbadge.started, .vbadge.running { color: #93c5fd; }
  .vbadge.skipped { color: var(--text-muted); }
  .mini { padding: 5px 8px; font-size: 11px; justify-self: start; }
  .peak-badge { color: var(--text-secondary); font-variant-numeric: tabular-nums; }
  .status { gap: 8px; color: var(--text-muted); background: #39393b; border: 1px solid #39393b; border-bottom-color: #232425; border-radius: var(--border-radius-md); padding: 8px 14px; font-weight: 600; }
  .status.running { color: var(--accent-primary); }
  .status.pass { color: var(--accent-primary); }
  .status.fail { color: #fca5a5; }
  .log-tools { justify-content: space-between; gap: 10px; padding: 8px 16px; border-bottom: 1px solid var(--border-color); flex-shrink: 0; }
  .log-tools-right { gap: 6px; flex-wrap: wrap; }
  .log-trunc-hint, .log-paused-hint { font-size: 11px; color: var(--text-muted); }
  .log-paused-hint { color: #fbbf24; }
  .auto-scroll { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: 12px; }
  .auto-scroll input { width: auto; }
  .log { flex: 1; min-height: 0; overflow: auto; margin: 0; padding: 18px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; }
  .danger { background: rgba(239, 68, 68, 0.18); border-color: rgba(239, 68, 68, 0.4); color: #fecaca; }
  .danger:hover:not(:disabled) { background: rgba(239, 68, 68, 0.28); }
  .launch-stats-card { padding: 12px; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: var(--bg-tertiary); margin-bottom: 14px; display: grid; gap: 6px; }
  .launch-stats-card h3 { color: var(--text-secondary); font-size: 12px; margin: 0 0 4px; text-transform: uppercase; letter-spacing: .04em; }
  .ls-row { display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .ls-row span { color: var(--text-muted); }
  .ls-row strong { color: var(--text-primary); font-size: 16px; }
  .ls-row strong.danger { color: #fca5a5; }
  .ls-row span:last-child { font-size: 10px; color: var(--text-muted); }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1099px) {
    .work {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(160px, 32vh) minmax(0, 1fr);
    }
    .launch-bar { flex-direction: column; align-items: stretch; }
  }

  .validation-report { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; }
  .validation-report.compact { padding: 12px; }
  .val-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; gap: 8px; }
  .val-header h3 { display: flex; align-items: center; gap: 8px; font-size: 14px; color: var(--text-primary); margin: 0; }
  .val-failed { display: flex; align-items: center; gap: 6px; color: #fca5a5; font-weight: 700; font-size: 12px; }
  .val-stats { display: grid; grid-template-columns: repeat(3, minmax(70px, 1fr)); gap: 8px; margin-bottom: 8px; }
  .val-stat { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 8px; display: grid; gap: 2px; text-align: center; }
  .val-stat strong { font-size: 18px; color: var(--text-primary); }
  .val-stat span { font-size: 11px; color: var(--text-muted); }
  .val-stat.danger { border-color: rgba(239,68,68,.35); background: rgba(239,68,68,.06); }
  .val-stat.danger strong { color: #fca5a5; }
</style>
