<script lang="ts">
  import Sidebar from "./components/Sidebar.svelte";
  import Header from "./components/Header.svelte";
  import Dashboard from "./components/Dashboard.svelte";
  import ToastContainer from "./components/ToastContainer.svelte";
  import KeyboardHelp from "./components/KeyboardHelp.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import ScrollToTopButton from "./components/ScrollToTopButton.svelte";
  import ViewLoading from "./components/ViewLoading.svelte";
  import SwarmOnboarding from "./components/SwarmOnboarding.svelte";
  import ShareCapsuleDialog from "./components/ShareCapsuleDialog.svelte";
  import TaskProgressPanel from "./components/TaskProgressPanel.svelte";
  import type { ComponentType, SvelteComponent } from "svelte";
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { projectPath, projectInfo, recentProjects, launchLogPath, launchLogTitle, closeLaunchLog, autoHideWorkflowRail, sidebarMode, normalizeSidebarMode, applyUiScale, applyRoundedCorners, detectWeakHardware, youtubePlayerSession, closeYoutubePlayer, type LauncherSettings } from "./lib/store";
  import YoutubePlayer from "./components/YoutubePlayer.svelte";
  import { api } from "./lib/api";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { toasts } from "./lib/toast";
  import LaunchLogModal from "./components/LaunchLogModal.svelte";
  import {
    registerLaunchCrashListener,
    registerProcessListeners,
    refreshRunningInstances,
  } from "./lib/launch";
  import { registerSoftVerifyListeners } from "./lib/softVerify";

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
    | "chats"
    | "me";

  /** Sidebar-ish order — drives slide direction between tabs. */
  const VIEW_ORDER: View[] = [
    "dashboard",
    "me",
    "ide",
    "mods",
    "graph",
    "world",
    "library",
    "chats",
    "diagnostics",
    "crash-votes",
    "snapshots",
    "configs",
    "ore-gen",
    "recipes",
    "quests",
    "project-settings",
    "settings",
  ];

  /** Views loaded on demand (see ensureViewLoaded) — keeps startup bundle/parse cost
      to just the Dashboard on weak machines instead of every screen at once. */
  type LazyView = Exclude<View, "dashboard">;
  type LazyComponent = ComponentType<SvelteComponent>;

  const VIEW_LOADERS: Record<LazyView, () => Promise<{ default: LazyComponent }>> = {
    ide: () => import("./components/IdeWorkspace.svelte"),
    mods: () => import("./components/Mods.svelte"),
    graph: () => import("./components/Graph.svelte"),
    world: () => import("./components/World.svelte"),
    diagnostics: () => import("./components/Diagnostics.svelte"),
    "crash-votes": () => import("./components/CrashVotes.svelte"),
    snapshots: () => import("./components/Snapshots.svelte"),
    configs: () => import("./components/ConfigEditor.svelte"),
    settings: () => import("./components/Settings.svelte"),
    "project-settings": () => import("./components/ProjectSettings.svelte"),
    "ore-gen": () => import("./components/OreGenVisualizer.svelte"),
    recipes: () => import("./components/RecipeBrowser.svelte"),
    quests: () => import("./components/QuestEditor.svelte"),
    library: () => import("./components/Library.svelte"),
    chats: () => import("./components/Chats.svelte"),
    me: () => import("./components/Me.svelte"),
  };

  let loadedViews = $state<Partial<Record<LazyView, LazyComponent>>>({});
  const viewsLoading = new Set<LazyView>();
  let viewLoadError = $state<string | null>(null);

  async function ensureViewLoaded(view: View) {
    if (view === "dashboard") return;
    const key = view as LazyView;
    if (loadedViews[key] || viewsLoading.has(key)) return;
    viewsLoading.add(key);
    viewLoadError = null;
    try {
      const mod = await VIEW_LOADERS[key]();
      loadedViews = { ...loadedViews, [key]: mod.default };
    } catch (e) {
      console.error(`[App] failed to load view "${key}"`, e);
      viewLoadError = String(e);
    } finally {
      viewsLoading.delete(key);
    }
  }

  let currentView = $state<View>("dashboard");
  $effect(() => {
    void ensureViewLoaded(currentView);
  });

  let showShortcuts = $state(false);
  let showCommandPalette = $state(false);
  let contentEl = $state<HTMLElement | undefined>(undefined);
  let showSwarmOnboarding = $state(false);
  let shareCapsuleOpen = $state(false);
  let shareCapsulePath = $state("");
  let shareCapsuleExplanation = $state("");
  let shareResolutionId = $state<string | null>(null);
  let shareBusy = $state(false);
  /** 1 = deeper in nav (slide from right), -1 = back (from left). */
  let viewDir = $state(1);
  let prevViewForDir = $state<View>("dashboard");

  function prefersReducedMotion(): boolean {
    if (typeof document === "undefined") return true;
    if (document.documentElement.classList.contains("potato-pc")) return true;
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  function viewIntro(node: Element) {
    if (prefersReducedMotion()) return { duration: 0 };
    return fly(node, {
      x: viewDir * 36,
      y: 8,
      duration: 320,
      opacity: 0,
      easing: quintOut,
    });
  }

  $effect(() => {
    if (currentView !== prevViewForDir) {
      const a = VIEW_ORDER.indexOf(prevViewForDir);
      const b = VIEW_ORDER.indexOf(currentView);
      viewDir = a >= 0 && b >= 0 && a !== b ? (b > a ? 1 : -1) : 1;
      prevViewForDir = currentView;
    }
  });

  $effect(() => {
    if (currentView) {
      tick().then(() => {
        document.querySelector(".content")?.scrollTo({ top: 0 });
      });
    }
  });

  /**
   * One-time (per install) weak-hardware check. Runs after launcher settings
   * resolve; if the user has never had potato-pc decided for them, checks
   * `navigator.hardwareConcurrency`/`deviceMemory` and auto-enables reduced
   * motion on obviously low-end machines. Never re-runs and never overrides
   * a choice the user makes afterwards in Settings (guarded by
   * `perfAutoDetected`, persisted alongside the rest of LauncherSettings).
   */
  async function applyPerfAutoDetect(s: LauncherSettings) {
    if (s.perfAutoDetected) return;
    const patch: Partial<LauncherSettings> = { perfAutoDetected: true };
    if (!s.potatoPc && detectWeakHardware()) {
      patch.potatoPc = true;
      localStorage.setItem("tuffbox-reduced-motion", "1");
      document.documentElement.classList.add("potato-pc");
      toasts.info(
        "Detected lower-end hardware — enabled reduced-motion mode to keep things smooth. Turn it off anytime in Settings → Appearance.",
        8000,
      );
    }
    try {
      await api.launcher.save({ ...s, ...patch });
    } catch {
      // best-effort — perfAutoDetected stays false server-side, so we simply retry next launch
    }
  }

  onMount(() => {
    if (localStorage.getItem("tuffbox-reduced-motion") === "1") {
      document.documentElement.classList.add("potato-pc");
    }
    // Apply rounded corners ASAP (before launcher settings resolve).
    const storedRounded = localStorage.getItem("tuffbox-rounded-corners");
    applyRoundedCorners(storedRounded !== "0");
    void registerLaunchCrashListener();
    void registerProcessListeners();
    void refreshRunningInstances();
    const unlistenSoftVerify = registerSoftVerifyListeners();
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
      autoHideWorkflowRail.set(!!s.autoHideWorkflowRail);
      sidebarMode.set(normalizeSidebarMode(s.sidebarMode));
      applyUiScale(s.uiScalePercent);
      applyRoundedCorners(s.roundedCorners !== false);
      void applyPerfAutoDetect(s);
    }).catch(() => {});
    const onOpenGraph = () => {
      currentView = "graph";
    };
    window.addEventListener("tuffbox:open-graph", onOpenGraph);

    const onOpenDiagnostics = () => {
      currentView = "diagnostics";
    };
    window.addEventListener("tuffbox:open-diagnostics", onOpenDiagnostics);

    const onOpenProjectSettings = () => {
      currentView = "project-settings";
    };
    window.addEventListener("tuffbox:open-project-settings", onOpenProjectSettings);

    const onOpenMe = () => {
      currentView = "me";
    };
    window.addEventListener("tuffbox:open-me", onOpenMe);

    const onShowShortcuts = () => {
      showShortcuts = true;
    };
    window.addEventListener("tuffbox:show-shortcuts", onShowShortcuts);

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
      window.removeEventListener("tuffbox:open-project-settings", onOpenProjectSettings);
      window.removeEventListener("tuffbox:open-me", onOpenMe);
      window.removeEventListener("tuffbox:show-shortcuts", onShowShortcuts);
      window.removeEventListener("tuffbox:share-capsule", onShareCapsule);
      unlistenDistill?.();
      unlistenSoftVerify();
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

  async function shareCapsule(payload: {
    humanExplanation: string;
    actions: Record<string, unknown>[];
    fingerprintKey: string | null;
  }) {
    if (!shareCapsulePath) {
      shareCapsuleOpen = false;
      return;
    }
    shareBusy = true;
    try {
      const result: any = await invoke("publish_experience_capsule", {
        path: shareCapsulePath,
        fingerprintKey: payload.fingerprintKey,
        humanExplanation: payload.humanExplanation || shareCapsuleExplanation || null,
        actions: payload.actions ?? null,
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
    chats: true, me: true,
  };

  function handleCommandPaletteNavigate(id: string) {
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
    <main
      class="content"
      class:ide-view={currentView === "ide"}
      class:fill-view={currentView === "world" || currentView === "configs" || currentView === "quests"}
      bind:this={contentEl}
    >
      {#key currentView}
        <div class="view-pane" in:viewIntro>
          {#if currentView === "dashboard"}
            <Dashboard bind:currentView />
          {:else if currentView === "ide"}
            {#if loadedViews.ide}{@const IdeView = loadedViews.ide}<IdeView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "mods"}
            {#if loadedViews.mods}{@const ModsView = loadedViews.mods}<ModsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "graph"}
            {#if loadedViews.graph}{@const GraphView = loadedViews.graph}<GraphView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "diagnostics"}
            {#if loadedViews.diagnostics}{@const DiagnosticsView = loadedViews.diagnostics}<DiagnosticsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "crash-votes"}
            {#if loadedViews["crash-votes"]}{@const CrashVotesView = loadedViews["crash-votes"]}<CrashVotesView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "snapshots"}
            {#if loadedViews.snapshots}{@const SnapshotsView = loadedViews.snapshots}<SnapshotsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "configs"}
            {#if loadedViews.configs}{@const ConfigsView = loadedViews.configs}<ConfigsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "settings"}
            {#if loadedViews.settings}{@const SettingsView = loadedViews.settings}<SettingsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "project-settings"}
            {#if loadedViews["project-settings"]}{@const ProjectSettingsView = loadedViews["project-settings"]}<ProjectSettingsView onBack={() => (currentView = "dashboard")} />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "ore-gen"}
            {#if loadedViews["ore-gen"]}{@const OreGenView = loadedViews["ore-gen"]}<OreGenView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "recipes"}
            {#if loadedViews.recipes}{@const RecipesView = loadedViews.recipes}<RecipesView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "quests"}
            {#if loadedViews.quests}{@const QuestsView = loadedViews.quests}<QuestsView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "world"}
            {#if loadedViews.world}{@const WorldView = loadedViews.world}<WorldView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "library"}
            {#if loadedViews.library}{@const LibraryView = loadedViews.library}<LibraryView bind:currentView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "chats"}
            {#if loadedViews.chats}{@const ChatsView = loadedViews.chats}<ChatsView bind:currentView />{:else}<ViewLoading error={viewLoadError} />{/if}
          {:else if currentView === "me"}
            {#if loadedViews.me}{@const MeView = loadedViews.me}<MeView onBack={() => (currentView = "dashboard")} />{:else}<ViewLoading error={viewLoadError} />{/if}
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
  <KeyboardHelp onclose={() => (showShortcuts = false)} />
{/if}
{#if showCommandPalette}
  <CommandPalette
    onclose={() => (showCommandPalette = false)}
    onnavigate={handleCommandPaletteNavigate}
  />
{/if}

{#if showSwarmOnboarding}
  <SwarmOnboarding
    onenable={() => finishSwarmOnboarding(true)}
    onskip={() => finishSwarmOnboarding(false)}
  />
{/if}

{#if shareCapsuleOpen}
  <ShareCapsuleDialog
    path={shareCapsulePath}
    resolutionId={shareResolutionId}
    seedExplanation={shareCapsuleExplanation}
    onconfirm={shareCapsule}
    ondismiss={dismissShareCapsule}
  />
{/if}

{#if $launchLogPath}
  <LaunchLogModal projectPath={$launchLogPath} title={$launchLogTitle} onclose={closeLaunchLog} />
{/if}

{#if $youtubePlayerSession}
  {#key $youtubePlayerSession.videoId + String($youtubePlayerSession.startMini)}
    <YoutubePlayer
      videoId={$youtubePlayerSession.videoId}
      title={$youtubePlayerSession.title}
      originRect={$youtubePlayerSession.originRect}
      startMini={$youtubePlayerSession.startMini}
      onclose={closeYoutubePlayer}
    />
  {/key}
{/if}

<svelte:window
  onkeydown={(e) => {
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
    /* Compensate Chromium zoom so the shell still fills the window. */
    width: calc(100vw / var(--ui-scale, 1));
    height: calc(100vh / var(--ui-scale, 1));
    overflow: hidden;
    background: var(--bg-primary);
    color: var(--text-primary);
    zoom: var(--ui-scale, 1);
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
    display: flex;
    flex-direction: column;
  }

  .view-pane {
    width: 100%;
    min-width: 0;
  }

  /* IDE workflow fills the pane so the stage rail stays docked at the bottom
     instead of floating mid-window under short pages (Brief) or getting clipped. */
  .content.ide-view .view-pane {
    flex: 1;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .content.ide-view .view-pane > :global(.ide-workspace) {
    flex: 1;
    min-height: 0;
  }

  /* World (MCA map) needs a real height chain; otherwise flex:1 map-stage collapses to 0. */
  .content.fill-view {
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 16px 20px;
  }
  .content.fill-view .view-pane {
    flex: 1;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .content.fill-view .view-pane > :global(.worlds-view) {
    flex: 1;
    min-height: 0;
  }
  .content.fill-view .view-pane > :global(.config-editor) {
    flex: 1;
    min-height: 0;
    height: 100%;
  }
  .content.fill-view .view-pane > :global(.qe.ftbq) {
    flex: 1;
    min-height: 0;
    height: 100%;
  }
</style>
