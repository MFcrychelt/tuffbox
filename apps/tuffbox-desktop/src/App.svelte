<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import Header from "./components/Header.svelte";
  import Dashboard from "./components/Dashboard.svelte";
  import Mods from "./components/Mods.svelte";
  import Graph from "./components/Graph.svelte";
  import Diagnostics from "./components/Diagnostics.svelte";
  import Snapshots from "./components/Snapshots.svelte";
  import Settings from "./components/Settings.svelte";
  import ProjectSettings from "./components/ProjectSettings.svelte";
  import { projectPath, projectInfo } from "./lib/store";

  type View =
    | "dashboard"
    | "mods"
    | "graph"
    | "diagnostics"
    | "snapshots"
    | "settings"
    | "project-settings";
  let currentView: View = "dashboard";
</script>

<div class="app-shell">
  <Sidebar bind:currentView />
  <div class="main">
    <Header {currentView} />
    <main class="content">
      {#if currentView === "dashboard"}
        <Dashboard bind:currentView />
      {:else if currentView === "mods"}
        <Mods />
      {:else if currentView === "graph"}
        <Graph />
      {:else if currentView === "diagnostics"}
        <Diagnostics />
      {:else if currentView === "snapshots"}
        <Snapshots />
      {:else if currentView === "settings"}
        <Settings />
      {:else if currentView === "project-settings"}
        <ProjectSettings onBack={() => (currentView = "dashboard")} />
      {/if}
    </main>
  </div>
</div>

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .content {
    flex: 1;
    overflow: auto;
    padding: 24px 32px;
  }
</style>
