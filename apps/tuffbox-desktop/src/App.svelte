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
  import type { Component } from "svelte";
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import { projectPath, projectInfo, recentProjects, launchLogPath, launchLogTitle, closeLaunchLog, autoHideWorkflowRail, sidebarMode, normalizeSidebarMode, applyUiScale, applyUiScaleFromSettings, applyRoundedCorners, detectWeakHardware, suggestUiScalePercent, resolveUiScaleMode, youtubePlayerSession, closeYoutubePlayer, ideStageRequest, ideSuggestedStage, requestIdeNextAction, pushIdeRecent, launcherSettingsLive, ideIssueCount, loginModalOpen, type LauncherSettings } from "./lib/store";
  import YoutubePlayer from "./components/YoutubePlayer.svelte";
  import { api } from "./lib/api";
  import { applyHomeSnapshot, ensureHomeEnrichListener } from "./lib/homeBootstrap";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { toasts } from "./lib/toast";
  import LaunchLogModal from "./components/LaunchLogModal.svelte";
  import MinecraftLogin from "./components/MinecraftLogin.svelte";
  import { launchWithFeedback, registerLaunchCrashListener, registerProcessListeners, refreshRunningInstances } from "./lib/launch";
  import { registerSoftVerifyListeners } from "./lib/softVerify";

  const SWARM_ONBOARD_KEY = "tuffbox.swarm.onboarding.done";

  function swarmOnboardLocallyDone(): boolean {
    try {
      return localStorage.getItem(SWARM_ONBOARD_KEY) === "1";
    } catch {
      return false;
    }
  }

  function markSwarmOnboardLocallyDone() {
    try {
      localStorage.setItem(SWARM_ONBOARD_KEY, "1");
    } catch {
      /* ignore */
    }
  }

  import type { View } from "./lib/types";

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
  type LazyComponent = Component<any, any, any>;

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

  function retryLoad() {
    const key = currentView as LazyView;
    if (key === "dashboard") return;
    delete loadedViews[key];
    loadedViews = { ...loadedViews };
    viewLoadError = null;
    void ensureViewLoaded(currentView);
  }

  let currentView = $state<View>("dashboard");
  $effect(() => {
    void ensureViewLoaded(currentView);
  });

  /** Diagnostics are expensive — only refresh the IDE badge when IDE is open. */
  $effect(() => {
    if (currentView !== "ide") return;
    const path = $projectPath;
    if (!path) {
      ideIssueCount.set(0);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const diags: { severity?: string }[] = await invoke("get_diagnostics", { path });
        if (cancelled) return;
        const blocking = (diags ?? []).filter((d) => {
          const sev = String(d.severity ?? "");
          return sev === "Error" || sev === "error" || sev === "critical";
        });
        ideIssueCount.set(blocking.length);
      } catch {
        /* keep last */
      }
    })();
    return () => {
      cancelled = true;
    };
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
  let shareError = $state<string | null>(null);
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
    // Orphan portals / error overlays can survive HMR and eat all clicks.
    try {
      document
        .querySelectorAll(".yp-shell, .sw-backdrop, vite-error-overlay")
        .forEach((el) => el.remove());
      const app = document.getElementById("app");
      if (app) app.style.pointerEvents = "auto";
      closeYoutubePlayer();
      showSwarmOnboarding = false;
      showCommandPalette = false;
      showShortcuts = false;
    } catch {
      /* ignore */
    }
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
    let stopHomeEnrich: (() => void) | null = null;
    void ensureHomeEnrichListener().then((stop) => {
      stopHomeEnrich = stop;
    });
    // Sync potato + concurrency from persisted launcher settings (best-effort).
    let launcherSnapshot: LauncherSettings | null = null;
    let scaleResizeTimer: ReturnType<typeof setTimeout> | undefined;
    const onUiScaleResize = () => {
      if (!launcherSnapshot || resolveUiScaleMode(launcherSnapshot) !== "auto") return;
      clearTimeout(scaleResizeTimer);
      scaleResizeTimer = setTimeout(() => {
        if (!launcherSnapshot || resolveUiScaleMode(launcherSnapshot) !== "auto") return;
        const suggested = suggestUiScalePercent();
        applyUiScale(suggested);
        if (launcherSnapshot.uiScalePercent === suggested) return;
        const next = {
          ...launcherSnapshot,
          uiScaleMode: "auto" as const,
          uiScalePercent: suggested,
        };
        launcherSnapshot = next;
        void api.launcher.save(next).catch(() => {});
      }, 150);
    };

    const onLauncherSettings = (ev: Event) => {
      const detail = (ev as CustomEvent<LauncherSettings>).detail;
      if (!detail || typeof detail !== "object") return;
      const mode = resolveUiScaleMode(detail);
      launcherSnapshot = { ...detail, uiScaleMode: mode };
      if (mode === "auto") {
        const suggested = suggestUiScalePercent();
        applyUiScale(suggested);
        launcherSnapshot = { ...launcherSnapshot, uiScalePercent: suggested };
      } else {
        applyUiScaleFromSettings(launcherSnapshot);
      }
    };
    window.addEventListener("tuffbox:launcher-settings", onLauncherSettings);

    void api.launcher.get().then((s) => {
      const mode = resolveUiScaleMode(s);
      launcherSnapshot = { ...s, uiScaleMode: mode };
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
      const applied = applyUiScaleFromSettings(launcherSnapshot);
      launcherSnapshot = { ...launcherSnapshot, uiScalePercent: applied };
      if (mode === "auto" && s.uiScalePercent !== applied) {
        void api.launcher
          .save({ ...launcherSnapshot, uiScaleMode: "auto", uiScalePercent: applied })
          .catch(() => {});
      }
      applyRoundedCorners(s.roundedCorners !== false);
      launcherSettingsLive.set(launcherSnapshot);
      void applyPerfAutoDetect(s);
    }).catch(() => {});
    window.addEventListener("resize", onUiScaleResize);
    const onOpenGraph = () => {
      currentView = "graph";
    };
    window.addEventListener("tuffbox:open-graph", onOpenGraph);

    const onOpenDiagnostics = () => {
      // Prefer IDE Diagnose stage when already in workspace; else standalone Diagnose view.
      if (currentView === "ide" || currentView === "library" || currentView === "home") {
        currentView = "ide";
        ideStageRequest.set("diagnose");
      } else {
        currentView = "diagnostics";
      }
    };
    window.addEventListener("tuffbox:open-diagnostics", onOpenDiagnostics);

    const onOpenProjectSettings = () => {
      currentView = "project-settings";
    };
    window.addEventListener("tuffbox:open-project-settings", onOpenProjectSettings);

    const onOpenSettings = () => {
      currentView = "settings";
    };
    window.addEventListener("tuffbox:open-settings", onOpenSettings);

    const onOpenMe = () => {
      currentView = "me";
    };
    window.addEventListener("tuffbox:open-me", onOpenMe);

    const onOpenLibrary = () => {
      currentView = "library";
    };
    window.addEventListener("tuffbox:open-library", onOpenLibrary);

    const onOpenCrashVotes = () => {
      currentView = "crash-votes";
    };
    window.addEventListener("tuffbox:open-crash-votes", onOpenCrashVotes);

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
      // Never block the launcher behind a fullscreen modal. First-run swarm
      // choice defaults to "off"; user can enable later in Settings.
      showSwarmOnboarding = false;
      if (isTauri() && !swarmOnboardLocallyDone()) {
        try {
          const swarm = await invoke<{
            onboardingDone?: boolean;
            onboarding_done?: boolean;
          }>("get_swarm_settings");
          const done = !!(swarm?.onboardingDone ?? swarm?.onboarding_done);
          if (!done) {
            try {
              await invoke("complete_swarm_onboarding", { enabled: false });
            } catch {
              /* ignore — local flag still prevents any future modal */
            }
          }
        } catch {
          /* ignore */
        }
        markSwarmOnboardLocallyDone();
      }
      try {
        // Desktop shortcut / CLI: `--launch <manifest>` opens the instance and starts the client.
        const pendingLaunch = await api.files.takePendingLaunch();
        if (pendingLaunch) {
          try {
            const info = await api.project.validate(pendingLaunch);
            const manifestPath = info.manifestPath || pendingLaunch;
            recentProjects.add({ path: manifestPath, info: info as any });
            projectPath.set(manifestPath);
            projectInfo.set(info as any);
            void api.session.setLastOpened(manifestPath).catch(() => {});
            currentView = "library";
            void launchWithFeedback({ path: manifestPath, profile: "client" });
          } catch (e) {
            toasts.error(`Could not launch from shortcut: ${e}`);
          }
        } else {
          // One invoke: recent + lastOpened + validate + auth cache + stats/icons.
          const snap = await api.home.bootstrap();
          applyHomeSnapshot(snap);
        }
      } catch {
        // Fallback: hydrate recent list only (old binary / offline).
        try {
          await recentProjects.hydrateFromDisk();
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
      }
    })();

    return () => {
      stopHomeEnrich?.();
      window.removeEventListener("tuffbox:open-graph", onOpenGraph);
      window.removeEventListener("tuffbox:open-diagnostics", onOpenDiagnostics);
      window.removeEventListener("tuffbox:open-project-settings", onOpenProjectSettings);
      window.removeEventListener("tuffbox:open-settings", onOpenSettings);
      window.removeEventListener("tuffbox:open-me", onOpenMe);
      window.removeEventListener("tuffbox:open-library", onOpenLibrary);
      window.removeEventListener("tuffbox:open-crash-votes", onOpenCrashVotes);
      window.removeEventListener("tuffbox:show-shortcuts", onShowShortcuts);
      window.removeEventListener("tuffbox:share-capsule", onShareCapsule);
      window.removeEventListener("tuffbox:launcher-settings", onLauncherSettings);
      window.removeEventListener("resize", onUiScaleResize);
      clearTimeout(scaleResizeTimer);
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
    shareError = null;
    shareBusy = false;
    shareCapsuleOpen = true;
  }

  async function finishSwarmOnboarding(enabled: boolean) {
    // Dismiss first so a slow/hung IPC never leaves the UI unclickable.
    showSwarmOnboarding = false;
    markSwarmOnboardLocallyDone();
    try {
      await invoke("complete_swarm_onboarding", { enabled });
      toasts.success(
        enabled
          ? "TuffSwarm network enabled"
          : "Network disabled — enable anytime in Settings",
      );
    } catch (e) {
      toasts.error(String(e));
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
    shareError = null;
    try {
      const result: any = await invoke("publish_experience_capsule", {
        path: shareCapsulePath,
        fingerprintKey: payload.fingerprintKey,
        humanExplanation: payload.humanExplanation || shareCapsuleExplanation || null,
        actions: payload.actions ?? null,
      });
      if (result?.published) {
        if (result?.supabaseOk) {
          toasts.success("Fix shared with the community network — other clients can reuse it");
        } else if (result?.p2pGossipOk === false) {
          toasts.success(
            `Saved on this PC and local P2P node; gossip to peers failed: ${result?.p2pGossipError ?? "no mesh peers"}`,
          );
        } else if (result?.p2pConfigured) {
          toasts.success("Fix shared with TuffSwarm peers — other clients can reuse it");
        } else if (result?.hubConfigured) {
          toasts.success("Fix shared with the swarm hub — other clients can reuse it");
        } else {
          toasts.success("Fix shared on the network — other clients can reuse it");
        }
      } else if (result?.sharedLocal) {
        toasts.success(
          result?.hubConfigured || result?.p2pConfigured || result?.supabaseConfigured
            ? `Saved on this PC; remote publish failed: ${result?.error ?? "unknown"}`
            : "Saved to shared local capsule store (enable TuffSwarm / P2P or set hub URL to sync)",
        );
      } else {
        toasts.success("Capsule saved");
      }
      shareCapsuleOpen = false;
    } catch (err) {
      const msg = String(err);
      shareError = msg;
      toasts.error(msg);
    } finally {
      shareBusy = false;
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
    shareError = null;
    shareBusy = false;
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
      import("./lib/store").then(({ openAddInstance }) => {
        currentView = "dashboard";
        openAddInstance("blank");
      });
      return;
    }
    if (id === "settings-java") {
      import("./lib/store").then(({ openLauncherSettings }) => {
        openLauncherSettings("java");
      });
      return;
    }
    if (id === "shortcuts") {
      showShortcuts = true;
      return;
    }
    if (id === "project-settings") {
      currentView = "project-settings";
      return;
    }
    if (id.startsWith("ide:")) {
      const stage = id.slice(4);
      ideStageRequest.set(stage);
      currentView = "ide";
      pushIdeRecent(id, `IDE · ${stage}`);
      return;
    }
    if (id === "ide") {
      ideStageRequest.set($ideSuggestedStage || "content");
      currentView = "ide";
      return;
    }
    if (id === "action:test-launch") {
      if ($projectPath) {
        if (currentView !== "ide") {
          ideStageRequest.set("test");
          currentView = "ide";
        }
        void launchWithFeedback({ path: $projectPath, profile: "client" });
      }
      return;
    }
    if (id === "action:next") {
      if (currentView !== "ide") {
        ideStageRequest.set($ideSuggestedStage || "content");
        currentView = "ide";
      }
      requestIdeNextAction();
      return;
    }
    if (id === "action:refresh-graph") {
      ideStageRequest.set("resolve");
      currentView = "ide";
      return;
    }
    if (id === "action:open-folder") {
      if ($projectPath) void invoke("open_project_folder", { path: $projectPath });
      return;
    }
    if (id === "action:optimize-pack") {
      ideStageRequest.set("content");
      currentView = "ide";
      // Content stage mounts Mods asynchronously — delay so the listener is ready.
      queueMicrotask(() => {
        window.dispatchEvent(new CustomEvent("tuffbox:open-optimize-pack"));
      });
      setTimeout(() => {
        window.dispatchEvent(new CustomEvent("tuffbox:open-optimize-pack"));
      }, 120);
      return;
    }
    if (id === "action:export-mrpack") {
      ideStageRequest.set("export");
      currentView = "ide";
      return;
    }
    if (id in VIEW_SET) {
      currentView = id as View;
    }
  }
</script>

<div class="app-shell">
  <Sidebar bind:currentView />
  <div class="main">
    {#if currentView !== "ide"}
      <Header {currentView} />
    {/if}
    <main
      class="content"
      class:ide-view={currentView === "ide"}
      class:fill-view={
        currentView === "world" ||
        currentView === "configs" ||
        currentView === "quests" ||
        currentView === "mods" ||
        currentView === "graph" ||
        currentView === "library" ||
        currentView === "chats" ||
        currentView === "diagnostics" ||
        currentView === "snapshots"
      }
      bind:this={contentEl}
    >
      {#key currentView}
        <div class="view-pane" in:viewIntro>
          {#if currentView === "dashboard"}
            <Dashboard bind:currentView />
          {:else if currentView === "ide"}
            {#if loadedViews.ide}{@const IdeView = loadedViews.ide}<IdeView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "mods"}
            {#if loadedViews.mods}{@const ModsView = loadedViews.mods}<ModsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "graph"}
            {#if loadedViews.graph}{@const GraphView = loadedViews.graph}<GraphView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "diagnostics"}
            {#if loadedViews.diagnostics}{@const DiagnosticsView = loadedViews.diagnostics}<DiagnosticsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "crash-votes"}
            {#if loadedViews["crash-votes"]}{@const CrashVotesView = loadedViews["crash-votes"]}<CrashVotesView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "snapshots"}
            {#if loadedViews.snapshots}{@const SnapshotsView = loadedViews.snapshots}<SnapshotsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "configs"}
            {#if loadedViews.configs}{@const ConfigsView = loadedViews.configs}<ConfigsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "settings"}
            {#if loadedViews.settings}{@const SettingsView = loadedViews.settings}<SettingsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "project-settings"}
            {#if loadedViews["project-settings"]}{@const ProjectSettingsView = loadedViews["project-settings"]}<ProjectSettingsView onBack={() => (currentView = "dashboard")} />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "ore-gen"}
            {#if loadedViews["ore-gen"]}{@const OreGenView = loadedViews["ore-gen"]}<OreGenView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "recipes"}
            {#if loadedViews.recipes}{@const RecipesView = loadedViews.recipes}<RecipesView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "quests"}
            {#if loadedViews.quests}{@const QuestsView = loadedViews.quests}<QuestsView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "world"}
            {#if loadedViews.world}{@const WorldView = loadedViews.world}<WorldView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "library"}
            {#if loadedViews.library}{@const LibraryView = loadedViews.library}<LibraryView bind:currentView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "chats"}
            {#if loadedViews.chats}{@const ChatsView = loadedViews.chats}<ChatsView bind:currentView />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
          {:else if currentView === "me"}
            {#if loadedViews.me}{@const MeView = loadedViews.me}<MeView onBack={() => (currentView = "dashboard")} />{:else}<ViewLoading error={viewLoadError} onRetry={retryLoad} />{/if}
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
{#if $loginModalOpen}
  <MinecraftLogin onclose={() => loginModalOpen.set(false)} />
{/if}
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
    onEnable={() => finishSwarmOnboarding(true)}
    onSkip={() => finishSwarmOnboarding(false)}
  />
{/if}

{#if shareCapsuleOpen}
  <ShareCapsuleDialog
    path={shareCapsulePath}
    resolutionId={shareResolutionId}
    seedExplanation={shareCapsuleExplanation}
    {shareBusy}
    {shareError}
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
    if (e.key === "k" && (e.ctrlKey || e.metaKey)) {
      showCommandPalette = !showCommandPalette;
      e.preventDefault();
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      switch (e.key) {
        case "1": currentView = "dashboard"; e.preventDefault(); break;
        case "2":
          ideStageRequest.set($ideSuggestedStage || "content");
          currentView = "ide";
          e.preventDefault();
          break;
        case "3":
          ideStageRequest.set("content");
          currentView = "ide";
          e.preventDefault();
          break;
        case "4":
          ideStageRequest.set("resolve");
          currentView = "ide";
          e.preventDefault();
          break;
        case "5":
          ideStageRequest.set("configs");
          currentView = "ide";
          e.preventDefault();
          break;
        case "6":
          ideStageRequest.set("diagnose");
          currentView = "ide";
          e.preventDefault();
          break;
        case "7":
          ideStageRequest.set("snapshots");
          currentView = "ide";
          e.preventDefault();
          break;
        case "8":
          ideStageRequest.set("world-map");
          currentView = "ide";
          e.preventDefault();
          break;
      }
      return;
    }
    if (e.key === "?" && !showShortcuts) {
      showShortcuts = true;
      e.preventDefault();
      return;
    }
    if (e.key === "Escape") {
      if (showSwarmOnboarding) {
        void finishSwarmOnboarding(false);
        e.preventDefault();
      } else if ($youtubePlayerSession) {
        closeYoutubePlayer();
        e.preventDefault();
      } else if (showCommandPalette) {
        showCommandPalette = false;
        e.preventDefault();
      } else if (showShortcuts) {
        showShortcuts = false;
        e.preventDefault();
      }
    }
  }}
/>

<style>
  .app-shell {
    display: flex;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--app-shell-bg, var(--bg-primary));
    color: var(--text-primary);
    pointer-events: auto;
    /* UI scale: zoom on <html> via applyUiScale — not on this shell. */
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
    scrollbar-gutter: stable;
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
  .content.fill-view:has(:global(.library)) {
    padding: 0 16px 12px;
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
  .content.fill-view .view-pane > :global(.mods),
  .content.fill-view .view-pane > :global(.library),
  .content.fill-view .view-pane > :global(.diagnostics),
  .content.fill-view .view-pane > :global(.chats) {
    flex: 1;
    min-height: 0;
    height: 100%;
  }
  .content.fill-view .view-pane > :global(.diagnostics) {
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .content.fill-view .view-pane > :global(.graph) {
    flex: 1;
    min-height: 0;
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .content.fill-view .view-pane > :global(.snapshots) {
    flex: 1;
    min-height: 0;
    height: 100%;
    overflow-y: auto;
  }
</style>
