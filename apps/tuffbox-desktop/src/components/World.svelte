<script lang="ts">
  import { Globe, Download, RefreshCw, Database, ChevronRight, HardDrive, Clock, MapPin, Swords, Shield } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import { api } from "../lib/api";
  import type { WorldListItem, WorldDetail } from "../lib/api";
  import WorldMap from "./WorldMap.svelte";
  import EmptyState from "./EmptyState.svelte";

  let worlds: WorldListItem[] = [];
  let loading = false;
  let error: string | null = null;
  let selectedWorld: string | null = null;
  let worldDetail: WorldDetail | null = null;
  let detailLoading = false;
  let backupMsg: string | null = null;

  async function loadWorlds() {
    const p = $projectPath;
    if (!p) return;
    loading = true;
    error = null;
    try {
      worlds = await api.worlds.list(p);
      if (worlds.length > 0 && !selectedWorld) {
        selectWorld(worlds[0].name);
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function selectWorld(name: string) {
    const p = $projectPath;
    if (!p) return;
    selectedWorld = name;
    worldDetail = null;
    detailLoading = true;
    try {
      worldDetail = await api.worlds.readInfo(name, p);
    } catch (e) {
      worldDetail = null;
    } finally {
      detailLoading = false;
    }
  }

  async function backupWorld() {
    const p = $projectPath;
    if (!selectedWorld || !p) return;
    try {
      const path = await api.worlds.backup(selectedWorld, p);
      backupMsg = `Backed up to ${path.split(/[\\/]/).pop()}`;
      setTimeout(() => (backupMsg = null), 3000);
    } catch (e) {
      backupMsg = null;
    }
  }

  function gameTypeLabel(t: string | number): string {
    if (typeof t === "string" && t.length > 0 && isNaN(Number(t))) return t;
    const n = typeof t === "number" ? t : Number(t);
    switch (n) {
      case 0: return "Survival";
      case 1: return "Creative";
      case 2: return "Adventure";
      case 3: return "Spectator";
      default: return String(t ?? "—");
    }
  }

  function difficultyLabel(d: string | number): string {
    if (typeof d === "string" && d.length > 0 && isNaN(Number(d))) return d;
    const n = typeof d === "number" ? d : Number(d);
    switch (n) {
      case 0: return "Peaceful";
      case 1: return "Easy";
      case 2: return "Normal";
      case 3: return "Hard";
      default: return String(d ?? "—");
    }
  }

  function formatTime(ticks: number): string {
    const totalMinutes = Math.floor(ticks / 1200);
    const days = Math.floor(totalMinutes / 1440);
    const hours = Math.floor((totalMinutes % 1440) / 60);
    const mins = totalMinutes % 60;
    if (days > 0) return `${days}d ${hours}h ${mins}m`;
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
  }

  $: if ($projectPath) { loadWorlds(); }
</script>

<div class="worlds-view">
  <div class="worlds-layout">
    <!-- Sidebar: world list -->
    <aside class="worlds-sidebar">
      <div class="sidebar-header">
        <Globe size={18} />
        <span>Worlds</span>
        <button class="icon-btn" on:click={loadWorlds} disabled={loading} title="Refresh">
          <RefreshCw size={14} class={loading ? "spin" : ""} />
        </button>
      </div>

      <div class="world-list">
        {#if worlds.length === 0 && !loading}
          <EmptyState
            icon={Globe}
            title="No worlds found"
            description="Generate a world by launching the game, then refresh."
          />
        {:else}
          {#each worlds as w (w.name)}
            <button
              class="world-item"
              class:active={selectedWorld === w.name}
              on:click={() => selectWorld(w.name)}
            >
              <div class="world-icon">
                <Database size={16} />
              </div>
              <div class="world-info">
                <span class="world-name">{w.name}</span>
                <span class="world-meta">{w.sizeFormatted}</span>
              </div>
              <ChevronRight size={14} class="chevron" />
            </button>
          {/each}
        {/if}
      </div>
    </aside>

    <!-- Main content -->
    <div class="worlds-main">
      {#if selectedWorld}
        <div class="world-header">
          <div class="world-title">
            <Globe size={20} />
            <h2>{selectedWorld}</h2>
          </div>
          <div class="world-actions">
            <button class="ghost" on:click={backupWorld} title="Backup this world">
              <Download size={14} /> Backup
            </button>
          </div>
        </div>

        {#if backupMsg}
          <div class="backup-msg">{backupMsg}</div>
        {/if}

        <div class="info-strip">
          {#if worldDetail}
            <div class="info-chip">
              <MapPin size={11} />
              <span class="lbl">Seed</span>
              <span class="val seed">{worldDetail.seed?.toString()}</span>
            </div>
            <div class="info-chip">
              <Swords size={11} />
              <span class="lbl">Mode</span>
              <span class="val">{gameTypeLabel(worldDetail.gameType)}</span>
            </div>
            <div class="info-chip">
              <Shield size={11} />
              <span class="lbl">Diff</span>
              <span class="val">{difficultyLabel(worldDetail.difficulty)}</span>
            </div>
            <div class="info-chip">
              <HardDrive size={11} />
              <span class="lbl">Size</span>
              <span class="val">{worldDetail.sizeFormatted}</span>
            </div>
            <div class="info-chip">
              <Clock size={11} />
              <span class="lbl">Play</span>
              <span class="val">{formatTime(worldDetail.time || 0)}</span>
            </div>
            <div class="info-chip">
              <MapPin size={11} />
              <span class="lbl">Spawn</span>
              <span class="val">{worldDetail.spawnX}, {worldDetail.spawnY}, {worldDetail.spawnZ}</span>
            </div>
          {:else if detailLoading}
            <span class="info-loading">
              <RefreshCw size={12} class="spin" /> Loading world info…
            </span>
          {:else}
            <span class="info-loading">World info unavailable (map still works from saves/)</span>
          {/if}
        </div>

        <div class="map-stage">
          <WorldMap worldName={selectedWorld} layout="dock" />
        </div>
      {:else}
        <EmptyState
          icon={Globe}
          title="Select a world"
          description="Choose a world from the sidebar to view its map and details."
        />
      {/if}
    </div>
  </div>
</div>

<style>
  .worlds-view { height: 100%; display: flex; flex-direction: column; }
  .worlds-layout { display: flex; gap: 0; height: 100%; min-height: 0; }

  .worlds-sidebar {
    width: 260px;
    flex-shrink: 0;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border-color);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 16px 16px 12px;
    font-weight: 700;
    font-size: 14px;
    color: var(--text-primary);
    border-bottom: 1px solid var(--border-color);
  }

  .sidebar-header .icon-btn {
    margin-left: auto;
    background: none;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    padding: 4px;
    cursor: pointer;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sidebar-header .icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .world-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .world-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid transparent;
    border-radius: var(--border-radius-md);
    background: transparent;
    cursor: pointer;
    text-align: left;
    transition: all 0.12s ease;
    width: 100%;
  }

  .world-item:hover {
    background: var(--bg-hover);
  }

  .world-item.active {
    background: rgba(27, 217, 106, 0.12);
    border-color: rgba(27, 217, 106, 0.24);
  }

  .world-item.active .world-name { color: var(--accent-primary); }

  .world-icon {
    width: 32px;
    height: 32px;
    border-radius: var(--border-radius-sm);
    background: var(--bg-tertiary);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--accent-secondary);
  }

  .world-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .world-name { font-weight: 600; font-size: 13px; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .world-meta { font-size: 11px; color: var(--text-muted); }

  .worlds-main {
    flex: 1;
    min-width: 0;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    overflow: hidden;
  }

  .world-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-shrink: 0;
  }

  .world-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .world-title h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .world-actions { display: flex; gap: 8px; }

  .backup-msg {
    font-size: 12px;
    color: var(--accent-primary);
    padding: 4px 10px;
    background: rgba(27, 217, 106, 0.1);
    border-radius: var(--border-radius-sm);
    border: 1px solid rgba(27, 217, 106, 0.2);
    flex-shrink: 0;
  }

  .info-strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 6px 10px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    flex-shrink: 0;
  }

  .info-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .info-chip .lbl {
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-size: 10px;
  }

  .info-chip .val {
    font-weight: 600;
    color: var(--text-primary);
    font-size: 12px;
  }

  .info-chip .val.seed {
    font-family: monospace;
    font-size: 11px;
  }

  .info-loading {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .map-stage {
    flex: 1;
    min-height: 0;
    display: flex;
  }
</style>
