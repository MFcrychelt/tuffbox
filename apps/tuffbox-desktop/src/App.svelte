<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import Header from "./components/Header.svelte";
  import Dashboard from "./components/Dashboard.svelte";
  import IdeWorkspace from "./components/IdeWorkspace.svelte";
  import Mods from "./components/Mods.svelte";
  import Graph from "./components/Graph.svelte";
  import Diagnostics from "./components/Diagnostics.svelte";
  import CrashVotes from "./components/CrashVotes.svelte";
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
  import SwarmOnboarding from "./components/SwarmOnboarding.svelte";
  import ShareCapsuleDialog from "./components/ShareCapsuleDialog.svelte";
  import TaskProgressPanel from "./components/TaskProgressPanel.svelte";
  import { onMount, tick } from "svelte";
  import { projectPath, projectInfo, recentProjects, launchLogPath, closeLaunchLog } from "./lib/store";
  import { api } from "./lib/api";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { toasts } from "./lib/toast";
  import LaunchLogModal from "./components/LaunchLogModal.svelte";

  type View =
    | "dashboard"
    | "ide"
    | "mods"
    | "graph"
    | "world"
    | "diagnostics"
    | "crash-votes"
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
  let showSwarmOnboarding = false;
  let shareCapsuleOpen = false;
  let shareCapsulePath = "";
  let shareCapsuleExplanation = "";
  let shareResolutionId: string | null = null;
  let shareBusy = false;

  $: if (currentView) {
    tick().then(() => {
      document.querySelector(".content")?.scrollTo({ top: 0 });
    });
  }

  onMount(() => {
    if (localStorage.getItem("tuffbox-reduced-motion") === "1") {
      document.documentElement.classList.add("potato-pc");
    }
    // Sync potato + concurrency from persisted launcher settings (best-effort).
    void api.launcher.get().then((s) => {
      if (s.potatoPc) {
        localStorage.setItem("tuffbox-reduced-motion", "1");
        document.documentElement.classList.add("potato-pc");
      }
      if (s.theme) {
        localStorage.setItem("tuffbox-theme", s.theme);
        document.documentElement.setAttribute("data-theme", s.theme === "light" ? "tuffbox-light" : s.theme);
      }
    }).catch(() => {});
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

    const onShareCapsule = (ev: Event) => {
      const detail = (ev as CustomEvent).detail as {
        path?: string;
        marker?: { humanExplanation?: string };
        resolution?: { id?: string; humanExplanation?: string };
      };
      openDistillDialog({
        path: detail?.path ?? "",
        explanation:
          detail?.resolution?.humanExplanation ?? detail?.marker?.humanExplanation ?? "",
        resolutionId: detail?.resolution?.id ?? null,
      });
    };
    window.addEventListener("tuffbox:share-capsule", onShareCapsule);

    let unlistenDistill: UnlistenFn | null = null;
    void listen<{
      path?: string;
      resolution?: { id?: string; humanExplanation?: string };
    }>("tuffbox:distill-resolution", (event) => {
      const payload = event.payload;
      openDistillDialog({
        path: payload?.path ?? "",
        explanation: payload?.resolution?.humanExplanation ?? "",
        resolutionId: payload?.resolution?.id ?? null,
      });
    }).then((u) => {
      unlistenDistill = u;
    });

    void (async () => {
      try {
        const swarm = await invoke<{ onboardingDone?: boolean; enabled?: boolean }>(
          "get_swarm_settings",
        );
        if (!swarm?.onboardingDone) {
          showSwarmOnboarding = true;
        }
      } catch {
        // ignore
      }
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
      window.removeEventListener("tuffbox:share-capsule", onShareCapsule);
      unlistenDistill?.();
    };
  });

  function openDistillDialog(opts: {
    path: string;
    explanation: string;
    resolutionId: string | null;
  }) {
    if (!opts.path || shareCapsuleOpen) return;
    shareCapsulePath = opts.path;
    shareCapsuleExplanation = opts.explanation;
    shareResolutionId = opts.resolutionId;
    shareCapsuleOpen = true;
  }

  async function finishSwarmOnboarding(enabled: boolean) {
    try {
      await invoke("complete_swarm_onboarding", { enabled });
      toasts.success(
        enabled
          ? "TuffSwarm network enabled"
          : "Network disabled — enable anytime in Settings",
      );
    } catch (e) {
      toasts.error(String(e));
    } finally {
      showSwarmOnboarding = false;
    }
  }

  async function shareCapsule(
    e: CustomEvent<{
      humanExplanation: string;
      actions: Record<string, unknown>[];
      fingerprintKey: string | null;
    }>,
  ) {
    if (!shareCapsulePath) {
      shareCapsuleOpen = false;
      return;
    }
    shareBusy = true;
    try {
      const result: any = await invoke("publish_experience_capsule", {
        path: shareCapsulePath,
        fingerprintKey: e.detail.fingerprintKey,
        humanExplanation: e.detail.humanExplanation || shareCapsuleExplanation || null,
        actions: e.detail.actions ?? null,
      });
      if (result?.published) {
        toasts.success("Fix shared with the swarm hub — other clients can reuse it");
      } else if (result?.sharedLocal) {
        toasts.success(
          result?.hubConfigured
            ? `Saved on this PC; hub publish failed: ${result?.error ?? "unknown"}`
            : "Saved to shared local capsule store (set Swarm hub URL to sync with other PCs)",
        );
      } else {
        toasts.success("Capsule saved");
      }
    } catch (err) {
      toasts.error(String(err));
    } finally {
      shareBusy = false;
      shareCapsuleOpen = false;
    }
  }

  async function dismissShareCapsule() {
    if (shareCapsulePath) {
      try {
        await invoke("dismiss_share_prompt", { path: shareCapsulePath });
      } catch {
        // ignore
      }
    }
    shareCapsuleOpen = false;
  }

  const VIEW_SET: Record<string, boolean> = {
    dashboard: true, ide: true, mods: true, graph: true, world: true,
    diagnostics: true, "crash-votes": true, snapshots: true, configs: true, settings: true,
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
        <div class="view-wrapper tb-view-enter">
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
          {:else if currentView === "crash-votes"}
            <CrashVotes />
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
            <Me onBack={() => (currentView = "dashboard")} />
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
<TaskProgressPanel />
{#if showShortcuts}
  <KeyboardHelp on:close={() => (showShortcuts = false)} />
{/if}
{#if showCommandPalette}
  <CommandPalette
    on:close={() => (showCommandPalette = false)}
    on:navigate={handleCommandPaletteNavigate}
  />
{/if}

{#if showSwarmOnboarding}
  <SwarmOnboarding
    on:enable={() => finishSwarmOnboarding(true)}
    on:skip={() => finishSwarmOnboarding(false)}
  />
{/if}

{#if shareCapsuleOpen}
  <ShareCapsuleDialog
    path={shareCapsulePath}
    resolutionId={shareResolutionId}
    seedExplanation={shareCapsuleExplanation}
    on:confirm={shareCapsule}
    on:dismiss={dismissShareCapsule}
  />
{/if}

{#if $launchLogPath}
  <LaunchLogModal projectPath={$launchLogPath} on:close={closeLaunchLog} />
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
