import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { writable, get } from "svelte/store";
import { toasts } from "./toast";
import type { LaunchResult, LaunchErrorInfo, RunningInstance } from "./api";
import {
  isLaunching,
  openLaunchLog,
  projectPath,
  runningInstances,
  upsertRunning,
  removeRunning,
} from "./store";
import { shareCrashLogWithFeedback } from "./mclogs";
import { reportSoftVerifyCrash } from "./softVerify";

export type { LaunchErrorInfo };

export interface LaunchParams {
  path: string;
  /// "client" (default) | "server" | a custom profile id
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

// ─── Shared launch state machine ─────────────────────────────────────
//
// The backend exposes an explicit lifecycle through Tauri events so every Play
// button in the app can render the same accurate state instead of each keeping
// its own local `launching` flag that is reset the instant `invoke` returns
// (BUG_REPORT Bug 2). Phases flow:
//
//   (user clicks Play)
//     → "preparing"   (launchWithFeedback starts the invoke)
//     → "resolving_java" / "downloading"  (optional, from `launch-phase` events)
//     → "starting"    (JVM spawn begins)
//     → "running"     (`process-started` / `launch-phase` running)
//     → "exited"      (`process-exited` / `launch-crashed`)
//
// A path is considered "launching" for phases preparing…starting and stays that
// way until the backend confirms `running` or the run ends — never reset by the
// return of the invoke alone.

export type LaunchPhase =
  | "idle"
  | "preparing"
  | "resolving_java"
  | "downloading"
  | "starting"
  | "running"
  | "exited";

export interface LaunchPhaseState {
  path: string;
  phase: LaunchPhase;
  message: string | null;
  /** Set on launch failure / crash (kept for deep-link UI). */
  error?: LaunchErrorInfo | null;
  /** The LaunchResult when the invoke succeeded (running/exited). */
  result?: LaunchResult | null;
}

/** Per-path launch phase map, keyed by manifest path (`running.id`). */
export const launchStates = writable<Record<string, LaunchPhaseState>>({});

/**
 * The single path whose launch is currently in a pre-run phase
 * (preparing…starting). Cleared once the game is `running` or `exited`.
 * This is the shared replacement for the per-component `launching` flags.
 */
export const launchingPath = writable<string | null>(null);

const LAUNCHING_PHASES: ReadonlySet<LaunchPhase> = new Set<LaunchPhase>([
  "preparing",
  "resolving_java",
  "downloading",
  "starting",
]);

export function isLaunchPhase(phase: LaunchPhase | null | undefined): boolean {
  return phase != null && LAUNCHING_PHASES.has(phase);
}

/** True while `path` is in a pre-run phase (preparing…starting). */
export function isPathLaunching(
  path: string | null | undefined,
  states?: Record<string, LaunchPhaseState>,
): boolean {
  if (!path) return false;
  const s = (states ?? get(launchStates))[path];
  return s ? isLaunchPhase(s.phase) : false;
}

export function setLaunchPhase(
  path: string,
  phase: LaunchPhase,
  message?: string | null,
  result?: LaunchResult | null,
): void {
  launchStates.update((map) => {
    const prev = map[path];
    return {
      ...map,
      [path]: {
        path,
        phase,
        message: message ?? prev?.message ?? null,
        error: prev?.error ?? null,
        result: result ?? prev?.result ?? null,
      },
    };
  });
}

export function setLaunchError(path: string, error: LaunchErrorInfo): void {
  launchStates.update((map) => ({
    ...map,
    [path]: {
      path,
      phase: "exited",
      message: error.message,
      error,
      result: map[path]?.result ?? null,
    },
  }));
}

/** Mark a run as running (game is up) — clears the launching flag. */
function markRunning(path: string): void {
  setLaunchPhase(path, "running", "Running");
  launchingPath.update((p) => (p === path ? null : p));
  if (get(launchingPath) === null) isLaunching.set(false);
}

/** Mark a run as exited — clears the launching flag for this path. */
function markExited(path: string): void {
  setLaunchPhase(path, "exited", "Exited");
  launchingPath.update((p) => (p === path ? null : p));
  if (get(launchingPath) === null) isLaunching.set(false);
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

export function isLaunchError(e: unknown): e is LaunchErrorInfo {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e
  );
}

function isRetryable(info: LaunchErrorInfo): boolean {
  return RETRYABLE.has(info.kind);
}

// Remember the last launch so the crash listener can offer a Retry for a JVM
// that started but then exited non-zero after the launch command returned.
let lastLaunch: LaunchParams | null = null;
let lastOnStarted: ((r: LaunchResult) => void) | null = null;

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
      throw { kind: "install", message: "Pick a server folder before Run server." };
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

/// Launch a profile and surface a categorized, optionally-retryable toast on
/// failure. Returns the `LaunchResult` on success, or `null` after the error
/// toast has been shown.
///
/// The `launching`/`isLaunching` state is **not** reset here — it is driven by
/// the backend lifecycle events (`process-started` / `process-exited` /
/// `launch-phase` / `launch-crashed`) so the spinner stays up until the game is
/// actually running or the run has ended/failed. Callers must not clear it in a
/// `finally` block.
export async function launchWithFeedback(
  params: LaunchParams,
  opts?: {
    onStarted?: (r: LaunchResult) => void;
    showSuccess?: boolean;
    openLog?: boolean;
    /** Manifest path for the log modal (e.g. staged server dir). */
    logPath?: string | null;
    logTitle?: string | null;
  },
): Promise<LaunchResult | null> {
  lastLaunch = params;
  lastOnStarted = opts?.onStarted ?? null;
  const showLog = opts?.openLog !== false;
  if (showLog) openLaunchLog(opts?.logPath ?? params.path, opts?.logTitle ?? null);
  // Enter the pre-run launch phase. Kept until process-started / exit events.
  launchingPath.set(params.path);
  isLaunching.set(true);
  setLaunchPhase(params.path, "preparing", "Preparing…");
  try {
    const result = await doLaunch(params);
    // Successfully spawned — the game is starting. Keep `launching` true until
    // the backend confirms `running` via process-started, or the run ends.
    // Only advance a pre-run phase to "starting"; never downgrade a phase the
    // backend already advanced (the process-started event may beat the invoke).
    const cur = get(launchStates)[params.path];
    if (cur && isLaunchPhase(cur.phase)) {
      setLaunchPhase(params.path, "starting", "Starting game…", result);
    } else if (cur) {
      setLaunchPhase(params.path, cur.phase, cur.message, result);
    }
    if (opts?.showSuccess) toasts.success("Launch started");
    opts?.onStarted?.(result);
    // After a successful start, confirm any pending crash-fix as resolved when
    // latest.log looks healthy. On verified resolution the backend emits
    // `tuffbox:distill-resolution` for the Confirm → publish UI (no auto-upload).
    void (async () => {
      try {
        await invoke("confirm_crash_resolution_after_launch", { path: params.path });
      } catch {
        // optional bookkeeping
      }
    })();
    return result;
  } catch (e) {
    // Invoke failed before the process started — this path is no longer launching.
    const info: LaunchErrorInfo = isLaunchError(e)
      ? e
      : ({ kind: "unknown", message: String(e) } as LaunchErrorInfo);
    setLaunchError(params.path, info);
    launchingPath.update((p) => (p === params.path ? null : p));
    if (get(launchingPath) === null) isLaunching.set(false);
    showLaunchError(e, () => launchWithFeedback(params, opts));
    return null;
  }
}

/// Kill the Minecraft process for a project. Backend emits `process-exited`
/// (which clears `launching` via the shared state machine).
export async function killWithFeedback(path: string): Promise<boolean> {
  try {
    await invoke("kill_running_instance", { instanceId: path });
    removeRunning(path);
    markExited(path);
    toasts.info("Game stopped");
    return true;
  } catch (e) {
    toasts.error(`Stop failed: ${e}`);
    return false;
  }
}

export async function refreshRunningInstances(): Promise<void> {
  try {
    const list = await invoke<RunningInstance[]>("list_running_instances");
    runningInstances.set(Array.isArray(list) ? list : []);
    // Reconcile phase map with the source of truth: any running id should read
    // "running"; anything no longer in the list that was running → exited.
    const ids = new Set(Array.isArray(list) ? list.map((r) => r.id) : []);
    const states = get(launchStates);
    for (const id of ids) {
      const s = states[id];
      if (!s || s.phase === "running") continue;
      setLaunchPhase(id, "running", "Running");
    }
    for (const id of Object.keys(states)) {
      if (ids.has(id)) continue;
      if (states[id] && isLaunchPhase(states[id].phase)) {
        setLaunchPhase(id, "exited", "Exited");
      }
    }
  } catch {
    // backend not ready / optional
  }
}

/// Display a launch error as a toast with Retry / View log actions when
/// appropriate.
export function showLaunchError(e: unknown, retry?: () => void): void {
  const info: LaunchErrorInfo = isLaunchError(e)
    ? e
    : ({ kind: "unknown", message: String(e) } as LaunchErrorInfo);
  const actions: { label: string; run: () => void }[] = [];
  if (retry && isRetryable(info)) {
    actions.push({ label: "Retry", run: retry });
  }
  if (info.logPath) {
    actions.push({
      label: "Open log",
      run: () => {
        open(info.logPath as string).catch(() => {});
      },
    });
  }
  // A JVM crash produced a fresh latest.log / crash-report — jump straight into
  // the existing Crash Assistant report for the project so the user can read
  // the structured findings and apply a fix without re-navigating.
  if (info.kind === "launch_crash") {
    actions.push({
      label: "Fix it",
      run: () => {
        window.dispatchEvent(new Event("tuffbox:open-diagnostics"));
      },
    });
    actions.push({
      label: "Share log",
      run: () => {
        const path = get(projectPath);
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
      run: () => {
        window.dispatchEvent(new Event("tuffbox:open-project-settings"));
      },
    });
  }
  toasts.error(info.message, 16000, actions);
}

let crashListener: Promise<UnlistenFn> | null = null;
let processListeners: Promise<UnlistenFn[]> | null = null;

/// Register the global `launch-crashed` handler exactly once. The JVM can exit
/// non-zero after the launch command has already returned "started", so the
/// backend emits this event from the process-exit callback.
export function registerLaunchCrashListener(): Promise<UnlistenFn> {
  if (!crashListener) {
    crashListener = listen<LaunchErrorInfo>("launch-crashed", (event) => {
      const info = event.payload;
      const path = lastLaunch?.path ?? get(projectPath);
      if (path) {
        setLaunchError(path, info);
        launchingPath.update((p) => (p === path ? null : p));
        if (get(launchingPath) === null) isLaunching.set(false);
        void reportSoftVerifyCrash(path);
      }
      const retry = lastLaunch
        ? () =>
            launchWithFeedback(
              lastLaunch!,
              lastOnStarted ? { onStarted: lastOnStarted } : undefined,
            )
        : undefined;
      showLaunchError(info, retry);
    });
  }
  return crashListener;
}

/// Keep `runningInstances` + the shared launch state machine in sync with
/// backend process-started / process-exited / launch-phase events.
export function registerProcessListeners(): Promise<UnlistenFn[]> {
  if (!processListeners) {
    processListeners = Promise.all([
      listen<RunningInstance>("process-started", (event) => {
        const inst = event.payload;
        if (!inst?.id) return;
        upsertRunning(inst);
        markRunning(inst.id);
      }),
      listen<{ id: string; code?: number | null }>("process-exited", (event) => {
        if (!event.payload?.id) return;
        removeRunning(event.payload.id);
        markExited(event.payload.id);
      }),
      listen<{
        path?: string;
        id?: string;
        phase?: string;
        message?: string | null;
      }>("launch-phase", (event) => {
        const p = event.payload;
        const id = p?.path ?? p?.id;
        const phase = (p?.phase ?? "").toLowerCase();
        if (!id || !phase) return;
        const known: Record<string, LaunchPhase> = {
          preparing: "preparing",
          resolving_java: "resolving_java",
          downloading: "downloading",
          starting: "starting",
          running: "running",
          exited: "exited",
        };
        const mapped = known[phase];
        if (!mapped) return;
        if (mapped === "running") {
          markRunning(id);
        } else if (mapped === "exited") {
          markExited(id);
        } else {
          setLaunchPhase(id, mapped, p?.message ?? null);
          // Keep the launching flag aligned for backend-driven pre-run phases.
          launchingPath.set(id);
          isLaunching.set(true);
        }
      }),
    ]);
  }
  return processListeners;
}
