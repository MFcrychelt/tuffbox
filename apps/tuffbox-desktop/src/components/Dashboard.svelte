<script lang="ts">
  import { onMount } from "svelte";
  import {
    Play,
    Square,
    Plus,
    Settings,
    Workflow,
    LogIn,
    User,
    Package,
    GitGraph,
    Stethoscope,
    History,
    Puzzle,
    Sparkles,
    FolderOpen,
    HardDrive,
    Clock,
    Users,
    ShieldAlert,
  } from "@lucide/svelte";
  import HeadAvatar from "./HeadAvatar.svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-shell";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    authState,
    skinPath,
    newProjectOpen,
    isLaunching,
    runningInstances,
    isProjectRunning,
    loginTypeLabel,
    formatPlaytime,
    type RecentProject,
    type CapeProvider,
    type CapeCatalog,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import { launchWithFeedback, killWithFeedback, registerLaunchCrashListener } from "../lib/launch";
  import {
    fetchCrashFixBanner,
    rollbackLastCrashFix,
  } from "../lib/softVerify";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import MinecraftLogin from "./MinecraftLogin.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import SkinPreview3D from "./SkinPreview3D.svelte";
  import AccountManager from "./AccountManager.svelte";
  import InstanceHome from "./InstanceHome.svelte";
  import YoutubeFeed from "./YoutubeFeed.svelte";
  import DashboardInstancesSection from "./DashboardInstancesSection.svelte";

  let { currentView = $bindable() }: { currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "me" | "library" | "chats" | "world" } = $props();

  /** Home layout: 1 classic | 2 yt main + instances under skin | 3 yt under skin | 4 hide yt */
  type HomeLayout = "classic" | "yt-main" | "yt-under-skin" | "yt-hidden";
  const HOME_LAYOUT_KEY = "tuffbox-home-layout";
  const HOME_LAYOUT_OPTIONS: { id: HomeLayout; label: string }[] = [
    { id: "classic", label: "YouTube then instances" },
    { id: "yt-main", label: "YouTube main · instances under skin" },
    { id: "yt-under-skin", label: "Instances main · YouTube under skin" },
    { id: "yt-hidden", label: "Hide YouTube" },
  ];

  function loadHomeLayout(): HomeLayout {
    try {
      const v = localStorage.getItem(HOME_LAYOUT_KEY);
      if (v === "classic" || v === "yt-main" || v === "yt-under-skin" || v === "yt-hidden") {
        return v;
      }
    } catch {}
    return "classic";
  }

  let homeLayout = $state<HomeLayout>(loadHomeLayout());
  let authReady = $state(false);

  function setHomeLayout(next: HomeLayout) {
    homeLayout = next;
    try {
      localStorage.setItem(HOME_LAYOUT_KEY, next);
    } catch {}
  }

  function onHomeLayoutChange(e: Event) {
    const el = e.currentTarget;
    if (!(el instanceof HTMLSelectElement)) return;
    const v = el.value;
    if (HOME_LAYOUT_OPTIONS.some((o) => o.id === v)) {
      setHomeLayout(v as HomeLayout);
    }
  }

  type ProjectStatBrief = { playtime: number; lastLaunch: string | null };
  let projectStats = $state<Record<string, ProjectStatBrief>>({});

  async function loadProjectStats(path: string) {
    try {
      const s = await api.stats.get(path);
      projectStats[path] = {
        playtime: s.totalPlaytimeSeconds ?? 0,
        lastLaunch: s.lastLaunch ?? null,
      };
      projectStats = { ...projectStats };
    } catch {
      projectStats[path] = { playtime: 0, lastLaunch: null };
      projectStats = { ...projectStats };
    }
  }

  function ensureStats(paths: string[]) {
    for (const path of paths) {
      if (projectStats[path] !== undefined) continue;
      void loadProjectStats(path);
    }
  }

  $effect(() => {
    ensureStats($recentProjects.map((p) => p.path));
  });

  /** Last launched first; unknown lastLaunch keeps relative store order. */
  const sortedProjects = $derived([...$recentProjects].sort((a, b) => {
    const la = projectStats[a.path]?.lastLaunch;
    const lb = projectStats[b.path]?.lastLaunch;
    if (la && lb) return lb.localeCompare(la);
    if (la && !lb) return -1;
    if (!la && lb) return 1;
    return 0;
  }));

  let selectedPath = $state<string | null>($projectPath);
  let activeMenuPath = $state<string | null>(null);
  let menuAnchor = $state<HTMLElement | null>(null);
  let showLoginModal = $state(false);
  let showAccountManager = $state(false);
  let showWorldPrompt = $state(false);
  let worldPromptOptions = $state<string[]>([]);
  let worldPromptTarget = $state<RecentProject | null>(null);
  let showClonePrompt = $state(false);
  let clonePromptName = $state("");
  let cloneTarget = $state<RecentProject | null>(null);
  let capeCatalog = $state<CapeCatalog | null>(null);
  let capeBusy = $state(false);
  let mojangCapeMenuOpen = $state(false);
  let potatoPc = $state(false);
  const capeProviderOptions: { id: CapeProvider; label: string }[] = [
    { id: "mojang", label: "Mojang" },
    { id: "optifine", label: "OptiFine" },
    { id: "tlauncher", label: "TLauncher" },
    { id: "none", label: "None" },
  ];

  const selectedProject = $derived($recentProjects.find((p) => p.path === selectedPath));
  const selectedRunning = $derived(isProjectRunning(selectedPath, $runningInstances));
  const hasInstanceHome = $derived(!!(selectedPath && selectedProject));
  const skinUrl = $derived($authState.profile?.skinUrl ?? null);
  const capeUrl = $derived($authState.profile?.capeUrl ?? null);
  const accountKey = $derived($authState.activeAccountUuid ?? $authState.profile?.uuid ?? "");
  const mojangCapeOffers = $derived((capeCatalog?.offers ?? []).filter((o) => o.provider === "mojang"));
  const otherCapeOffers = $derived((capeCatalog?.offers ?? []).filter((o) => o.provider !== "mojang"));
  const canChangeMojangCape = $derived(
    $authState.loginType === "microsoft" && mojangCapeOffers.some((o) => o.canActivate),
  );

  type CrashFixBanner = {
    snapshotId: string;
    fingerprintKey: string;
    planSource?: string | null;
    humanExplanation: string;
    matchedCaseIds: string[];
    actionsSummary: string[];
    createdAt: string;
    resolved: boolean;
    rolledBack: boolean;
    softVerifyStartedUnix?: number | null;
    minPlaytimeSecs: number;
  };
  let crashFixBanner = $state<CrashFixBanner | null>(null);
  let crashFixBusy = $state(false);

  async function refreshCrashFixBanner(path: string | null) {
    if (!path) {
      crashFixBanner = null;
      return;
    }
    crashFixBanner = await fetchCrashFixBanner(path);
  }

  $effect(() => {
    void refreshCrashFixBanner(selectedPath);
  });

  async function onRollbackCrashFix() {
    if (!selectedPath || crashFixBusy) return;
    crashFixBusy = true;
    try {
      const ok = await rollbackLastCrashFix(selectedPath);
      if (ok) await refreshCrashFixBanner(selectedPath);
    } finally {
      crashFixBusy = false;
    }
  }

  async function refreshCapes() {
    if (!$authState.loggedIn || !$authState.profile) {
      capeCatalog = null;
      return;
    }
    try {
      capeCatalog = await api.mcAuth.listCapes();
    } catch {
      capeCatalog = null;
    }
  }

  async function selectCapeProvider(provider: CapeProvider) {
    if (capeBusy) return;
    capeBusy = true;
    try {
      const state = await api.mcAuth.setCapeProvider(provider);
      authState.set(state);
      await refreshCapes();
      // Only Mojang owns multiple switchable capes — open the change menu after catalog loads.
      mojangCapeMenuOpen =
        provider === "mojang" &&
        state.loginType === "microsoft" &&
        (capeCatalog?.offers ?? []).some((o) => o.provider === "mojang" && o.canActivate);
      toasts.success(`Cape: ${provider === "none" ? "hidden" : provider}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      capeBusy = false;
    }
  }

  async function activateMojangCape(capeId: string) {
    if (capeBusy) return;
    capeBusy = true;
    try {
      const state = await api.mcAuth.applyCape(capeId);
      authState.set(state);
      mojangCapeMenuOpen = true;
      if (state.profile) {
        try {
          skinPath.set(await api.mcAuth.getSkinPath(state.profile.uuid));
        } catch {}
      }
      await refreshCapes();
      toasts.success("Mojang cape activated");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      capeBusy = false;
    }
  }

  function openMojangCapeMenu() {
    mojangCapeMenuOpen = true;
    if (($authState.capeProvider ?? "mojang") !== "mojang") {
      void selectCapeProvider("mojang");
    }
  }

  let lastCapeRefreshKey = $state("");
  const capeRefreshKey = $derived(
    $authState.loggedIn
      ? `${$authState.activeAccountUuid ?? ""}:${$authState.capeProvider ?? "mojang"}`
      : "",
  );
  $effect(() => {
    if (capeRefreshKey && capeRefreshKey !== lastCapeRefreshKey) {
      lastCapeRefreshKey = capeRefreshKey;
      void refreshCapes();
    }
  });

  onMount(() => {
    let cleanup: (() => void) | undefined;
    void (async () => {
      potatoPc = document.documentElement.classList.contains("potato-pc");
      try {
        const status = await api.mcAuth.getAuthStatus();
        authState.set(status);
        if (status.loggedIn && status.profile) {
          try {
            const path = await api.mcAuth.getSkinPath(status.profile.uuid);
            skinPath.set(path);
          } catch {}
        }
      } catch {
      } finally {
        authReady = true;
      }

      if (selectedPath && !selectedProject && $recentProjects.length > 0) {
        selectProject($recentProjects[0].path);
      }

      // Global handler for JVM crashes that happen after the launch command
      // has returned "started" — surfaces a categorized, retryable toast.
      registerLaunchCrashListener();

      // Refresh playtime when a session ends.
      const { listen } = await import("@tauri-apps/api/event");
      const unlistenExit = await listen<{ id: string }>("process-exited", (event) => {
        const id = event.payload?.id;
        if (id) void loadProjectStats(id);
      });
      const unlistenSoft = await listen("tuffbox:soft-verify-outcome", () => {
        void refreshCrashFixBanner(selectedPath);
      });
      const onCrashFixApplied = () => {
        void refreshCrashFixBanner(selectedPath);
      };
      window.addEventListener("tuffbox:crash-fix-applied", onCrashFixApplied);
      cleanup = () => {
        unlistenExit();
        unlistenSoft();
        window.removeEventListener("tuffbox:crash-fix-applied", onCrashFixApplied);
      };
    })();
    return () => cleanup?.();
  });

  async function loadProject(path: string) {
    const info = await invoke("validate_project", { path }) as import("../lib/api").ProjectSummary;
    const manifestPath = info.manifestPath || path;
    const project: RecentProject = { path: manifestPath, info: info as any };
    recentProjects.add(project);
    projectPath.set(manifestPath);
    projectInfo.set(project.info);
    selectedPath = manifestPath;
  }

  function selectProject(path: string) {
    const project = $recentProjects.find((p) => p.path === path);
    if (project) {
      selectedPath = path;
      projectPath.set(path);
      projectInfo.set(project.info);
    }
    activeMenuPath = null;
  }

  async function launch() {
    if (!selectedPath) return;
    await invoke("set_last_opened_project", { path: selectedPath });
    await launchWithFeedback({ path: selectedPath, profile: "client" });
    const project = $recentProjects.find((p) => p.path === selectedPath);
    if (project) recentProjects.add(project);
    void loadProjectStats(selectedPath);
  }

  async function stopGame() {
    if (!selectedPath) return;
    await killWithFeedback(selectedPath);
  }

  function openSettings() {
    currentView = "project-settings";
  }

  function toggleMenu(event: MouseEvent, path: string) {
    event.stopPropagation();
    if (activeMenuPath === path) {
      activeMenuPath = null;
    } else {
      activeMenuPath = path;
      menuAnchor = event.currentTarget as HTMLElement;
    }
  }

  function closeMenu() {
    activeMenuPath = null;
  }

  let pinnedPaths = $state<Record<string, boolean>>({});
  let actionBusy = $state(false);

  async function togglePin(event: MouseEvent, projectPath: string) {
    event.stopPropagation();
    const isPinned = !pinnedPaths[projectPath];
    pinnedPaths[projectPath] = isPinned;
    pinnedPaths = { ...pinnedPaths };
    try {
      await api.session.pin(isPinned, projectPath);
    } catch {}
  }

  function ensurePins(paths: string[]) {
    let changed = false;
    for (const path of paths) {
      if (pinnedPaths[path] !== undefined) continue;
      pinnedPaths[path] = false;
      changed = true;
      api.session.isPinned(path).then((pinned) => {
        pinnedPaths[path] = pinned;
        pinnedPaths = { ...pinnedPaths };
      }).catch(() => {});
    }
    if (changed) pinnedPaths = { ...pinnedPaths };
  }

  let instanceSizes = $state<Record<string, string>>({});
  let loadingSizes = $state<Record<string, boolean>>({});

  async function loadSize(projectPath: string) {
    if (instanceSizes[projectPath] || loadingSizes[projectPath]) return;
    loadingSizes[projectPath] = true;
    try {
      instanceSizes[projectPath] = await api.instance.getSize(projectPath);
      instanceSizes = { ...instanceSizes };
    } catch {
      instanceSizes[projectPath] = "?";
    } finally {
      loadingSizes[projectPath] = false;
    }
  }

  function ensureSizes(paths: string[]) {
    for (const path of paths) loadSize(path);
  }

  $effect(() => {
    ensurePins($recentProjects.map((p) => p.path));
  });
  $effect(() => {
    ensureSizes($recentProjects.map((p) => p.path));
  });

  async function handleAction(action: string, project: RecentProject) {
    activeMenuPath = null;
    switch (action) {
      case "open-folder":
        await invoke("open_project_folder", { path: project.path });
        break;
      case "change-version":
        currentView = "project-settings";
        selectProject(project.path);
        break;
      case "server-pack":
        actionBusy = true;
        try {
          await invoke("export_server_pack", { path: project.path, targetPath: null });
          toasts.success(`Server pack exported.`);
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "links":
        actionBusy = true;
        try {
          const config: any = await invoke("get_publish_config", { path: project.path });
          const links: string[] = [];
          if (config?.modrinthProjectId) links.push(`https://modrinth.com/modpack/${config.modrinthProjectId}`);
          if (config?.curseforgeProjectId) links.push(`https://www.curseforge.com/minecraft/modpacks/${config.curseforgeProjectId}`);
          if (config?.githubRepository) links.push(`https://github.com/${config.githubRepository}/releases`);
          if (links.length === 0) toasts.info("No publish links yet.", 5000);
          else { await open(links[0]); toasts.success(`Opened ${links[0]}`); }
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "worlds":
        actionBusy = true;
        try {
          const worlds: any[] = await invoke("list_worlds", { path: project.path });
          if (worlds.length === 0) toasts.info("No worlds found.");
          else { toasts.info(`${worlds.length} world(s) found.`, 5000); }
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "backup-world":
        actionBusy = true;
        try {
          const worlds: any[] = await invoke("list_worlds", { path: project.path });
          if (worlds.length === 0) { toasts.info("No worlds to backup."); break; }
          worldPromptOptions = worlds.map((w: any) => w.name);
          worldPromptTarget = project;
          showWorldPrompt = true;
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "logs-zip":
        actionBusy = true;
        try {
          await invoke("create_logs_zip", { path: project.path });
          toasts.success(`Logs archive created.`);
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "copy-link":
        await navigator.clipboard.writeText(project.path);
        toasts.success("Path copied to clipboard");
        break;
      case "clone":
        clonePromptName = `${project.info.name} copy`;
        cloneTarget = project;
        showClonePrompt = true;
        break;
      case "share":
        actionBusy = true;
        try {
          const exported: any = await api.export.modrinthPack(null, project.path);
          await navigator.clipboard.writeText(exported.path);
          toasts.success(`Exported .mrpack: ${exported.path}`);
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "cleanup":
        actionBusy = true;
        try {
          const result: any = await invoke("cleanup_project", { path: project.path });
          toasts.success(`Cleaned ${result.count} files.`);
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "repair":
        actionBusy = true;
        try {
          const report: any = await invoke("repair_project", { path: project.path });
          const downloaded = report.downloaded?.length ?? 0;
          const failed = report.failed?.length ?? 0;
          toasts.success(
            downloaded === 0 && failed === 0
              ? "All mod files present and valid."
              : `Repaired: ${downloaded} file(s) re-downloaded${failed ? `, ${failed} failed` : ""}.`
          );
        } catch (e) { toasts.error(String(e)); }
        finally { actionBusy = false; }
        break;
      case "remove":
        recentProjects.remove(project.path);
        if (selectedPath === project.path) {
          selectedPath = $recentProjects[0]?.path ?? null;
          projectPath.set(selectedPath);
          projectInfo.set($recentProjects[0]?.info ?? null);
        }
        break;
      case "delete": {
        const ok = await confirm(`Delete "${project.info.name}"?`, { title: "Delete", kind: "warning" });
        if (!ok) break;
        try {
          await invoke("delete_project", { path: project.path });
          recentProjects.remove(project.path);
          if (selectedPath === project.path) {
            selectedPath = $recentProjects[0]?.path ?? null;
            projectPath.set(selectedPath);
            projectInfo.set($recentProjects[0]?.info ?? null);
          }
        } catch (e) { toasts.error(String(e)); }
        break;
      }
    }
  }

  async function handleLogout() {
    try {
      const state = await api.mcAuth.logout();
      authState.set(state);
      if (state.profile?.uuid) {
        try {
          skinPath.set(await api.mcAuth.getSkinPath(state.profile.uuid));
        } catch {
          skinPath.set(null);
        }
      } else {
        skinPath.set(null);
      }
      capeCatalog = null;
      toasts.info(state.loggedIn ? `Switched to ${state.profile?.name ?? "account"}` : "Logged out");
    } catch (e) {
      toasts.error(String(e));
    }
  }

  function gradientFrom(name: string) {
    const colors = ["#1bd96a", "#8b5cf6", "#3b82f6", "#f59e0b", "#ec4899", "#06b6d4", "#ef4444"];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
  }

  async function confirmBackupWorld(worldName: string) {
    showWorldPrompt = false;
    if (!worldPromptTarget) return;
    try {
      await invoke("backup_world", { path: worldPromptTarget.path, worldName });
      toasts.success(`World "${worldName}" backed up.`);
    } catch (e) { toasts.error(String(e)); }
  }

  async function confirmClone(newName: string) {
    showClonePrompt = false;
    if (!cloneTarget || !newName.trim()) return;
    actionBusy = true;
    try {
      const clonedPath = await invoke<string>("clone_project", { path: cloneTarget.path, newName: newName.trim() });
      const info = await invoke("validate_project", { path: clonedPath }) as import("../lib/api").ProjectSummary;
      const manifestPath = info.manifestPath || clonedPath;
      recentProjects.add({ path: manifestPath, info: info as any });
      toasts.success(`Cloned to: ${manifestPath}`);
    } catch (e) { toasts.error(String(e)); }
    finally { actionBusy = false; }
  }
</script>

<svelte:window onclick={closeMenu} />

<div class="home fade-slide-in">
  <!-- Top bar: Quick actions left, Avatar right -->
  <div class="top-bar">
    <div class="quick-nav">
      <button class="quick-action" onclick={() => (currentView = "mods")} title="Mods">
        <Package size={18} />
        <span>Mods</span>
      </button>
      <button class="quick-action" onclick={() => (currentView = "graph")} title="Dependency Graph">
        <GitGraph size={18} />
        <span>Graph</span>
      </button>
      <button class="quick-action" onclick={() => (currentView = "diagnostics")} title="Diagnostics">
        <Stethoscope size={18} />
        <span>Diagnostics</span>
      </button>
      <button class="quick-action" onclick={() => (currentView = "snapshots")} title="Snapshots">
        <History size={18} />
        <span>Snapshots</span>
      </button>
      {#if selectedProject}
        <button class="quick-action" onclick={() => (currentView = "recipes")} title="Recipes">
          <Puzzle size={18} />
          <span>Recipes</span>
        </button>
        <button class="quick-action" onclick={() => (currentView = "quests")} title="Quests">
          <Sparkles size={18} />
          <span>Quests</span>
        </button>
      {/if}
    </div>

    <!-- Account avatar in top-right (sign-in lives in the skin panel) -->
    <div class="account-avatar-section">
      {#if $authState.loggedIn && $authState.profile}
        <button class="account-avatar-btn" onclick={() => (currentView = "me")} title="Me — account & playtime">
          <HeadAvatar skinSrc={$skinPath} size={32} alt={$authState.profile.name} />
          <span class="avatar-name">{$authState.profile.name}</span>
          <span
            class="avatar-badge"
            class:microsoft={$authState.loginType === "microsoft"}
            class:offline={$authState.loginType === "offline"}
            class:ygg={$authState.loginType === "yggdrasil"}
          >
            {loginTypeLabel(
              $authState.loginType,
              $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority
            )}
          </span>
        </button>
      {/if}
    </div>
  </div>

  <div class="main-layout" data-layout={homeLayout}>
    <div class="home-main">
      <!-- Hero: Play button + project info -->
      <section class="hero">
        <div class="hero-left">
          <button
            class="play-btn"
            class:stop={selectedRunning && !$isLaunching}
            onclick={selectedRunning && !$isLaunching ? stopGame : launch}
            disabled={!selectedPath || $isLaunching}
          >
            {#if $isLaunching}
              <span class="spinner"></span>
              <span class="play-text">Launching...</span>
            {:else if selectedRunning}
              <Square size={24} fill="currentColor" />
              <span class="play-text">Stop</span>
            {:else}
              <Play size={28} fill="currentColor" />
              <span class="play-text">Play</span>
            {/if}
          </button>

          <div class="hero-main">
            {#if selectedProject}
              <div class="project-quick-info">
                <span class="project-name">{selectedProject.info.name}</span>
                <span class="project-version">{selectedProject.info.minecraftVersion} · {selectedProject.info.loaderKind}</span>
              </div>
            {:else}
              <div class="project-quick-info">
                <span class="project-name muted">No instance selected</span>
                <span class="project-hint">Select an instance below or create a new one</span>
              </div>
            {/if}

            <div class="hero-actions">
              {#if selectedProject}
                <button class="action-btn primary" onclick={() => (currentView = "ide")}>
                  <Workflow size={15} />
                  IDE
                </button>
                <button class="action-btn" onclick={openSettings}>
                  <Settings size={15} />
                  Settings
                </button>
                <button class="action-btn" onclick={() => invoke("open_project_folder", { path: selectedProject.path })}>
                  <FolderOpen size={15} />
                  Folder
                </button>
              {/if}
              <button class="action-btn accent" onclick={() => (newProjectOpen.set(true))}>
                <Plus size={15} />
                New
              </button>
            </div>

            {#if crashFixBanner}
              <div class="crash-fix-banner" role="status">
                <ShieldAlert size={16} />
                <div class="crash-fix-banner-body">
                  <strong>Crash fix applied</strong>
                  <span>
                    {crashFixBanner.softVerifyStartedUnix
                      ? `Soft-verify in progress (≥${crashFixBanner.minPlaytimeSecs}s stable play)…`
                      : "Launch to soft-verify. One-click restore available."}
                  </span>
                  {#if crashFixBanner.actionsSummary?.length}
                    <span class="crash-fix-actions">
                      {crashFixBanner.actionsSummary.slice(0, 3).join(" · ")}
                    </span>
                  {/if}
                </div>
                <button
                  class="action-btn"
                  type="button"
                  disabled={crashFixBusy}
                  onclick={onRollbackCrashFix}
                >
                  Restore snapshot
                </button>
                <button
                  class="action-btn"
                  type="button"
                  onclick={() => (currentView = "diagnostics")}
                >
                  <Stethoscope size={14} /> Diagnostics
                </button>
              </div>
            {/if}
          </div>
        </div>

        {#if selectedProject}
          <div class="hero-right">
            <div class="instance-stats">
              <div class="stat">
                <HardDrive size={14} />
                <span>{instanceSizes[selectedProject.path] || "..."}</span>
              </div>
              {#if projectStats[selectedProject.path]?.playtime}
                <div class="stat">
                  <Clock size={14} />
                  <span>{formatPlaytime(projectStats[selectedProject.path].playtime)}</span>
                </div>
              {:else if projectStats[selectedProject.path] === undefined}
                <div class="stat skel-stat" aria-hidden="true">
                  <span class="skeleton skeleton-block skeleton-line short" style="width: 52px; height: 12px;"></span>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </section>

      {#if hasInstanceHome && selectedPath}
        <InstanceHome
          projectPath={selectedPath}
          onOpenMods={() => (currentView = "mods")}
          onOpenWorld={() => (currentView = "world")}
        />
      {/if}

      {#if homeLayout !== "yt-hidden" && homeLayout !== "yt-under-skin"}
        <YoutubeFeed variant="row" />
      {/if}

      {#if homeLayout !== "yt-main"}
        <DashboardInstancesSection
          {homeLayout}
          {sortedProjects}
          {selectedPath}
          {instanceSizes}
          {loadingSizes}
          {projectStats}
          {pinnedPaths}
          {activeMenuPath}
          homeLayoutOptions={HOME_LAYOUT_OPTIONS}
          onHomeLayoutChange={onHomeLayoutChange}
          {selectProject}
          {toggleMenu}
          {togglePin}
          {handleAction}
          {gradientFrom}
        />
      {/if}
    </div>

    <aside class="home-side">
      <div class="skin-panel" aria-busy={!authReady}>
        {#if !authReady}
          <div class="skin-skel" aria-hidden="true">
            <div class="skin-skel-canvas skeleton skeleton-block skeleton-card"></div>
            <div class="skin-skel-footer">
              <span class="skeleton skeleton-block skeleton-round" style="width: 72px; height: 22px;"></span>
              <span class="skeleton skeleton-block skeleton-round" style="width: 88px; height: 28px;"></span>
            </div>
            <div class="skin-skel-name skeleton skeleton-block skeleton-line medium" style="width: 40%; height: 14px; margin: 0 auto 12px;"></div>
            <div class="skin-skel-cape">
              <span class="skeleton skeleton-block skeleton-line short" style="width: 90px; height: 10px; margin-bottom: 10px;"></span>
              <div class="skin-skel-cape-row home-skel-stagger">
                {#each Array(4) as _, i (i)}
                  <span class="skeleton skeleton-block skeleton-round" style={`--i: ${i}; width: 64px; height: 28px;`}></span>
                {/each}
              </div>
            </div>
          </div>
        {:else if $authState.loggedIn && $authState.profile}
          {#if potatoPc}
            <div class="skin-static-fallback">
              <HeadAvatar skinSrc={$skinPath} size={120} alt={$authState.profile.name} />
              <span class="skin-static-name">{$authState.profile.name}</span>
            </div>
          {:else}
          <SkinPreview3D
            skinUrl={skinUrl}
            capeUrl={capeUrl}
            accountKey={accountKey}
            playerName={$authState.profile.name}
            showName={false}
            width={300}
            height={homeLayout === "yt-under-skin" ? 280 : 400}
          />
          {/if}
          <div class="skin-panel-footer">
            <div class="skin-meta">
              <span
                class="type-badge"
                class:microsoft={$authState.loginType === "microsoft"}
                class:offline={$authState.loginType === "offline"}
                class:ygg={$authState.loginType === "yggdrasil"}
              >
                {loginTypeLabel(
                  $authState.loginType,
                  $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority
                )}
              </span>
            </div>
            <button class="change-skin-btn" onclick={() => (showAccountManager = true)}>
              <Users size={14} />
              {$authState.accounts.length > 1
                ? `${$authState.accounts.length} accounts`
                : "Accounts"}
            </button>
          </div>
          <div class="skin-player-name" title={$authState.profile.name}>
            {$authState.profile.name}
          </div>

          <div class="cape-panel">
            <div class="cape-row-label">Cape provider</div>
            <div class="cape-provider-grid">
              {#each capeProviderOptions as opt (opt.id)}
                <button
                  type="button"
                  class="cape-provider-btn"
                  class:active={($authState.capeProvider ?? "mojang") === opt.id}
                  disabled={capeBusy}
                  onclick={() => selectCapeProvider(opt.id)}
                >
                  {opt.label}
                </button>
              {/each}
            </div>

            {#if canChangeMojangCape}
              <div class="cape-mojang-actions">
                <button
                  type="button"
                  class="cape-activate"
                  disabled={capeBusy}
                  onclick={() => (mojangCapeMenuOpen ? (mojangCapeMenuOpen = false) : openMojangCapeMenu())}
                >
                  {mojangCapeMenuOpen ? "Hide cape menu" : "Show cape"}
                </button>
              </div>
            {/if}

            {#if mojangCapeMenuOpen && canChangeMojangCape}
              <div class="cape-row-label">Change Mojang cape</div>
              <div class="cape-offers">
                {#each mojangCapeOffers as offer (offer.id)}
                  <div class="cape-offer" class:active={offer.active}>
                    <img src={offer.url} alt={offer.label} class="cape-thumb" />
                    <div class="cape-offer-info">
                      <strong>{offer.label}</strong>
                      <span>mojang</span>
                    </div>
                    <button
                      class="cape-activate"
                      disabled={capeBusy || offer.active}
                      onclick={() => activateMojangCape(offer.id)}
                    >
                      {offer.active ? "Active" : "Equip"}
                    </button>
                  </div>
                {/each}
              </div>
            {:else if mojangCapeOffers.length && !canChangeMojangCape}
              <div class="cape-row-label">Mojang cape</div>
              <div class="cape-offers">
                {#each mojangCapeOffers as offer (offer.id)}
                  <div
                    class="cape-offer"
                    class:active={($authState.capeProvider ?? "mojang") === "mojang"}
                  >
                    <img src={offer.url} alt={offer.label} class="cape-thumb" />
                    <div class="cape-offer-info">
                      <strong>{offer.label}</strong>
                      <span>mojang</span>
                    </div>
                    {#if ($authState.capeProvider ?? "mojang") !== "mojang"}
                      <button
                        class="cape-activate"
                        disabled={capeBusy}
                        onclick={() => selectCapeProvider("mojang")}
                      >
                        Show
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}

            {#if otherCapeOffers.length}
              <div class="cape-row-label">Other sources</div>
              <div class="cape-offers">
                {#each otherCapeOffers as offer (offer.provider + offer.id)}
                  <div
                    class="cape-offer"
                    class:active={($authState.capeProvider ?? "mojang") === offer.provider}
                  >
                    <img src={offer.url} alt={offer.label} class="cape-thumb" />
                    <div class="cape-offer-info">
                      <strong>{offer.label}</strong>
                      <span>{offer.provider}</span>
                    </div>
                    {#if ($authState.capeProvider ?? "mojang") !== offer.provider}
                      <button
                        class="cape-activate"
                        disabled={capeBusy}
                        onclick={() => selectCapeProvider(offer.provider)}
                      >
                        Show
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {:else if !mojangCapeOffers.length}
              <p class="cape-empty">No capes found for this username on the selected sources.</p>
            {/if}
          </div>
        {:else}
          <div class="skin-panel-empty">
            <User size={48} />
            <p>Sign in to see your skin</p>
            <button class="action-btn accent" onclick={() => (showLoginModal = true)}>
              <LogIn size={16} />
              Sign In
            </button>
          </div>
        {/if}
      </div>
      {#if homeLayout === "yt-under-skin"}
        <div class="skin-rail-youtube">
          <YoutubeFeed variant="rail" />
        </div>
      {/if}

      {#if homeLayout === "yt-main"}
        <DashboardInstancesSection
          {homeLayout}
          {sortedProjects}
          {selectedPath}
          {instanceSizes}
          {loadingSizes}
          {projectStats}
          {pinnedPaths}
          {activeMenuPath}
          homeLayoutOptions={HOME_LAYOUT_OPTIONS}
          onHomeLayoutChange={onHomeLayoutChange}
          {selectProject}
          {toggleMenu}
          {togglePin}
          {handleAction}
          {gradientFrom}
          sideColumn={true}
        />
      {/if}
    </aside>
  </div>
</div>

{#if showLoginModal}
  <MinecraftLogin onclose={() => (showLoginModal = false)} />
{/if}

{#if showAccountManager}
  <AccountManager onclose={() => (showAccountManager = false)} />
{/if}

{#if $newProjectOpen}
  <AddInstanceModal
    onclose={() => (newProjectOpen.set(false))}
    oncreated={(path) => loadProject(path)}
  />
{/if}

{#if showWorldPrompt}
  <PromptDialog
    title="Backup World"
    message="Select a world to back up."
    mode="select"
    options={worldPromptOptions}
    defaultValue={worldPromptOptions[0]}
    confirmLabel="Backup"
    onconfirm={(v) => confirmBackupWorld(v)}
    oncancel={() => (showWorldPrompt = false)}
  />
{/if}

{#if showClonePrompt}
  <PromptDialog
    title="Clone Instance"
    message="Enter a name for the cloned instance."
    mode="text"
    defaultValue={clonePromptName}
    confirmLabel="Clone"
    onconfirm={(v) => confirmClone(v)}
    oncancel={() => (showClonePrompt = false)}
  />
{/if}

<style>
  .home {
    max-width: 1400px;
    margin: 0 auto;
  }

  /* ─── Top Bar ─────────────────────────────────────── */
  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
    gap: 16px;
  }

  .quick-nav {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .quick-action {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    transition: all 0.15s ease;
  }

  .quick-action:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--bg-hover);
  }

  /* ─── Account Avatar ─────────────────────────────── */
  .account-avatar-section {
    flex-shrink: 0;
  }

  .account-avatar-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 6px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    cursor: pointer;
    transition: all 0.15s;
  }

  .account-avatar-btn:hover {
    border-color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.04);
  }

  .avatar-name {
    font-family: var(--font-minecraft);
    font-weight: 400;
    font-size: 10px;
    letter-spacing: 0.4px;
    color: var(--text-primary);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .avatar-badge {
    font-size: 9px;
    font-weight: 800;
    padding: 1px 4px;
    border-radius: 3px;
    text-transform: uppercase;
  }

  .avatar-badge.microsoft {
    color: #00a4ef;
    background: rgba(0, 164, 239, 0.12);
  }

  .avatar-badge.offline {
    color: var(--text-muted);
    background: var(--bg-hover);
  }

  .avatar-badge.ygg {
    color: #e9d5ff;
    background: rgba(168, 85, 247, 0.15);
  }

  /* ─── Main Layout (2-column stack) ─── */
  .main-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    align-items: start;
    gap: 24px;
  }

  .home-main {
    display: flex;
    flex-direction: column;
    gap: 24px;
    min-width: 0;
  }

  .home-side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 320px;
    max-width: 100%;
    position: sticky;
    top: 20px;
    max-height: calc(100vh - 40px);
    overflow-y: auto;
    align-self: start;
  }

  /*
   * yt-under-skin: skin + YouTube share the sticky column.
   * Don't nest a column scrollbar on top of the page scroll + feed scroll —
   * let the page scroll, keep the rail feed at natural height.
   */
  .main-layout[data-layout="yt-under-skin"] .home-side {
    max-height: none;
    overflow: visible;
  }

  .skin-rail-youtube {
    min-width: 0;
    flex-shrink: 0;
  }

  .main-layout[data-layout="yt-under-skin"] .skin-rail-youtube {
    padding-top: 0;
  }

  .skin-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    /* Don't let yt-under-skin rail compress the 3D preview (overflow clips the model). */
    flex-shrink: 0;
  }

  /* Keep the canvas frame from being flexed/squashed inside the panel. */
  .skin-panel :global(.skin-3d-wrap),
  .skin-panel :global(.skin-3d-container) {
    flex-shrink: 0;
  }

  .skin-panel-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-top: 1px solid var(--border-color);
    gap: 8px;
  }

  .skin-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .type-badge {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 3px 7px;
    border-radius: 6px;
  }
  .type-badge.microsoft {
    color: #93c5fd;
    background: rgba(59, 130, 246, 0.15);
    border: 1px solid rgba(59, 130, 246, 0.35);
  }
  .type-badge.offline {
    color: #fde68a;
    background: rgba(245, 158, 11, 0.12);
    border: 1px solid rgba(245, 158, 11, 0.3);
  }
  .type-badge.ygg {
    color: #e9d5ff;
    background: rgba(168, 85, 247, 0.15);
    border: 1px solid rgba(168, 85, 247, 0.35);
  }

  .skin-player-name {
    font-family: var(--font-minecraft);
    font-weight: 400;
    font-size: 12px;
    line-height: 1.4;
    letter-spacing: 0.5px;
    color: var(--text-primary);
    text-shadow:
      2px 2px 0 color-mix(in srgb, var(--text-primary) 18%, #3f3f3f),
      -1px 0 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      1px 0 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      0 -1px 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      0 1px 0 color-mix(in srgb, var(--bg-primary) 70%, #000);
    text-align: center;
    padding: 0 16px 12px;
    margin-top: -4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .change-skin-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border-radius: var(--border-radius-sm);
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .change-skin-btn:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .cape-panel {
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .cape-row-label {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .cape-provider-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .cape-provider-btn {
    padding: 7px 4px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 700;
    cursor: pointer;
  }
  .cape-provider-btn.active {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
  }
  .cape-provider-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .cape-mojang-actions { display: flex; }
  .cape-offers { display: flex; flex-direction: column; gap: 6px; max-height: 180px; overflow: auto; }
  .cape-offer {
    display: flex; align-items: center; gap: 10px;
    padding: 8px; border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color); background: var(--bg-primary);
  }
  .cape-offer.active { border-color: var(--accent-primary); }
  .cape-thumb {
    width: 36px; height: 28px; object-fit: contain;
    image-rendering: pixelated; background: #111; border-radius: 4px;
  }
  .cape-offer-info { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .cape-offer-info strong { font-size: 12px; color: var(--text-primary); }
  .cape-offer-info span { font-size: 10px; color: var(--text-muted); text-transform: uppercase; }
  .cape-activate {
    padding: 5px 8px; border-radius: 6px; border: 1px solid var(--border-color);
    background: var(--bg-elevated); color: var(--text-secondary);
    font-size: 11px; font-weight: 700; cursor: pointer;
  }
  .cape-activate:hover:not(:disabled) { border-color: var(--accent-primary); color: var(--accent-primary); }
  .cape-activate:disabled { opacity: 0.55; cursor: default; }
  .cape-empty { margin: 0; font-size: 11px; color: var(--text-muted); }

  .skin-panel-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 60px 24px;
    text-align: center;
    color: var(--text-muted);
  }

  .skin-panel-empty p {
    font-size: 13px;
  }

  .skin-static-fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px 24px;
    min-height: 400px;
    background: var(--bg-primary);
  }

  .skin-static-name {
    font-family: var(--font-minecraft);
    font-size: 12px;
    letter-spacing: 0.5px;
    color: var(--text-primary);
  }

  .skin-skel {
    display: flex;
    flex-direction: column;
    width: 100%;
  }

  .skin-skel-canvas {
    width: 100%;
    height: 400px;
    border-radius: 0;
  }

  .skin-skel-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-top: 1px solid var(--border-color);
    gap: 8px;
  }

  .skin-skel-cape {
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border-color);
  }

  .skin-skel-cape-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .skel-stat {
    display: inline-flex;
    align-items: center;
  }

  /* ─── Hero ────────────────────────────────────────── */
  .hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 32px;
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.06), rgba(139, 92, 246, 0.04));
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    margin-bottom: 0;
    gap: 24px;
  }

  .hero-left {
    display: flex;
    align-items: center;
    gap: 16px 24px;
    min-width: 0;
    flex: 1;
    flex-wrap: wrap;
  }

  .hero-main {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }

  .hero-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .instance-stats {
    display: flex;
    gap: 8px;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-secondary);
    padding: 6px 10px;
    border-radius: var(--border-radius-sm);
  }

  .play-btn {
    width: 160px;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    font-size: 18px;
    border-radius: var(--border-radius-lg);
    box-shadow: 0 8px 24px rgba(27, 217, 106, 0.3);
    padding: 0 24px;
    flex-shrink: 0;
  }

  .play-btn:hover {
    box-shadow: 0 12px 32px rgba(27, 217, 106, 0.4);
  }

  .play-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }

  .play-btn.stop {
    background: var(--accent-danger, #ef4444);
    box-shadow: 0 8px 24px rgba(239, 68, 68, 0.3);
  }

  .play-btn.stop:hover {
    box-shadow: 0 12px 32px rgba(239, 68, 68, 0.4);
  }

  .play-text {
    font-weight: 800;
  }

  .project-quick-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    max-width: 420px;
  }

  .project-name {
    font-weight: 700;
    font-size: 15px;
    line-height: 1.3;
    color: var(--text-primary);
  }

  .project-name.muted {
    color: var(--text-muted);
  }

  .project-version {
    font-size: 12px;
    line-height: 1.35;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  /* Empty-state copy — do not Title-Case the sentence. */
  .project-hint {
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-muted);
    text-transform: none;
  }

  .hero-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }

  .crash-fix-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 10px;
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--accent-primary, #1bd96a) 35%, transparent);
    background: color-mix(in srgb, var(--accent-primary, #1bd96a) 10%, var(--bg-secondary));
    max-width: 560px;
  }

  .crash-fix-banner-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 160px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .crash-fix-banner-body strong {
    color: var(--text-secondary);
    font-size: 13px;
  }

  .crash-fix-actions {
    opacity: 0.85;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: var(--border-radius-md);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .action-btn.primary {
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .action-btn.primary:hover {
    background: rgba(27, 217, 106, 0.1);
  }

  .action-btn.accent {
    background: var(--accent-primary);
    color: #000;
    border-color: transparent;
  }

  .action-btn.accent:hover {
    background: var(--accent-hover);
  }

  .spinner {
    width: 20px;
    height: 20px;
    border: 2.5px solid rgba(0, 0, 0, 0.15);
    border-top-color: #000;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
