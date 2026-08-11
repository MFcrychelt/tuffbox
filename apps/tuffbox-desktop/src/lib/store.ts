import { get, writable } from "svelte/store";
import type { RunningInstance } from "./api";
import { api } from "./api";

export interface ProjectInfo {
  id: string;
  name: string;
  version: string;
  minecraftVersion: string;
  loaderKind: string;
  loaderVersion: string;
  javaPath: string | null;
  memoryMb: number;
  jvmArgs: string[];
  playerName: string;
}

export interface RecentProject {
  path: string;
  info: ProjectInfo;
}

function persistRecent(projects: RecentProject[]) {
  try {
    localStorage.setItem("recentProjects", JSON.stringify(projects));
  } catch {
    /* ignore quota / private mode */
  }
  // Disk copy survives WebView2 profile wipes (EBWebView clears).
  void api.session.saveRecentProjects(projects).catch(() => {});
}

function createRecentProjects() {
  let initial: RecentProject[] = [];
  try {
    const stored = typeof window !== "undefined" ? localStorage.getItem("recentProjects") : null;
    if (stored) initial = JSON.parse(stored);
  } catch {
    initial = [];
  }
  const { subscribe, set, update } = writable<RecentProject[]>(initial);

  return {
    subscribe,
    add: (project: RecentProject, opts?: { reorder?: boolean; replacePath?: string }) => {
      const reorder = opts?.reorder !== false;
      const replacePath = opts?.replacePath;
      update((projects) => {
        const matchIdx = projects.findIndex(
          (p) =>
            p.path === project.path ||
            (replacePath != null && p.path === replacePath),
        );
        let next: RecentProject[];
        if (!reorder && matchIdx >= 0) {
          next = projects
            .map((p, i) => (i === matchIdx ? project : p))
            .filter((p, i) => i === matchIdx || p.path !== project.path);
        } else if (!reorder) {
          const filtered = projects.filter(
            (p) =>
              p.path !== project.path &&
              (replacePath == null || p.path !== replacePath),
          );
          next = [...filtered, project].slice(0, 20);
        } else {
          const filtered = projects.filter(
            (p) =>
              p.path !== project.path &&
              (replacePath == null || p.path !== replacePath),
          );
          next = [project, ...filtered].slice(0, 20);
        }
        persistRecent(next);
        return next;
      });
    },
    updateInfo: (path: string, info: ProjectInfo) => {
      update((projects) => {
        const next = projects.map((p) =>
          p.path === path ? { ...p, info } : p
        );
        persistRecent(next);
        return next;
      });
    },
    remove: (path: string) => {
      update((projects) => {
        const next = projects.filter((p) => p.path !== path);
        persistRecent(next);
        return next;
      });
    },
    set: (projects: RecentProject[]) => {
      persistRecent(projects);
      set(projects);
    },
    /** Merge disk-backed list after WebView localStorage was wiped. */
    async hydrateFromDisk() {
      try {
        const disk = await api.session.loadRecentProjects();
        if (!disk?.length) return;
        update((projects) => {
          if (projects.length > 0) {
            // Keep local order; fill gaps from disk.
            const have = new Set(projects.map((p) => p.path));
            const merged = [...projects];
            for (const p of disk) {
              if (!have.has(p.path)) merged.push(p);
            }
            const next = merged.slice(0, 20);
            persistRecent(next);
            return next;
          }
          const next = disk.slice(0, 20);
          persistRecent(next);
          return next;
        });
      } catch {
        /* offline / old binary */
      }
    },
  };
}

export const recentProjects = createRecentProjects();
export const projectPath = writable<string | null>(null);
export const projectInfo = writable<ProjectInfo | null>(null);

// ─── Minecraft Auth ──────────────────────────────────────────────

export type SkinSource = "mojang" | "elyby" | "tlauncher" | "offline";
export type LoginType = "microsoft" | "offline" | "yggdrasil";
export type CapeProvider = "mojang" | "optifine" | "tlauncher" | "none";

export interface McCapeEntry {
  id: string;
  alias: string | null;
  url: string;
  state: string;
}

export interface McProfile {
  uuid: string;
  name: string;
  skinUrl: string | null;
  capeUrl: string | null;
  capes: McCapeEntry[];
}

export interface AccountEntry {
  uuid: string;
  name: string;
  loginType: LoginType;
  skinSource: SkinSource;
  addedAt: number;
  /** Yggdrasil / authlib-injector API root (Ely.by, LittleSkin, custom). */
  authority?: string | null;
  /** Microsoft OAuth backend for token refresh (`azure` | `live`). */
  msOauthBackend?: "azure" | "live" | null;
}

export interface YggdrasilPreset {
  id: string;
  label: string;
  authority: string;
}

export interface PresenceSettings {
  discordRpcEnabled: boolean;
  discordClientId: string;
}

export interface GameResolution {
  width: number;
  height: number;
}

export interface LauncherSettings {
  theme: string;
  potatoPc: boolean;
  /** Set once the one-time weak-hardware check (see detectWeakHardware) has run. */
  perfAutoDetected: boolean;
  concurrentDownloads: number;
  gameResolution: GameResolution | null;
  preLaunchHook: string | null;
  postExitHook: string | null;
  wrapperCommand: string | null;
  runtimePath: string | null;
  instancesPath: string | null;
  defaultJavaPath: string | null;
  javaCustomArgs: string | null;
  defaultMemoryMb: number;
  /** Litube-style in-app player (default on). false = thumbnail preview → system browser. */
  youtubeInlinePlayer: boolean;
  /** Inject the in-game overlay bridge (YouTube player + friends/chat) on launch. */
  ingameOverlay: boolean;
  /** Hide IDE bottom workflow rail until cursor hits the window bottom edge. */
  autoHideWorkflowRail: boolean;
  /** Left nav: full labels | icons (button toggle) | autoHide (left-edge hover). */
  sidebarMode: SidebarMode;
  /** Interface zoom percent (75–150). */
  uiScalePercent: number;
  /** `auto` follows screen/window size; `manual` locks uiScalePercent. */
  uiScaleMode: UiScaleMode;
  /** Round corners on panels/cards/buttons everywhere. */
  roundedCorners: boolean;
  /** Hide InstanceHome (mods/packs/worlds) preview block on the home dashboard. */
  hideInstanceHome: boolean;
}

export type SidebarMode = "full" | "icons" | "autoHide";
export type UiScaleMode = "auto" | "manual";

export const UI_SCALE_STEPS = [75, 90, 100, 110, 125, 150] as const;

/** Live applied UI zoom percent — App binds `data-ui-scaled` / zoom from this. */
export const uiScalePercentLive = writable(100);

export function normalizeSidebarMode(raw: unknown): SidebarMode {
  if (raw === "icons" || raw === "autoHide" || raw === "full") return raw;
  return "full";
}

export function normalizeUiScalePercent(raw: unknown): number {
  const n = typeof raw === "number" ? raw : Number(raw);
  if (!Number.isFinite(n)) return 100;
  return Math.min(150, Math.max(75, Math.round(n)));
}

export function normalizeUiScaleMode(raw: unknown): UiScaleMode {
  return raw === "manual" ? "manual" : "auto";
}

/** Migrate unset mode: preserve non-100% as manual. */
export function resolveUiScaleMode(settings: {
  uiScaleMode?: unknown;
  uiScalePercent?: unknown;
}): UiScaleMode {
  const raw = settings.uiScaleMode;
  if (raw === "auto" || raw === "manual") return raw;
  return normalizeUiScalePercent(settings.uiScalePercent) !== 100 ? "manual" : "auto";
}

function snapUiScalePercent(raw: number): number {
  let best = UI_SCALE_STEPS[0];
  let bestDist = Math.abs(raw - best);
  for (const step of UI_SCALE_STEPS) {
    const d = Math.abs(raw - step);
    if (d < bestDist) {
      best = step;
      bestDist = d;
    }
  }
  return best;
}

/**
 * Suggest UI zoom from screen/window size + devicePixelRatio.
 * Snaps to the same chips used in Settings (75–150).
 *
 * Tuned for laptop panels (1366×768, 1440×900, 1600×900) that feel cramped at 100%.
 */
export function suggestUiScalePercent(): number {
  if (typeof window === "undefined") return 100;
  const screenW = window.screen?.availWidth || window.screen?.width || 1920;
  const screenH = window.screen?.availHeight || window.screen?.height || 1080;
  const innerW = window.innerWidth || screenW;
  const innerH = window.innerHeight || screenH;
  // Tighter of screen vs window — a small window on a big monitor still needs denser UI.
  const width = Math.min(screenW, innerW);
  const height = Math.min(screenH, innerH);
  const dpr = window.devicePixelRatio || 1;
  const shortSide = Math.min(width, height);

  let suggested = 100;
  // Tiny / phone-like or very short height
  if (width < 1100 || height < 720 || shortSide < 700) suggested = 125;
  // Classic laptops: 1366×768, 1400×900, 1440×900, 1600×900
  else if (width <= 1440 || height <= 900) suggested = 110;
  else if (dpr >= 2 && width < 1800) suggested = 110;
  else if (width < 1600 && height < 1000) suggested = 110;
  else if (width >= 1920 && height >= 1080 && dpr <= 1) suggested = 100;
  else suggested = 100;

  return snapUiScalePercent(suggested);
}

/** Current CSS zoom factor from `--ui-scale` (1 = 100%). */
export function getUiScale(): number {
  if (typeof document === "undefined") return 1;
  const el = document.documentElement as HTMLElement & { currentCSSZoom?: number };
  if (typeof el.currentCSSZoom === "number" && el.currentCSSZoom > 0) {
    return el.currentCSSZoom;
  }
  const raw = getComputedStyle(el).getPropertyValue("--ui-scale").trim();
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? n : 1;
}

/**
 * Map viewport (clientX/Y) → CSS `position: fixed` left/top.
 * Zoom lives on `html`, so Chromium uses one coordinate frame — return as-is.
 * Kept for callers / WebKit edge cases that report visual px vs layout px.
 */
export function viewportToShellFixed(clientX: number, clientY: number): { x: number; y: number } {
  return { x: clientX, y: clientY };
}

/** Apply Chromium zoom on <html> so fixed menus, drag, and hit-tests share one coord space. */
export function applyUiScale(percent: unknown) {
  const p = normalizeUiScalePercent(percent);
  if (typeof document === "undefined") {
    uiScalePercentLive.set(p);
    return p;
  }
  const scale = p / 100;
  const root = document.documentElement;
  root.style.setProperty("--ui-scale", String(scale));
  uiScalePercentLive.set(p);

  // Zoom on html (not .app-shell): shell-only zoom desyncs clientX from
  // getBoundingClientRect / fixed overlays and breaks Library drag + context menus.
  if (p === 100) {
    root.removeAttribute("data-ui-scaled");
    root.style.removeProperty("zoom");
  } else {
    root.setAttribute("data-ui-scaled", "1");
    root.style.zoom = String(scale);
  }

  // Clear legacy shell zoom from older sessions / HMR.
  const shell = document.querySelector(".app-shell");
  if (shell instanceof HTMLElement) {
    shell.removeAttribute("data-ui-scaled");
  }
  return p;
}

/** Resolve mode + apply (auto → suggest, manual → stored percent). */
export function applyUiScaleFromSettings(settings: {
  uiScaleMode?: unknown;
  uiScalePercent?: unknown;
}): number {
  const mode = resolveUiScaleMode(settings);
  const pct =
    mode === "auto" ? suggestUiScalePercent() : normalizeUiScalePercent(settings.uiScalePercent);
  return applyUiScale(pct);
}

/**
 * Cheap, synchronous heuristic for "this machine is probably weak" — used
 * once on first launch to decide whether to auto-enable potato-pc (reduced
 * motion) mode. Deliberately conservative (only obviously low-end hardware
 * trips it) since it can only ever be overridden by the user afterwards.
 *
 * `navigator.deviceMemory` is Chromium-only (undefined on the WebKit
 * webview Tauri uses on macOS/Linux) so it's treated as an optional signal,
 * not a requirement.
 */
export function detectWeakHardware(): boolean {
  if (typeof navigator === "undefined") return false;
  const cores = navigator.hardwareConcurrency;
  if (typeof cores === "number" && cores > 0 && cores <= 2) return true;
  const memoryGb = (navigator as Navigator & { deviceMemory?: number }).deviceMemory;
  if (typeof memoryGb === "number" && memoryGb > 0 && memoryGb <= 2) return true;
  return false;
}

/** Toggle rounded-corners appearance mode (`data-rounded-corners` on <html>). */
export function applyRoundedCorners(enabled: unknown) {
  const on = enabled !== false;
  if (typeof document === "undefined") return on;
  document.documentElement.setAttribute("data-rounded-corners", on ? "on" : "off");
  try {
    localStorage.setItem("tuffbox-rounded-corners", on ? "1" : "0");
  } catch {
    /* ignore */
  }
  return on;
}

/** Human label for account provider badges. */
export function loginTypeLabel(type: LoginType, authority?: string | null): string {
  if (type === "microsoft") return "Mojang";
  if (type === "offline") return "Offline";
  const a = (authority ?? "").toLowerCase();
  if (a.includes("ely.by")) return "Ely.by";
  if (a.includes("littleskin")) return "LittleSkin";
  return "Yggdrasil";
}

export function formatPlaytime(secs: number): string {
  const s = Math.max(0, Math.floor(secs || 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

export interface CapeOffer {
  provider: CapeProvider;
  id: string;
  label: string;
  url: string;
  canActivate: boolean;
  active: boolean;
}

export interface CapeCatalog {
  selectedProvider: CapeProvider;
  displayUrl: string | null;
  offers: CapeOffer[];
}

export interface AuthState {
  loggedIn: boolean;
  profile: McProfile | null;
  expiresAt: number | null;
  loginType: LoginType;
  skinSource: SkinSource;
  capeProvider: CapeProvider;
  accounts: AccountEntry[];
  activeAccountUuid: string | null;
}

export interface DeviceCodeInfo {
  userCode: string;
  verificationUri: string;
  message: string;
  expiresIn: number;
  /** Suggested poll interval in seconds (from Microsoft). */
  interval: number;
}

export const authState = writable<AuthState>({
  loggedIn: false,
  profile: null,
  expiresAt: null,
  loginType: "offline",
  skinSource: "mojang",
  capeProvider: "mojang",
  accounts: [],
  activeAccountUuid: null,
});

export const skinPath = writable<string | null>(null);

// ─── UI / navigation state ───────────────────────────────────────

// Drives the "New instance" (AddInstanceModal) from anywhere in the app,
// including the sidebar's + button which lives outside the Dashboard tree.
export const newProjectOpen = writable<boolean>(false);

/** Global Minecraft login modal (Home skin panel, Play auth-gate, etc.). */
export const loginModalOpen = writable<boolean>(false);

/** Mode for AddInstanceModal when `newProjectOpen` is set. `"catalog"` redirects to Library Discover. */
export type AddInstanceMode = "blank" | "import" | "catalog";
export const addInstanceMode = writable<AddInstanceMode>("blank");

/** Open a Library tab (`discover` / `yours` / `create`). Cleared by Library when applied. */
export const libraryTabRequest = writable<"yours" | "discover" | "create" | null>(null);

export function openAddInstance(mode: AddInstanceMode = "blank") {
  if (mode === "catalog") {
    libraryTabRequest.set("discover");
    if (typeof window !== "undefined") {
      window.dispatchEvent(new CustomEvent("tuffbox:open-library"));
    }
    return;
  }
  addInstanceMode.set(mode);
  newProjectOpen.set(true);
}

/** Deep-link into Settings (consumed once by Settings.svelte). */
export type SettingsNavRequest = {
  tab: "appearance" | "launcher" | "ai" | "integrations" | "about";
  launcherSub?: "general" | "java" | "commands" | "runtime";
};
export const settingsNavRequest = writable<SettingsNavRequest | null>(null);

/** Open Settings → Launcher (optionally a sub-tab such as Java). */
export function openLauncherSettings(
  launcherSub: NonNullable<SettingsNavRequest["launcherSub"]> = "general",
) {
  settingsNavRequest.set({ tab: "launcher", launcherSub });
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent("tuffbox:open-settings"));
  }
}

// Global launch state — true while a launch is in progress.
// Used by Header to show spinner, and by Dashboard to disable play button.
export const isLaunching = writable<boolean>(false);

/** Structured launch phase while `isLaunching` (Java → mods → install → starting). */
export type LaunchProgressState = {
  phase: string;
  message: string;
  percent: number | null;
};

export const launchProgress = writable<LaunchProgressState | null>(null);

/** Currently running Minecraft processes, keyed by project manifest path (`id`). */
export const runningInstances = writable<RunningInstance[]>([]);

export function isProjectRunning(path: string | null | undefined, list: RunningInstance[]): boolean {
  if (!path) return false;
  return list.some((r) => r.id === path);
}

export function upsertRunning(inst: RunningInstance) {
  runningInstances.update((list) => {
    const without = list.filter((r) => r.id !== inst.id);
    return [...without, inst];
  });
}

export function removeRunning(id: string) {
  runningInstances.update((list) => list.filter((r) => r.id !== id));
}

/** Opens the live launch-log modal for the given project manifest path. */
export const launchLogPath = writable<string | null>(null);
export const launchLogTitle = writable<string | null>(null);

export function openLaunchLog(path: string, title?: string | null) {
  launchLogTitle.set(title ?? null);
  launchLogPath.set(path);
}

export function closeLaunchLog() {
  launchLogPath.set(null);
  launchLogTitle.set(null);
}

/** One-shot: open IDE on this stage id (e.g. "content" = Mods). Cleared by IdeWorkspace. */
export const ideStageRequest = writable<string | null>(null);

/** Live IDE stage while IdeWorkspace is mounted (for left-nav active highlight). */
export const ideActiveStage = writable<string | null>(null);

/** Suggested IDE stage when opening from Home (updated by IdeNextBar / diagnostics refresh). */
export const ideSuggestedStage = writable<string>("content");

/** Blocking pack issues count (missing deps + conflicts) for Home badge / Next bar. */
export const ideIssueCount = writable(0);

/** Optional crash/needs-fix hint for Next Action priority. */
export const ideNeedsHealth = writable(false);

export type WorkTrailAction = {
  id: string;
  label: string;
  /** Navigate to IDE stage, launch test, or dismiss. */
  kind: "stage" | "play" | "dismiss";
  stage?: string;
};

export type WorkTrail = {
  message: string;
  actions: WorkTrailAction[];
  createdAt: number;
  /** True after ignore-timeout escalation to pack problems / verify nudge. */
  escalated?: boolean;
};

/** How long a contextual work trail may sit ignored before escalating. */
export const WORK_TRAIL_ESCALATE_MS = 5 * 60 * 1000;

/** Contextual continue strip after Content / Resolve / Diagnose mutations. */
export const workTrail = writable<WorkTrail | null>(null);

export function pushWorkTrail(
  message: string,
  actions: WorkTrailAction[],
  opts?: { escalated?: boolean },
) {
  workTrail.set({
    message,
    actions,
    createdAt: Date.now(),
    escalated: opts?.escalated ?? false,
  });
}

export function clearWorkTrail() {
  workTrail.set(null);
}

/**
 * If a work trail was ignored long enough, replace it with the current
 * main pack problems (or a soft Test-launch nudge when the graph is clean).
 * Returns true when the trail was replaced.
 */
export function escalateIgnoredWorkTrail(opts: {
  issueCount: number;
  needsHealth: boolean;
}): boolean {
  const current = get(workTrail);
  if (!current || current.escalated) return false;
  if (Date.now() - current.createdAt < WORK_TRAIL_ESCALATE_MS) return false;

  if (opts.issueCount > 0) {
    pushWorkTrail(
      `${opts.issueCount} pack issue${opts.issueCount === 1 ? "" : "s"} still need attention`,
      [
        { id: "resolve", label: "Fix in Resolve", kind: "stage", stage: "resolve" },
        { id: "test", label: "Test launch", kind: "play" },
        { id: "dismiss", label: "Dismiss", kind: "dismiss" },
      ],
      { escalated: true },
    );
    return true;
  }

  if (opts.needsHealth) {
    pushWorkTrail(
      "Crash still needs a Health check",
      [
        { id: "diagnose", label: "Open Health", kind: "stage", stage: "diagnose" },
        { id: "test", label: "Test launch", kind: "play" },
        { id: "dismiss", label: "Dismiss", kind: "dismiss" },
      ],
      { escalated: true },
    );
    return true;
  }

  pushWorkTrail(
    "Still waiting — verify with a Test launch?",
    [
      { id: "test", label: "Test launch", kind: "play" },
      { id: "dismiss", label: "Dismiss", kind: "dismiss" },
    ],
    { escalated: true },
  );
  return true;
}

/** Bump to ask IdeNextBar to run its Next action (Ctrl+Enter). */
export const ideNextTrigger = writable(0);

export function requestIdeNextAction() {
  ideNextTrigger.update((n) => n + 1);
}

/** Bump to refresh issue counts on IdeNextBar. */
export const ideIssuesRefresh = writable(0);

export function requestIdeIssuesRefresh() {
  ideIssuesRefresh.update((n) => n + 1);
}

/** Bump to ask IdeNextBar to run Play (Ctrl+Shift+P). */
export const idePlayTrigger = writable(0);

export function requestIdePlay() {
  idePlayTrigger.update((n) => n + 1);
}

/** Recent IDE stages / commands for the command palette (max 5). */
export const ideRecentCommands = writable<{ id: string; label: string }[]>([]);

export function pushIdeRecent(id: string, label: string) {
  ideRecentCommands.update((list) => {
    const next = [{ id, label }, ...list.filter((x) => x.id !== id)];
    return next.slice(0, 5);
  });
}

/** Deterministic Next Action for IdeNextBar / Open IDE. */
export function computeIdeNextAction(opts: {
  issueCount: number;
  needsHealth: boolean;
  briefDirty: boolean;
  tuneDirty: boolean;
  questDirty: boolean;
}): { label: string; stage: string | null; kind: "stage" | "none"; detail?: string } {
  if (opts.issueCount > 0) {
    return {
      label: "Fix pack graph",
      stage: "resolve",
      kind: "stage",
      detail: `${opts.issueCount} issue${opts.issueCount === 1 ? "" : "s"}`,
    };
  }
  if (opts.needsHealth) {
    return { label: "Open Health", stage: "diagnose", kind: "stage", detail: "Crash needs a look" };
  }
  if (opts.briefDirty) {
    return { label: "Finish Brief", stage: "brief", kind: "stage", detail: "Unsaved listing" };
  }
  if (opts.tuneDirty) {
    return { label: "Save Tune", stage: "configs", kind: "stage", detail: "Unsaved configs" };
  }
  if (opts.questDirty) {
    return { label: "Save Quests", stage: "quests", kind: "stage", detail: "Unsaved quests" };
  }
  return { label: "Open Launch", stage: "test", kind: "stage", detail: "Verify the pack in Test" };
}

/** One-shot: open Quests AI sidebar on this quest chat session id. Cleared by QuestAiSidebar. */
export const questChatFocusId = writable<string | null>(null);

/** One-shot: open Tune Config AI sidebar on this tune chat session id. Cleared by TuneAiSidebar. */
export const tuneChatFocusId = writable<string | null>(null);

/** Focus a History event id after navigating to History stage. Cleared by ChangeHistory. */
export const historyFocusEventId = writable<string | null>(null);

/** Focus History episode by crash fingerprint (cleared by ChangeHistory). */
export const historyFocusFingerprintKey = writable<string | null>(null);

/** Focus History entries linked to this snapshot id (cleared by ChangeHistory). */
export const historyFocusSnapshotId = writable<string | null>(null);

/** History → Content (Mods): focus mod by manifest id/slug. Cleared by Mods. */
export const modsFocusId = writable<string | null>(null);

/** History → Content (Mods): fallback match by jar fileName. Cleared by Mods. */
export const modsFocusFileName = writable<string | null>(null);

/** History → Tune (ConfigEditor): open relative config path. Cleared by ConfigEditor. */
export const configFocusPath = writable<string | null>(null);

/** Optional paths to highlight when opening Diagnose from History. */
export const diagnoseFocusPaths = writable<string[] | null>(null);

/** Rich History → Diagnose handoff (paths + crash fingerprint / log). Cleared by Diagnostics. */
export type DiagnoseFocus = {
  paths?: string[] | null;
  fingerprintKey?: string | null;
  logPath?: string | null;
  episodeId?: string | null;
};
export const diagnoseFocus = writable<DiagnoseFocus | null>(null);

/** True while Tune/ConfigEditor has unsaved edits — IdeWorkspace confirms leave. */
export const tuneDirty = writable(false);

/** True while Brief listing editor has unsaved edits — IdeWorkspace confirms leave. */
export const briefDirty = writable(false);

/** True while Quests editor has unsaved edits — IdeWorkspace confirms leave. */
export const questDirty = writable(false);

/** Live mirror of launcherSettings.autoHideWorkflowRail for IDE rail. */
export const autoHideWorkflowRail = writable(false);

/** Live mirror of launcherSettings.sidebarMode. */
export const sidebarMode = writable<SidebarMode>("full");

/** Latest launcher settings — updated when Settings persists (keeps App auto-scale in sync). */
export const launcherSettingsLive = writable<LauncherSettings | null>(null);

export function notifyLauncherSettingsChanged(settings: LauncherSettings) {
  launcherSettingsLive.set(settings);
  if (typeof window !== "undefined") {
    window.dispatchEvent(
      new CustomEvent("tuffbox:launcher-settings", { detail: settings }),
    );
  }
}

/** Icons mode only: true = labels hidden (icon rail). Persisted in localStorage. */
function createSidebarIconsCollapsed() {
  let initial = false;
  try {
    initial =
      typeof localStorage !== "undefined" &&
      localStorage.getItem("tuffbox.sidebar.icons-collapsed") === "true";
  } catch {
    initial = false;
  }
  const { subscribe, set, update } = writable(initial);
  return {
    subscribe,
    set: (v: boolean) => {
      try {
        localStorage.setItem("tuffbox.sidebar.icons-collapsed", String(v));
      } catch {
        /* ignore */
      }
      set(v);
    },
    toggle: () =>
      update((v) => {
        const next = !v;
        try {
          localStorage.setItem("tuffbox.sidebar.icons-collapsed", String(next));
        } catch {
          /* ignore */
        }
        return next;
      }),
  };
}
export const sidebarIconsCollapsed = createSidebarIconsCollapsed();

/** App chrome brand mark: classic amber "T" or creeper-in-a-box. */
export type BrandIconId = "classic" | "creeper";

const BRAND_ICON_KEY = "tuffbox.brand-icon";

function readBrandIcon(): BrandIconId {
  try {
    const v = localStorage.getItem(BRAND_ICON_KEY);
    if (v === "creeper" || v === "classic") return v;
  } catch {
    /* ignore */
  }
  return "classic";
}

function createBrandIcon() {
  const { subscribe, set } = writable<BrandIconId>(
    typeof window !== "undefined" ? readBrandIcon() : "classic",
  );
  return {
    subscribe,
    set: (id: BrandIconId) => {
      try {
        localStorage.setItem(BRAND_ICON_KEY, id);
      } catch {
        /* ignore */
      }
      set(id);
    },
  };
}

export const brandIcon = createBrandIcon();

export const BRAND_ICON_CREEPER_SRC = "/brand/creeper-box.png";
export const BRAND_ICON_CREEPER_SRC_SM = "/brand/creeper-box-128.png";

/** Global YouTube player session — survives view switches so mini player stays on any page. */
export type YoutubePlayerSession = {
  videoId: string;
  title: string;
  originRect: DOMRect | null;
  startMini: boolean;
};

export const youtubePlayerSession = writable<YoutubePlayerSession | null>(null);

export function openYoutubePlayer(session: YoutubePlayerSession) {
  youtubePlayerSession.set(session);
}

export function closeYoutubePlayer() {
  youtubePlayerSession.set(null);
}
