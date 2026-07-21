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

  function gameTypeLabel(t: number): string {
    switch (t) {
      case 0: return "Survival";
      case 1: return "Creative";
      case 2: return "Adventure";
      case 3: return "Spectator";
      default: return `Type ${t}`;
    }
  }

  function difficultyLabel(d: number): string {
    switch (d) {
      case 0: return "Peaceful";
      case 1: return "Easy";
      case 2: return "Normal";
      case 3: return "Hard";
      default: return `Level ${d}`;
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
          {#each worlds as w}
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
        <!-- World detail header -->
        <div class="world-header">
          <div class="world-title">
            <Globe size={22} />
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

        <!-- World info cards -->
        {#if worldDetail}
          <div class="info-grid">
            <div class="info-card">
              <div class="info-label"><MapPin size={12} /> Seed</div>
              <div class="info-value seed">{worldDetail.seed?.toString()}</div>
            </div>
            <div class="info-card">
              <div class="info-label"><Swords size={12} /> Game Mode</div>
              <div class="info-value">{gameTypeLabel(worldDetail.gameType)}</div>
            </div>
            <div class="info-card">
              <div class="info-label"><Shield size={12} /> Difficulty</div>
              <div class="info-value">{difficultyLabel(worldDetail.difficulty)}</div>
            </div>
            <div class="info-card">
              <div class="info-label"><HardDrive size={12} /> Size</div>
              <div class="info-value">{worldDetail.sizeFormatted}</div>
            </div>
            <div class="info-card">
              <div class="info-label"><Clock size={12} /> Play Time</div>
              <div class="info-value">{formatTime(worldDetail.time || 0)}</div>
            </div>
            <div class="info-card">
              <div class="info-label"><MapPin size={12} /> Spawn</div>
              <div class="info-value">{worldDetail.spawnX}, {worldDetail.spawnY}, {worldDetail.spawnZ}</div>
            </div>
          </div>

          <!-- World map (mcaselector) -->
          <div class="map-section">
            <WorldMap worldName={selectedWorld} />
          </div>
        {:else if detailLoading}
          <div class="loading-state">
            <RefreshCw size={20} class="spin" />
            <span>Loading world data…</span>
          </div>
        {/if}
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
    display: flex;
    flex-direction: column;
    gap: 16px;
    overflow-y: auto;
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
    gap: 10px;
  }

  .world-title h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .world-actions { display: flex; gap: 8px; }

  .backup-msg {
    font-size: 12px;
    color: var(--accent-primary);
    padding: 6px 12px;
    background: rgba(27, 217, 106, 0.1);
    border-radius: var(--border-radius-sm);
    border: 1px solid rgba(27, 217, 106, 0.2);
  }

  .info-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px;
    flex-shrink: 0;
  }

  .info-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    padding: 12px;
  }

  .info-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
    margin-bottom: 6px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .info-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .info-value.seed {
    font-family: monospace;
    font-size: 13px;
    word-break: break-all;
  }

  .map-section {
    flex: 1;
    min-height: 0;
  }

  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 40px;
    color: var(--text-muted);
    font-size: 14px;
  }
</style>
