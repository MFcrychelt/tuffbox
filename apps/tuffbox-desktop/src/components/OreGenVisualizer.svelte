<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Mountain, RefreshCw, Database } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type OreEntry = {
    resource: string;
    configFile: string;
    enabledKey: string;
    enabledValue: string;
    veinSize?: [string, string] | null;
    minHeight?: [string, string] | null;
    maxHeight?: [string, string] | null;
    spawnsPerChunk?: [string, string] | null;
    confidence: string;
    knownMod?: string | null;
  };

  type WorldEntry = {
    name: string;
    size: number;
    sizeFormatted: string;
    hasLevelDat: boolean;
  };

  type WorldInfo = {
    name: string;
    seed?: string;
    gameType?: string;
    versionName?: string;
    lastPlayed?: number;
    time?: number;
    spawnX?: number;
    spawnY?: number;
    spawnZ?: number;
    difficulty?: string;
    hardcore?: boolean;
    cheatsEnabled?: boolean;
    sizeFormatted?: string;
  };

  let ores: OreEntry[] = [];
  let loading = false;
  let error: string | null = null;
  let selectedOre: string | null = null;

  // World management section
  let worlds: WorldEntry[] = [];
  let worldsLoading = false;
  let worldsError: string | null = null;
  let selectedWorld: string | null = null;
  let selectedWorldInfo: WorldInfo | null = null;
  let backingUp = false;
  let backupMessage: string | null = null;

  async function loadWorlds() {
    if (!$projectPath) return;
    worldsLoading = true;
    worldsError = null;
    try {
      worlds = (await invoke("list_worlds", { path: $projectPath })) as WorldEntry[];
    } catch (e) {
      worldsError = String(e);
    } finally {
      worldsLoading = false;
    }
  }

  async function selectWorld(name: string) {
    selectedWorld = name;
    selectedWorldInfo = null;
    if (!$projectPath) return;
    try {
      selectedWorldInfo = (await invoke("read_world_info", {
        path: $projectPath,
        worldName: name,
      })) as WorldInfo;
    } catch {
      selectedWorldInfo = null;
    }
  }

  async function backupSelectedWorld() {
    if (!$projectPath || !selectedWorld) return;
    backingUp = true;
    backupMessage = null;
    worldsError = null;
    try {
      const zip: string = await invoke("backup_world", {
        path: $projectPath,
        worldName: selectedWorld,
      });
      backupMessage = `Backed up to ${zip}`;
    } catch (e) {
      worldsError = String(e);
    } finally {
      backingUp = false;
    }
  }

  // Y-level constants: world goes from -64 to 320
  const WORLD_MIN = -64;
  const WORLD_MAX = 320;
  const CANVAS_HEIGHT = 520;
  const CANVAS_WIDTH = 260;

  function yToCanvas(y: number): number {
    const ratio = (y - WORLD_MIN) / (WORLD_MAX - WORLD_MIN);
    return CANVAS_HEIGHT - (ratio * CANVAS_HEIGHT);
  }

  async function scan() {
    if (!$projectPath) return;
    loading = true; error = null;
    try {
      ores = await invoke("scan_ore_generation", { path: $projectPath });
    } catch (e) { error = String(e); }
    finally { loading = false; }
  }

  function parseHeight(val: any): number | null {
    if (!val || !Array.isArray(val) || val.length < 2) return null;
    const n = Number(val[1]);
    return Number.isFinite(n) ? n : null;
  }

  $: oreBars = ores.map(ore => {
    const minH = parseHeight(ore.minHeight);
    const maxH = parseHeight(ore.maxHeight);
    const vein = parseHeight(ore.veinSize);
    const freq = parseHeight(ore.spawnsPerChunk);
    return {
      resource: ore.resource,
      configFile: ore.configFile,
      confidence: ore.confidence,
      knownMod: ore.knownMod,
      minY: minH ?? -32,
      maxY: maxH ?? 64,
      veinSize: ore.veinSize?.[1] ?? "?",
      spawnsPerChunk: ore.spawnsPerChunk?.[1] ?? "?",
      enabled: ore.enabledValue === "true" || ore.enabledValue === "1",
    };
  });

  // Color per resource
  const oreColors: Record<string, string> = {
    coal: "#2d2d2d", iron: "#d4a373", gold: "#ffd60a", diamond: "#48cae4",
    emerald: "#2d6a4f", copper: "#e07a5f", tin: "#c4c4c4", lead: "#5c5c8a",
    silver: "#e0e0e0", nickel: "#b0b878", uranium: "#6bc148", zinc: "#a8bd99",
    aluminum: "#f0e2c8", osmium: "#8bbaff", platinum: "#c0c8e0",
    ruby: "#e63946", sapphire: "#457b9d", cobalt: "#1d3557",
    sulfur: "#ffea00", quartz: "#f0f0f0", iridium: "#d5ceff",
  };

  function colorFor(resource: string): string {
    for (const [key, color] of Object.entries(oreColors)) {
      if (resource.toLowerCase().includes(key)) return color;
    }
    return "#7c7c8a";
  }

  $: if ($projectPath && ores.length === 0) scan();
  $: if ($projectPath && worlds.length === 0 && !worldsLoading) loadWorlds();
</script>

<div class="ore-visualizer">
  <div class="toolbar">
    <div class="title"><Mountain size={18} /> Ore generation</div>
    <button class="ghost" on:click={scan} disabled={!$projectPath || loading}>
      <RefreshCw size={16} class={loading ? "spin" : ""} />
      {loading ? "Scanning..." : "Refresh"}
    </button>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}

  {#if !$projectPath}
    <div class="empty">Open a project to scan ore generation.</div>
  {:else if oreBars.length === 0}
    <div class="empty">
      <Database size={32} />
      <p>No ore generation detected. Run "Ore gen scan" from Diagnostics.</p>
    </div>
  {:else}
    <div class="layout">
      <div class="chart-shell">
        <svg viewBox="0 0 {CANVAS_WIDTH + 80} {CANVAS_HEIGHT + 40}" class="ore-chart">
          <!-- Y-axis -->
          <line x1="60" y1="10" x2="60" y2={CANVAS_HEIGHT + 10} stroke="rgba(255,255,255,.12)" stroke-width="1" />
          <!-- Y-axis labels -->
          {#each [320, 256, 192, 128, 64, 0, -64] as y}
            {@const cy = yToCanvas(y) + 10}
            <text x="54" y={cy + 4} text-anchor="end" fill="#6b7280" font-size="10">{y}</text>
            <line x1="58" y1={cy} x2={CANVAS_WIDTH + 60} y2={cy} stroke="rgba(255,255,255,.04)" stroke-width="1" />
          {/each}

          <!-- Legend -->
          <text x="5" y="15" fill="#6b7280" font-size="9">Y</text>

          <!-- Ore bars -->
          {#each oreBars as ore, idx}
            {@const barX = 68 + idx * 24}
            {@const topY = yToCanvas(Math.min(ore.maxY, WORLD_MAX)) + 10}
            {@const botY = yToCanvas(Math.max(ore.minY, WORLD_MIN)) + 10}
            {@const barH = Math.max(2, botY - topY)}
            <rect
              x={barX} y={topY} width="18" height={barH} rx="2"
              fill={colorFor(ore.resource)} opacity={ore.enabled ? 0.8 : 0.2}
              stroke={colorFor(ore.resource)} stroke-width="1"
            />
            <text
              x={barX + 9} y={CANVAS_HEIGHT + 28} text-anchor="middle"
              fill="#9ca3af" font-size="9" transform="rotate(-35,{barX+9},{CANVAS_HEIGHT+28})"
            >{ore.resource}</text>
          {/each}
        </svg>
      </div>

      <div class="ore-list">
        <h3>Detected ores ({oreBars.length})</h3>
        {#each oreBars as ore}
          <button class="ore-row" class:selected={selectedOre === ore.resource} on:click={() => (selectedOre = selectedOre === ore.resource ? null : ore.resource)}>
            <span class="ore-dot" style="background:{colorFor(ore.resource)}" />
            <div class="ore-detail">
              <strong>{ore.resource}</strong>
              <span>Y{ore.minY} – Y{ore.maxY} · vein {ore.veinSize} · {ore.spawnsPerChunk}/chunk</span>
            </div>
            <div class="ore-tags">
              <span class="ore-conf-tag {ore.confidence}">{ore.confidence}</span>
              {#if !ore.enabled}<span class="ore-disabled">off</span>{/if}
              {#if ore.knownMod}<span class="ore-mod-tag">{ore.knownMod}</span>{/if}
            </div>
          </button>
          {#if selectedOre === ore.resource}
            <div class="ore-details">
              <code>{ore.configFile}</code>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {/if}

  <section class="worlds-section">
    <div class="toolbar">
      <div class="title"><Database size={18} /> Worlds ({worlds.length})</div>
      <button class="ghost" on:click={loadWorlds} disabled={!$projectPath || worldsLoading}>
        <RefreshCw size={16} class={worldsLoading ? "spin" : ""} />
        {worldsLoading ? "Loading..." : "Refresh"}
      </button>
    </div>

    {#if worldsError}<div class="notice error">{worldsError}</div>{/if}
    {#if backupMessage}<div class="notice success">{backupMessage}</div>{/if}

    {#if !$projectPath}
      <div class="empty">Open a project to manage worlds.</div>
    {:else if worlds.length === 0}
      <div class="empty">
        <Database size={32} />
        <p>No worlds found in saves/. Test runs will create them automatically.</p>
      </div>
    {:else}
      <div class="worlds-layout">
        <div class="world-list">
          {#each worlds as world}
            <button
              class="world-row"
              class:selected={selectedWorld === world.name}
              on:click={() => selectWorld(world.name)}
            >
              <div class="world-main">
                <strong>{world.name}</strong>
                <span>{world.sizeFormatted}{#if !world.hasLevelDat} · no level.dat{/if}</span>
              </div>
              {#if selectedWorld === world.name}
                <button class="mini" on:click|stopPropagation={backupSelectedWorld} disabled={backingUp || !world.hasLevelDat}>
                  <RefreshCw size={12} /> {backingUp ? "Backing up..." : "Backup"}
                </button>
              {/if}
            </button>
          {/each}
        </div>

        {#if selectedWorldInfo}
          <div class="world-detail">
            <h3>{selectedWorldInfo.name}</h3>
            <div class="world-meta">
              {#if selectedWorldInfo.seed}<div><span>Seed</span><code>{selectedWorldInfo.seed}</code></div>{/if}
              {#if selectedWorldInfo.gameType}<div><span>Game type</span><code>{selectedWorldInfo.gameType}</code></div>{/if}
              {#if selectedWorldInfo.versionName}<div><span>Version</span><code>{selectedWorldInfo.versionName}</code></div>{/if}
              {#if selectedWorldInfo.difficulty}<div><span>Difficulty</span><code>{selectedWorldInfo.difficulty}</code></div>{/if}
              {#if selectedWorldInfo.spawnX !== undefined}
                <div><span>Spawn</span><code>X {selectedWorldInfo.spawnX} · Y {selectedWorldInfo.spawnY} · Z {selectedWorldInfo.spawnZ}</code></div>
              {/if}
              <div class="world-flags">
                {#if selectedWorldInfo.hardcore}<span class="flag danger">hardcore</span>{/if}
                {#if selectedWorldInfo.cheatsEnabled}<span class="flag">cheats on</span>{/if}
                {#if selectedWorldInfo.sizeFormatted}<span class="flag">size {selectedWorldInfo.sizeFormatted}</span>{/if}
              </div>
            </div>
          </div>
        {:else}
          <div class="world-detail empty-detail">Select a world to see level.dat details.</div>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  .ore-visualizer { max-width: none; width: 100%; }
  .toolbar, .title { display: flex; align-items: center; gap: 10px; }
  .toolbar { justify-content: space-between; margin-bottom: 16px; }
  .title { color: var(--text-secondary); font-weight: 700; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239,68,68,.08); border-color: rgba(239,68,68,.28); }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .layout { display: grid; grid-template-columns: 1fr 380px; gap: 16px; }
  .chart-shell { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 12px; overflow: auto; }
  .ore-chart { width: 100%; }
  .ore-list { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; max-height: 620px; overflow: auto; }
  .ore-list h3 { color: var(--text-secondary); font-size: 14px; margin: 0 0 10px; }
  .ore-row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 8px; background: transparent; color: var(--text-secondary); border: 1px solid transparent; text-align: left; margin-bottom: 4px; transform: none; }
  .ore-row:hover, .ore-row.selected { background: var(--bg-tertiary); border-color: rgba(27,217,106,.25); }
  .ore-dot { width: 12px; height: 12px; border-radius: 50%; flex-shrink: 0; }
  .ore-detail { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .ore-detail strong { color: var(--text-primary); font-size: 12px; text-transform: capitalize; }
  .ore-detail span { color: var(--text-muted); font-size: 10px; }
  .ore-tags { display: flex; gap: 4px; flex-shrink: 0; }
  .ore-conf-tag { font-size: 9px; text-transform: uppercase; padding: 2px 5px; border-radius: 4px; background: var(--bg-elevated); font-weight: 700; }
  .ore-conf-tag.high { color: var(--accent-primary); }
  .ore-conf-tag.medium { color: #fbbf24; }
  .ore-conf-tag.low { color: var(--text-muted); }
  .ore-disabled { font-size: 9px; padding: 2px 5px; border-radius: 4px; background: rgba(239,68,68,.15); color: #fca5a5; }
  .ore-mod-tag { font-size: 9px; padding: 2px 5px; border-radius: 4px; background: rgba(139,92,246,.12); color: var(--accent-secondary); }
  .ore-details { margin-left: 22px; margin-bottom: 6px; padding: 6px 10px; border-radius: 6px; background: var(--bg-tertiary); }
  .ore-details code { font-size: 10px; color: var(--text-muted); word-break: break-all; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 920px) { .layout { grid-template-columns: 1fr; } }

  .worlds-section {
    margin-top: 24px;
    padding-top: 20px;
    border-top: 1px solid var(--border-color);
  }
  .worlds-layout {
    display: grid;
    grid-template-columns: 1fr 380px;
    gap: 16px;
  }
  .world-list {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 14px;
    max-height: 520px;
    overflow: auto;
  }
  .world-row {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    text-align: left;
    margin-bottom: 4px;
    transform: none;
  }
  .world-row:hover, .world-row.selected {
    background: var(--bg-tertiary);
    border-color: rgba(27,217,106,.25);
  }
  .world-main { display: grid; gap: 2px; min-width: 0; }
  .world-main strong { color: var(--text-primary); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .world-main span { color: var(--text-muted); font-size: 11px; }
  .world-row .mini {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 10px; font-size: 11px; flex-shrink: 0;
  }
  .world-detail {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 16px;
    max-height: 520px;
    overflow: auto;
  }
  .world-detail h3 { margin: 0 0 12px; color: var(--text-primary); font-size: 15px; word-break: break-all; }
  .world-meta { display: grid; gap: 8px; }
  .world-meta > div {
    display: flex; justify-content: space-between; align-items: center; gap: 12px;
    padding: 8px 10px; border-radius: 8px; background: var(--bg-tertiary); border: 1px solid var(--border-color);
  }
  .world-meta span { color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: .04em; }
  .world-meta code { color: var(--text-primary); font-size: 12px; font-family: ui-monospace, monospace; text-align: right; word-break: break-all; }
  .world-flags { flex-wrap: wrap; background: transparent !important; border: none !important; padding: 0 !important; justify-content: flex-start !important; }
  .flag { font-size: 9px; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); color: var(--text-muted); font-weight: 700; }
  .flag.danger { background: rgba(239,68,68,.15); color: #fca5a5; }
  .empty-detail { color: var(--text-muted); display: flex; align-items: center; justify-content: center; text-align: center; }
  @media (max-width: 920px) { .worlds-layout { grid-template-columns: 1fr; } }
</style>
