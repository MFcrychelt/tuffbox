<script lang="ts">
  import { Play, FolderOpen, ChevronRight } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { projectPath, projectInfo } from "../lib/store";

  export let currentView: string;

  const titles: Record<string, string> = {
    dashboard: "Home",
    mods: "Mods",
    graph: "Dependency Graph",
    diagnostics: "Health Check",
    snapshots: "Snapshot History",
    settings: "Settings",
  };

  async function selectProject() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "TuffBox Project", extensions: ["tuffbox.json"] }],
    });
    if (selected && typeof selected === "string") {
      projectPath.set(selected);
      const info = await invoke("validate_project", { path: selected });
      projectInfo.set(info as any);
    }
  }

  async function launch() {
    const path = $projectPath;
    if (!path) return;
    await invoke("launch_profile", { path, profile: "client" });
  }
</script>

<header class="header">
  <div class="left">
    <div class="breadcrumb">
      <span class="crumb">TuffBox</span>
      <ChevronRight size={14} class="separator" />
      <span class="crumb active">{titles[currentView]}</span>
    </div>
    <h1 class="page-title">{titles[currentView]}</h1>
  </div>

  <div class="right">
    {#if $projectInfo}
      <div class="project-chip">
        <span class="project-name">{$projectInfo.name}</span>
        <span class="project-meta"
          >{$projectInfo.minecraftVersion} · {$projectInfo.loaderKind}
          {$projectInfo.loaderVersion}</span
        >
      </div>
    {/if}

    <button class="secondary" on:click={selectProject}>
      <FolderOpen size={16} />
      {$projectPath ? "Switch" : "Open"}
    </button>

    <button on:click={launch} disabled={!$projectPath}>
      <Play size={16} fill="currentColor" />
      Launch
    </button>
  </div>
</header>

<style>
  .header {
    height: 72px;
    padding: 0 32px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border-color);
    background: rgba(18, 18, 20, 0.8);
    backdrop-filter: blur(12px);
    flex-shrink: 0;
  }

  .left {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .breadcrumb :global(.separator) {
    color: var(--text-muted);
  }

  .crumb.active {
    color: var(--text-secondary);
  }

  .page-title {
    font-size: 20px;
    font-weight: 800;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .project-chip {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    padding-right: 12px;
    border-right: 1px solid var(--border-color);
  }

  .project-name {
    font-weight: 700;
    font-size: 14px;
  }

  .project-meta {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: capitalize;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:disabled:hover {
    transform: none;
    background: var(--accent-primary);
  }
</style>
