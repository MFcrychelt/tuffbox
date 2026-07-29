import { writable } from "svelte/store";
import type { RunningInstance } from "./api";

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
    add: (project: RecentProject) => {
      update((projects) => {
        const filtered = projects.filter((p) => p.path !== project.path);
        const next = [project, ...filtered].slice(0, 20);
        try {
          localStorage.setItem("recentProjects", JSON.stringify(next));
        } catch {}
        return next;
      });
    },
    updateInfo: (path: string, info: ProjectInfo) => {
      update((projects) => {
        const next = projects.map((p) =>
          p.path === path ? { ...p, info } : p
        );
        try {
          localStorage.setItem("recentProjects", JSON.stringify(next));
        } catch {}
        return next;
      });
    },
    remove: (path: string) => {
      update((projects) => {
        const next = projects.filter((p) => p.path !== path);
        try {
          localStorage.setItem("recentProjects", JSON.stringify(next));
        } catch {}
        return next;
      });
    },
    set,
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
  /** Hide IDE bottom workflow rail until cursor hits the window bottom edge. */
  autoHideWorkflowRail: boolean;
  /** Left nav: full labels | icons (button toggle) | autoHide (left-edge hover). */
  sidebarMode: SidebarMode;
  /** Interface zoom percent (75–150). */
  uiScalePercent: number;
}

export type SidebarMode = "full" | "icons" | "autoHide";

export function normalizeSidebarMode(raw: unknown): SidebarMode {
  if (raw === "icons" || raw === "autoHide" || raw === "full") return raw;
  return "full";
}

export function normalizeUiScalePercent(raw: unknown): number {
  const n = typeof raw === "number" ? raw : Number(raw);
  if (!Number.isFinite(n)) return 100;
  return Math.min(150, Math.max(75, Math.round(n)));
}

/** Apply Chromium zoom via CSS variable (buttons, cards, modals). */
export function applyUiScale(percent: unknown) {
  const p = normalizeUiScalePercent(percent);
  if (typeof document === "undefined") return p;
  document.documentElement.style.setProperty("--ui-scale", String(p / 100));
  return p;
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

// Global launch state — true while a launch is in progress.
// Used by Header to show spinner, and by Dashboard to disable play button.
export const isLaunching = writable<boolean>(false);

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

export function openLaunchLog(path: string) {
  launchLogPath.set(path);
}

export function closeLaunchLog() {
  launchLogPath.set(null);
}

/** One-shot: open IDE on this stage id (e.g. "content" = Mods). Cleared by IdeWorkspace. */
export const ideStageRequest = writable<string | null>(null);

/** Live mirror of launcherSettings.autoHideWorkflowRail for IDE rail. */
export const autoHideWorkflowRail = writable(false);

/** Live mirror of launcherSettings.sidebarMode. */
export const sidebarMode = writable<SidebarMode>("full");

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
