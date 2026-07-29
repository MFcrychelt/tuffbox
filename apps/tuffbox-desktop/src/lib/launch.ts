import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
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
import { get } from "svelte/store";

export type { LaunchErrorInfo };

export interface LaunchParams {
  path: string;
  /// "client" (default) | "server" | a custom profile id
  profile?: string;
  quickPlayType?: string | null;
  quickPlayValue?: string | null;
  /** One-shot memory override for smoke/low-end runs (does not mutate manifest). */
  memoryMbOverride?: number | null;
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
  if (profile === "server" && memoryMbOverride == null) {
    return invoke<LaunchResult>("launch_server", { path: params.path });
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
export async function launchWithFeedback(
  params: LaunchParams,
  opts?: { onStarted?: (r: LaunchResult) => void; showSuccess?: boolean; openLog?: boolean },
): Promise<LaunchResult | null> {
  lastLaunch = params;
  lastOnStarted = opts?.onStarted ?? null;
  const showLog = opts?.openLog !== false;
  if (showLog) openLaunchLog(params.path);
  isLaunching.set(true);
  try {
    const result = await doLaunch(params);
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
    showLaunchError(e, () => launchWithFeedback(params, opts));
    return null;
  } finally {
    isLaunching.set(false);
  }
}

/// Kill the Minecraft process for a project. Backend emits `process-exited`.
export async function killWithFeedback(path: string): Promise<boolean> {
  try {
    await invoke("kill_running_instance", { instanceId: path });
    removeRunning(path);
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

/// Keep `runningInstances` in sync with backend process-started / process-exited.
export function registerProcessListeners(): Promise<UnlistenFn[]> {
  if (!processListeners) {
    processListeners = Promise.all([
      listen<RunningInstance>("process-started", (event) => {
        upsertRunning(event.payload);
      }),
      listen<{ id: string; code?: number | null }>("process-exited", (event) => {
        if (event.payload?.id) removeRunning(event.payload.id);
      }),
    ]);
  }
  return processListeners;
}
