<script lang="ts">
  import {
    Package,
    Image,
    Sparkles,
    Globe,
    Server,
    RefreshCw,
    Plus,
    Trash2,
    Power,
    FolderOpen,
    ExternalLink,
    Play,
    Search,
    ChevronDown,
    ChevronUp,
  } from "@lucide/svelte";
  import { api, type WorldListItem } from "../lib/api";
  import { libraryTabRequest } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { launchWithFeedback } from "../lib/launch";

  let {
    projectPath,
    onOpenMods = () => {},
    onOpenWorld = () => {},
  }: {
    projectPath: string;
    onOpenMods?: () => void;
    onOpenWorld?: () => void;
  } = $props();

  type Tab = "mods" | "resourcepacks" | "shaderpacks" | "worlds" | "servers";
  type PackEntry = {
    name: string;
    fileName: string;
    enabled: boolean;
    kind: string;
    size: number;
    sizeFormatted: string;
  };
  type ServerEntry = {
    name: string;
    address: string;
    icon: string | null;
    acceptTextures: number | null;
  };
  type PingResult = {
    address: string;
    online: boolean;
    latencyMs: number | null;
    error: string | null;
  };

  const COLLAPSE_KEY = "tuffbox-instance-home-collapsed";

  let tab = $state<Tab>("resourcepacks");
  let loading = $state(false);
  let resourcePacks = $state<PackEntry[]>([]);
  let shaderPacks = $state<PackEntry[]>([]);
  let worlds = $state<WorldListItem[]>([]);
  let servers = $state<ServerEntry[]>([]);
  let pings = $state<Record<string, PingResult>>({});
  let modCount = $state<number | null>(null);
  let busyKey = $state<string | null>(null);
  /** Tabs that already have a successful load — avoid skeleton/collapse on revisit. */
  let primed = $state<Partial<Record<Tab, boolean>>>({});
  let loadGen = 0;
  let collapsed = $state(
    typeof localStorage === "undefined"
      ? false
      : localStorage.getItem(COLLAPSE_KEY) === "true",
  );

  let newServerName = $state("");
  let newServerAddress = $state("");

  const packs = $derived(tab === "shaderpacks" ? shaderPacks : resourcePacks);
  const tabPrimed = $derived(!!primed[tab]);
  const showSkeleton = $derived(
    loading &&
      !tabPrimed &&
      tab !== "mods" &&
      (tab === "resourcepacks" || tab === "shaderpacks"
        ? packs.length === 0
        : tab === "worlds"
          ? worlds.length === 0
          : servers.length === 0),
  );

  const collapsedSummary = $derived((() => {
    switch (tab) {
      case "mods":
        return modCount == null ? "Mods" : `${modCount} mods`;
      case "resourcepacks":
        return `Resource packs · ${resourcePacks.length}`;
      case "shaderpacks":
        return `Shaders · ${shaderPacks.length}`;
      case "worlds":
        return `Worlds · ${worlds.length}`;
      case "servers":
        return `Servers · ${servers.length}`;
    }
  })());

  function toggleCollapsed() {
    collapsed = !collapsed;
    try {
      localStorage.setItem(COLLAPSE_KEY, String(collapsed));
    } catch {
      /* ignore */
    }
  }

  async function load(opts?: { force?: boolean }) {
    if (!projectPath) return;
    const t = tab;
    const force = !!opts?.force;
    const first = !primed[t];
    const gen = ++loadGen;
    // Only flash loading UI on cold tab — keeps panel height stable when switching.
    if (first || force) loading = true;
    try {
      if (t === "resourcepacks" || t === "shaderpacks") {
        const next = await api.content.listPacks(t, projectPath);
        if (gen !== loadGen) return;
        if (t === "shaderpacks") shaderPacks = next;
        else resourcePacks = next;
      } else if (t === "worlds") {
        const next = await api.worlds.list(projectPath);
        if (gen !== loadGen) return;
        worlds = next;
      } else if (t === "servers") {
        const next = await api.servers.list(projectPath);
        if (gen !== loadGen) return;
        servers = next;
      } else if (t === "mods") {
        try {
          const mods = await api.mods.list(projectPath);
          if (gen !== loadGen) return;
          modCount = Array.isArray(mods)
            ? mods.filter((m: any) => !m.contentType || m.contentType === "mod").length
            : 0;
        } catch {
          if (gen !== loadGen) return;
          modCount = null;
        }
      }
      if (gen !== loadGen) return;
      primed = { ...primed, [t]: true };
    } catch (e) {
      if (gen === loadGen) toasts.error(String(e));
    } finally {
      if (gen === loadGen) loading = false;
    }
  }

  let lastProjectPath = "";

  $effect(() => {
    if (!projectPath) return;
    if (projectPath !== lastProjectPath) {
      lastProjectPath = projectPath;
      resourcePacks = [];
      shaderPacks = [];
      worlds = [];
      servers = [];
      modCount = null;
      primed = {};
    }
    // Depend on tab so switching tabs reloads (from cache when primed).
    void tab;
    void load();
  });

  async function togglePack(pack: PackEntry) {
    busyKey = pack.fileName;
    try {
      await api.content.setEnabled(
        tab === "shaderpacks" ? "shaderpacks" : "resourcepacks",
        pack.fileName,
        !pack.enabled,
        projectPath,
      );
      await load({ force: true });
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busyKey = null;
    }
  }

  async function openFolder() {
    const folder =
      tab === "shaderpacks"
        ? "shaderpacks"
        : tab === "resourcepacks"
          ? "resourcepacks"
          : tab === "worlds"
            ? "saves"
            : tab === "mods"
              ? "mods"
              : null;
    try {
      await api.files.openFolder(projectPath, folder);
    } catch (e) {
      toasts.error(String(e));
    }
  }

  function openCatalog() {
    libraryTabRequest.set("discover");
    window.dispatchEvent(new CustomEvent("tuffbox:open-library"));
  }

  async function addServer() {
    if (!newServerName.trim() || !newServerAddress.trim()) return;
    busyKey = "add-server";
    try {
      servers = await api.servers.add(newServerName.trim(), newServerAddress.trim(), projectPath);
      newServerName = "";
      newServerAddress = "";
      toasts.success("Server added");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busyKey = null;
    }
  }

  async function removeServer(address: string) {
    busyKey = address;
    try {
      servers = await api.servers.remove(address, projectPath);
      delete pings[address];
      pings = pings;
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busyKey = null;
    }
  }

  async function pingServer(address: string) {
    busyKey = `ping:${address}`;
    try {
      const result = await api.servers.ping(address);
      pings = { ...pings, [address]: result };
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busyKey = null;
    }
  }

  async function joinServer(address: string) {
    busyKey = `join:${address}`;
    try {
      await launchWithFeedback({
        path: projectPath,
        profile: "client",
        quickPlayType: "multiplayer",
        quickPlayValue: address,
      });
    } finally {
      busyKey = null;
    }
  }

  async function playWorld(name: string) {
    busyKey = `play:${name}`;
    try {
      await launchWithFeedback({
        path: projectPath,
        profile: "client",
        quickPlayType: "singleplayer",
        quickPlayValue: name,
      });
    } finally {
      busyKey = null;
    }
  }
</script>

<section class="instance-home" class:collapsed>
  <div class="tabs">
    <button class:active={tab === "mods"} onclick={() => (tab = "mods")}>
      <Package size={14} /> Mods
    </button>
    <button class:active={tab === "resourcepacks"} onclick={() => (tab = "resourcepacks")}>
      <Image size={14} /> Resource packs
    </button>
    <button class:active={tab === "shaderpacks"} onclick={() => (tab = "shaderpacks")}>
      <Sparkles size={14} /> Shaders
    </button>
    <button class:active={tab === "worlds"} onclick={() => (tab = "worlds")}>
      <Globe size={14} /> Worlds
    </button>
    <button class:active={tab === "servers"} onclick={() => (tab = "servers")}>
      <Server size={14} /> Servers
    </button>
    <div class="tabs-spacer"></div>
    <button class="icon-btn" onclick={() => load({ force: true })} title="Refresh" disabled={loading}>
      <RefreshCw size={14} class={loading ? "spin" : ""} />
    </button>
    <button
      class="icon-btn"
      onclick={openFolder}
      title={
        tab === "shaderpacks"
          ? "Open shaderpacks folder"
          : tab === "resourcepacks"
            ? "Open resourcepacks folder"
            : tab === "worlds"
              ? "Open saves folder"
              : tab === "mods"
                ? "Open mods folder"
                : "Open instance folder"
      }
    >
      <FolderOpen size={14} />
    </button>
    <button
      class="icon-btn"
      onclick={toggleCollapsed}
      title={collapsed ? "Expand" : "Collapse"}
      aria-expanded={!collapsed}
    >
      {#if collapsed}
        <ChevronDown size={14} />
      {:else}
        <ChevronUp size={14} />
      {/if}
    </button>
  </div>

  {#if collapsed}
    <div class="collapsed-summary">{collapsedSummary}</div>
  {:else}
    <div class="panel" class:is-loading={loading && tabPrimed}>
      {#if showSkeleton}
        <div class="list home-skel-stagger" aria-busy="true" aria-hidden="true">
          {#each Array(4) as _, i (i)}
            <div class="row skel-row" style={`--i: ${i}`}>
              <div class="row-main skel">
                <span class="skeleton skeleton-block skeleton-line medium" style="height: 12px;"></span>
                <span class="skeleton skeleton-block skeleton-line short" style="height: 10px;"></span>
              </div>
              <span class="skeleton skeleton-block skeleton-round" style="width: 56px; height: 28px; flex-shrink: 0;"></span>
            </div>
          {/each}
        </div>
      {:else if tab === "mods"}
        {#if !loading && modCount === 0}
          <div class="mods-cta empty-mods">
            <div>
              <strong class="empty-title">No mods yet</strong>
              <span>Install from the catalog or open the Mods view.</span>
            </div>
            <div class="empty-actions">
              <button class="accent" onclick={onOpenMods}>
                <ExternalLink size={14} /> Open Mods
              </button>
              <button class="ghost" onclick={openCatalog}>
                <Search size={14} /> Browse catalog
              </button>
            </div>
          </div>
        {:else}
          <div class="mods-cta">
            <div>
              {#if loading && modCount == null}
                <strong class="skeleton skeleton-block" style="display:inline-block; width: 28px; height: 18px; vertical-align: middle;"></strong>
              {:else}
                <strong>{modCount == null ? "—" : modCount}</strong>
              {/if}
              <span>mods in this instance</span>
            </div>
            <button class="accent" onclick={onOpenMods}>
              <ExternalLink size={14} /> Open Mods
            </button>
          </div>
        {/if}
      {:else if tab === "resourcepacks" || tab === "shaderpacks"}
        {#if packs.length === 0}
          <div class="empty">
            <p>No {tab === "shaderpacks" ? "shader packs" : "resource packs"} yet.</p>
            <div class="empty-actions">
              <button class="ghost" onclick={openFolder}>
                <FolderOpen size={14} /> Open folder
              </button>
              <button class="accent" onclick={openCatalog}>
                <Search size={14} /> Browse catalog
              </button>
            </div>
          </div>
        {:else}
          <div class="list">
            {#each packs as pack (pack.fileName)}
              <div class="row" class:disabled={!pack.enabled}>
                <div class="row-main">
                  <strong>{pack.name}</strong>
                  <span>{pack.kind} · {pack.sizeFormatted}</span>
                </div>
                <button
                  class="toggle"
                  class:on={pack.enabled}
                  disabled={busyKey === pack.fileName}
                  onclick={() => togglePack(pack)}
                  title={pack.enabled ? "Disable" : "Enable"}
                >
                  <Power size={14} />
                  {pack.enabled ? "On" : "Off"}
                </button>
              </div>
            {/each}
          </div>
        {/if}
      {:else if tab === "worlds"}
        {#if worlds.length === 0}
          <div class="empty">
            <p>No worlds yet.</p>
            <p class="hint">Create one in-game, or open the saves folder.</p>
            <div class="empty-actions">
              <button class="ghost" onclick={openFolder}>
                <FolderOpen size={14} /> Open saves
              </button>
              <button class="accent" onclick={onOpenWorld}>
                <Globe size={14} /> World tools
              </button>
            </div>
          </div>
        {:else}
          <div class="list">
            {#each worlds as world (world.name)}
              <div class="row">
                <div class="row-main">
                  <strong>{world.name}</strong>
                  <span>{world.sizeFormatted}{#if !world.hasLevelDat} · missing level.dat{/if}</span>
                </div>
                <button
                  class="accent"
                  disabled={busyKey === `play:${world.name}`}
                  onclick={() => playWorld(world.name)}
                >
                  <Play size={14} /> Play
                </button>
                <button class="ghost" onclick={onOpenWorld}>Open World tools</button>
              </div>
            {/each}
          </div>
        {/if}
      {:else if tab === "servers"}
        <form class="add-server" onsubmit={(e) => { e.preventDefault(); addServer(); }}>
          <input bind:value={newServerName} placeholder="Name" maxlength={64} />
          <input bind:value={newServerAddress} placeholder="Address (host:port)" maxlength={128} />
          <button type="submit" class="accent" disabled={busyKey === "add-server" || !newServerName.trim() || !newServerAddress.trim()}>
            <Plus size={14} /> Add
          </button>
        </form>
        {#if servers.length === 0}
          <div class="empty">
            <p>Add a server above, or join from the game.</p>
            <button class="ghost quiet-link" onclick={openFolder}>
              <FolderOpen size={14} /> Open instance folder
            </button>
          </div>
        {:else}
          <div class="list">
            {#each servers as srv (srv.address)}
              <div class="row">
                <div class="row-main">
                  <strong>{srv.name}</strong>
                  <span>{srv.address}</span>
                  {#if pings[srv.address]}
                    <span class="ping" class:online={pings[srv.address].online} class:offline={!pings[srv.address].online}>
                      {#if pings[srv.address].online}
                        {pings[srv.address].latencyMs ?? "?"} ms
                      {:else}
                        offline
                      {/if}
                    </span>
                  {/if}
                </div>
                <button
                  class="accent"
                  disabled={busyKey === `join:${srv.address}`}
                  onclick={() => joinServer(srv.address)}
                >
                  <Play size={14} /> Join
                </button>
                <button class="ghost" disabled={busyKey === `ping:${srv.address}`} onclick={() => pingServer(srv.address)}>Ping</button>
                <button class="danger" disabled={busyKey === srv.address} onclick={() => removeServer(srv.address)}>
                  <Trash2 size={14} />
                </button>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .instance-home {
    margin-top: 0;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    background: var(--bg-secondary);
    overflow: hidden;
  }
  .instance-home.collapsed .tabs {
    border-bottom: none;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-color);
    flex-wrap: wrap;
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 9px;
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent);
  }
  .tabs button:hover:not(.active) { color: var(--text-primary); background: var(--bg-hover); }
  .tabs-spacer { flex: 1; }
  .icon-btn {
    width: 28px; height: 28px; padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: var(--border-radius-sm); border: 1px solid var(--border-color);
    background: var(--bg-primary); color: var(--text-muted); cursor: pointer;
  }
  .icon-btn:hover { color: var(--text-primary); }

  .collapsed-summary {
    padding: 4px 12px 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .panel {
    padding: 6px;
    min-height: 132px;
    height: auto;
    max-height: 200px;
    overflow: auto;
    box-sizing: border-box;
  }
  .panel.is-loading {
    opacity: 0.85;
  }
  .mods-cta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 100%;
    padding: 10px;
    border-radius: var(--border-radius-md);
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    box-sizing: border-box;
  }
  .empty {
    color: var(--text-muted);
    font-size: 13px;
    padding: 8px 6px;
    text-align: center;
    min-height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    box-sizing: border-box;
  }
  .empty p { margin: 0; }
  .empty .hint { font-size: 12px; margin-top: 0; }
  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin-top: 4px;
  }
  .quiet-link {
    margin-top: 4px;
    opacity: 0.85;
  }
  .mods-cta.empty-mods {
    flex-wrap: wrap;
  }
  .mods-cta .empty-actions {
    margin-top: 0;
  }

  .list { display: flex; flex-direction: column; gap: 4px; }
  .row {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 8px; border-radius: var(--border-radius-sm);
    background: var(--bg-primary); border: 1px solid var(--border-color);
  }
  .row.disabled { opacity: 0.55; }
  .row.skel-row {
    border-color: transparent;
    background: color-mix(in srgb, var(--bg-primary) 70%, transparent);
  }
  .row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .row-main.skel { width: 100%; }
  .row-main strong { font-size: 13px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-main span { font-size: 11px; color: var(--text-muted); }

  .toggle, .ghost, .danger, .accent {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 10px; border-radius: var(--border-radius-sm); border: 1px solid var(--border-color);
    background: var(--bg-elevated); color: var(--text-secondary);
    font-size: 11px; font-weight: 700; cursor: pointer;
  }
  .toggle.on { color: var(--accent-primary); border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent); }
  .accent { background: var(--accent-primary); color: var(--on-accent, #000); border-color: transparent; }
  .danger:hover { color: #ef4444; border-color: rgba(239, 68, 68, 0.35); }
  .ghost:hover { color: var(--text-primary); }

  .mods-cta strong { display: block; font-size: 28px; color: var(--accent-primary); }
  .mods-cta span { color: var(--text-muted); font-size: 12px; }
  .mods-cta strong.empty-title {
    font-size: 16px;
    color: var(--text-primary);
  }

  .add-server {
    display: grid; grid-template-columns: 1fr 1.4fr auto; gap: 8px; margin-bottom: 10px;
  }
  .add-server input {
    padding: 8px 10px; border-radius: var(--border-radius-sm); border: 1px solid var(--border-color);
    background: var(--bg-primary); color: var(--text-primary); font-size: 12px;
  }
  .ping { font-weight: 700; }
  .ping.online { color: var(--accent-primary); }
  .ping.offline { color: #f87171; }

  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
