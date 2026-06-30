<script lang="ts">
  import {
    Play,
    Plus,
    Settings,
    MoreVertical,
    Folder,
    Trash2,
    Copy,
    Link2,
    Wrench,
    Share2,
    GitBranch,
    FileArchive,
    Download,
    ExternalLink,
    ShieldAlert,
    Terminal,
    Minus,
    Workflow,
  } from "lucide-svelte";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { recentProjects, projectPath, projectInfo, type RecentProject } from "../lib/store";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import LaunchLogModal from "./LaunchLogModal.svelte";

  export let currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings";

  let selectedPath: string | null = $projectPath;
  let launching = false;
  let activeMenuPath: string | null = null;
  let showAddModal = false;
  let showLogModal = false;

  $: selectedProject = $recentProjects.find((p) => p.path === selectedPath);

  async function loadProject(path: string) {
    const info = await invoke("validate_project", { path });
    const project: RecentProject = { path, info: info as any };
    recentProjects.add(project);
    projectPath.set(path);
    projectInfo.set(project.info);
    selectedPath = path;
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
    launching = true;
    showLogModal = true;
    try {
      await invoke("launch_profile", { path: selectedPath, profile: "client" });
    } catch (e) {
      alert(`Launch failed: ${e}`);
    } finally {
      launching = false;
    }
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

  async function handleAction(action: string, project: RecentProject) {
    activeMenuPath = null;
    switch (action) {
      case "change-version":
        alert("Change Version: not implemented yet");
        break;
      case "desktop-shortcut":
        alert("Create Desktop Shortcut: not implemented yet");
        break;
      case "server-pack":
        alert("Download Server Pack: not implemented yet");
        break;
      case "links":
        alert("Links: not implemented yet");
        break;
      case "open-folder":
        await invoke("open_project_folder", { path: project.path });
        break;
      case "logs-zip":
        alert("Create logs.zip: not implemented yet");
        break;
      case "copy-link":
        await navigator.clipboard.writeText(project.path);
        alert("Modpack path copied to clipboard");
        break;
      case "profile-options":
        currentView = "project-settings";
        selectProject(project.path);
        break;
      case "clone":
        alert("Clone as...: not implemented yet");
        break;
      case "share":
        alert("Share Profile: not implemented yet");
        break;
      case "repair":
        alert("Repair Profile: not implemented yet");
        break;
      case "remove":
        removeFromLauncher(project);
        break;
      case "delete":
        await deleteProject(project);
        break;
    }
  }

  function removeFromLauncher(project: RecentProject) {
    recentProjects.remove(project.path);
    if (selectedPath === project.path) {
      selectedPath = $recentProjects[0]?.path ?? null;
      projectPath.set(selectedPath);
      projectInfo.set($recentProjects[0]?.info ?? null);
    }
  }

  async function deleteProject(project: RecentProject) {
    const ok = await confirm(
      `Delete profile "${project.info.name}"? This removes the project manifest file.`,
      { title: "Delete Profile", kind: "warning" }
    );
    if (!ok) return;
    try {
      await invoke("delete_project", { path: project.path });
      recentProjects.remove(project.path);
      if (selectedPath === project.path) {
        selectedPath = $recentProjects[0]?.path ?? null;
        projectPath.set(selectedPath);
        projectInfo.set($recentProjects[0]?.info ?? null);
      }
    } catch (e) {
      alert(`Failed to delete profile: ${e}`);
    }
  }

  function gradientFrom(name: string) {
    const colors = [
      "#1bd96a",
      "#8b5cf6",
      "#3b82f6",
      "#f59e0b",
      "#ec4899",
      "#06b6d4",
      "#ef4444",
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
  }

  if (selectedPath && !selectedProject && $recentProjects.length > 0) {
    selectProject($recentProjects[0].path);
  }
</script>

<svelte:window on:click={closeMenu} />

<div class="home">
  <section class="hero">
    <button class="play-btn" on:click={launch} disabled={!selectedPath || launching}>
      {#if launching}
        <span class="spinner" />
        <span class="play-label">Launching...</span>
      {:else}
        <Play size={32} fill="currentColor" />
        <span class="play-label">Play</span>
        {#if selectedProject}
          <span class="play-subtitle">{selectedProject.info.name}</span>
          <span class="play-meta"
            >{selectedProject.info.minecraftVersion} · {selectedProject.info.loaderKind}</span
          >
        {:else}
          <span class="play-subtitle muted">No instance selected</span>
        {/if}
      {/if}
    </button>

    {#if selectedProject}
      <div class="hero-actions">
        <button class="ide-cta" on:click={() => (currentView = "ide")}>
          <Workflow size={16} />
          Open IDE
        </button>
        <button class="ghost" on:click={openSettings}>
          <Settings size={16} />
          Settings
        </button>
      </div>
    {/if}
  </section>

  <section class="projects-section">
    <div class="section-header">
      <h2>Instances</h2>
      <button class="ghost add-btn" on:click={() => (showAddModal = true)}>
        <Plus size={16} />
        Add
      </button>
    </div>

    {#if $recentProjects.length === 0}
      <div class="empty-grid">
        <div class="empty-card">
          <p>No instances yet</p>
          <button on:click={() => (showAddModal = true)}>Add instance</button>
        </div>
      </div>
    {:else}
      <div class="projects-grid">
        {#each $recentProjects as project}
          <div
            class="project-tile"
            class:active={selectedPath === project.path}
            role="button"
            tabindex="0"
            on:click={() => selectProject(project.path)}
            on:keydown={(e) => e.key === 'Enter' && selectProject(project.path)}
          >
            <div
              class="tile-icon"
              style="background: linear-gradient(135deg, {gradientFrom(project.info.name)}, {gradientFrom(
                project.info.id
              )})"
            >
              {project.info.name[0]}
            </div>
            <div class="tile-info">
              <span class="tile-name">{project.info.name}</span>
              <span class="tile-meta"
                >{project.info.minecraftVersion} · {project.info.loaderKind}</span
              >
            </div>
            <button
              class="tile-menu"
              class:active={activeMenuPath === project.path}
              on:click={(e) => toggleMenu(e, project.path)}
              aria-label="Actions"
            >
              <MoreVertical size={18} />
            </button>

            {#if activeMenuPath === project.path}
              <div class="actions-menu" role="menu" tabindex="-1" on:click|stopPropagation>
                <div class="menu-group">
                  <button on:click={() => handleAction("change-version", project)}>
                    <ShieldAlert size={14} /> Change Version
                  </button>
                  <button on:click={() => handleAction("desktop-shortcut", project)}>
                    <ExternalLink size={14} /> Create Desktop Shortcut
                  </button>
                  <button on:click={() => handleAction("server-pack", project)}>
                    <Download size={14} /> Download Server Pack
                  </button>
                  <button on:click={() => handleAction("links", project)}>
                    <Link2 size={14} /> Links
                  </button>
                  <button on:click={() => handleAction("open-folder", project)}>
                    <Folder size={14} /> Open Folder
                  </button>
                  <button on:click={() => handleAction("logs-zip", project)}>
                    <FileArchive size={14} /> Create logs.zip
                  </button>
                  <button on:click={() => handleAction("copy-link", project)}>
                    <Copy size={14} /> Copy Modpack Link
                  </button>
                  <button on:click={() => handleAction("profile-options", project)}>
                    <Terminal size={14} /> Profile Options
                  </button>
                  <button on:click={() => handleAction("clone", project)}>
                    <GitBranch size={14} /> Clone as...
                  </button>
                  <button on:click={() => handleAction("share", project)}>
                    <Share2 size={14} /> Share Profile
                  </button>
                  <button on:click={() => handleAction("repair", project)}>
                    <Wrench size={14} /> Repair Profile
                  </button>
                </div>
                <div class="menu-separator" />
                <div class="menu-group">
                  <button on:click={() => handleAction("remove", project)}>
                    <Minus size={14} /> Remove from launcher
                  </button>
                </div>
                <div class="menu-group danger">
                  <button on:click={() => handleAction("delete", project)}>
                    <Trash2 size={14} /> Delete Profile
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/each}

        <button class="project-tile add-tile" on:click={() => (showAddModal = true)}>
          <div class="tile-icon add-icon">
            <Plus size={24} />
          </div>
          <span class="tile-name">Add instance</span>
        </button>
      </div>
    {/if}
  </section>
</div>

{#if showAddModal}
  <AddInstanceModal
    on:close={() => (showAddModal = false)}
    on:created={(e) => loadProject(e.detail)}
  />
{/if}

{#if showLogModal && selectedPath}
  <LaunchLogModal projectPath={selectedPath} on:close={() => (showLogModal = false)} />
{/if}

<style>
  .home {
    max-width: 1200px;
  }

  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 56px 32px;
    background: linear-gradient(180deg, rgba(27, 217, 106, 0.08), transparent);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    margin-bottom: 32px;
  }

  .play-btn {
    width: 280px;
    min-height: 140px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 20px;
    border-radius: var(--border-radius-xl);
    box-shadow: 0 16px 40px rgba(27, 217, 106, 0.35);
    margin-bottom: 16px;
    padding: 20px;
  }

  .play-btn:hover {
    box-shadow: 0 20px 48px rgba(27, 217, 106, 0.45);
  }

  .play-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }

  .play-label {
    font-size: 26px;
    font-weight: 900;
    line-height: 1;
    margin-top: 2px;
  }

  .play-subtitle {
    font-size: 14px;
    font-weight: 700;
    color: rgba(0, 0, 0, 0.65);
    margin-top: 4px;
  }

  .play-subtitle.muted {
    color: rgba(0, 0, 0, 0.45);
    font-weight: 600;
  }

  .play-meta {
    font-size: 12px;
    font-weight: 700;
    color: rgba(0, 0, 0, 0.5);
    text-transform: capitalize;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 3px solid rgba(0, 0, 0, 0.2);
    border-top-color: #000;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .hero-actions {
    display: flex;
    gap: 12px;
  }

  .projects-section {
    margin-bottom: 32px;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .section-header h2 {
    font-size: 18px;
    font-weight: 700;
  }

  .projects-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 16px;
  }

  .project-tile {
    position: relative;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px;
    background: var(--bg-secondary);
    border: 2px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    text-align: left;
    transition: all 0.15s ease;
    cursor: pointer;
  }

  .project-tile:hover {
    border-color: var(--bg-hover);
    transform: translateY(-2px);
  }

  .project-tile.active {
    border-color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.06);
  }

  .tile-icon {
    width: 52px;
    height: 52px;
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 22px;
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
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tile-meta {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  .tile-menu {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-muted);
    border: none;
    cursor: pointer;
    flex-shrink: 0;
  }

  .tile-menu:hover,
  .tile-menu.active {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .actions-menu {
    position: absolute;
    top: 46px;
    right: 12px;
    width: 220px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
    z-index: 50;
    padding: 6px;
  }

  .actions-menu button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--border-radius-md);
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  .actions-menu button:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .menu-separator {
    height: 1px;
    background: var(--border-color);
    margin: 6px 0;
  }

  .menu-group.danger button:hover {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
  }

  .add-tile:hover .tile-icon {
    color: var(--accent-primary);
    border-color: var(--accent-primary);
  }

  .empty-grid {
    display: flex;
    justify-content: center;
  }

  .empty-card {
    background: var(--bg-secondary);
    border: 2px dashed var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 48px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    color: var(--text-muted);
  }

  .empty-card p {
    color: var(--text-secondary);
    font-weight: 500;
  }

</style>
