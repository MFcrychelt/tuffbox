<script lang="ts">
  import {
    LayoutDashboard,
    Package,
    GitGraph,
    Stethoscope,
    History,
    Workflow,
    Settings,
    Plus,
    Library,
  } from "lucide-svelte";

  type View = "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library";
  export let currentView: View;

  const items: { id: View; label: string; icon: any; featured?: boolean }[] = [
    { id: "dashboard", label: "Launcher", icon: LayoutDashboard },
    { id: "ide", label: "Open IDE", icon: Workflow, featured: true },
    { id: "mods", label: "Mods", icon: Package },
    { id: "graph", label: "Graph", icon: GitGraph },
    { id: "library", label: "Library", icon: Library },
    { id: "diagnostics", label: "Diagnostics", icon: Stethoscope },
    { id: "snapshots", label: "Snapshots", icon: History },
  ];
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="logo">T</div>
  </div>

  <nav class="nav">
    {#each items as item}
      <button
        class="nav-item"
        class:active={currentView === item.id}
        class:featured={item.featured}
        on:click={() => (currentView = item.id)}
        title={item.label}
      >
        <svelte:component this={item.icon} size={26} />
      </button>
    {/each}

    <button class="nav-item add" title="New project" on:click={() => (currentView = "dashboard")}>
      <Plus size={26} />
    </button>
  </nav>

  <div class="bottom">
    <button
      class="nav-item"
      class:active={currentView === "settings"}
      on:click={() => (currentView = "settings")}
      title="Settings"
    >
      <Settings size={26} />
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: 72px;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 20px 0;
    flex-shrink: 0;
  }

  .brand {
    margin-bottom: 28px;
  }

  .logo {
    width: 40px;
    height: 40px;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 20px;
    color: #000;
    box-shadow: 0 4px 14px rgba(27, 217, 106, 0.35);
  }

  .nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    flex: 1;
    width: 100%;
  }

  .nav-item {
    width: 52px;
    height: 52px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s ease;
  }

  .nav-item:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
  }

  .nav-item.active {
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }

  .nav-item.featured {
    margin-top: 10px;
    border: 1px solid rgba(27, 217, 106, 0.24);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.12), rgba(139, 92, 246, 0.08));
  }

  .nav-item.featured.active {
    box-shadow: 0 0 22px rgba(27, 217, 106, 0.18);
  }

  .nav-item.add {
    margin-top: 16px;
    color: var(--text-secondary);
    border: 1px dashed var(--border-color);
  }

  .nav-item.add:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .bottom {
    margin-top: auto;
    width: 100%;
    display: flex;
    justify-content: center;
  }
</style>
