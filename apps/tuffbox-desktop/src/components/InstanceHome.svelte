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
  } from "lucide-svelte";
  import { api, type WorldListItem } from "../lib/api";
  import { toasts } from "../lib/toast";

  export let projectPath: string;
  export let onOpenMods: () => void = () => {};
  export let onOpenWorld: () => void = () => {};

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

  let tab: Tab = "resourcepacks";
  let loading = false;
  let packs: PackEntry[] = [];
  let worlds: WorldListItem[] = [];
  let servers: ServerEntry[] = [];
  let pings: Record<string, PingResult> = {};
  let modCount: number | null = null;
  let busyKey: string | null = null;

  let newServerName = "";
  let newServerAddress = "";

  async function load() {
    if (!projectPath) return;
    loading = true;
    try {
      if (tab === "resourcepacks" || tab === "shaderpacks") {
        packs = await api.content.listPacks(tab, projectPath);
      } else if (tab === "worlds") {
        worlds = await api.worlds.list(projectPath);
      } else if (tab === "servers") {
        servers = await api.servers.list(projectPath);
      } else if (tab === "mods") {
        try {
          const mods = await api.mods.list(projectPath);
          modCount = Array.isArray(mods) ? mods.filter((m: any) => !m.contentType || m.contentType === "mod").length : 0;
        } catch {
          modCount = null;
        }
      }
    } catch (e) {
      toasts.error(String(e));
    } finally {
      loading = false;
    }
  }

  $: if (projectPath && tab) {
    void load();
  }

  async function togglePack(pack: PackEntry) {
    busyKey = pack.fileName;
    try {
      await api.content.setEnabled(tab === "shaderpacks" ? "shaderpacks" : "resourcepacks", pack.fileName, !pack.enabled, projectPath);
      await load();
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busyKey = null;
    }
  }

  async function openFolder() {
    const folder = tab === "shaderpacks" ? "shaderpacks" : tab === "resourcepacks" ? "resourcepacks" : tab === "worlds" ? "saves" : null;
    if (!folder) {
      await api.files.openFolder(projectPath);
      return;
    }
    try {
      await api.files.openFolder(projectPath);
    } catch (e) {
      toasts.error(String(e));
    }
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
</script>

<section class="instance-home">
  <div class="tabs">
    <button class:active={tab === "mods"} on:click={() => (tab = "mods")}>
      <Package size={14} /> Mods
    </button>
    <button class:active={tab === "resourcepacks"} on:click={() => (tab = "resourcepacks")}>
      <Image size={14} /> Resource packs
    </button>
    <button class:active={tab === "shaderpacks"} on:click={() => (tab = "shaderpacks")}>
      <Sparkles size={14} /> Shaders
    </button>
    <button class:active={tab === "worlds"} on:click={() => (tab = "worlds")}>
      <Globe size={14} /> Worlds
    </button>
    <button class:active={tab === "servers"} on:click={() => (tab = "servers")}>
      <Server size={14} /> Servers
    </button>
    <div class="tabs-spacer"></div>
    <button class="icon-btn" on:click={load} title="Refresh" disabled={loading}>
      <RefreshCw size={14} class={loading ? "spin" : ""} />
    </button>
    <button class="icon-btn" on:click={openFolder} title="Open instance folder">
      <FolderOpen size={14} />
    </button>
  </div>

  <div class="panel">
    {#if loading && packs.length === 0 && worlds.length === 0 && servers.length === 0 && tab !== "mods"}
      <div class="empty muted">Loading…</div>
    {:else if tab === "mods"}
      <div class="mods-cta">
        <div>
          <strong>{modCount == null ? "—" : modCount}</strong>
          <span>mods in this instance</span>
        </div>
        <button class="accent" on:click={onOpenMods}>
          <ExternalLink size={14} /> Open Mods
        </button>
      </div>
    {:else if tab === "resourcepacks" || tab === "shaderpacks"}
      {#if packs.length === 0}
        <div class="empty">
          <p>No {tab === "shaderpacks" ? "shader packs" : "resource packs"} yet.</p>
          <p class="hint">Drop `.zip` files into the folder or install from Mods → content type filter.</p>
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
                on:click={() => togglePack(pack)}
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
        <div class="empty"><p>No worlds in `saves/`.</p></div>
      {:else}
        <div class="list">
          {#each worlds as world (world.name)}
            <div class="row">
              <div class="row-main">
                <strong>{world.name}</strong>
                <span>{world.sizeFormatted}{#if !world.hasLevelDat} · missing level.dat{/if}</span>
              </div>
              <button class="ghost" on:click={onOpenWorld}>Open World tools</button>
            </div>
          {/each}
        </div>
      {/if}
    {:else if tab === "servers"}
      <form class="add-server" on:submit|preventDefault={addServer}>
        <input bind:value={newServerName} placeholder="Name" maxlength={64} />
        <input bind:value={newServerAddress} placeholder="Address (host:port)" maxlength={128} />
        <button type="submit" class="accent" disabled={busyKey === "add-server" || !newServerName.trim() || !newServerAddress.trim()}>
          <Plus size={14} /> Add
        </button>
      </form>
      {#if servers.length === 0}
        <div class="empty"><p>No servers in `servers.dat`.</p></div>
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
              <button class="ghost" disabled={busyKey === `ping:${srv.address}`} on:click={() => pingServer(srv.address)}>Ping</button>
              <button class="danger" disabled={busyKey === srv.address} on:click={() => removeServer(srv.address)}>
                <Trash2 size={14} />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</section>

<style>
  .instance-home {
    margin-top: 16px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    background: var(--bg-secondary);
    overflow: hidden;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border-color);
    flex-wrap: wrap;
  }
  .tabs button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
  }
  .tabs button:hover:not(.active) { color: var(--text-primary); background: var(--bg-hover); }
  .tabs-spacer { flex: 1; }
  .icon-btn {
    width: 30px; height: 30px; padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 8px; border: 1px solid var(--border-color);
    background: var(--bg-primary); color: var(--text-muted); cursor: pointer;
  }
  .icon-btn:hover { color: var(--text-primary); }

  .panel { padding: 12px; min-height: 140px; max-height: 320px; overflow: auto; }
  .empty { color: var(--text-muted); font-size: 13px; padding: 18px 8px; text-align: center; }
  .empty .hint { font-size: 12px; margin-top: 6px; }
  .empty.muted { opacity: 0.7; }

  .list { display: flex; flex-direction: column; gap: 6px; }
  .row {
    display: flex; align-items: center; gap: 8px;
    padding: 10px 12px; border-radius: 10px;
    background: var(--bg-primary); border: 1px solid var(--border-color);
  }
  .row.disabled { opacity: 0.55; }
  .row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .row-main strong { font-size: 13px; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-main span { font-size: 11px; color: var(--text-muted); }

  .toggle, .ghost, .danger, .accent {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 10px; border-radius: 8px; border: 1px solid var(--border-color);
    background: var(--bg-elevated); color: var(--text-secondary);
    font-size: 11px; font-weight: 700; cursor: pointer;
  }
  .toggle.on { color: var(--accent-primary); border-color: rgba(27, 217, 106, 0.35); }
  .accent { background: var(--accent-primary); color: #000; border-color: transparent; }
  .danger:hover { color: #ef4444; border-color: rgba(239, 68, 68, 0.35); }
  .ghost:hover { color: var(--text-primary); }

  .mods-cta {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 16px; border-radius: 12px; background: var(--bg-primary); border: 1px solid var(--border-color);
  }
  .mods-cta strong { display: block; font-size: 28px; color: var(--accent-primary); }
  .mods-cta span { color: var(--text-muted); font-size: 12px; }

  .add-server {
    display: grid; grid-template-columns: 1fr 1.4fr auto; gap: 8px; margin-bottom: 10px;
  }
  .add-server input {
    padding: 8px 10px; border-radius: 8px; border: 1px solid var(--border-color);
    background: var(--bg-primary); color: var(--text-primary); font-size: 12px;
  }
  .ping { font-weight: 700; }
  .ping.online { color: var(--accent-primary); }
  .ping.offline { color: #f87171; }

  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
