<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { Home, Workflow, Plus, Settings, User, Play, Terminal } from "@lucide/svelte";
  import {
    newProjectOpen,
    projectPath,
    projectInfo,
    recentProjects,
    runningInstances,
    isProjectRunning,
    ideStageRequest,
    ideSuggestedStage,
    openLaunchLog,
    isLaunching,
    brandIcon,
    BRAND_ICON_CREEPER_SRC_SM,
  } from "../lib/store";
  import { api } from "../lib/api";
  import { launchWithFeedback } from "../lib/launch";

  import type { View } from "../lib/types";
  let { currentView = $bindable() }: { currentView: View } = $props();

  /** Real pack icon (data URL from the instance listing) keyed by project path. */
  let instanceIcons = $state<Record<string, string | null>>({});
  const iconRequested = new SvelteSet<string>();

  async function loadInstanceIcon(path: string) {
    try {
      const listing = await api.project.getListing(path);
      const rel = listing.iconPath;
      if (!rel) {
        instanceIcons[path] = null;
        return;
      }
      instanceIcons[path] = await api.project.readListingAsset(rel, path);
    } catch {
      instanceIcons[path] = null;
    }
    instanceIcons = { ...instanceIcons };
  }

  $effect(() => {
    for (const p of $recentProjects) {
      if (iconRequested.has(p.path)) continue;
      iconRequested.add(p.path);
      void loadInstanceIcon(p.path);
    }
  });

  /**
   * Fallback identity when a pack has no icon: theme-token gradients
   * (accent + surfaces), hashed by name so each instance stays stable and
   * follows the active data-theme without a JS subscription.
   */
  function themeGradient(name: string): [string, string] {
    const pairs: [string, string][] = [
      [
        "color-mix(in srgb, var(--accent-primary) 32%, var(--bg-primary))",
        "color-mix(in srgb, var(--accent-primary) 82%, var(--bg-secondary))",
      ],
      [
        "color-mix(in srgb, var(--accent-secondary) 28%, var(--bg-primary))",
        "color-mix(in srgb, var(--accent-primary) 72%, var(--bg-tertiary))",
      ],
      [
        "color-mix(in srgb, var(--accent-primary) 18%, var(--bg-secondary))",
        "color-mix(in srgb, var(--accent-hover) 78%, var(--bg-primary))",
      ],
      [
        "color-mix(in srgb, var(--bg-tertiary) 45%, var(--accent-primary))",
        "color-mix(in srgb, var(--accent-primary) 88%, var(--accent-secondary))",
      ],
      [
        "color-mix(in srgb, var(--accent-secondary) 22%, var(--bg-primary))",
        "color-mix(in srgb, var(--accent-secondary) 65%, var(--accent-primary))",
      ],
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return pairs[Math.abs(hash) % pairs.length];
  }

  function openHome() {
    currentView = "dashboard";
  }

  function openIde() {
    ideStageRequest.set($ideSuggestedStage || "content");
    currentView = "ide";
  }

  function openNewProject() {
    // Dashboard owns the modal, so make sure we're on that view before
    // raising the flag — otherwise the modal component wouldn't be mounted.
    currentView = "dashboard";
    newProjectOpen.set(true);
  }

  async function playClient() {
    if (!$projectPath || $isLaunching) return;
    await launchWithFeedback({ path: $projectPath, profile: "client" });
  }

  function openLogs() {
    if ($projectPath) openLaunchLog($projectPath);
  }

  async function selectInstance(path: string) {
    if ($projectPath === path) {
      currentView = "dashboard";
      return;
    }
    try {
      const info = await api.project.validate(path);
      const manifestPath = info.manifestPath || path;
      // reorder:false — rail icons must not jump around on every click.
      recentProjects.add({ path: manifestPath, info: info as any }, { reorder: false });
      projectPath.set(manifestPath);
      projectInfo.set(info as any);
      void api.session.setLastOpened(manifestPath).catch(() => {});
    } catch {
      const cached = $recentProjects.find((p) => p.path === path);
      if (!cached) return;
      projectPath.set(cached.path);
      projectInfo.set(cached.info);
    }
    currentView = "dashboard";
  }
</script>

<aside class="rail">
  <!-- Brand mark — constant identity, not a nav button. -->
  <div class="rail-brand" title="TuffBox">
    {#if $brandIcon === "creeper"}
      <img
        class="brand-logo brand-logo-img"
        src={BRAND_ICON_CREEPER_SRC_SM}
        alt=""
        draggable="false"
        aria-hidden="true"
      />
    {:else}
      <span class="brand-logo" aria-hidden="true">T</span>
    {/if}
  </div>

  <nav class="rail-zone" aria-label="App">
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        class:active={currentView === "dashboard"}
        title="Home — Launcher"
        aria-label="Home — Launcher"
        onclick={openHome}
      >
        <Home size={21} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        class:active={currentView === "ide"}
        title="IDE"
        aria-label="IDE"
        onclick={openIde}
      >
        <Workflow size={21} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn add"
        title="Add instance"
        aria-label="Add instance"
        onclick={openNewProject}
      >
        <Plus size={22} />
      </button>
    </div>
  </nav>

  <div class="rail-divider" aria-hidden="true"></div>

  <nav class="rail-zone rail-instances" aria-label="Instances">
    {#each $recentProjects as instance (instance.path)}
      {@const icon = instanceIcons[instance.path]}
      {@const running = isProjectRunning(instance.path, $runningInstances)}
      {@const [g0, g1] = themeGradient(instance.info.name)}
      <div class="rail-item">
        <button
          type="button"
          class="rail-btn instance"
          class:active={$projectPath === instance.path}
          class:has-icon={!!icon}
          title={instance.info.name}
          aria-label={instance.info.name}
          style={icon ? undefined : `background: linear-gradient(135deg, ${g0}, ${g1})`}
          onclick={() => selectInstance(instance.path)}
        >
          {#if icon}
            <img class="instance-img" src={icon} alt="" draggable="false" />
          {:else}
            <span class="instance-letter">{instance.info.name[0]}</span>
          {/if}
          {#if running}
            <span class="running-dot" title="Running"></span>
          {/if}
        </button>
      </div>
    {/each}
  </nav>

  <nav class="rail-zone rail-bottom" aria-label="Launcher">
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        title="Play"
        aria-label="Play"
        disabled={!$projectPath || $isLaunching}
        onclick={playClient}
      >
        <Play size={21} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        title="Logs"
        aria-label="Logs"
        disabled={!$projectPath}
        onclick={openLogs}
      >
        <Terminal size={21} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        class:active={currentView === "settings"}
        title="Settings"
        aria-label="Settings"
        onclick={() => (currentView = "settings")}
      >
        <Settings size={21} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn ghost"
        class:active={currentView === "me"}
        title="Profile"
        aria-label="Profile"
        onclick={() => (currentView = "me")}
      >
        <User size={21} />
      </button>
    </div>
  </nav>
</aside>

<style>
  .rail {
    width: 72px;
    flex-shrink: 0;
    height: 100%;
    min-height: 0;
    box-sizing: border-box;
    /* Themes (esp. Minimal) override via --rail-* tokens. */
    background: var(--rail-bg, color-mix(in srgb, var(--bg-secondary) 92%, var(--bg-tertiary)));
    border-right: 1px solid var(--rail-border, var(--border-color));
    -webkit-backdrop-filter: var(--rail-backdrop, none);
    backdrop-filter: var(--rail-backdrop, none);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0;
    position: relative;
    z-index: 30;
  }

  /* Brand mark — constant amber identity, not a nav button. */
  .rail-brand {
    padding: 2px 0 12px;
    flex-shrink: 0;
    user-select: none;
  }

  .brand-logo {
    width: 40px;
    height: 40px;
    border-radius: var(--border-radius-lg);
    background: var(--brand-mark-gradient, linear-gradient(135deg, #ffc500, #ff9500));
    color: var(--brand-mark-fg, #241703);
    font-weight: 900;
    font-size: 19px;
    display: flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--brand-mark-shadow, 0 4px 14px rgba(255, 197, 0, 0.28));
    animation: tb-logo-reveal 1.15s cubic-bezier(0.22, 1, 0.36, 1) both;
  }

  .brand-logo-img {
    display: block;
    object-fit: cover;
    background: transparent;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
    color: transparent;
    font-size: 0;
  }

  .rail-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    width: 100%;
    flex-shrink: 0;
  }

  .rail-divider {
    width: 32px;
    height: 2px;
    border-radius: 1px;
    background: color-mix(in srgb, var(--text-primary) 10%, transparent);
    margin: 10px 0;
    flex-shrink: 0;
  }

  /* Middle zone owns the scroll; scrollbar hidden like server rails. */
  .rail-instances {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: none;
    padding: 2px 0;
    flex-shrink: 1;
  }

  .rail-instances::-webkit-scrollbar {
    display: none;
  }

  .rail-bottom {
    padding-top: 10px;
  }

  .rail-item {
    position: relative;
    width: 100%;
    display: flex;
    justify-content: center;
    flex-shrink: 0;
  }

  /* Active/hover pill on the rail's left edge (height animates, Discord-style). */
  .rail-item::before {
    content: "";
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 4px;
    height: 0;
    border-radius: var(--rail-indicator-radius, 0 4px 4px 0);
    background: var(--accent-primary);
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent-primary) 55%, transparent);
    opacity: 0;
    transition:
      height var(--motion-fast, 160ms) var(--ease-hover-in, ease),
      opacity var(--motion-fast, 160ms) var(--ease-hover-in, ease);
    pointer-events: none;
  }

  .rail-item:hover::before {
    height: 20px;
    opacity: 1;
  }

  .rail-item:has(.rail-btn.active)::before {
    height: 36px;
    opacity: 1;
  }

  /* `.rail` prefix lifts specificity above the global themed button radius
     (html[data-rounded-corners] :where(button)) so the shape holds.
     Default: circle at rest → squircle on hover/active.
     Sharp themes override via --rail-btn-radius* → hard squares. */
  .rail .rail-btn {
    position: relative;
    width: 48px;
    height: 48px;
    padding: 0;
    gap: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--rail-btn-radius, 50%);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    overflow: hidden;
    /* Kill the global button hover translate — it would desync the edge pill. */
    transform: none !important;
    transition:
      border-radius var(--motion-med, 240ms) var(--ease-hover-in, ease),
      background-color var(--motion-fast, 160ms) var(--ease-hover-in, ease),
      color var(--motion-fast, 160ms) var(--ease-hover-in, ease);
  }

  .rail .rail-btn:hover,
  .rail .rail-btn.active {
    border-radius: var(--rail-btn-radius-active, var(--border-radius-lg));
  }

  /* Ghost nav (Home / IDE / Settings / Profile): quiet until touched. */
  .rail .rail-btn.ghost {
    background: transparent;
    color: var(--text-muted);
  }

  .rail .rail-btn.ghost:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .rail .rail-btn.ghost.active {
    background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
    color: var(--accent-primary);
  }

  .rail .rail-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .rail .rail-btn:disabled:hover {
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--rail-btn-radius, 50%);
  }

  /* Add instance: quiet amber plus. */
  .rail .rail-btn.add {
    background: transparent;
    color: var(--accent-primary);
  }

  .rail .rail-btn.add:hover {
    background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
    color: var(--accent-primary);
  }

  /* Instance avatars: theme-token gradient (inline) or the real pack icon.
     `position: relative` on `.rail-btn` makes the abspos icon/running-dot
     clip to the same circle → squircle mask as the letter fallback. */
  .rail .rail-btn.instance {
    color: var(--text-primary);
  }

  .rail .rail-btn.instance.has-icon {
    background: transparent;
  }

  .rail .rail-btn.instance:hover,
  .rail .rail-btn.instance.active {
    background-color: transparent;
    color: var(--text-primary);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-primary) 45%, transparent);
  }

  .instance-img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    border-radius: inherit;
    pointer-events: none;
  }

  .instance-letter {
    font-weight: 900;
    font-size: 18px;
    line-height: 1;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--accent-primary) 18%, var(--text-primary));
    text-shadow: 0 1px 2px color-mix(in srgb, var(--bg-primary) 55%, transparent);
    pointer-events: none;
  }

  /* Running indicator — green dot pinned to the avatar's lower right. */
  .running-dot {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 13px;
    height: 13px;
    border-radius: var(--rail-btn-radius, 50%);
    background: var(--accent-primary);
    border: 3px solid var(--bg-primary);
    box-shadow: 0 0 8px color-mix(in srgb, var(--accent-primary) 60%, transparent);
    pointer-events: none;
  }
</style>
