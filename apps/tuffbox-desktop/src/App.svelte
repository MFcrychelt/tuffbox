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
  import Library from "./components/Library.svelte";
  import World from "./components/World.svelte";
  import ToastContainer from "./components/ToastContainer.svelte";
  import KeyboardHelp from "./components/KeyboardHelp.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import ScrollToTopButton from "./components/ScrollToTopButton.svelte";
  import Settings from "./components/Settings.svelte";
  import ProjectSettings from "./components/ProjectSettings.svelte";
  import Me from "./components/Me.svelte";
  import { onMount, tick } from "svelte";
  import { projectPath, projectInfo, recentProjects } from "./lib/store";
  import { api } from "./lib/api";

  type View =
    | "dashboard"
    | "ide"
    | "mods"
    | "graph"
    | "world"
    | "diagnostics"
    | "snapshots"
    | "configs"
    | "settings"
    | "project-settings"
    | "ore-gen"
    | "recipes"
    | "quests"
    | "library"
    | "me";
  let currentView: View = "dashboard";
  let showShortcuts = false;
  let showCommandPalette = false;
  let contentEl: HTMLElement;

  $: if (currentView) {
    tick().then(() => {
      document.querySelector(".content")?.scrollTo({ top: 0 });
    });
  }

  onMount(() => {
    const onOpenGraph = () => {
      currentView = "graph";
    };
    window.addEventListener("tuffbox:open-graph", onOpenGraph);

    const onOpenDiagnostics = () => {
      currentView = "diagnostics";
    };
    window.addEventListener("tuffbox:open-diagnostics", onOpenDiagnostics);

    const onOpenMe = () => {
      currentView = "me";
    };
    window.addEventListener("tuffbox:open-me", onOpenMe);

    void (async () => {
      try {
        const lastPath = await api.session.getLastOpened();
        if (lastPath) {
          const info = await api.project.validate(lastPath);
          const manifestPath = info.manifestPath || lastPath;
          recentProjects.add({ path: manifestPath, info: info as any });
          projectPath.set(manifestPath);
          projectInfo.set(info as any);
        }
      } catch {
        // no last project — that's fine
      }
    })();

    return () => {
      window.removeEventListener("tuffbox:open-graph", onOpenGraph);
      window.removeEventListener("tuffbox:open-diagnostics", onOpenDiagnostics);
      window.removeEventListener("tuffbox:open-me", onOpenMe);
    };
  });

  const VIEW_SET: Record<string, boolean> = {
    dashboard: true, ide: true, mods: true, graph: true, world: true,
    diagnostics: true, snapshots: true, configs: true, settings: true,
    "project-settings": true, "ore-gen": true, recipes: true, quests: true, library: true,
    me: true,
  };

  function handleCommandPaletteNavigate(e: CustomEvent<string>) {
    const id = e.detail;
    if (id === "new-instance") {
      import("./lib/store").then(({ newProjectOpen }) => {
        currentView = "dashboard";
        newProjectOpen.set(true);
      });
    } else if (id === "shortcuts") {
      showShortcuts = true;
    } else if (id in VIEW_SET) {
      currentView = id as View;
    }
  }
</script>

<div class="app-shell">
  <Sidebar bind:currentView />
  <div class="main">
    <Header {currentView} />
    <main class="content" class:ide-view={currentView === "ide"} bind:this={contentEl}>
      {#key currentView}
        <div class="view-wrapper">
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
          {:else if currentView === "world"}
            <World />
          {:else if currentView === "library"}
            <Library bind:currentView />
          {:else if currentView === "me"}
            <Me />
          {/if}
        </div>
      {/key}
    </main>
    {#if currentView !== "ide"}
      <ScrollToTopButton container={contentEl} />
    {/if}
  </div>
</div>

<ToastContainer />
{#if showShortcuts}
  <KeyboardHelp on:close={() => (showShortcuts = false)} />
{/if}
{#if showCommandPalette}
  <CommandPalette
    on:close={() => (showCommandPalette = false)}
    on:navigate={handleCommandPaletteNavigate}
  />
{/if}

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
        case '8': currentView = 'world'; e.preventDefault(); break;
      }
    } else if (e.key === '?' && !showShortcuts) {
      showShortcuts = true;
      e.preventDefault();
    } else if (e.key === "k" && (e.ctrlKey || e.metaKey)) {
      showCommandPalette = !showCommandPalette;
      e.preventDefault();
    }
  }}
/>

<style>
  .app-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    overflow: hidden;
    background: var(--bg-primary);
    color: var(--text-primary);
  }

  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: 24px 32px;
    position: relative;
  }

  .content.ide-view {
    overflow: hidden;
    padding: 0;
  }

  .view-wrapper {
    height: 100%;
    width: 100%;
  }
</style>
