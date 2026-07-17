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
export type LoginType = "microsoft" | "offline";

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
}

export interface AuthState {
  loggedIn: boolean;
  profile: McProfile | null;
  expiresAt: number | null;
  loginType: LoginType;
  skinSource: SkinSource;
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
  accounts: [],
  activeAccountUuid: null,
});

export const skinPath = writable<string | null>(null);
