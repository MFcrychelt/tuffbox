import { writable } from "svelte/store";

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
