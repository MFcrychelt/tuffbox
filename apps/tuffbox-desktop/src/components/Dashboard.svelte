<script lang="ts">
  import { onMount } from "svelte";
  import {
    Play,
    Square,
    Settings,
    Workflow,
    LogIn,
    User,
    Package,
    GitGraph,
    Stethoscope,
    History,
    Puzzle,
    Sparkles,
    FolderOpen,
    HardDrive,
    Clock,
    Users,
    ShieldAlert,
    Gamepad2,
  } from "@lucide/svelte";
  import HeadAvatar from "./HeadAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    authState,
    skinPath,
    newProjectOpen,
    isLaunching,
    runningInstances,
    isProjectRunning,
    loginTypeLabel,
    formatPlaytime,
    ideStageRequest,
    ideSuggestedStage,
    ideIssueCount,
    launcherSettingsLive,
    type RecentProject,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import { launchWithFeedback, killWithFeedback, registerLaunchCrashListener } from "../lib/launch";
  import {
    fetchCrashFixBanner,
    rollbackLastCrashFix,
  } from "../lib/softVerify";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import MinecraftLogin from "./MinecraftLogin.svelte";
  import SkinPreview3D from "./SkinPreview3D.svelte";
  import AccountManager from "./AccountManager.svelte";
  import InstanceHome from "./InstanceHome.svelte";
  import YoutubeFeed from "./YoutubeFeed.svelte";

  import type { View } from "../lib/types";

  let { currentView = $bindable() }: { currentView: View } = $props();

  let authReady = $state(false);

  type ProjectStatBrief = { playtime: number; lastLaunch: string | null };
  let projectStats = $state<Record<string, ProjectStatBrief>>({});

  async function loadProjectStats(path: string) {
    try {
      const s = await api.stats.get(path);
      projectStats[path] = {
        playtime: s.totalPlaytimeSeconds ?? 0,
        lastLaunch: s.lastLaunch ?? null,
      };
      projectStats = { ...projectStats };
    } catch {
      projectStats[path] = { playtime: 0, lastLaunch: null };
      projectStats = { ...projectStats };
    }
  }

  const statsRequested = new Set<string>();

  function ensureStats(paths: string[]) {
    for (const path of paths) {
      if (statsRequested.has(path) || projectStats[path] !== undefined) continue;
      statsRequested.add(path);
      void loadProjectStats(path);
    }
  }

  $effect(() => {
    ensureStats($recentProjects.map((p) => p.path));
  });

  let selectedPath = $state<string | null>($projectPath);
  let showLoginModal = $state(false);
  let showAccountManager = $state(false);
  let potatoPc = $state(false);
  let accountSwitchBusy = $state(false);
  let accountSkinPaths = $state<Record<string, string>>({});

  const selectedProject = $derived($recentProjects.find((p) => p.path === selectedPath));
  const selectedRunning = $derived(isProjectRunning(selectedPath, $runningInstances));
  const hasInstanceHome = $derived(!!(selectedPath && selectedProject));
  const hideInstanceHome = $derived(!!$launcherSettingsLive?.hideInstanceHome);

  // The sidebar rail switches instances through the global store — mirror it here.
  $effect(() => {
    const p = $projectPath;
    if (p && p !== selectedPath) selectedPath = p;
  });
  const skinUrl = $derived($authState.profile?.skinUrl ?? null);
  const capeUrl = $derived($authState.profile?.capeUrl ?? null);
  const accountKey = $derived($authState.activeAccountUuid ?? $authState.profile?.uuid ?? "");
  /** Shrink Minecraft nick under the skin preview so long names fit the 320px rail. */
  const skinNameFontPx = $derived.by(() => {
    const n = ($authState.profile?.name ?? "").length;
    if (n <= 8) return 12;
    if (n <= 12) return 11;
    if (n <= 16) return 10;
    if (n <= 20) return 9;
    return 8;
  });

  type CrashFixBanner = {
    snapshotId: string;
    fingerprintKey: string;
    planSource?: string | null;
    humanExplanation: string;
    matchedCaseIds: string[];
    actionsSummary: string[];
    createdAt: string;
    resolved: boolean;
    rolledBack: boolean;
    softVerifyStartedUnix?: number | null;
    minPlaytimeSecs: number;
  };
  let crashFixBanner = $state<CrashFixBanner | null>(null);
  let crashFixBusy = $state(false);
  let softVerifyNowUnix = $state(Math.floor(Date.now() / 1000));

  const softVerifyRemainingSecs = $derived.by(() => {
    const b = crashFixBanner;
    if (!b?.softVerifyStartedUnix) return null;
    const min = Number(b.minPlaytimeSecs ?? 180);
    const started = Number(b.softVerifyStartedUnix);
    const elapsed = Math.max(0, softVerifyNowUnix - started);
    return Math.max(0, min - elapsed);
  });

  async function refreshCrashFixBanner(path: string | null) {
    if (!path) {
      crashFixBanner = null;
      return;
    }
    crashFixBanner = await fetchCrashFixBanner(path);
  }

  $effect(() => {
    void refreshCrashFixBanner(selectedPath);
  });

  $effect(() => {
    const started = crashFixBanner?.softVerifyStartedUnix;
    if (!started) return;
    softVerifyNowUnix = Math.floor(Date.now() / 1000);
    const id = setInterval(() => {
      softVerifyNowUnix = Math.floor(Date.now() / 1000);
    }, 1000);
    return () => clearInterval(id);
  });

  async function onRollbackCrashFix() {
    if (!selectedPath || crashFixBusy) return;
    crashFixBusy = true;
    try {
      const ok = await rollbackLastCrashFix(selectedPath);
      if (ok) await refreshCrashFixBanner(selectedPath);
    } finally {
      crashFixBusy = false;
    }
  }

  async function ensureAccountSkinPaths(uuids: string[]) {
    for (const uuid of uuids) {
      if (!uuid || accountSkinPaths[uuid]) continue;
      try {
        const path = await api.mcAuth.getSkinPath(uuid);
        accountSkinPaths = { ...accountSkinPaths, [uuid]: path };
      } catch {
        /* ignore — HeadAvatar falls back */
      }
    }
  }

  $effect(() => {
    void ensureAccountSkinPaths($authState.accounts.map((a) => a.uuid));
  });

  async function switchHomeAccount(uuid: string) {
    if (accountSwitchBusy || uuid === $authState.activeAccountUuid) return;
    accountSwitchBusy = true;
    try {
      const state = await api.mcAuth.switchAccount(uuid);
      authState.set(state);
      if (state.profile) {
        try {
          const path = await api.mcAuth.getSkinPath(state.profile.uuid);
          skinPath.set(path);
          accountSkinPaths = { ...accountSkinPaths, [state.profile.uuid]: path };
        } catch {
          skinPath.set(null);
        }
      } else {
        skinPath.set(null);
      }
      toasts.success(`Switched to ${state.profile?.name ?? "account"}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      accountSwitchBusy = false;
    }
  }

  onMount(() => {
    let cleanup: (() => void) | undefined;
    void (async () => {
      potatoPc = document.documentElement.classList.contains("potato-pc");
      try {
        const status = await api.mcAuth.getAuthStatus();
        authState.set(status);
        if (status.loggedIn && status.profile) {
          try {
            const path = await api.mcAuth.getSkinPath(status.profile.uuid);
            skinPath.set(path);
          } catch {}
        }
      } catch {
      } finally {
        authReady = true;
      }

      if (selectedPath && !selectedProject && $recentProjects.length > 0) {
        selectProject($recentProjects[0].path);
      }

      // Global handler for JVM crashes that happen after the launch command
      // has returned "started" — surfaces a categorized, retryable toast.
      registerLaunchCrashListener();

      // Refresh playtime when a session ends.
      const { listen } = await import("@tauri-apps/api/event");
      const unlistenExit = await listen<{ id: string }>("process-exited", (event) => {
        const id = event.payload?.id;
        if (id) void loadProjectStats(id);
      });
      const unlistenSoft = await listen("tuffbox:soft-verify-outcome", () => {
        void refreshCrashFixBanner(selectedPath);
      });
      const onCrashFixApplied = () => {
        void refreshCrashFixBanner(selectedPath);
      };
      window.addEventListener("tuffbox:crash-fix-applied", onCrashFixApplied);
      cleanup = () => {
        unlistenExit();
        unlistenSoft();
        window.removeEventListener("tuffbox:crash-fix-applied", onCrashFixApplied);
      };
    })();
    return () => cleanup?.();
  });

  async function loadProject(path: string) {
    const info = await invoke("validate_project", { path }) as import("../lib/api").ProjectSummary;
    const manifestPath = info.manifestPath || path;
    const project: RecentProject = { path: manifestPath, info: info as any };
    recentProjects.add(project);
    projectPath.set(manifestPath);
    projectInfo.set(project.info);
    selectedPath = manifestPath;
  }

  async function selectProject(path: string) {
    try {
      await loadProject(path);
    } catch {
      const project = $recentProjects.find((p) => p.path === path);
      if (project) {
        selectedPath = path;
        projectPath.set(path);
        projectInfo.set(project.info);
      }
    }
  }

  async function launch() {
    if (!selectedPath) return;
    await invoke("set_last_opened_project", { path: selectedPath });
    await launchWithFeedback({ path: selectedPath, profile: "client" });
    const project = $recentProjects.find((p) => p.path === selectedPath);
    if (project) recentProjects.add(project);
    void loadProjectStats(selectedPath);
  }

  async function stopGame() {
    if (!selectedPath) return;
    await killWithFeedback(selectedPath);
  }

  function openSettings() {
    ideStageRequest.set("setup");
    currentView = "ide";
  }

  let instanceSizes = $state<Record<string, string>>({});
  let loadingSizes = $state<Record<string, boolean>>({});
  const sizeRequested = new Set<string>();

  async function loadSize(projectPath: string) {
    if (sizeRequested.has(projectPath)) return;
    sizeRequested.add(projectPath);
    loadingSizes[projectPath] = true;
    loadingSizes = { ...loadingSizes };
    try {
      instanceSizes[projectPath] = await api.instance.getSize(projectPath);
      instanceSizes = { ...instanceSizes };
    } catch {
      instanceSizes[projectPath] = "?";
      instanceSizes = { ...instanceSizes };
    } finally {
      loadingSizes[projectPath] = false;
      loadingSizes = { ...loadingSizes };
    }
  }

  function ensureSizes(paths: string[]) {
    for (const path of paths) void loadSize(path);
  }

  $effect(() => {
    ensureSizes($recentProjects.map((p) => p.path));
  });

  function openIdeStage(stage: string) {
    ideStageRequest.set(stage);
    currentView = "ide";
  }

  function openIdeSuggested() {
    ideStageRequest.set($ideSuggestedStage || "content");
    currentView = "ide";
  }

  async function refreshIdeIssueBadge() {
    if (!$projectPath) {
      ideIssueCount.set(0);
      return;
    }
    try {
      const diags: { severity?: string }[] = await invoke("get_diagnostics", { path: $projectPath });
      const blocking = (diags ?? []).filter((d) => {
        const sev = String(d.severity ?? "");
        return sev === "Error" || sev === "error" || sev === "critical";
      });
      ideIssueCount.set(blocking.length);
    } catch {
      /* keep last */
    }
  }

  $effect(() => {
    void $projectPath;
    void refreshIdeIssueBadge();
  });
</script>

<div class="home fade-slide-in">
  <!-- Top bar: Quick actions left, Avatar right -->
  <div class="top-bar">
    <div class="quick-nav">
      <button class="quick-action" onclick={() => openIdeStage("content")} title="Mods">
        <Package size={18} />
        <span>Mods</span>
      </button>
      <button class="quick-action" onclick={() => openIdeStage("resolve")} title="Dependency Graph">
        <GitGraph size={18} />
        <span>Graph</span>
      </button>
      <button class="quick-action" onclick={() => openIdeStage("diagnose")} title="Diagnostics">
        <Stethoscope size={18} />
        <span>Diagnostics</span>
      </button>
      <button class="quick-action" onclick={() => openIdeStage("snapshots")} title="Snapshots">
        <History size={18} />
        <span>Snapshots</span>
      </button>
      {#if selectedProject}
        <button class="quick-action" onclick={() => openIdeStage("recipes")} title="Recipes">
          <Puzzle size={18} />
          <span>Recipes</span>
        </button>
        <button class="quick-action" onclick={() => openIdeStage("quests")} title="Quests">
          <Sparkles size={18} />
          <span>Quests</span>
        </button>
      {/if}
    </div>

    <!-- Account avatar in top-right (sign-in lives in the skin panel) -->
    <div class="account-avatar-section">
      {#if $authState.loggedIn && $authState.profile}
        <button class="account-avatar-btn" onclick={() => (currentView = "me")} title="Me — account & playtime">
          <HeadAvatar skinSrc={$skinPath} size={32} alt={$authState.profile.name} />
          <span class="avatar-name">{$authState.profile.name}</span>
          <span
            class="avatar-badge"
            class:microsoft={$authState.loginType === "microsoft"}
            class:offline={$authState.loginType === "offline"}
            class:ygg={$authState.loginType === "yggdrasil"}
          >
            {loginTypeLabel(
              $authState.loginType,
              $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority
            )}
          </span>
        </button>
      {/if}
    </div>
  </div>

  <div class="main-layout">
    <div class="home-main">
      <!-- Hero: Play button + project info -->
      <section class="hero">
        <div class="hero-left">
          <button
            class="play-btn"
            class:stop={selectedRunning && !$isLaunching}
            onclick={selectedRunning && !$isLaunching ? stopGame : launch}
            disabled={!selectedPath || $isLaunching}
            aria-busy={$isLaunching}
          >
            {#if $isLaunching}
              <span class="spinner" aria-hidden="true"></span>
              <span class="play-text">Launching...</span>
            {:else if selectedRunning}
              <Square size={24} fill="currentColor" />
              <span class="play-text">Stop</span>
            {:else}
              <Play size={28} fill="currentColor" />
              <span class="play-text">Play</span>
            {/if}
          </button>

          <div class="hero-main">
            {#if !selectedProject}
              <div class="project-quick-info">
                <span class="project-name muted">No instance selected</span>
                <span class="project-hint">Pick an instance in the left rail or create a new one</span>
              </div>
            {/if}

            <div class="hero-actions">
              {#if selectedProject}
                <button class="action-btn primary ide-open-btn" onclick={openIdeSuggested}>
                  <Workflow size={15} />
                  IDE
                  {#if $ideIssueCount > 0}
                    <span class="ide-issue-badge" title="{$ideIssueCount} pack issue{$ideIssueCount === 1 ? '' : 's'}">{$ideIssueCount}</span>
                  {/if}
                </button>
                <button class="action-btn" onclick={openSettings}>
                  <Settings size={15} />
                  Settings
                </button>
                <button class="action-btn" onclick={() => invoke("open_project_folder", { path: selectedProject.path })}>
                  <FolderOpen size={15} />
                  Folder
                </button>
              {/if}
            </div>

            {#if crashFixBanner}
              <div class="crash-fix-banner" role="status">
                <ShieldAlert size={16} />
                <div class="crash-fix-banner-body">
                  <strong>Crash fix applied</strong>
                  <span>
                    {#if crashFixBanner.softVerifyStartedUnix}
                      Soft-verify: ~{softVerifyRemainingSecs ?? 0}s left (≥{crashFixBanner.minPlaytimeSecs}s stable play)
                    {:else}
                      Launch to soft-verify. One-click restore available.
                    {/if}
                  </span>
                  {#if crashFixBanner.actionsSummary?.length}
                    <span class="crash-fix-actions">
                      {crashFixBanner.actionsSummary.slice(0, 3).join(" · ")}
                    </span>
                  {/if}
                </div>
                <button
                  class="action-btn"
                  type="button"
                  disabled={crashFixBusy}
                  onclick={onRollbackCrashFix}
                >
                  Restore snapshot
                </button>
                <button
                  class="action-btn"
                  type="button"
                  onclick={() => (currentView = "diagnostics")}
                >
                  <Stethoscope size={14} /> Diagnostics
                </button>
              </div>
            {/if}
          </div>
        </div>

        {#if selectedProject}
          <div class="hero-right">
            <div class="instance-stats">
              <div class="stat version-stat" title="Minecraft version · loader">
                <Gamepad2 size={14} />
                <span>{selectedProject.info.minecraftVersion} · {selectedProject.info.loaderKind}</span>
              </div>
              <div
                class="stat size-stat"
                title={instanceSizes[selectedProject.path] || "Calculating size…"}
              >
                <HardDrive size={14} />
                {#if instanceSizes[selectedProject.path]}
                  <span>{instanceSizes[selectedProject.path]}</span>
                {:else if loadingSizes[selectedProject.path]}
                  <span class="skeleton skeleton-block skeleton-line short" style="width: 48px; height: 12px;"></span>
                {:else}
                  <span class="skeleton skeleton-block skeleton-line short" style="width: 48px; height: 12px;"></span>
                {/if}
              </div>
              {#if projectStats[selectedProject.path]?.playtime}
                <div class="stat">
                  <Clock size={14} />
                  <span>{formatPlaytime(projectStats[selectedProject.path].playtime)}</span>
                </div>
              {:else if projectStats[selectedProject.path] === undefined}
                <div class="stat skel-stat" aria-hidden="true">
                  <span class="skeleton skeleton-block skeleton-line short" style="width: 52px; height: 12px;"></span>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </section>

      {#if hasInstanceHome && selectedPath && !hideInstanceHome}
        <InstanceHome
          projectPath={selectedPath}
          onOpenMods={() => openIdeStage("content")}
          onOpenWorld={() => (currentView = "world")}
        />
      {/if}

      <YoutubeFeed variant="grid" />
    </div>

    <aside class="home-side">
      <div class="skin-panel" aria-busy={!authReady}>
        {#if !authReady}
          <div class="skin-skel" aria-hidden="true">
            <div class="skin-skel-canvas skeleton skeleton-block skeleton-card"></div>
            <div class="skin-skel-footer">
              <span class="skeleton skeleton-block skeleton-round" style="width: 72px; height: 22px;"></span>
              <span class="skeleton skeleton-block skeleton-round" style="width: 88px; height: 28px;"></span>
            </div>
            <div class="skin-skel-name skeleton skeleton-block skeleton-line medium" style="width: 40%; height: 14px; margin: 0 auto 12px;"></div>
            <div class="skin-skel-cape">
              <span class="skeleton skeleton-block skeleton-line short" style="width: 90px; height: 10px; margin-bottom: 10px;"></span>
              <div class="skin-skel-cape-row home-skel-stagger">
                {#each Array(3) as _, i (i)}
                  <span class="skeleton skeleton-block skeleton-round" style={`--i: ${i}; width: 100%; height: 36px;`}></span>
                {/each}
              </div>
            </div>
          </div>
        {:else if $authState.loggedIn && $authState.profile}
          {#if potatoPc}
            <div class="skin-static-fallback">
              <HeadAvatar skinSrc={$skinPath} size={120} alt={$authState.profile.name} />
              <span class="skin-static-name" style={`font-size: ${skinNameFontPx}px`}>{$authState.profile.name}</span>
            </div>
          {:else}
          <SkinPreview3D
            skinUrl={skinUrl}
            capeUrl={capeUrl}
            accountKey={accountKey}
            playerName={$authState.profile.name}
            showName={false}
            width={318}
            height={400}
          />
          {/if}
          <div class="skin-panel-footer">
            <div class="skin-meta">
              <span
                class="type-badge"
                class:microsoft={$authState.loginType === "microsoft"}
                class:offline={$authState.loginType === "offline"}
                class:ygg={$authState.loginType === "yggdrasil"}
              >
                {loginTypeLabel(
                  $authState.loginType,
                  $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority
                )}
              </span>
            </div>
            <button class="change-skin-btn" onclick={() => (showAccountManager = true)}>
              <Users size={14} />
              Manage
            </button>
          </div>
          <div
            class="skin-player-name"
            title={$authState.profile.name}
            style={`font-size: ${skinNameFontPx}px`}
          >
            {$authState.profile.name}
          </div>

          {#if $authState.accounts.length > 0}
            <div class="accounts-switcher">
              <div class="accounts-switcher-label">Accounts</div>
              <div class="accounts-switcher-list">
                {#each $authState.accounts as account (account.uuid)}
                  <button
                    type="button"
                    class="account-chip"
                    class:active={account.uuid === $authState.activeAccountUuid}
                    disabled={accountSwitchBusy}
                    title={account.name}
                    onclick={() => switchHomeAccount(account.uuid)}
                  >
                    <HeadAvatar
                      skinSrc={accountSkinPaths[account.uuid] ?? null}
                      size={22}
                      alt={account.name}
                    />
                    <span class="account-chip-name">{account.name}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/if}
        {:else}
          <div class="skin-panel-empty">
            <User size={48} />
            <p>Sign in to see your skin</p>
            <button class="action-btn accent" onclick={() => (showLoginModal = true)}>
              <LogIn size={16} />
              Sign In
            </button>
          </div>
        {/if}
      </div>
    </aside>
  </div>
</div>

{#if showLoginModal}
  <MinecraftLogin onclose={() => (showLoginModal = false)} />
{/if}

{#if showAccountManager}
  <AccountManager onclose={() => (showAccountManager = false)} />
{/if}

{#if $newProjectOpen}
  <AddInstanceModal
    onclose={() => (newProjectOpen.set(false))}
    oncreated={(path) => loadProject(path)}
  />
{/if}

<style>
  .home {
    max-width: 1400px;
    margin: 0 auto;
  }

  /* ─── Top Bar ─────────────────────────────────────── */
  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
    gap: 16px;
  }

  .quick-nav {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }

  .quick-action {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    transition: all 0.15s ease;
  }

  .quick-action:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--bg-hover);
  }

  /* ─── Account Avatar ─────────────────────────────── */
  .account-avatar-section {
    flex-shrink: 0;
  }

  .account-avatar-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px 6px 6px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    cursor: pointer;
    transition: all 0.15s;
  }

  .account-avatar-btn:hover {
    border-color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 4%, transparent);
  }

  .avatar-name {
    font-family: var(--font-minecraft);
    font-weight: 400;
    font-size: 10px;
    letter-spacing: 0.4px;
    color: var(--text-primary);
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .avatar-badge {
    font-size: 9px;
    font-weight: 800;
    padding: 1px 4px;
    border-radius: 3px;
    text-transform: uppercase;
  }

  .avatar-badge.microsoft {
    color: #00a4ef;
    background: rgba(0, 164, 239, 0.12);
  }

  .avatar-badge.offline {
    color: var(--text-muted);
    background: var(--bg-hover);
  }

  .avatar-badge.ygg {
    color: #e9d5ff;
    background: rgba(168, 85, 247, 0.15);
  }

  /* ─── Main Layout (2-column stack) ─── */
  .main-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    align-items: start;
    gap: 24px;
  }

  .home-main {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
    overflow: visible;
  }

  .home-main :global(.youtube-feed) {
    min-width: 0;
    width: 100%;
  }

  .home-side {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 320px;
    max-width: 100%;
    position: sticky;
    top: 20px;
    align-self: start;
  }

  .skin-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    /* Don't let yt-under-skin rail compress the 3D preview (overflow clips the model). */
    flex-shrink: 0;
  }

  /* Keep the canvas frame from being flexed/squashed inside the panel;
     fill the panel width and drop nested rounding (panel already clips). */
  .skin-panel :global(.skin-3d-wrap),
  .skin-panel :global(.skin-3d-container) {
    flex-shrink: 0;
    width: 100% !important;
    max-width: 100%;
    border-radius: 0;
    border-left: none;
    border-right: none;
    border-top: none;
  }

  .skin-panel :global(.skin-3d-wrap) {
    align-items: stretch;
  }

  .skin-panel-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-top: 1px solid var(--border-color);
    gap: 8px;
  }

  .skin-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .type-badge {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 3px 7px;
    border-radius: 4px;
  }
  .type-badge.microsoft {
    color: var(--badge-ms-fg, #93c5fd);
    background: var(--badge-ms-bg, rgba(59, 130, 246, 0.15));
    border: 1px solid var(--badge-ms-border, rgba(59, 130, 246, 0.35));
  }
  .type-badge.offline {
    color: var(--badge-offline-fg, #fde68a);
    background: var(--badge-offline-bg, rgba(245, 158, 11, 0.12));
    border: 1px solid var(--badge-offline-border, rgba(245, 158, 11, 0.3));
  }
  .type-badge.ygg {
    color: var(--badge-ygg-fg, #e9d5ff);
    background: var(--badge-ygg-bg, rgba(168, 85, 247, 0.15));
    border: 1px solid var(--badge-ygg-border, rgba(168, 85, 247, 0.35));
  }

  .skin-player-name {
    font-family: var(--font-minecraft);
    font-weight: 400;
    font-size: 12px;
    line-height: 1.4;
    letter-spacing: 0.5px;
    color: var(--mc-nick-color, var(--text-primary));
    text-shadow: var(
      --mc-nick-shadow,
      2px 2px 0 color-mix(in srgb, var(--text-primary) 18%, #3f3f3f),
      -1px 0 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      1px 0 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      0 -1px 0 color-mix(in srgb, var(--bg-primary) 70%, #000),
      0 1px 0 color-mix(in srgb, var(--bg-primary) 70%, #000)
    );
    text-align: center;
    padding: 0 10px 12px;
    margin-top: -4px;
    max-width: 100%;
    box-sizing: border-box;
    overflow: hidden;
    white-space: nowrap;
  }

  .change-skin-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border-radius: var(--border-radius-sm);
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .change-skin-btn:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .accounts-switcher {
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .accounts-switcher-label {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .accounts-switcher-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 200px;
    overflow: auto;
  }

  .account-chip {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-secondary);
    cursor: pointer;
    text-align: left;
    transition:
      border-color var(--motion-fast, 160ms) var(--ease-hover-in, ease),
      background var(--motion-fast, 160ms) var(--ease-hover-in, ease),
      color var(--motion-fast, 160ms) var(--ease-hover-in, ease);
  }

  .account-chip:hover:not(:disabled) {
    border-color: var(--accent-primary);
    color: var(--text-primary);
  }

  .account-chip.active {
    border-color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 8%, transparent);
    color: var(--accent-primary);
  }

  .account-chip:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .account-chip-name {
    font-family: var(--font-minecraft);
    font-size: 11px;
    letter-spacing: 0.4px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .skin-panel-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 60px 24px;
    text-align: center;
    color: var(--text-muted);
  }

  .skin-panel-empty p {
    font-size: 13px;
  }

  .skin-static-fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px 24px;
    min-height: 400px;
    background: var(--bg-primary);
  }

  .skin-static-name {
    font-family: var(--font-minecraft);
    font-size: 12px;
    letter-spacing: 0.5px;
    color: var(--text-primary);
  }

  .skin-skel {
    display: flex;
    flex-direction: column;
    width: 100%;
  }

  .skin-skel-canvas {
    width: 100%;
    height: 400px;
    border-radius: 0;
  }

  .skin-skel-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    border-top: 1px solid var(--border-color);
    gap: 8px;
  }

  .skin-skel-cape {
    padding: 12px 16px 16px;
    border-top: 1px solid var(--border-color);
  }

  .skin-skel-cape-row {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .skel-stat {
    display: inline-flex;
    align-items: center;
  }

  /* ─── Hero ────────────────────────────────────────── */
  .hero {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    padding: 24px 32px;
    background: linear-gradient(135deg, color-mix(in srgb, var(--accent-primary) 6%, transparent), color-mix(in srgb, var(--accent-secondary) 4%, transparent));
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    margin-bottom: 0;
    gap: 24px;
  }

  .hero-left {
    display: flex;
    align-items: center;
    gap: 16px 24px;
    min-width: 0;
    flex: 1;
    flex-wrap: wrap;
  }

  .hero-main {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }

  .hero-right {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
  }

  .instance-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    flex-shrink: 0;
  }

  .stat {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-secondary);
    padding: 6px 10px;
    border-radius: var(--border-radius-sm);
    flex-shrink: 0;
    white-space: nowrap;
  }

  .stat span {
    white-space: nowrap;
    overflow: visible;
    text-overflow: clip;
  }

  .size-stat {
    min-width: max-content;
  }

  .play-btn {
    width: 160px;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    font-size: 18px;
    border-radius: var(--border-radius-lg);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent-primary) 30%, transparent);
    padding: 0 24px;
    flex-shrink: 0;
  }

  .play-btn:hover {
    box-shadow: 0 12px 32px color-mix(in srgb, var(--accent-primary) 40%, transparent);
  }

  .play-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }

  .play-btn.stop {
    background: var(--accent-danger, #ef4444);
    box-shadow: 0 8px 24px rgba(239, 68, 68, 0.3);
  }

  .play-btn.stop:hover {
    box-shadow: 0 12px 32px rgba(239, 68, 68, 0.4);
  }

  .play-text {
    font-weight: 800;
  }

  .project-quick-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    max-width: 420px;
  }

  .project-name {
    font-weight: 700;
    font-size: 15px;
    line-height: 1.3;
    color: var(--text-primary);
  }

  .project-name.muted {
    color: var(--text-muted);
  }

  .version-stat {
    color: var(--text-secondary);
    font-weight: 600;
    text-transform: capitalize;
  }

  /* Empty-state copy — do not Title-Case the sentence. */
  .project-hint {
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-muted);
    text-transform: none;
  }

  .hero-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }

  .crash-fix-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 10px;
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 35%, transparent);
    background: color-mix(in srgb, var(--accent-primary) 10%, var(--bg-secondary));
    max-width: 560px;
  }

  .crash-fix-banner-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 160px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .crash-fix-banner-body strong {
    color: var(--text-secondary);
    font-size: 13px;
  }

  .crash-fix-actions {
    opacity: 0.85;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: var(--border-radius-md);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    transition: all 0.15s ease;
  }

  .action-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .action-btn.primary {
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .action-btn.primary:hover {
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  }

  .ide-open-btn {
    position: relative;
  }

  .ide-issue-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    margin-left: 2px;
    border-radius: 999px;
    background: var(--danger, #e5484d);
    color: #fff;
    font-size: 10px;
    font-weight: 800;
    line-height: 1;
  }

  .action-btn.accent {
    background: var(--accent-primary);
    color: var(--on-accent, #000);
    border-color: transparent;
  }

  .action-btn.accent:hover {
    background: var(--accent-hover);
  }

  .spinner {
    width: 20px;
    height: 20px;
    border: 2.5px solid rgba(0, 0, 0, 0.15);
    border-top-color: #000;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
