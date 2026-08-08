<script lang="ts">
  import { FolderOpen, ChevronRight, Terminal } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { projectPath, projectInfo, openLaunchLog } from "../lib/store";

  import type { View } from "../lib/types";

  let { currentView }: { currentView: View } = $props();

  let onlineCount = $state(0);
  let onlineOk = $state(false);
  let onlineTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshOnline() {
    try {
      if (!isTauri()) {
        onlineOk = false;
        onlineCount = 0;
        return;
      }
      const stats: any = await invoke("get_launcher_online");
      onlineCount = Number(stats?.onlineCount ?? 0);
      onlineOk = true;
    } catch {
      onlineOk = false;
    }
  }

  onMount(() => {
    void refreshOnline();
    onlineTimer = setInterval(() => void refreshOnline(), 15000);
  });

  onDestroy(() => {
    if (onlineTimer) clearInterval(onlineTimer);
  });

  const titles: Record<View, string> = {
    dashboard: "Launcher",
    ide: "IDE Workflow",
    mods: "Mods",
    graph: "Dependency Graph",
    world: "World · MCA map",
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

  /** Inside an instance the pack name IS the page heading. */
  const pageTitle = $derived(
    currentView === "dashboard" && $projectInfo
      ? $projectInfo.name
      : (titles[currentView] ?? ""),
  );

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
</script>

<header class="header">
  <div class="left">
    {#key currentView + ($projectInfo?.name ?? "")}
      <div class="title-swap" in:titleIntro>
        <div class="breadcrumb">
          <span class="crumb">TuffBox</span>
          <ChevronRight size={14} class="separator" />
          <span class="crumb active">{titles[currentView]}</span>
        </div>
        <h1 class="page-title">{pageTitle}</h1>
      </div>
    {/key}
  </div>

  <div class="right">
    <div
      class="online-chip"
      class:live={onlineOk}
      title={onlineOk
        ? "Users with TuffBox open right now"
        : isTauri()
          ? "Community presence unavailable (no network / Supabase)"
          : "Presence requires the Tauri app (browser preview is offline)"}
    >
      <span class="online-dot" class:on={onlineOk && onlineCount > 0}></span>
      <span class="online-label">{onlineOk ? onlineCount : "—"}</span>
      <span class="online-hint">{onlineOk ? "online" : isTauri() ? "offline" : "preview"}</span>
    </div>

    <button class="secondary" onclick={selectProject}>
      <FolderOpen size={16} />
      {$projectPath ? "Switch" : "Open"}
    </button>

    <button
      class="secondary"
      disabled={!$projectPath}
      title="Live logs of the running build"
      onclick={() => $projectPath && openLaunchLog($projectPath)}
    >
      <Terminal size={16} />
      Logs
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
    font-size: 24px;
    font-weight: 800;
    letter-spacing: -0.3px;
    line-height: 1.15;
    margin: 0;
    max-width: 52vw;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .online-chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    background: rgba(255, 255, 255, 0.03);
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    user-select: none;
  }
  .online-chip.live {
    color: var(--text-secondary);
  }
  .online-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #64748b;
    box-shadow: none;
  }
  .online-dot.on {
    background: #22c55e;
    box-shadow: 0 0 0 3px rgba(34, 197, 94, 0.22);
  }
  .online-label {
    font-variant-numeric: tabular-nums;
    color: var(--text-primary);
    min-width: 1ch;
  }
  .online-hint {
    text-transform: lowercase;
    letter-spacing: 0.02em;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:disabled:hover {
    transform: none;
    background: inherit;
  }
</style>
