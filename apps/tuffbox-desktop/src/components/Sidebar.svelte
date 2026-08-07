<script lang="ts">
  import { Home, Workflow, Plus, Settings, User } from "@lucide/svelte";
  import {
    newProjectOpen,
    projectPath,
    projectInfo,
    recentProjects,
    ideStageRequest,
    ideSuggestedStage,
  } from "../lib/store";
  import { api } from "../lib/api";

  type View = "dashboard" | "ide" | "mods" | "graph" | "world" | "diagnostics" | "crash-votes" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "me" | "chats";
  let { currentView = $bindable() }: { currentView: View } = $props();

  /** Same name-hash gradient the instance tiles use, so rail avatars match pack identity. */
  function gradientFrom(name: string) {
    const colors = ["#1bd96a", "#8b5cf6", "#3b82f6", "#f59e0b", "#ec4899", "#06b6d4", "#ef4444"];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
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
  <nav class="rail-zone" aria-label="App">
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn"
        class:active={currentView === "dashboard"}
        title="Launcher"
        aria-label="Launcher"
        onclick={openHome}
      >
        <Home size={22} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn"
        class:active={currentView === "ide"}
        title="IDE"
        aria-label="IDE"
        onclick={openIde}
      >
        <Workflow size={22} />
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
      <div class="rail-item">
        <button
          type="button"
          class="rail-btn instance"
          class:active={$projectPath === instance.path}
          title={instance.info.name}
          aria-label={instance.info.name}
          style="background: linear-gradient(135deg, {gradientFrom(instance.info.name)}, {gradientFrom(instance.info.id)})"
          onclick={() => selectInstance(instance.path)}
        >
          <span class="instance-letter">{instance.info.name[0]}</span>
        </button>
      </div>
    {/each}
  </nav>

  <nav class="rail-zone rail-bottom" aria-label="Launcher">
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn"
        class:active={currentView === "settings"}
        title="Settings"
        aria-label="Settings"
        onclick={() => (currentView = "settings")}
      >
        <Settings size={22} />
      </button>
    </div>
    <div class="rail-item">
      <button
        type="button"
        class="rail-btn"
        class:active={currentView === "me"}
        title="Profile"
        aria-label="Profile"
        onclick={() => (currentView = "me")}
      >
        <User size={22} />
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
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0;
    position: relative;
    z-index: 30;
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
    background: var(--border-color);
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
    border-radius: 0 4px 4px 0;
    background: var(--accent-primary);
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent-primary) 55%, transparent);
    opacity: 0;
    transition:
      height var(--motion-fast, 160ms) var(--ease-out, ease),
      opacity var(--motion-fast, 160ms) var(--ease-out, ease);
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
     (html[data-rounded-corners] :where(button)) so the Discord circle holds. */
  .rail .rail-btn {
    width: 48px;
    height: 48px;
    padding: 0;
    gap: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 50%;
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    /* Kill the global button hover translate — it would desync the edge pill. */
    transform: none !important;
    transition:
      border-radius var(--motion-med, 240ms) var(--ease-out, ease),
      background-color var(--motion-fast, 160ms) var(--ease-out, ease),
      color var(--motion-fast, 160ms) var(--ease-out, ease);
  }

  /* Circle → squircle morph on hover/active. */
  .rail .rail-btn:hover,
  .rail .rail-btn.active {
    border-radius: var(--border-radius-lg);
    background-color: var(--accent-primary);
    color: #000;
  }

  .rail-btn.add {
    color: var(--accent-primary);
  }

  .rail-btn.add:hover {
    color: #000;
  }

  /* Instance avatars carry their own gradient (inline) — hover only morphs shape. */
  .rail-btn.instance {
    color: #fff;
  }

  .rail-btn.instance:hover,
  .rail-btn.instance.active {
    background-color: inherit;
    color: #fff;
  }

  .instance-letter {
    font-weight: 900;
    font-size: 18px;
    line-height: 1;
    text-transform: uppercase;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }
</style>
