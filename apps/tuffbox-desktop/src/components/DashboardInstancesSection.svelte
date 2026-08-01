<script lang="ts">
  import {
    Plus,
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
    Minus,
    Clock,
    LayoutGrid,
    Package,
  } from "@lucide/svelte";
  import {
    formatPlaytime,
    newProjectOpen,
    type RecentProject,
  } from "../lib/store";

  type HomeLayout = "classic" | "yt-main" | "yt-under-skin" | "yt-hidden";

  let {
    homeLayout,
    sortedProjects,
    selectedPath,
    instanceSizes,
    loadingSizes,
    projectStats,
    pinnedPaths,
    activeMenuPath,
    menuPos = null,
    homeLayoutOptions,
    sideColumn = false,
    onHomeLayoutChange,
    selectProject,
    toggleMenu,
    togglePin,
    handleAction,
    gradientFrom,
  }: {
    homeLayout: HomeLayout;
    sortedProjects: RecentProject[];
    selectedPath: string | null;
    instanceSizes: Record<string, string>;
    loadingSizes: Record<string, boolean>;
    projectStats: Record<string, { playtime: number; lastLaunch: string | null }>;
    pinnedPaths: Record<string, boolean>;
    activeMenuPath: string | null;
    menuPos?: { top: number; left: number } | null;
    homeLayoutOptions: { id: HomeLayout; label: string }[];
    sideColumn?: boolean;
    onHomeLayoutChange: (e: Event) => void;
    selectProject: (path: string) => void;
    toggleMenu: (event: MouseEvent, path: string) => void;
    togglePin: (event: MouseEvent, projectPath: string) => void;
    handleAction: (action: string, project: RecentProject) => void;
    gradientFrom: (name: string) => string;
  } = $props();
</script>

<section class="projects-section" class:side-instances={sideColumn}>
  <div class="section-header">
    <h2>Instances</h2>
    <span class="instance-count">{sortedProjects.length}</span>
    <label class="layout-picker" title="Home layout">
      <LayoutGrid size={14} />
      <select value={homeLayout} onchange={onHomeLayoutChange}>
        {#each homeLayoutOptions as opt (opt.id)}
          <option value={opt.id}>{opt.label}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if sortedProjects.length === 0}
    <div class="empty-state">
      <div class="empty-icon">
        <Package size={40} />
      </div>
      <h3>No instances yet</h3>
      <p>Create your first modpack instance to get started.</p>
      <button class="action-btn accent" onclick={() => (newProjectOpen.set(true))}>
        <Plus size={16} />
        Create instance
      </button>
    </div>
  {:else}
    <div class="projects-grid tb-stagger">
      {#each sortedProjects as project, i (project.path)}
        <div
          class="project-tile tb-card"
          style={`--i: ${i}`}
          class:active={selectedPath === project.path}
          role="button"
          tabindex="0"
          onclick={() => selectProject(project.path)}
          onkeydown={(e) => e.key === "Enter" && selectProject(project.path)}
          oncontextmenu={(e) => { e.preventDefault(); toggleMenu(e, project.path); }}
        >
          <div
            class="tile-icon tb-cover-media"
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
            {#if projectStats[project.path]}
              <span class="tile-playtime">
                <Clock size={11} />
                {formatPlaytime(projectStats[project.path].playtime)}
              </span>
            {:else}
              <span class="tile-playtime skel-playtime" aria-hidden="true">
                <span class="skeleton skeleton-block skeleton-line short" style="width: 48px; height: 10px;"></span>
              </span>
            {/if}
          </div>
          <button
            class="tile-pin"
            class:pinned={pinnedPaths[project.path]}
            onclick={(e) => togglePin(e, project.path)}
            title={pinnedPaths[project.path] ? "Unpin" : "Pin"}
          >
            <Pin size={14} />
          </button>
          <button
            class="tile-menu"
            class:active={activeMenuPath === project.path}
            onclick={(e) => toggleMenu(e, project.path)}
            aria-label="Actions"
          >
            <MoreVertical size={18} />
          </button>

          {#if activeMenuPath === project.path && menuPos}
            <div
              class="actions-menu"
              role="menu"
              tabindex="-1"
              style={`top:${menuPos.top}px; left:${menuPos.left}px`}
              onclick={(e) => e.stopPropagation()}
              onkeydown={() => {}}
            >
              <div class="menu-group">
                <button onclick={() => handleAction("change-version", project)}>
                  <ShieldAlert size={14} /> Change Version
                </button>
                <button onclick={() => handleAction("open-folder", project)}>
                  <Folder size={14} /> Open Folder
                </button>
                <button onclick={() => handleAction("server-pack", project)}>
                  <Download size={14} /> Server Pack
                </button>
                <button onclick={() => handleAction("links", project)}>
                  <Link2 size={14} /> Links
                </button>
                <button onclick={() => handleAction("worlds", project)}>
                  <Globe size={14} /> Worlds
                </button>
                <button onclick={() => handleAction("backup-world", project)}>
                  <Download size={14} /> Backup World
                </button>
                <button onclick={() => handleAction("logs-zip", project)}>
                  <FileArchive size={14} /> Logs ZIP
                </button>
                <button onclick={() => handleAction("copy-link", project)}>
                  <Copy size={14} /> Copy Path
                </button>
                <button onclick={() => handleAction("clone", project)}>
                  <GitBranch size={14} /> Clone
                </button>
                <button onclick={() => handleAction("share", project)}>
                  <Share2 size={14} /> Export
                </button>
                <button onclick={() => handleAction("cleanup", project)}>
                  <Eraser size={14} /> Cleanup
                </button>
                <button onclick={() => handleAction("repair", project)}>
                  <Wrench size={14} /> Repair
                </button>
              </div>
              <div class="menu-separator"></div>
              <div class="menu-group">
                <button onclick={() => handleAction("remove", project)}>
                  <Minus size={14} /> Remove
                </button>
              </div>
              <div class="menu-group danger">
                <button onclick={() => handleAction("delete", project)}>
                  <Trash2 size={14} /> Delete
                </button>
              </div>
            </div>
          {/if}
        </div>
      {/each}

      <button class="project-tile add-tile" onclick={() => (newProjectOpen.set(true))}>
        <div class="tile-icon add-icon">
          <Plus size={24} />
        </div>
        <span class="tile-name">Add instance</span>
      </button>
    </div>
  {/if}
</section>

<style>
  .projects-section {
    margin-bottom: 0;
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
    border-radius: var(--border-radius-md);
  }

  .layout-picker {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .layout-picker select {
    max-width: 240px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 11px;
    padding: 4px 6px;
    cursor: pointer;
  }

  .side-instances .projects-grid {
    grid-template-columns: 1fr;
  }

  .side-instances .layout-picker select {
    max-width: 160px;
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
    cursor: pointer;
  }

  .project-tile:hover {
    background: var(--bg-tertiary);
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
    overflow: hidden;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 18px;
    color: #fff;
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

  .tile-playtime {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary, var(--text-muted));
    margin-top: 2px;
  }

  .size-loading {
    animation: pulse 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }

  .tile-pin {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    padding: 0;
    opacity: 0;
    transition: opacity 0.15s, color 0.15s;
  }
  .project-tile:hover .tile-pin {
    opacity: 1;
  }
  .tile-pin.pinned {
    opacity: 1;
    color: var(--accent-primary);
  }
  .tile-pin:hover {
    color: var(--accent-primary) !important;
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
    opacity: 0;
    transition: opacity 0.15s;
  }
  .project-tile:hover .tile-menu {
    opacity: 1;
  }
  .tile-menu:hover,
  .tile-menu.active {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .actions-menu {
    position: fixed;
    width: 200px;
    max-height: min(420px, calc(100vh - 16px));
    overflow-y: auto;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.4);
    z-index: 400;
    padding: 4px;
    scrollbar-width: thin;
  }

  .actions-menu button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: var(--border-radius-md);
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 12px;
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
    margin: 4px 0;
  }
  .menu-group.danger button:hover {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
  }

  .add-tile:hover .tile-icon {
    color: var(--accent-primary);
    border-color: var(--accent-primary);
  }

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

  .action-btn.accent {
    background: var(--accent-primary);
    color: #000;
    border-color: transparent;
  }

  .action-btn.accent:hover {
    background: var(--accent-hover);
  }
</style>
