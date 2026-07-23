<script lang="ts">
  import { Play, FolderOpen, ChevronRight, Terminal } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { projectPath, projectInfo, isLaunching, openLaunchLog } from "../lib/store";
  import { launchWithFeedback } from "../lib/launch";

  export let currentView: string;

  const titles: Record<string, string> = {
    dashboard: "Launcher",
    ide: "IDE Workflow",
    mods: "Mods",
    graph: "Dependency Graph",
    world: "World Map",
    library: "Library",
    chats: "Chats",
    diagnostics: "Health Check",
    "crash-votes": "Crash Votes",
    snapshots: "Snapshot History",
    configs: "Config Editor",
    settings: "Settings",
    "project-settings": "Instance Settings",
    "ore-gen": "Ore Generation",
    recipes: "Recipe Browser",
    quests: "Quest Editor",
    me: "Me",
  };

  function prefersReducedMotion(): boolean {
    if (typeof document === "undefined") return true;
    if (document.documentElement.classList.contains("potato-pc")) return true;
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  function titleIntro(node: Element) {
    if (prefersReducedMotion()) return { duration: 0 };
    return fly(node, { y: 12, duration: 320, opacity: 0, easing: quintOut });
  }

  async function selectProject() {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "TuffBox Project", extensions: ["tuffbox.json"] }],
    });
    if (selected && typeof selected === "string") {
      const info = await invoke("validate_project", { path: selected }) as import("../lib/api").ProjectSummary;
      const manifestPath = info.manifestPath || selected;
      projectPath.set(manifestPath);
      projectInfo.set(info as any);
    }
  }

  async function launch() {
    const path = $projectPath;
    if (!path) return;
    await launchWithFeedback({ path, profile: "client" });
  }
</script>

<header class="header">
  <div class="left">
    {#key currentView}
      <div class="title-swap" in:titleIntro>
        <div class="breadcrumb">
          <span class="crumb">TuffBox</span>
          <ChevronRight size={14} class="separator" />
          <span class="crumb active">{titles[currentView]}</span>
        </div>
        <h1 class="page-title">{titles[currentView]}</h1>
      </div>
    {/key}
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

    <button
      class="secondary"
      disabled={!$projectPath}
      title="Live logs of the running build"
      on:click={() => $projectPath && openLaunchLog($projectPath)}
    >
      <Terminal size={16} />
      Logs
    </button>

    <button class="launch-btn" on:click={launch} disabled={!$projectPath || $isLaunching}>
      {#if $isLaunching}
        <span class="spinner"></span>
        <span>Launching…</span>
      {:else}
        <Play size={16} fill="currentColor" />
        <span>Launch</span>
      {/if}
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
    min-width: 0;
    position: relative;
  }

  .title-swap {
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
    margin: 0;
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
    background: inherit;
  }

  .launch-btn {
    min-width: 100px;
  }

  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid rgba(0, 0, 0, 0.2);
    border-top-color: #000;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
