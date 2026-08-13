<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    LogIn,
    User,
    Users,
  } from "@lucide/svelte";
  import HeadAvatar from "./HeadAvatar.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    authState,
    skinPath,
    newProjectOpen,
    loginModalOpen,
    isLaunching,
    launchProgress,
    runningInstances,
    isProjectRunning,
    loginTypeLabel,
    formatPlaytime,
    libraryTabRequest,
    addInstanceMode,
    openAddInstance,
    launcherSettingsLive,
    type RecentProject,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import { launchWithFeedback, killWithFeedback, registerLaunchCrashListener } from "../lib/launch";
  import { fetchCrashFixBanner, rollbackLastCrashFix } from "../lib/softVerify";
  import {
    homeCrashFixBanner,
    homeIcons,
    homeSizes,
    homeSkinPaths,
    homeStats,
  } from "../lib/homeBootstrap";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import SkinPreview3D from "./SkinPreview3D.svelte";
  import AccountManager from "./AccountManager.svelte";
  import HomeHero, { type PosterCoverKind } from "./HomeHero.svelte";
  import YoutubeFeed from "./YoutubeFeed.svelte";
  import PromptDialog from "./PromptDialog.svelte";

  import type { View } from "../lib/types";

  let { currentView = $bindable() }: { currentView: View } = $props();

  let authReady = $state(false);

  function browseLibrary() {
    libraryTabRequest.set("discover");
    currentView = "library";
  }

  const projectStats = $derived($homeStats);
  const instanceSizes = $derived($homeSizes);
  const accountSkinPaths = $derived($homeSkinPaths);
  const crashFixBanner = $derived($homeCrashFixBanner);

  let selectedPath = $state<string | null>($projectPath);
  let showAccountManager = $state(false);
  let potatoPc = $state(
    typeof document !== "undefined" && document.documentElement.classList.contains("potato-pc"),
  );
  let accountSwitchBusy = $state(false);
  let heroOverflowOpen = $state(false);
  let heroActionBusy = $state(false);
  let showRenamePrompt = $state(false);
  let showClonePrompt = $state(false);
  let renameDefault = $state("");
  let cloneDefault = $state("");

  const selectedProject = $derived($recentProjects.find((p) => p.path === selectedPath));
  const selectedRunning = $derived(isProjectRunning(selectedPath, $runningInstances));
  const selectedInstanceMeta = $derived.by(() => {
    const info = selectedProject?.info;
    if (!info) return "";
    const loader = info.loaderVersion?.trim()
      ? `${info.loaderKind} ${info.loaderVersion.trim()}`
      : info.loaderKind;
    const bits = [`${info.minecraftVersion} · ${loader}`];
    const path = selectedProject?.path;
    if (path && projectStats[path]?.playtime) {
      bits.push(formatPlaytime(projectStats[path].playtime));
    }
    if (path && instanceSizes[path] && instanceSizes[path] !== "?") {
      bits.push(instanceSizes[path]);
    }
    return bits.join(" · ");
  });

  type CoverState = { url: string | null; kind: PosterCoverKind };
  let coverByPath = $state<Record<string, CoverState>>({});
  const listingFetched = new SvelteSet<string>();

  const posterCover = $derived.by((): CoverState => {
    if (!selectedPath) return { url: null, kind: "none" };
    return coverByPath[selectedPath] ?? { url: null, kind: "none" };
  });

  $effect(() => {
    const path = selectedPath;
    if (!path || listingFetched.has(path)) return;
    listingFetched.add(path);
    let cancelled = false;
    void (async () => {
      try {
        const listing = await api.project.getListing(path);
        if (cancelled) return;
        const first = listing.gallery?.[0];
        if (first?.url) {
          coverByPath = { ...coverByPath, [path]: { url: first.url, kind: "gallery" } };
          return;
        }
        if (first?.path) {
          const data = await api.project.readListingAsset(first.path, path);
          if (cancelled) return;
          coverByPath = { ...coverByPath, [path]: { url: data, kind: "gallery" } };
          return;
        }
        const icon = $homeIcons[path];
        if (icon && !potatoPc) {
          coverByPath = { ...coverByPath, [path]: { url: icon, kind: "icon" } };
          return;
        }
        coverByPath = { ...coverByPath, [path]: { url: null, kind: "none" } };
      } catch {
        if (cancelled) return;
        const icon = $homeIcons[path];
        coverByPath = {
          ...coverByPath,
          [path]: icon && !potatoPc ? { url: icon, kind: "icon" } : { url: null, kind: "none" },
        };
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    const path = selectedPath;
    if (!path || potatoPc) return;
    const existing = coverByPath[path];
    if (existing?.kind === "gallery") return;
    const icon = $homeIcons[path];
    if (icon && (!existing || existing.kind === "none")) {
      coverByPath = { ...coverByPath, [path]: { url: icon, kind: "icon" } };
    }
  });

  // The sidebar rail switches instances through the global store — mirror it here.
  $effect(() => {
    const p = $projectPath;
    if (p && p !== selectedPath) {
      selectedPath = p;
      heroOverflowOpen = false;
    }
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
      homeCrashFixBanner.set(null);
      return;
    }
    try {
      homeCrashFixBanner.set(await fetchCrashFixBanner(path));
    } catch {
      homeCrashFixBanner.set(null);
    }
  }

  $effect(() => {
    void refreshCrashFixBanner(selectedPath);
  });

  $effect(() => {
    // Selected size only — never walk every recent instance on home.
    const path = selectedPath;
    if (!path) return;
    if ($homeSizes[path]) return;
    let cancelled = false;
    void (async () => {
      try {
        const label = await api.instance.getSize(path);
        if (!cancelled) {
          homeSizes.update((m) => ({ ...m, [path]: label }));
        }
      } catch {
        if (!cancelled) {
          homeSizes.update((m) => ({ ...m, [path]: "?" }));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
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

  $effect(() => {
    const uuids = $authState.accounts.map((a) => a.uuid).filter(Boolean);
    const missing = uuids.filter((u) => !$homeSkinPaths[u]);
    if (!missing.length) return;
    void api.home
      .accountSkinPaths(missing)
      .then((map) => {
        if (map && Object.keys(map).length) {
          homeSkinPaths.update((prev) => ({ ...prev, ...map }));
        }
      })
      .catch(() => {});
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
          homeSkinPaths.update((prev) => ({ ...prev, [state.profile!.uuid]: path }));
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
      // Auth usually arrives via home bootstrap; refresh only if still empty.
      if (!$authState.loggedIn && !$authState.profile) {
        try {
          const status = await api.mcAuth.getAuthStatus();
          authState.set(status);
          if (status.loggedIn && status.profile) {
            try {
              const path = await api.mcAuth.getSkinPath(status.profile.uuid);
              skinPath.set(path);
              homeSkinPaths.update((prev) => ({
                ...prev,
                [status.profile!.uuid]: path,
              }));
            } catch {}
          }
        } catch {
        } finally {
          authReady = true;
        }
      } else {
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
        if (id) {
          void api.stats.get(id).then((s) => {
            homeStats.update((prev) => ({
              ...prev,
              [id]: {
                playtime: s.totalPlaytimeSeconds ?? 0,
                lastLaunch: s.lastLaunch ?? null,
              },
            }));
          }).catch(() => {});
          void api.home.invalidateCache(id).catch(() => {});
        }
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
    await launchWithFeedback({ path: selectedPath, profile: "client" });
    const project = $recentProjects.find((p) => p.path === selectedPath);
    if (project) recentProjects.add(project);
    void api.stats.get(selectedPath).then((s) => {
      homeStats.update((prev) => ({
        ...prev,
        [selectedPath!]: {
          playtime: s.totalPlaytimeSeconds ?? 0,
          lastLaunch: s.lastLaunch ?? null,
        },
      }));
    }).catch(() => {});
  }

  async function stopGame() {
    if (!selectedPath) return;
    await killWithFeedback(selectedPath);
  }

  function openSettings() {
    currentView = "project-settings";
  }

  function closeHeroOverflow() {
    heroOverflowOpen = false;
  }

  function toggleHeroOverflow() {
    if (heroActionBusy) return;
    heroOverflowOpen = !heroOverflowOpen;
  }

  function openRenamePrompt() {
    if (!selectedProject || heroActionBusy) return;
    closeHeroOverflow();
    renameDefault = selectedProject.info.name;
    showRenamePrompt = true;
  }

  function openClonePrompt() {
    if (!selectedProject || heroActionBusy) return;
    closeHeroOverflow();
    cloneDefault = `${selectedProject.info.name} copy`;
    showClonePrompt = true;
  }

  async function confirmRename(newName: string) {
    showRenamePrompt = false;
    const project = selectedProject;
    const name = newName.trim();
    if (!project || !name) return;
    if (name === project.info.name) return;
    heroActionBusy = true;
    try {
      const listing = await api.project.getListing(project.path);
      await api.project.updateListing({ ...listing, name }, project.path);
      const info = await api.project.validate(project.path);
      const updated: RecentProject = {
        path: project.path,
        info: info as RecentProject["info"],
      };
      recentProjects.add(updated, { reorder: false });
      projectInfo.set(updated.info);
      toasts.success(`Renamed to “${name}”`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      heroActionBusy = false;
    }
  }

  async function confirmClone(newName: string) {
    showClonePrompt = false;
    const project = selectedProject;
    const name = newName.trim();
    if (!project || !name) return;
    heroActionBusy = true;
    try {
      const clonedPath = await api.files.cloneProject(name, project.path);
      await loadProject(clonedPath);
      toasts.success(`Cloned to “${name}”`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      heroActionBusy = false;
    }
  }

  async function deleteSelectedInstance() {
    const project = selectedProject;
    if (!project || heroActionBusy) return;
    closeHeroOverflow();
    const ok = await confirm(`Delete "${project.info.name}" from disk?`, {
      title: "Delete instance",
      kind: "warning",
    });
    if (!ok) return;
    heroActionBusy = true;
    try {
      await api.files.deleteProject(project.path);
      const next = $recentProjects.find((p) => p.path !== project.path) ?? null;
      recentProjects.remove(project.path);
      selectedPath = next?.path ?? null;
      projectPath.set(selectedPath);
      projectInfo.set(next?.info ?? null);
      toasts.success(`Deleted “${project.info.name}”`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      heroActionBusy = false;
    }
  }
</script>

<div class="home fade-slide-in">
  <div class="main-layout">
    <div class="home-main">
      <HomeHero
        hasSelection={!!selectedProject}
        emptyZero={$recentProjects.length === 0}
        meta={selectedInstanceMeta}
        launching={$isLaunching}
        launchMessage={$launchProgress?.message ?? ""}
        launchPercent={$launchProgress?.percent ?? null}
        running={selectedRunning}
        playDisabled={!selectedPath}
        coverUrl={posterCover.url}
        coverKind={posterCover.kind}
        potato={potatoPc}
        actionBusy={heroActionBusy}
        overflowOpen={heroOverflowOpen}
        signedIn={$authState.loggedIn}
        playerName={$authState.profile?.name ?? ""}
        crashBanner={crashFixBanner}
        crashFixBusy={crashFixBusy}
        softVerifyRemainingSecs={softVerifyRemainingSecs}
        onPlay={launch}
        onStop={stopGame}
        onSettings={openSettings}
        onFolder={() => {
          if (selectedProject) void invoke("open_project_folder", { path: selectedProject.path });
        }}
        onToggleOverflow={toggleHeroOverflow}
        onRename={openRenamePrompt}
        onClone={openClonePrompt}
        onDelete={() => void deleteSelectedInstance()}
        onCreate={() => openAddInstance("blank")}
        onImport={() => openAddInstance("import")}
        onBrowse={browseLibrary}
        onRollback={() => void onRollbackCrashFix()}
        onDiagnostics={() => (currentView = "diagnostics")}
        onSignIn={() => loginModalOpen.set(true)}
      />

      {#if $launcherSettingsLive?.showYoutubeOnHome === true}
        <div class="home-feed">
          <YoutubeFeed variant="row" />
        </div>
      {/if}
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
            cachedPath={$skinPath}
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
                class={[
                  "type-badge",
                  {
                    microsoft: $authState.loginType === "microsoft",
                    offline: $authState.loginType === "offline",
                    ygg: $authState.loginType === "yggdrasil",
                  },
                ]}
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
                    class={["account-chip", { active: account.uuid === $authState.activeAccountUuid }]}
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
            <User size={48} aria-hidden="true" />
            <h2 class="skin-panel-empty-title">Not signed in</h2>
            <p class="skin-panel-empty-copy">
              Sign in with Microsoft or an offline account to play.
            </p>
            <button class="action-btn accent" onclick={() => loginModalOpen.set(true)}>
              <LogIn size={16} />
              Sign In
            </button>
            <button
              type="button"
              class="skin-panel-empty-manage"
              onclick={() => loginModalOpen.set(true)}
            >
              More sign-in options
            </button>
          </div>
        {/if}
      </div>
    </aside>
  </div>
</div>

{#if showAccountManager}
  <AccountManager onclose={() => (showAccountManager = false)} />
{/if}

{#if $newProjectOpen}
  <AddInstanceModal
    initialMode={$addInstanceMode}
    onclose={() => (newProjectOpen.set(false))}
    oncreated={(path) => loadProject(path)}
  />
{/if}

{#if showRenamePrompt && selectedProject}
  <PromptDialog
    title="Rename instance"
    message={`Rename “${selectedProject.info.name}”`}
    mode="text"
    defaultValue={renameDefault}
    confirmLabel="Rename"
    onconfirm={(v) => void confirmRename(v)}
    oncancel={() => (showRenamePrompt = false)}
  />
{/if}

{#if showClonePrompt && selectedProject}
  <PromptDialog
    title="Clone instance"
    message={`Create a copy of “${selectedProject.info.name}”`}
    mode="text"
    defaultValue={cloneDefault}
    confirmLabel="Clone"
    onconfirm={(v) => void confirmClone(v)}
    oncancel={() => (showClonePrompt = false)}
  />
{/if}

<style>
  .home {
    max-width: 1400px;
    margin: 0 auto;
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

  .home-feed {
    margin-top: 0;
    min-width: 0;
    width: 100%;
  }

  .home-main:has(:global(.youtube-feed.is-full)) {
    min-height: calc(100dvh - 6.5rem);
  }

  .home-feed:has(:global(.youtube-feed.is-full)) {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .home-feed :global(.youtube-feed) {
    min-width: 0;
    width: 100%;
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
    -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
    backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
    box-shadow:
      var(--shadow-md),
      inset 0 1px 0 var(--glass-highlight);
    border-radius: var(--border-radius-xl);
    padding: 10px 14px;
  }

  .home-feed :global(.youtube-feed.is-full) {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    min-height: calc(100dvh - 6.5rem);
  }

  .home-feed :global(.youtube-feed.is-collapsed) {
    padding: 8px 14px;
  }

  .home-feed :global(.youtube-feed.is-collapsed .section-header) {
    min-height: 36px;
  }

  .home-feed :global(.youtube-feed.is-collapsed .section-header h2) {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .home-feed :global(.youtube-feed.is-collapsed .section-header svg) {
    width: 16px;
    height: 16px;
  }

  .home-feed :global(.youtube-feed .feed-row) {
    max-width: 100%;
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
    background: var(--glass-bg);
    border: 1px solid var(--glass-border);
    -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
    backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
    border-radius: var(--border-radius-xl);
    box-shadow:
      var(--shadow-md),
      inset 0 1px 0 var(--glass-highlight);
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
    text-overflow: ellipsis;
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

  .skin-panel-empty-title {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .skin-panel-empty-copy {
    margin: 0;
    max-width: 220px;
    font-size: 13px;
    line-height: 1.4;
    color: var(--text-muted);
  }

  .skin-panel-empty-manage {
    margin-top: 4px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .skin-panel-empty-manage:hover {
    color: var(--text-secondary);
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

  .action-btn.accent {
    background: var(--accent-primary);
    color: var(--on-accent, #000);
    border-color: transparent;
  }

  .action-btn.accent:hover {
    background: var(--accent-hover);
  }
</style>
