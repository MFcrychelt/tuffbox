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
  } from "lucide-svelte";
  import { afterUpdate, tick } from "svelte";
  import { newProjectOpen } from "../lib/store";

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

  function openNewProject() {
    // Dashboard owns the modal, so make sure we're on that view before
    // raising the flag — otherwise the modal component wouldn't be mounted.
    currentView = "dashboard";
    newProjectOpen.set(true);
  }

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

  $: currentView, void syncIndicator();
  afterUpdate(() => {
    void syncIndicator();
  });
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="logo">T</div>
    <span class="brand-name">TuffBox</span>
  </div>

  <nav class="nav" bind:this={navEl}>
    <div
      class="nav-indicator"
      class:ready={indicatorReady && !indicatorInBottom}
      style={`transform: translateY(${indicatorY}px); height: ${indicatorH}px`}
      aria-hidden="true"
    ></div>
    {#each items as item}
      <button
        class="nav-item tb-icon-hover"
        class:active={currentView === item.id}
        class:featured={item.featured}
        on:click={() => (currentView = item.id)}
        title={item.shortcut ? `${item.label} (${item.shortcut})` : item.label}
      >
        <svelte:component this={item.icon} size={20} />
        <span class="nav-label">{item.label}</span>
        {#if item.shortcut}
          <span class="shortcut">{item.shortcut}</span>
        {/if}
      </button>
    {/each}

    <button class="nav-item add tb-icon-hover" title="New instance" on:click={openNewProject}>
      <Plus size={20} />
      <span class="nav-label">New</span>
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
      title="Settings"
    >
      <Settings size={20} />
      <span class="nav-label">Settings</span>
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: 212px;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    padding: 16px 12px;
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 18px;
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

  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
    transform: translateX(3px);
  }

  .nav-item.active {
    color: var(--accent-primary);
    background: transparent;
  }

  .nav-item.active:hover {
    background: transparent;
    transform: translateX(1px);
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
