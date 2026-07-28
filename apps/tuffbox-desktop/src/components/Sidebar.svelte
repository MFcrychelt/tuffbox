<script lang="ts">
  import {
    LayoutDashboard,
    Package,
    GitGraph,
    Globe,
    Stethoscope,
    History,
    Vote,
    Workflow,
    Settings,
    Plus,
    Library,
    User,
    MessagesSquare,
    PanelLeftClose,
    PanelLeftOpen,
  } from "lucide-svelte";
  import { afterUpdate, onDestroy, tick } from "svelte";
  import {
    newProjectOpen,
    sidebarMode,
    sidebarIconsCollapsed,
  } from "../lib/store";

  type View = "dashboard" | "ide" | "mods" | "graph" | "world" | "diagnostics" | "crash-votes" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "me" | "chats";
  export let currentView: View;

  const items: { id: View; label: string; icon: any; featured?: boolean; shortcut?: string }[] = [
    { id: "dashboard", label: "Launcher", icon: LayoutDashboard, shortcut: "Ctrl+1" },
    { id: "me", label: "Me", icon: User },
    { id: "ide", label: "Open IDE", icon: Workflow, featured: true, shortcut: "Ctrl+2" },
    { id: "mods", label: "Mods", icon: Package, shortcut: "Ctrl+3" },
    { id: "graph", label: "Graph", icon: GitGraph, shortcut: "Ctrl+4" },
    { id: "world", label: "World", icon: Globe, shortcut: "Ctrl+8" },
    { id: "library", label: "Library", icon: Library },
    { id: "chats", label: "Chats", icon: MessagesSquare },
    { id: "diagnostics", label: "Diagnostics", icon: Stethoscope, shortcut: "Ctrl+6" },
    { id: "crash-votes", label: "Crash Votes", icon: Vote },
    { id: "snapshots", label: "Snapshots", icon: History, shortcut: "Ctrl+7" },
  ];

  let navEl: HTMLElement | null = null;
  let bottomEl: HTMLElement | null = null;
  let indicatorY = 0;
  let indicatorH = 42;
  let indicatorReady = false;
  let indicatorInBottom = false;

  let railRevealed = false;
  let railHideTimer: ReturnType<typeof setTimeout> | null = null;

  $: autoHide = $sidebarMode === "autoHide";
  $: iconsMode = $sidebarMode === "icons";
  $: iconsCollapsed = iconsMode && $sidebarIconsCollapsed;

  function openNewProject() {
    // Dashboard owns the modal, so make sure we're on that view before
    // raising the flag — otherwise the modal component wouldn't be mounted.
    currentView = "dashboard";
    newProjectOpen.set(true);
  }

  function clearRailHideTimer() {
    if (railHideTimer) {
      clearTimeout(railHideTimer);
      railHideTimer = null;
    }
  }

  function revealRail() {
    if (!autoHide) return;
    clearRailHideTimer();
    railRevealed = true;
  }

  function scheduleHideRail() {
    if (!autoHide) return;
    clearRailHideTimer();
    railHideTimer = setTimeout(() => {
      railRevealed = false;
      railHideTimer = null;
    }, 160);
  }

  $: if (!autoHide) {
    railRevealed = false;
    clearRailHideTimer();
  }

  onDestroy(() => clearRailHideTimer());

  async function syncIndicator() {
    await tick();
    const inBottom = currentView === "settings";
    indicatorInBottom = inBottom;
    const host = inBottom ? bottomEl : navEl;
    const btn = host?.querySelector(".nav-item.active") as HTMLElement | null;
    if (!host || !btn) {
      indicatorReady = false;
      return;
    }
    const hr = host.getBoundingClientRect();
    const br = btn.getBoundingClientRect();
    indicatorY = br.top - hr.top;
    indicatorH = br.height;
    indicatorReady = true;
  }

  $: currentView, iconsCollapsed, railRevealed, $sidebarMode, void syncIndicator();
  afterUpdate(() => {
    void syncIndicator();
  });
</script>

<div
  class="sidebar-slot"
  class:auto-hide={autoHide}
  class:icons-collapsed={iconsCollapsed}
  class:revealed={railRevealed || !autoHide}
>
  {#if autoHide}
    <div class="sidebar-hotzone" aria-hidden="true" on:mouseenter={revealRail}></div>
  {/if}

  <aside
    class="sidebar"
    class:compact={iconsCollapsed}
    class:auto-hide-panel={autoHide}
    class:revealed={railRevealed || !autoHide}
    on:mouseenter={revealRail}
    on:mouseleave={scheduleHideRail}
  >
    <div class="brand">
      <div class="logo">T</div>
      {#if !iconsCollapsed}
        <span class="brand-name">TuffBox</span>
      {/if}
      {#if iconsMode}
        <button
          type="button"
          class="collapse-btn tb-icon-hover"
          title={iconsCollapsed ? "Expand sidebar" : "Collapse to icons"}
          aria-expanded={!iconsCollapsed}
          on:click={() => sidebarIconsCollapsed.toggle()}
        >
          {#if iconsCollapsed}
            <PanelLeftOpen size={16} />
          {:else}
            <PanelLeftClose size={16} />
          {/if}
        </button>
      {/if}
    </div>

    <nav class="nav" bind:this={navEl}>
      <div
        class="nav-indicator"
        class:ready={indicatorReady && !indicatorInBottom}
        style={`transform: translateY(${indicatorY}px); height: ${indicatorH}px`}
        aria-hidden="true"
      ></div>
      {#each items as item (item.id)}
        <button
          class="nav-item tb-icon-hover"
          class:active={currentView === item.id}
          class:featured={item.featured}
          on:click={() => (currentView = item.id)}
          on:focus={revealRail}
          title={item.shortcut ? `${item.label} (${item.shortcut})` : item.label}
        >
          <svelte:component this={item.icon} size={20} />
          {#if !iconsCollapsed}
            <span class="nav-label">{item.label}</span>
            {#if item.shortcut}
              <span class="shortcut">{item.shortcut}</span>
            {/if}
          {/if}
        </button>
      {/each}

      <button
        class="nav-item add tb-icon-hover"
        title="New instance"
        on:click={openNewProject}
        on:focus={revealRail}
      >
        <Plus size={20} />
        {#if !iconsCollapsed}
          <span class="nav-label">New</span>
        {/if}
      </button>
    </nav>

    <div class="bottom" bind:this={bottomEl}>
      <div
        class="nav-indicator"
        class:ready={indicatorReady && indicatorInBottom}
        style={`transform: translateY(${indicatorY}px); height: ${indicatorH}px`}
        aria-hidden="true"
      ></div>
      <button
        class="nav-item tb-icon-hover"
        class:active={currentView === "settings"}
        on:click={() => (currentView = "settings")}
        on:focus={revealRail}
        title="Settings"
      >
        <Settings size={20} />
        {#if !iconsCollapsed}
          <span class="nav-label">Settings</span>
        {/if}
      </button>
    </div>
  </aside>
</div>

<style>
  .sidebar-slot {
    flex-shrink: 0;
    width: 212px;
    position: relative;
    z-index: 30;
    transition: width 0.14s cubic-bezier(0.2, 0.8, 0.2, 1);
  }

  .sidebar-slot.icons-collapsed {
    width: 68px;
  }

  .sidebar-slot.auto-hide {
    width: 0;
    overflow: visible;
  }

  .sidebar-hotzone {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 14px;
    z-index: 32;
  }

  .sidebar-slot.auto-hide:has(.sidebar.revealed) .sidebar-hotzone,
  .sidebar-slot.auto-hide:has(.sidebar:focus-within) .sidebar-hotzone {
    pointer-events: none;
  }

  .sidebar {
    width: 212px;
    height: 100%;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    padding: 16px 12px;
    box-sizing: border-box;
  }

  .sidebar.compact {
    width: 68px;
    padding: 16px 8px;
  }

  .sidebar.auto-hide-panel {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    transform: translateX(-100%);
    transition: transform 0.14s cubic-bezier(0.2, 0.8, 0.2, 1);
    box-shadow: 10px 0 28px rgba(0, 0, 0, 0.28);
    pointer-events: none;
  }

  .sidebar.auto-hide-panel.revealed,
  .sidebar.auto-hide-panel:focus-within {
    transform: translateX(0);
    pointer-events: auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 18px;
    min-height: 48px;
  }

  .sidebar.compact .brand {
    flex-direction: column;
    gap: 8px;
    padding: 4px 0 14px;
  }

  .logo {
    width: 36px;
    height: 36px;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 18px;
    color: #000;
    box-shadow: 0 4px 14px rgba(27, 217, 106, 0.35);
    flex-shrink: 0;
    animation: tb-logo-reveal 1.15s cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  .brand-name {
    font-weight: 700;
    font-size: 15px;
    color: var(--text-primary);
    letter-spacing: 0.2px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .collapse-btn {
    margin-left: auto;
    flex-shrink: 0;
    width: 28px;
    height: 28px;
    padding: 0;
    display: grid;
    place-items: center;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    background: var(--bg-primary);
    color: var(--text-muted);
    cursor: pointer;
  }

  .sidebar.compact .collapse-btn {
    margin-left: 0;
  }

  .collapse-btn:hover {
    color: var(--accent-primary);
    border-color: rgba(27, 217, 106, 0.4);
  }

  .nav,
  .bottom {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
  }

  .nav {
    flex: 1;
  }

  .nav-indicator {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    border-radius: var(--border-radius-md);
    background: rgba(27, 217, 106, 0.12);
    border: 1px solid rgba(27, 217, 106, 0.22);
    pointer-events: none;
    opacity: 0;
    transition:
      transform var(--motion-page, 400ms) var(--ease-spring, ease),
      height var(--motion-med, 240ms) var(--ease-out, ease),
      opacity var(--motion-fast, 160ms) var(--ease-out, ease);
    z-index: 0;
  }

  .nav-indicator.ready {
    opacity: 1;
  }

  .nav-indicator::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 56%;
    border-radius: 0 3px 3px 0;
    background: var(--accent-primary);
    box-shadow: 0 0 12px rgba(27, 217, 106, 0.45);
  }

  .nav-item {
    position: relative;
    z-index: 1;
    width: 100%;
    height: 42px;
    padding: 0 12px;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 14px;
    font-weight: 500;
    transition: background var(--motion-fast, 160ms) var(--ease-out, ease),
      color var(--motion-fast, 160ms) var(--ease-out, ease),
      border-color var(--motion-fast, 160ms) var(--ease-out, ease),
      transform var(--motion-fast, 160ms) var(--ease-spring, ease);
  }

  .sidebar.compact .nav-item {
    justify-content: center;
    padding: 0;
    gap: 0;
  }

  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
    transform: translateX(3px);
  }

  .sidebar.compact .nav-item:hover {
    transform: none;
  }

  .nav-item.active {
    color: var(--accent-primary);
    background: transparent;
  }

  .nav-item.active:hover {
    background: transparent;
    transform: translateX(1px);
  }

  .sidebar.compact .nav-item.active:hover {
    transform: none;
  }

  .nav-label {
    flex: 1;
    text-align: left;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .shortcut {
    font-size: 11px;
    color: var(--text-faint, #6b7280);
    background: var(--bg-tertiary, rgba(255, 255, 255, 0.05));
    border-radius: 4px;
    padding: 1px 6px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .nav-item.featured {
    margin-top: 8px;
    border: 1px solid rgba(27, 217, 106, 0.24);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.12), rgba(139, 92, 246, 0.08));
    color: var(--text-secondary);
  }

  .nav-item.featured:hover {
    color: var(--accent-primary);
  }

  .nav-item.featured.active {
    box-shadow: 0 0 22px rgba(27, 217, 106, 0.18);
  }

  .nav-item.add {
    margin-top: 8px;
    color: var(--accent-primary);
    border: 1px dashed rgba(27, 217, 106, 0.4);
  }

  .nav-item.add:hover {
    background: rgba(27, 217, 106, 0.1);
    border-color: var(--accent-primary);
  }

  .bottom {
    margin-top: 12px;
  }
</style>
