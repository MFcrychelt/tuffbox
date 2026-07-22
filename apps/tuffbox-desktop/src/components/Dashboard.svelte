<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    Play,
    Plus,
    Settings,
    MoreVertical,
    Pin,
    Folder,
    Trash2,
    Eraser,
    Copy,
    Link2,
    Wrench,
    Share2,
    GitBranch,
    FileArchive,
    Download,
    Globe,
    ShieldAlert,
    Terminal,
    Minus,
    Workflow,
    LogIn,
    LogOut,
    User,
    Package,
    GitGraph,
    Stethoscope,
    History,
    Puzzle,
    Sparkles,
    FolderOpen,
    RefreshCw,
    ChevronRight,
    HardDrive,
    Palette,
  } from "lucide-svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-shell";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { recentProjects, projectPath, projectInfo, authState, skinPath, newProjectOpen, isLaunching, loginTypeLabel, type RecentProject, type CapeProvider, type CapeCatalog } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import { launchWithFeedback, registerLaunchCrashListener } from "../lib/launch";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import LaunchLogModal from "./LaunchLogModal.svelte";
  import MinecraftLogin from "./MinecraftLogin.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import SkinPreview3D from "./SkinPreview3D.svelte";
  import AccountManager from "./AccountManager.svelte";
  import InstanceHome from "./InstanceHome.svelte";

  export let currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "me" | "library" | "world";

  let selectedPath: string | null = $projectPath;
  let activeMenuPath: string | null = null;
  let menuAnchor: HTMLElement | null = null;
  let showLogModal = false;
  let showLoginModal = false;
  let showAccountManager = false;
  let showWorldPrompt = false;
  let worldPromptOptions: string[] = [];
  let worldPromptTarget: RecentProject | null = null;
  let showClonePrompt = false;
  let clonePromptName = "";
  let cloneTarget: RecentProject | null = null;
  let capeCatalog: CapeCatalog | null = null;
  let capeBusy = false;
  let mojangCapeMenuOpen = false;
  const capeProviderOptions: { id: CapeProvider; label: string }[] = [
    { id: "mojang", label: "Mojang" },
    { id: "optifine", label: "OptiFine" },
    { id: "tlauncher", label: "TLauncher" },
    { id: "none", label: "None" },
  ];

  $: selectedProject = $recentProjects.find((p) => p.path === selectedPath);
  $: skinUrl = $authState.profile?.skinUrl ?? null;
  $: capeUrl = $authState.profile?.capeUrl ?? null;
  $: accountKey = $authState.activeAccountUuid ?? $authState.profile?.uuid ?? "";
  $: mojangCapeOffers = (capeCatalog?.offers ?? []).filter((o) => o.provider === "mojang");
  $: otherCapeOffers = (capeCatalog?.offers ?? []).filter((o) => o.provider !== "mojang");
  $: canChangeMojangCape =
    $authState.loginType === "microsoft" && mojangCapeOffers.some((o) => o.canActivate);

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

  $: if ($authState.loggedIn && $authState.profile?.uuid) {
    void refreshCapes();
  }

  onMount(async () => {
    try {
      const status = await api.mcAuth.getAuthStatus();
      authState.set(status);
      if (status.loggedIn && status.profile) {
        try {
          const path = await api.mcAuth.getSkinPath(status.profile.uuid);
          skinPath.set(path);
        } catch {}
      }
    } catch {}

    if (selectedPath && !selectedProject && $recentProjects.length > 0) {
      selectProject($recentProjects[0].path);
    }

    // Global handler for JVM crashes that happen after the launch command
    // has returned "started" — surfaces a categorized, retryable toast.
    registerLaunchCrashListener();
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
    isLaunching.set(true);
    showLogModal = true;
    await invoke("set_last_opened_project", { path: selectedPath });
    await launchWithFeedback(
      { path: selectedPath, profile: "client" },
      { onStarted: () => { isLaunching.set(false); } },
    );
    isLaunching.set(false);
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

  let pinnedPaths: Record<string, boolean> = {};
  let actionBusy = false;

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

  let instanceSizes: Record<string, string> = {};
  let loadingSizes: Record<string, boolean> = {};

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

  $: ensurePins($recentProjects.map((p) => p.path));
  $: ensureSizes($recentProjects.map((p) => p.path));

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
      await api.mcAuth.logout();
      authState.set({
        loggedIn: false,
        profile: null,
        expiresAt: null,
        loginType: "offline",
        skinSource: "mojang",
        capeProvider: $authState.capeProvider ?? "mojang",
        accounts: $authState.accounts,
        activeAccountUuid: $authState.activeAccountUuid,
      });
      skinPath.set(null);
      capeCatalog = null;
      toasts.info("Logged out");
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

<svelte:window on:click={closeMenu} />

<div class="home">
  <!-- Top bar: Quick actions left, Avatar right -->
  <div class="top-bar">
    <div class="quick-nav">
      <button class="quick-action" on:click={() => (currentView = "mods")} title="Mods">
        <Package size={18} />
        <span>Mods</span>
      </button>
      <button class="quick-action" on:click={() => (currentView = "graph")} title="Dependency Graph">
        <GitGraph size={18} />
        <span>Graph</span>
      </button>
      <button class="quick-action" on:click={() => (currentView = "diagnostics")} title="Diagnostics">
        <Stethoscope size={18} />
        <span>Diagnostics</span>
      </button>
      <button class="quick-action" on:click={() => (currentView = "snapshots")} title="Snapshots">
        <History size={18} />
        <span>Snapshots</span>
      </button>
      {#if selectedProject}
        <button class="quick-action" on:click={() => (currentView = "recipes")} title="Recipes">
          <Puzzle size={18} />
          <span>Recipes</span>
        </button>
        <button class="quick-action" on:click={() => (currentView = "quests")} title="Quests">
          <Sparkles size={18} />
          <span>Quests</span>
        </button>
      {/if}
    </div>

    <!-- Account avatar in top-right (sign-in lives in the skin panel) -->
    <div class="account-avatar-section">
      {#if $authState.loggedIn && $authState.profile}
        <button class="account-avatar-btn" on:click={() => (currentView = "me")} title="Me — account & playtime">
          {#if $skinPath}
            <img src={convertFileSrc($skinPath)} alt={$authState.profile.name} class="avatar-img" />
          {:else}
            <div class="avatar-fallback">
              <User size={18} />
            </div>
          {/if}
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

  <!-- Main content: Left (hero + instances) + Right (3D skin) -->
  <div class="main-layout">
    <!-- Left column -->
    <div class="left-column">
      <!-- Hero: Play button + project info -->
      <section class="hero">
        <div class="hero-left">
          <button class="play-btn" on:click={launch} disabled={!selectedPath || $isLaunching}>
            {#if $isLaunching}
              <span class="spinner"></span>
              <span class="play-text">Launching...</span>
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
                <span class="project-version">Select an instance below or create a new one</span>
              </div>
            {/if}

            <div class="hero-actions">
              {#if selectedProject}
                <button class="action-btn primary" on:click={() => (currentView = "ide")}>
                  <Workflow size={15} />
                  IDE
                </button>
                <button class="action-btn" on:click={openSettings}>
                  <Settings size={15} />
                  Settings
                </button>
                <button class="action-btn" on:click={() => invoke("open_project_folder", { path: selectedProject.path })}>
                  <FolderOpen size={15} />
                  Folder
                </button>
              {/if}
              <button class="action-btn accent" on:click={() => (newProjectOpen.set(true))}>
                <Plus size={15} />
                New
              </button>
            </div>
          </div>
        </div>

        {#if selectedProject}
          <div class="hero-right">
            <div class="instance-stats">
              <div class="stat">
                <HardDrive size={14} />
                <span>{instanceSizes[selectedProject.path] || "..."}</span>
              </div>
            </div>
          </div>
        {/if}
      </section>

      {#if selectedPath && selectedProject}
        <InstanceHome
          projectPath={selectedPath}
          onOpenMods={() => (currentView = "mods")}
          onOpenWorld={() => (currentView = "world")}
        />
      {/if}

      <!-- Instances grid -->
      <section class="projects-section">
        <div class="section-header">
          <h2>Instances</h2>
          <span class="instance-count">{$recentProjects.length}</span>
        </div>

        {#if $recentProjects.length === 0}
          <div class="empty-state">
            <div class="empty-icon">
              <Package size={40} />
            </div>
            <h3>No instances yet</h3>
            <p>Create your first modpack instance to get started.</p>
            <button class="action-btn accent" on:click={() => (newProjectOpen.set(true))}>
              <Plus size={16} />
              Create instance
            </button>
          </div>
        {:else}
          <div class="projects-grid">
            {#each $recentProjects as project (project.path)}
              <div
                class="project-tile"
                class:active={selectedPath === project.path}
                role="button"
                tabindex="0"
                on:click={() => selectProject(project.path)}
                on:keydown={(e) => e.key === 'Enter' && selectProject(project.path)}
                on:contextmenu|preventDefault={(e) => toggleMenu(e, project.path)}
              >
                <div
                  class="tile-icon"
                  style="background: linear-gradient(135deg, {gradientFrom(project.info.name)}, {gradientFrom(project.info.id)})"
                >
                  {project.info.name[0]}
                </div>
                <div class="tile-info">
                  <span class="tile-name">{project.info.name}</span>
                  <span class="tile-meta">
                    {project.info.minecraftVersion} · {project.info.loaderKind}
                    {#if loadingSizes[project.path]}
                      <span class="size-loading">···</span>
                    {:else if instanceSizes[project.path]}
                      · {instanceSizes[project.path]}
                    {/if}
                  </span>
                </div>
                <button class="tile-pin" class:pinned={pinnedPaths[project.path]} on:click={(e) => togglePin(e, project.path)} title={pinnedPaths[project.path] ? "Unpin" : "Pin"}>
                  <Pin size={14} />
                </button>
                <button
                  class="tile-menu"
                  class:active={activeMenuPath === project.path}
                  on:click={(e) => toggleMenu(e, project.path)}
                  aria-label="Actions"
                >
                  <MoreVertical size={18} />
                </button>

                {#if activeMenuPath === project.path}
                  <div class="actions-menu" role="menu" tabindex="-1" on:keydown={() => {}}>
                    <div class="menu-group">
                      <button on:click={() => handleAction("change-version", project)}>
                        <ShieldAlert size={14} /> Change Version
                      </button>
                      <button on:click={() => handleAction("open-folder", project)}>
                        <Folder size={14} /> Open Folder
                      </button>
                      <button on:click={() => handleAction("server-pack", project)}>
                        <Download size={14} /> Server Pack
                      </button>
                      <button on:click={() => handleAction("links", project)}>
                        <Link2 size={14} /> Links
                      </button>
                      <button on:click={() => handleAction("worlds", project)}>
                        <Globe size={14} /> Worlds
                      </button>
                      <button on:click={() => handleAction("backup-world", project)}>
                        <Download size={14} /> Backup World
                      </button>
                      <button on:click={() => handleAction("logs-zip", project)}>
                        <FileArchive size={14} /> Logs ZIP
                      </button>
                      <button on:click={() => handleAction("copy-link", project)}>
                        <Copy size={14} /> Copy Path
                      </button>
                      <button on:click={() => handleAction("clone", project)}>
                        <GitBranch size={14} /> Clone
                      </button>
                      <button on:click={() => handleAction("share", project)}>
                        <Share2 size={14} /> Export
                      </button>
                      <button on:click={() => handleAction("cleanup", project)}>
                        <Eraser size={14} /> Cleanup
                      </button>
                      <button on:click={() => handleAction("repair", project)}>
                        <Wrench size={14} /> Repair
                      </button>
                    </div>
                    <div class="menu-separator"></div>
                    <div class="menu-group">
                      <button on:click={() => handleAction("remove", project)}>
                        <Minus size={14} /> Remove
                      </button>
                    </div>
                    <div class="menu-group danger">
                      <button on:click={() => handleAction("delete", project)}>
                        <Trash2 size={14} /> Delete
                      </button>
                    </div>
                  </div>
                {/if}
              </div>
            {/each}

            <button class="project-tile add-tile" on:click={() => (newProjectOpen.set(true))}>
              <div class="tile-icon add-icon">
                <Plus size={24} />
              </div>
              <span class="tile-name">Add instance</span>
            </button>
          </div>
        {/if}
      </section>
    </div>

    <!-- Right column: 3D Skin -->
    <div class="right-column">
      <div class="skin-panel">
        {#if $authState.loggedIn && $authState.profile}
          <SkinPreview3D
            skinUrl={skinUrl}
            capeUrl={capeUrl}
            accountKey={accountKey}
            playerName={$authState.profile.name}
            width={300}
            height={400}
          />
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
              {#if $authState.accounts.length > 1}
                <button class="change-skin-btn" on:click={() => (showAccountManager = true)}>
                  {$authState.accounts.length} accounts
                </button>
              {/if}
            </div>
            <button class="change-skin-btn" on:click={() => (showAccountManager = true)}>
              <Palette size={14} />
              Accounts
            </button>
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
                  on:click={() => selectCapeProvider(opt.id)}
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
                  on:click={() => (mojangCapeMenuOpen ? (mojangCapeMenuOpen = false) : openMojangCapeMenu())}
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
                      on:click={() => activateMojangCape(offer.id)}
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
                        on:click={() => selectCapeProvider("mojang")}
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
                        on:click={() => selectCapeProvider(offer.provider)}
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
            <button class="action-btn accent" on:click={() => (showLoginModal = true)}>
              <LogIn size={16} />
              Sign In
            </button>
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>

{#if showLoginModal}
  <MinecraftLogin on:close={() => (showLoginModal = false)} />
{/if}

{#if showAccountManager}
  <AccountManager on:close={() => (showAccountManager = false)} />
{/if}

{#if $newProjectOpen}
  <AddInstanceModal
    on:close={() => (newProjectOpen.set(false))}
    on:created={(e) => loadProject(e.detail)}
  />
{/if}

{#if showLogModal && selectedPath}
  <LaunchLogModal projectPath={selectedPath} on:close={() => (showLogModal = false)} />
{/if}

{#if showWorldPrompt}
  <PromptDialog
    title="Backup World"
    message="Select a world to back up."
    mode="select"
    options={worldPromptOptions}
    defaultValue={worldPromptOptions[0]}
    confirmLabel="Backup"
    on:confirm={(e) => confirmBackupWorld(e.detail)}
    on:cancel={() => (showWorldPrompt = false)}
  />
{/if}

{#if showClonePrompt}
  <PromptDialog
    title="Clone Instance"
    message="Enter a name for the cloned instance."
    mode="text"
    defaultValue={clonePromptName}
    confirmLabel="Clone"
    on:confirm={(e) => confirmClone(e.detail)}
    on:cancel={() => (showClonePrompt = false)}
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


  .avatar-img {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    object-fit: cover;
    image-rendering: pixelated;
  }

  .avatar-fallback {
    width: 32px;
    height: 32px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    color: var(--text-muted);
  }

  .avatar-name {
    font-weight: 700;
    font-size: 13px;
    color: var(--text-primary);
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

  /* ─── Main Layout ────────────────────────────────── */
  .main-layout {
    display: flex;
    gap: 24px;
    align-items: flex-start;
  }

  .left-column {
    flex: 1;
    min-width: 0;
    /* Own stacking layer above the WebGL skin canvas: without this, a
       compositing glitch in the 3D preview could make tabs/buttons
       (quick-nav, hero actions) unclickable in some environments. */
    position: relative;
    z-index: 1;
  }

  .right-column {
    width: 320px;
    flex-shrink: 0;
    position: sticky;
    top: 20px;
    z-index: 0;
    max-height: calc(100vh - 40px);
    overflow-y: auto;
  }

  .skin-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    overflow: hidden;
    display: flex;
    flex-direction: column;
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
    font-weight: 700;
    font-size: 14px;
    color: var(--text-primary);
  }

  .change-skin-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border-radius: 8px;
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
    border-radius: 8px;
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
    padding: 8px; border-radius: 8px;
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

  /* ─── Hero ────────────────────────────────────────── */
  .hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 32px;
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.06), rgba(139, 92, 246, 0.04));
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    margin-bottom: 24px;
    gap: 24px;
  }

  .hero-left {
    display: flex;
    align-items: center;
    gap: 16px 24px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .hero-main {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    min-width: 0;
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
    border-radius: 8px;
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

  .play-text {
    font-weight: 800;
  }

  .project-quick-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .project-name {
    font-weight: 700;
    font-size: 15px;
    color: var(--text-primary);
  }

  .project-name.muted {
    color: var(--text-muted);
  }

  .project-version {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  .hero-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
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

  /* ─── Instances ───────────────────────────────────── */
  .projects-section {
    margin-bottom: 32px;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
  }

  .section-header h2 {
    font-size: 18px;
    font-weight: 700;
  }

  .instance-count {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    padding: 2px 8px;
    border-radius: 12px;
  }

  .projects-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }

  .project-tile {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    text-align: left;
    transition: all 0.15s ease;
    cursor: pointer;
  }

  .project-tile:hover {
    border-color: var(--bg-hover);
    background: var(--bg-tertiary);
    transform: translateY(-1px);
  }

  .project-tile.active {
    border-color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.04);
  }

  .tile-icon {
    width: 44px;
    height: 44px;
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 18px;
    color: #fff;
    flex-shrink: 0;
  }

  .tile-icon.add-icon {
    background: var(--bg-elevated);
    color: var(--text-muted);
    border: 2px dashed var(--border-color);
  }

  .tile-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .tile-name {
    font-weight: 700;
    font-size: 13px;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tile-meta {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  .size-loading {
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }

  .tile-pin {
    width: 24px; height: 24px; display: flex; align-items: center; justify-content: center;
    border-radius: 4px; background: transparent; border: none; color: var(--text-muted);
    cursor: pointer; flex-shrink: 0; padding: 0; opacity: 0;
    transition: opacity .15s, color .15s;
  }
  .project-tile:hover .tile-pin { opacity: 1; }
  .tile-pin.pinned { opacity: 1; color: var(--accent-primary); }
  .tile-pin:hover { color: var(--accent-primary) !important; }

  .tile-menu {
    width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
    border-radius: var(--border-radius-sm); background: transparent; color: var(--text-muted);
    border: none; cursor: pointer; flex-shrink: 0; opacity: 0; transition: opacity .15s;
  }
  .project-tile:hover .tile-menu { opacity: 1; }
  .tile-menu:hover, .tile-menu.active { background: var(--bg-hover); color: var(--text-primary); }

  .actions-menu {
    position: absolute; top: 42px; right: 10px; width: 200px;
    background: var(--bg-elevated); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
    z-index: 50; padding: 4px;
  }

  .actions-menu button {
    width: 100%; display: flex; align-items: center; gap: 8px;
    padding: 7px 10px; border-radius: var(--border-radius-md);
    background: transparent; border: none; color: var(--text-secondary);
    font-size: 12px; text-align: left; cursor: pointer;
  }

  .actions-menu button:hover { background: var(--bg-hover); color: var(--text-primary); }

  .menu-separator { height: 1px; background: var(--border-color); margin: 4px 0; }
  .menu-group.danger button:hover { background: rgba(239, 68, 68, 0.12); color: #ef4444; }

  .add-tile:hover .tile-icon { color: var(--accent-primary); border-color: var(--accent-primary); }

  /* ─── Empty State ─────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 64px 32px;
    text-align: center;
    background: var(--bg-secondary);
    border: 2px dashed var(--border-color);
    border-radius: var(--border-radius-xl);
  }

  .empty-icon {
    width: 72px;
    height: 72px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    border-radius: 50%;
    color: var(--text-muted);
  }

  .empty-state h3 {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .empty-state p {
    font-size: 13px;
    color: var(--text-muted);
    max-width: 320px;
  }
</style>
