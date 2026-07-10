<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import Header from "./components/Header.svelte";
  import Dashboard from "./components/Dashboard.svelte";
  import IdeWorkspace from "./components/IdeWorkspace.svelte";
  import Mods from "./components/Mods.svelte";
  import Graph from "./components/Graph.svelte";
  import Diagnostics from "./components/Diagnostics.svelte";
  import Snapshots from "./components/Snapshots.svelte";
  import ConfigEditor from "./components/ConfigEditor.svelte";
  import OreGenVisualizer from "./components/OreGenVisualizer.svelte";
  import RecipeBrowser from "./components/RecipeBrowser.svelte";
  import QuestEditor from "./components/QuestEditor.svelte";
  import ToastContainer from "./components/ToastContainer.svelte";
  import Settings from "./components/Settings.svelte";
  import ProjectSettings from "./components/ProjectSettings.svelte";
  import { onMount } from "svelte";
  import { projectPath, projectInfo, recentProjects } from "./lib/store";
  import { api } from "./lib/api";

  type View =
    | "dashboard"
    | "ide"
    | "mods"
    | "graph"
    | "diagnostics"
    | "snapshots"
    | "configs"
    | "settings"
    | "project-settings"
    | "ore-gen"
    | "recipes"
    | "quests";
  let currentView: View = "dashboard";

  onMount(() => {
    const onOpenGraph = () => {
      currentView = "graph";
    };
    window.addEventListener("tuffbox:open-graph", onOpenGraph);

    void (async () => {
      try {
        const lastPath = await api.session.getLastOpened();
        if (lastPath) {
          const info = await api.project.validate(lastPath);
          recentProjects.add({ path: lastPath, info: info as any });
          projectPath.set(lastPath);
          projectInfo.set(info as any);
        }
      } catch {
        // no last project — that's fine
      }
    })();

    return () => {
      window.removeEventListener("tuffbox:open-graph", onOpenGraph);
    };
  });
</script>

<div class="app-shell">
  <Sidebar bind:currentView />
  <div class="main">
    <Header {currentView} />
    <main class="content">
      {#if currentView === "dashboard"}
        <Dashboard bind:currentView />
      {:else if currentView === "ide"}
        <IdeWorkspace />
      {:else if currentView === "mods"}
        <Mods />
      {:else if currentView === "graph"}
        <Graph />
      {:else if currentView === "diagnostics"}
        <Diagnostics />
      {:else if currentView === "snapshots"}
        <Snapshots />
      {:else if currentView === "configs"}
        <ConfigEditor />
      {:else if currentView === "settings"}
        <Settings />
      {:else if currentView === "project-settings"}
        <ProjectSettings onBack={() => (currentView = "dashboard")} />
      {:else if currentView === "ore-gen"}
        <OreGenVisualizer />
      {:else if currentView === "recipes"}
        <RecipeBrowser />
      {:else if currentView === "quests"}
        <QuestEditor />
      {/if}
    </main>
  </div>
</div>

<svelte:window
  on:keydown={(e) => {
    if (e.ctrlKey || e.metaKey) {
      switch (e.key) {
        case '1': currentView = 'dashboard'; e.preventDefault(); break;
        case '2': currentView = 'ide'; e.preventDefault(); break;
        case '3': currentView = 'mods'; e.preventDefault(); break;
        case '4': currentView = 'graph'; e.preventDefault(); break;
        case '5': currentView = 'configs'; e.preventDefault(); break;
        case '6': currentView = 'diagnostics'; e.preventDefault(); break;
        case '7': currentView = 'snapshots'; e.preventDefault(); break;
      }
    }
  }}
/>

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
