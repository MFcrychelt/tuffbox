import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { toasts } from "./toast";
import type { LaunchResult, LaunchErrorInfo } from "./api";

export type { LaunchErrorInfo };

export interface LaunchParams {
  path: string;
  /// "client" (default) | "server" | a custom profile id
  profile?: string;
  quickPlayType?: string | null;
  quickPlayValue?: string | null;
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
  if (params.quickPlayType || params.quickPlayValue) {
    return invoke<LaunchResult>("launch_with_quick_play", {
      path: params.path,
      profile,
      quickPlayType: params.quickPlayType ?? null,
      quickPlayValue: params.quickPlayValue ?? null,
    });
  }
  if (profile === "server") {
    return invoke<LaunchResult>("launch_server", { path: params.path });
  }
  return invoke<LaunchResult>("launch_profile", {
    path: params.path,
    profile,
  });
}

/// Launch a profile and surface a categorized, optionally-retryable toast on
/// failure. Returns the `LaunchResult` on success, or `null` after the error
/// toast has been shown.
export async function launchWithFeedback(
  params: LaunchParams,
  opts?: { onStarted?: (r: LaunchResult) => void; showSuccess?: boolean },
): Promise<LaunchResult | null> {
  lastLaunch = params;
  lastOnStarted = opts?.onStarted ?? null;
  try {
    const result = await doLaunch(params);
    if (opts?.showSuccess) toasts.success("Launch started");
    opts?.onStarted?.(result);
    // After a successful start, offer to share a recent crash→fix capsule (swarm opt-in).
    void (async () => {
      try {
        const prompt = await invoke<{
          fingerprintKey?: string;
          humanExplanation?: string;
        } | null>("get_share_prompt_after_launch", { path: params.path });
        if (prompt) {
          window.dispatchEvent(
            new CustomEvent("tuffbox:share-capsule", {
              detail: { path: params.path, marker: prompt },
            }),
          );
        }
      } catch {
        // ignore — share is optional
      }
    })();
    return result;
  } catch (e) {
    showLaunchError(e, () => launchWithFeedback(params, opts));
    return null;
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
      label: "View log",
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
      label: "Diagnose",
      run: () => {
        window.dispatchEvent(new Event("tuffbox:open-diagnostics"));
      },
    });
  }
  toasts.error(info.message, 12000, actions);
}

let crashListener: Promise<UnlistenFn> | null = null;

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
