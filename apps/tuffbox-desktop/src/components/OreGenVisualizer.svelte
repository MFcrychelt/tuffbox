<script lang="ts">
  import { Mountain, RefreshCw, Database, Map as MapIcon } from "@lucide/svelte";
  import { slide } from "svelte/transition";
  import { projectPath, projectInfo, ideStageRequest } from "../lib/store";
  import { api } from "../lib/api";
  import EmptyState from "./EmptyState.svelte";

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

  let ores = $state<OreEntry[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let selectedOre = $state<string | null>(null);
  let lastOreScanPath = $state<string | null>(null);
  let scanVersion = $state(0);

  const worldMin = $derived(isLegacyVersion($projectInfo?.minecraftVersion) ? 0 : -64);
  const worldMax = $derived(isLegacyVersion($projectInfo?.minecraftVersion) ? 255 : 320);

  function isLegacyVersion(ver: string | undefined): boolean {
    if (!ver) return false;
    const match = ver.match(/^1\.(\d+)/);
    if (!match) return false;
    const minor = parseInt(match[1], 10);
    return minor < 18;
  }

  const CANVAS_HEIGHT = 520;
  const BAR_SLOT = 28;
  const CHART_PAD_LEFT = 60;
  const CHART_PAD_RIGHT = 20;

  function yToCanvas(y: number): number {
    const ratio = (y - worldMin) / (worldMax - worldMin);
    return CANVAS_HEIGHT - ratio * CANVAS_HEIGHT;
  }

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  async function scan() {
    if (!$projectPath) return;
    const path = $projectPath;
    const myVersion = ++scanVersion;
    loading = true;
    error = null;
    try {
      const raw = await api.diagnostics.scanOre(path);
      if (myVersion !== scanVersion) return;
      ores = raw as OreEntry[];
    } catch (e) {
      if (myVersion !== scanVersion) return;
      error = String(e);
      ores = [];
    } finally {
      if (myVersion === scanVersion) {
        lastOreScanPath = path;
        loading = false;
      }
    }
  }

  $effect(() => {
    const path = $projectPath;
    if (!path) return;
    if (lastOreScanPath === path) return;
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => void scan(), 300);
    return () => { if (debounceTimer) clearTimeout(debounceTimer); };
  });

  function parseHeight(val: any): number | null {
    if (!val || !Array.isArray(val) || val.length < 2) return null;
    const raw = String(val[1]).trim();
    // Simple arithmetic: "64 + 16", "128-32", etc.
    const expr = raw.replace(/\s/g, "");
    const match = expr.match(/^(-?\d+(?:\.\d+)?)([+\-*/])(-?\d+(?:\.\d+)?)$/);
    if (match) {
      const [, a, op, b] = match;
      const va = Number(a), vb = Number(b);
      if (!Number.isFinite(va) || !Number.isFinite(vb)) return null;
      switch (op) {
        case "+": return va + vb;
        case "-": return va - vb;
        case "*": return va * vb;
        case "/": return vb !== 0 ? va / vb : null;
      }
    }
    const n = Number(raw);
    return Number.isFinite(n) ? n : null;
  }

  const oreBars = $derived(ores.map((ore) => {
    const minH = parseHeight(ore.minHeight);
    const maxH = parseHeight(ore.maxHeight);
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
  }));

  const chartWidth = $derived(
    Math.max(260, oreBars.length * BAR_SLOT + CHART_PAD_LEFT + CHART_PAD_RIGHT),
  );

  const yTicks = $derived((() => {
    const range = worldMax - worldMin;
    const step = range <= 128 ? 32 : 64;
    const ticks: number[] = [];
    for (let y = worldMax; y >= worldMin; y -= step) ticks.push(y);
    return ticks;
  })());

  const oreColors: Record<string, string> = {
    coal: "#2d2d2d",
    iron: "#d4a373",
    gold: "#ffd60a",
    diamond: "#48cae4",
    emerald: "#2d6a4f",
    copper: "#e07a5f",
    tin: "#c4c4c4",
    lead: "#5c5c8a",
    silver: "#e0e0e0",
    nickel: "#b0b878",
    uranium: "#6bc148",
    zinc: "#a8bd99",
    aluminum: "#f0e2c8",
    aluminium: "#f0e2c8",
    osmium: "#8bbaff",
    platinum: "#c0c8e0",
    ruby: "#e63946",
    sapphire: "#457b9d",
    cobalt: "#1d3557",
    sulfur: "#ffea00",
    quartz: "#f0f0f0",
    iridium: "#d5ceff",
    tungsten: "#8a8a8a",
    titanium: "#bfc4d0",
    chromium: "#8ecaff",
    certus: "#a8d8ea",
    fluorite: "#73e8a0",
    saltpeter: "#e8dcc8",
    redstone: "#ff3333",
    lapis: "#345ec3",
    netherite: "#4a3c2a",
    ancient_debris: "#5c4033",
    amethyst: "#9b59b6",
    topaz: "#ffc048",
    peridot: "#8bc34a",
    bauxite: "#c9a96e",
  };

  function colorFor(resource: string): string {
    const lower = resource.toLowerCase();
    for (const [key, color] of Object.entries(oreColors)) {
      if (lower.includes(key)) return color;
    }
    return "#7c7c8a";
  }

  function openWorldMap() {
    ideStageRequest.set("world-map");
  }

</script>

<div class="ore-visualizer">
  <div class="toolbar">
    <div class="title"><Mountain size={18} /> Ore generation</div>
    <div class="toolbar-actions">
      <button class="ghost" type="button" onclick={openWorldMap} title="Open MCA chunk map">
        <MapIcon size={16} /> World map
      </button>
      <button class="ghost" onclick={scan} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        {loading ? "Scanning..." : "Refresh"}
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}

  <p class="hint">
    Height / vein chart from configs. Chunk select, delete, and export live in
    <button type="button" class="linkish" onclick={openWorldMap}>World map</button>
    (IDE) or sidebar World.
  </p>

  {#if !$projectPath}
    <EmptyState icon={Mountain} title="No project selected" description="Open a project to scan ore generation." />
  {:else if oreBars.length === 0}
    <EmptyState
      icon={Database}
      title={loading ? "Scanning ore configs…" : "No ore data"}
      description={loading ? "Reading generation settings from configs." : "Scan configs to chart ore height ranges."}
      actionLabel={loading ? "" : "Scan ores"}
      onaction={scan}
    />
  {:else}
    <div class="layout">
      <div class="chart-shell">
        <svg viewBox="0 0 {chartWidth} {CANVAS_HEIGHT + 40}" class="ore-chart" width={chartWidth} role="img" aria-label="Ore height range chart">
          <title>Ore height ranges — Y axis from {worldMin} to {worldMax}</title>
          <line x1="60" y1="10" x2="60" y2={CANVAS_HEIGHT + 10} stroke="rgba(255,255,255,.12)" stroke-width="1" />
          {#each yTicks as y (y)}
            {@const cy = yToCanvas(y) + 10}
            <text x="54" y={cy + 4} text-anchor="end" fill="#6b7280" font-size="10">{y}</text>
            <line x1="58" y1={cy} x2={chartWidth - 20} y2={cy} stroke="rgba(255,255,255,.04)" stroke-width="1" />
          {/each}

          <text x="5" y="15" fill="#6b7280" font-size="9">Y</text>

          {#each oreBars as ore, idx (ore.resource + ore.configFile)}
            {@const barX = 68 + idx * BAR_SLOT}
            {@const topY = yToCanvas(Math.min(ore.maxY, worldMax)) + 10}
            {@const botY = yToCanvas(Math.max(ore.minY, worldMin)) + 10}
            {@const barH = Math.max(2, botY - topY)}
            <rect
              x={barX} y={topY} width="18" height={barH} rx="2"
              fill={colorFor(ore.resource)} opacity={ore.enabled ? 0.8 : 0.2}
              stroke={colorFor(ore.resource)} stroke-width="1"
              role="graphics-symbol"
              aria-label="{ore.resource}: Y{ore.minY} to Y{ore.maxY}, vein {ore.veinSize}, {ore.spawnsPerChunk}/chunk{ore.enabled ? '' : ', disabled'}"
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
        {#each oreBars as ore (ore.resource + ore.configFile)}
          <button class="ore-row" class:selected={selectedOre === ore.resource} onclick={() => (selectedOre = selectedOre === ore.resource ? null : ore.resource)}>
            <span class="ore-dot" style="background:{colorFor(ore.resource)}"></span>
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
            <div class="ore-details" transition:slide={{ duration: 150 }}>
              <code>{ore.configFile}</code>
            </div>
          {/if}
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .ore-visualizer { max-width: none; width: 100%; }
  .toolbar, .title { display: flex; align-items: center; gap: 10px; }
  .toolbar { justify-content: space-between; margin-bottom: 12px; }
  .toolbar-actions { display: flex; gap: 8px; align-items: center; }
  .title { color: var(--text-secondary); font-weight: 700; }
  .hint {
    margin: 0 0 14px;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .linkish {
    display: inline;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent-primary);
    cursor: pointer;
    font: inherit;
    text-decoration: underline;
  }
  .linkish:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 2px;
    border-radius: 2px;
  }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239,68,68,.08); border-color: rgba(239,68,68,.28); }
  .layout { display: grid; grid-template-columns: 1fr 380px; gap: 16px; }
  .chart-shell { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 12px; overflow: auto; min-height: 280px; max-height: min(70vh, 640px); }
  .ore-chart { width: 100%; height: auto; min-height: 240px; }
  .ore-list { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; max-height: min(70vh, 640px); overflow: auto; }
  .ore-list h3 { color: var(--text-secondary); font-size: 14px; margin: 0 0 10px; }
  .ore-row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: var(--border-radius-sm); background: transparent; color: var(--text-secondary); border: 1px solid transparent; text-align: left; margin-bottom: 4px; transform: none; }
  .ore-row:hover, .ore-row.selected { background: var(--bg-tertiary); border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent); }
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
  .ore-mod-tag { font-size: 9px; padding: 2px 5px; border-radius: 4px; background: color-mix(in srgb, var(--accent-secondary) 12%, transparent); color: var(--accent-secondary); }
  .ore-details { margin-left: 22px; margin-bottom: 6px; padding: 6px 10px; border-radius: 6px; background: var(--bg-tertiary); }
  .ore-details code { font-size: 10px; color: var(--text-muted); word-break: break-all; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 920px) { .layout { grid-template-columns: 1fr; } }
</style>
