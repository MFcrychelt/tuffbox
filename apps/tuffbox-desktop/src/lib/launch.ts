import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { get } from "svelte/store";
import { toasts } from "./toast";
import type {
  LaunchCrashEvent,
  LaunchErrorInfo,
  LaunchLifecycleEvent,
  LaunchPhase,
  LaunchResult,
  ProcessExitedEvent,
  RunningInstance,
} from "./api";
import { api } from "./api";
import {
  applyLaunchLifecycle,
  beginLaunchSession,
  failLaunchSession,
  isProjectLaunching,
  isProjectRunning,
  launchSessions,
  markLaunchRunning,
  openLaunchLog,
  projectPath,
  removeRunning,
  runningInstances,
  upsertRunning,
  authState,
  loginModalOpen,
  openLauncherSettings,
} from "./store";
import { shareCrashLogWithFeedback } from "./mclogs";
import { reportSoftVerifyCrash } from "./softVerify";

export type { LaunchErrorInfo, LaunchLifecycleEvent, LaunchPhase };

export interface LaunchParams {
  path: string;
  /** "client" (default) | "server" | a custom profile id */
  profile?: string;
  quickPlayType?: string | null;
  quickPlayValue?: string | null;
  /** One-shot memory override for smoke/low-end runs (does not mutate manifest). */
  memoryMbOverride?: number | null;
  /** Staged server instance directory (required for profile === "server"). */
  serverDir?: string | null;
  levelSeed?: string | null;
  onlineMode?: boolean | null;
}

export interface LaunchFeedbackOptions {
  onStarted?: (result: LaunchResult) => void;
  showSuccess?: boolean;
  openLog?: boolean;
  /** Manifest path for the log modal (for example, a staged server dir). */
  logPath?: string | null;
  logTitle?: string | null;
}

// Retryable error categories — mirrors `LaunchErrorKind::retryable` on the Rust
// side. Only these get a Retry button; fundamental config errors do not.
const RETRYABLE = new Set<string>([
  "offline",
  "host_unreachable",
  "version_resolve",
  "mod_download",
  "java_missing",
  "install",
  "launch_crash",
]);

const STARTUP_PHASES = new Set<LaunchPhase>([
  "preflight",
  "resolving_java",
  "downloading",
  "starting",
]);

/** Human-friendly text for buttons / status chips. The backend message is more
 * specific when available; this is a stable fallback for every Play surface. */
export function launchPhaseLabel(phase: LaunchPhase | null | undefined): string {
  switch (phase) {
    case "preflight": return "Checking…";
    case "resolving_java": return "Resolving Java…";
    case "downloading": return "Preparing files…";
    case "starting": return "Starting…";
    case "running": return "Running";
    case "stopping": return "Stopping…";
    case "exited": return "Exited";
    case "failed": return "Launch failed";
    default: return "Launch";
  }
}

export function isLaunchError(error: unknown): error is LaunchErrorInfo {
  return (
    typeof error === "object"
    && error !== null
    && "kind" in error
    && "message" in error
    && typeof (error as { kind?: unknown }).kind === "string"
    && typeof (error as { message?: unknown }).message === "string"
  );
}

function asLaunchError(error: unknown): LaunchErrorInfo {
  return isLaunchError(error)
    ? error
    : { kind: "unknown", message: String(error) };
}

function isRetryable(info: LaunchErrorInfo): boolean {
  return RETRYABLE.has(info.kind);
}

type LaunchProgressPayload = {
  phase?: string;
  message?: string;
  percent?: number | null;
};

async function pollDownloadOverlay() {
  try {
    const phase = get(launchProgress)?.phase ?? "";
    // Skip host work while not downloading assets/mods.
    if (phase && !/mod|asset|download|install|java|librar/i.test(phase)) return;
    const items = await api.system.getDownloadProgress();
    if (!Array.isArray(items) || items.length === 0) return;
    let downloaded = 0;
    let total = 0;
    for (const raw of items) {
      const it = raw as { downloaded?: number; total?: number; percent?: number };
      downloaded += Number(it.downloaded) || 0;
      total += Number(it.total) || 0;
    }
    const percent =
      total > 0 ? Math.min(99, Math.round((downloaded / total) * 100)) : null;
    const current = get(launchProgress);
    const baseMsg = current?.message?.replace(/\s·\s\d+%.*/, "") || "Downloading…";
    launchProgress.set({
      phase: current?.phase || "mods",
      message: percent != null ? `${baseMsg} · ${percent}%` : baseMsg,
      percent: percent ?? current?.percent ?? null,
    });
  } catch {
    /* optional overlay */
  }
}


async function doLaunch(params: LaunchParams): Promise<LaunchResult> {
  const profile = params.profile ?? "client";
  const memoryMbOverride = params.memoryMbOverride ?? null;
  if (params.quickPlayType || params.quickPlayValue) {
    return invoke<LaunchResult>("launch_with_quick_play", {
      path: params.path,
      profile,
      quickPlayType: params.quickPlayType ?? null,
      quickPlayValue: params.quickPlayValue ?? null,
      memoryMbOverride,
    });
  }
  if (profile === "server") {
    const serverDir = params.serverDir?.trim();
    if (!serverDir) {
      throw { kind: "install", message: "Pick a server folder before Run server." } satisfies LaunchErrorInfo;
    }
    return invoke<LaunchResult>("launch_server", {
      path: params.path,
      serverDir,
      levelSeed: params.levelSeed ?? null,
      onlineMode: params.onlineMode ?? null,
    });
  }
  return invoke<LaunchResult>("launch_profile", {
    path: params.path,
    profile,
    memoryMbOverride,
  });
}

type RememberedLaunch = {
  params: LaunchParams;
  options?: LaunchFeedbackOptions;
};

// A JVM can crash long after invoke returned. Keep the launch request per
// instance (not as one global "last launch") so its Retry action is always
// attached to the correct Play surface when several instances are used.
const rememberedLaunches = new Map<string, RememberedLaunch>();

function normaliseRunningResult(result: LaunchResult, fallback: LaunchParams): RunningInstance | null {
  const id = result.instanceId || fallback.path;
  const pid = Number(result.pid);
  const startedAt = Number(result.startedAt);
  if (!id || !Number.isFinite(pid) || pid <= 0 || !Number.isFinite(startedAt)) return null;
  return {
    id,
    pid,
    profile: result.profile || fallback.profile || "client",
    startedAt,
  };
}

/**
 * The one public path for starting Minecraft from the UI.
 *
 * It begins a shared lifecycle session before invoke, promotes it to Running
 * from the returned process identity / Tauri events, and leaves completion to
 * process-exited / launch-crashed. It intentionally has no `finally` that
 * clears UI state: invoke timing is not game lifecycle timing.
 */
export async function launchWithFeedback(
  params: LaunchParams,
  options?: LaunchFeedbackOptions,
): Promise<LaunchResult | null> {
  const profile = params.profile ?? "client";
  const sessions = get(launchSessions);
  if (isProjectLaunching(params.path, sessions)) {
    toasts.info("This instance is already launching.");
    return null;
  }
  if (isProjectRunning(params.path, get(runningInstances))) {
    toasts.info("This instance is already running.", 6000, [
      {
        label: "Stop",
        run: () => {
          void killWithFeedback(params.path);
        },
      },
    ]);
    return null;
  }

  if (profile !== "server") {
    const auth = get(authState);
    if (!auth.loggedIn || !auth.profile) {
      toasts.warning(
        "Sign in to play with your Minecraft account, or continue offline.",
        12000,
        [
          {
            label: "Sign in",
            run: () => loginModalOpen.set(true),
          },
          {
            label: "Play offline",
            run: () => {
              void launchWithFeedback(params, options);
            },
          },
        ],
      );
      return null;
    }
  }

  rememberedLaunches.set(params.path, { params, options });
  beginLaunchSession(params.path, profile);
  launchProgress.set({ phase: "preparing", message: "Preparing…", percent: 0 });
  void invoke("set_last_opened_project", { path: params.path }).catch(() => {});

  const showLog = options?.openLog !== false;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  try {
    void ensureLaunchProgressListener();
    pollTimer = setInterval(() => {
      void pollDownloadOverlay();
    }, 450);

    const result = await doLaunch(params);
    const running = normaliseRunningResult(result, params);
    if (running) {
      // A very short-lived JVM can emit `exited` before invoke returns. Never
      // resurrect that terminal state from the result; the process event stays
      // authoritative in that race.
      const phase = get(launchSessions)[running.id]?.phase;
      if (phase !== "exited" && phase !== "failed") {
        upsertRunning(running);
      }
      rememberedLaunches.set(running.id, { params, options });
    }
    if (options?.showSuccess) toasts.success("Launch started");
    options?.onStarted?.(result);
    // After a successful start, confirm any pending crash-fix as resolved when
    // latest.log looks healthy. On verified resolution the backend emits
    // `tuffbox:distill-resolution` for the Confirm → publish UI (no auto-upload).
    void invoke("confirm_crash_resolution_after_launch", { path: params.path }).catch(() => {
      // Optional bookkeeping must never change the launch lifecycle.
    });
    return result;
  } catch (error) {
    const info = asLaunchError(error);
    failLaunchSession(params.path, info);
    showLaunchError(info, () => void launchWithFeedback(params, options), { path: params.path });
    if (showLog) openLaunchLog(options?.logPath ?? params.path, options?.logTitle ?? null);
    return null;
  } finally {
    if (pollTimer) clearInterval(pollTimer);
    launchProgress.set(null);
  }
}

/** Stop a tracked game. The backend keeps it in `list_running_instances` until
 * Child::wait observes exit and then emits `process-exited`; do not optimistically
 * remove it here or the UI can claim the game stopped while it is still alive. */
export async function killWithFeedback(path: string): Promise<boolean> {
  const running = get(runningInstances).find((instance) => instance.id === path);
  if (running) {
    applyLaunchLifecycle({
      id: path,
      profile: running.profile,
      phase: "stopping",
      message: "Stopping game…",
      pid: running.pid,
      startedAt: running.startedAt,
      stopped: true,
    });
  }
  try {
    await invoke("kill_running_instance", { instanceId: path });
    toasts.info("Stopping game…");
    return true;
  } catch (e) {
    const msg = String(e).toLowerCase();
    // Stale UI / race: process already gone — treat as stopped.
    if (msg.includes("no running instance") || msg.includes("not found") || msg.includes("not running")) {
      removeRunning(path);
      toasts.info("Game already stopped");
      return true;
    }
    // If delivery of the kill signal failed, restore the known running state.
    if (running) markLaunchRunning(running);
    toasts.error(`Stop failed: ${e}`);
    return false;
  }
}

/** Reconcile initial app state with the backend process registry. Events keep
 * it current afterwards; this is only the startup/reconnect safety net. */
export async function refreshRunningInstances(): Promise<void> {
  try {
    const list = await invoke<RunningInstance[]>("list_running_instances");
    const instances = Array.isArray(list) ? list : [];
    runningInstances.set(instances);
    for (const instance of instances) markLaunchRunning(instance);
  } catch {
    // Backend not ready / optional in web preview.
  }
}

/** Keep running state truthful: prune dead PIDs on an adaptive interval + focus. */
export function startRunningInstancesWatch(): () => void {
  let timer: ReturnType<typeof setInterval> | null = null;

  const tick = () => {
    void refreshRunningInstances();
  };

  const schedule = () => {
    if (timer) clearInterval(timer);
    const busy = get(isLaunching) || get(runningInstances).length > 0;
    // Hot while launching / game open; cool when idle (less CPU for the launcher UI).
    timer = setInterval(tick, busy ? 2500 : 12000);
  };

  schedule();
  const unsubLaunch = isLaunching.subscribe(() => schedule());
  const unsubRun = runningInstances.subscribe(() => schedule());
  const onFocus = () => tick();
  window.addEventListener("focus", onFocus);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") tick();
  });

  return () => {
    if (timer) clearInterval(timer);
    unsubLaunch();
    unsubRun();
    window.removeEventListener("focus", onFocus);
  };
}

/// Display a launch error as a toast with Retry / View log actions when
/// appropriate.
export function showLaunchError(
  error: unknown,
  retry?: () => void,
  context?: { path?: string | null },
): void {
  const info = asLaunchError(error);
  const actions: { label: string; run: () => void }[] = [];
  if (retry && isRetryable(info)) {
    actions.push({ label: "Retry", run: retry });
  }
  if (context?.path) {
    actions.push({
      label: "Live log",
      run: () => openLaunchLog(context.path!),
    });
  }
  if (info.logPath) {
    actions.push({
      label: "Open file",
      run: () => { void open(info.logPath!).catch(() => {}); },
    });
  }
  // A JVM crash produced a fresh latest.log / crash-report — jump straight into
  // the existing Crash Assistant report so the user can investigate without
  // hunting for a hidden log. The Live log action remains available alongside it.
  if (info.kind === "launch_crash") {
    actions.push({
      label: "Fix it",
      run: () => window.dispatchEvent(new Event("tuffbox:open-diagnostics")),
    });
    actions.push({
      label: "Share log",
      run: () => {
        const path = context?.path ?? get(projectPath);
        if (!path) {
          toasts.warning("Open a project to share the crash log");
          return;
        }
        void shareCrashLogWithFeedback(path);
      },
    });
  }
  if (info.kind === "java_missing") {
    actions.push({
      label: "Java settings",
      run: () => openLauncherSettings("java"),
    });
  }
  toasts.error(info.message, 16000, actions);
}

let crashListener: Promise<UnlistenFn> | null = null;
let processListeners: Promise<UnlistenFn[]> | null = null;
let progressListener: Promise<UnlistenFn> | null = null;

function ensureLaunchProgressListener(): Promise<UnlistenFn> {
  if (!progressListener) {
    progressListener = listen<LaunchProgressPayload>("launch-progress", (ev) => {
      if (!get(isLaunching)) return;
      const p = ev.payload ?? {};
      const phase = String(p.phase || "preparing");
      const message = String(p.message || "Launching…");
      const percent =
        typeof p.percent === "number" && Number.isFinite(p.percent)
          ? Math.max(0, Math.min(100, Math.round(p.percent)))
          : null;
      launchProgress.set({ phase, message, percent });
    });
  }
  return progressListener;
}

function parseCrashPayload(payload: LaunchCrashEvent | LaunchErrorInfo): LaunchCrashEvent {
  if (typeof payload === "object" && payload !== null && "error" in payload) {
    const event = payload as LaunchCrashEvent;
    return {
      id: event.id || event.path || get(projectPath) || "",
      profile: event.profile || "client",
      error: asLaunchError(event.error),
      exitCode: event.exitCode,
    };
  }
  // Compatibility with older desktop binaries that emitted LaunchErrorInfo
  // directly. New backend events always include the instance id.
  return {
    id: get(projectPath) ?? "",
    profile: "client",
    error: asLaunchError(payload),
  };
}

/** Register the global `launch-crashed` handler exactly once. */
export function registerLaunchCrashListener(): Promise<UnlistenFn> {
  void ensureLaunchProgressListener();
  if (!crashListener) {
    crashListener = listen<LaunchCrashEvent | LaunchErrorInfo>("launch-crashed", (event) => {
      const crash = parseCrashPayload(event.payload);
      const path = crash.id || get(projectPath);
      if (path) {
        failLaunchSession(path, crash.error);
        void reportSoftVerifyCrash(path);
        // Keep the live log modal open on the crashed session.
        openLaunchLog(path);
      }
      const remembered = path ? rememberedLaunches.get(path) : undefined;
      const retry = remembered
        ? () => void launchWithFeedback(remembered.params, remembered.options)
        : undefined;
      showLaunchError(crash.error, retry, { path });
    });
  }
  return crashListener;
}

/**
 * Keep running process truth and shared lifecycle sessions in sync with Tauri.
 * `process-exited` is deliberately event-driven; stats polling is optional
 * observability and never decides whether a Play button becomes Stop.
 */
export function registerProcessListeners(): Promise<UnlistenFn[]> {
  if (!processListeners) {
    processListeners = Promise.all([
      listen<LaunchLifecycleEvent>("launch-phase", (event) => {
        const lifecycle = event.payload;
        if (!lifecycle?.id) return;
        applyLaunchLifecycle(lifecycle);
        if (lifecycle.phase === "running" && lifecycle.pid != null && lifecycle.startedAt != null) {
          upsertRunning({
            id: lifecycle.id,
            pid: lifecycle.pid,
            profile: lifecycle.profile || "client",
            startedAt: lifecycle.startedAt,
          });
        } else if (lifecycle.phase === "exited") {
          removeRunning(lifecycle.id, {
            profile: lifecycle.profile,
            startedAt: lifecycle.startedAt,
            code: lifecycle.exitCode,
            stopped: lifecycle.stopped,
            error: lifecycle.error,
          });
        } else if (lifecycle.phase === "failed" && lifecycle.error) {
          failLaunchSession(lifecycle.id, lifecycle.error);
        }
      }),
      listen<RunningInstance>("process-started", (event) => {
        if (event.payload?.id) upsertRunning(event.payload);
      }),
      listen<ProcessExitedEvent>("process-exited", (event) => {
        const exited = event.payload;
        if (exited?.id) {
          removeRunning(exited.id, {
            profile: exited.profile,
            startedAt: exited.startedAt,
            code: exited.code,
            stopped: exited.stopped,
          });
        }
      }),
    ]);
  }
  return processListeners;
}

/** Used by controls that need a short phase label without maintaining local
 * booleans. Kept exported to make the shared pattern easy to reuse. */
export function isStartupPhase(phase: LaunchPhase | null | undefined): boolean {
  return phase != null && STARTUP_PHASES.has(phase);
}
