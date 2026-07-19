<script lang="ts">
  import { Map as MapIcon, RefreshCw, Trash2, MousePointer2, Square, Layers, Download, CalendarRange, CheckSquare, XSquare } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import { api } from "../lib/api";
  import type { WorldMap as WorldMapData, RegionInfo, ChunkCell } from "../lib/api";

  // Status codes returned by the backend (mirror of region.rs).
  const STATUS_UNKNOWN = 0;
  const STATUS_EMPTY = 1;
  const STATUS_PARTIAL = 2;
  const STATUS_FULL = 3;

  // Color modes.
  type ColorMode = "status" | "date";

  export let worldName: string = "";

  let map: WorldMapData | null = null;
  let loading = false;
  let error: string | null = null;

  // View options
  let showRegions = false;
  let colorMode: ColorMode = "status";
  let selecting = false; // click-select mode
  let boxSelecting = false; // drag-rectangle mode
  let selection = new Set<string>(); // "rx:rz:index" keys

  // Hover tooltip
  let hover: { rx: number; rz: number; cx: number; cz: number; status: string; modified: number } | null = null;

  // Date filter
  let filterFrom = ""; // yyyy-mm-dd
  let filterTo = "";
  let filterActive = false;

  // Canvas geometry
  const CELL = 4; // px per chunk
  const GRID = 32; // chunks per region
  let canvas: HTMLCanvasElement;
  let cssW = 0;
  let cssH = 0;

  // Box drag state
  let dragStart: { x: number; y: number } | null = null;
  let dragCurrent: { x: number; y: number } | null = null;
  let dragAdd = true; // add to selection (default) vs subtract

  function statusLabel(code: number): string {
    return code === STATUS_EMPTY ? "empty"
      : code === STATUS_PARTIAL ? "partial"
      : code === STATUS_FULL ? "full" : "unknown";
  }

  function worldChunkX(rx: number, local: number) { return rx * GRID + local; }
  function worldChunkZ(rz: number, local: number) { return rz * GRID + local; }

  async function load() {
    if (!$projectPath || !worldName) return;
    loading = true; error = null; selection = new Set(); filterActive = false;
    try {
      map = await api.worlds.map(worldName, $projectPath);
    } catch (e) {
      map = null;
      error = String(e);
    } finally {
      loading = false;
      requestAnimationFrame(draw);
    }
  }

  function dateToEpoch(d: string): number {
    if (!d) return 0;
    const t = new Date(d + "T00:00:00").getTime() / 1000;
    return isNaN(t) ? 0 : t;
  }

  function globalMinMax(): [number, number] {
    if (!map) return [0, 1];
    let min = Infinity, max = 0;
    for (const r of map.regions) {
      if (r.present > 0) { min = Math.min(min, r.minModified); max = Math.max(max, r.maxModified); }
    }
    if (!isFinite(min)) return [0, 1];
    return [min, max];
  }

  function chunkColor(cell: ChunkCell, mode: ColorMode, minMod: number, maxMod: number): string {
    if (!cell.present) return "#15171c";
    if (mode === "date") {
      const span = Math.max(1, maxMod - minMod);
      const t = Math.max(0, Math.min(1, (cell.lastModified - minMod) / span));
      // blue (old) -> green -> yellow (new)
      const r = Math.round(40 + t * 200);
      const g = Math.round(90 + t * 120);
      const b = Math.round(200 - t * 170);
      return `rgb(${r},${g},${b})`;
    }
    switch (cell.status) {
      case STATUS_EMPTY: return "#3b4252";
      case STATUS_PARTIAL: return "#b08968";
      case STATUS_FULL: {
        const span = Math.max(1, maxMod - minMod);
        const t = Math.max(0, Math.min(1, (cell.lastModified - minMod) / span));
        const r = Math.round(27 + t * 12);
        const g = Math.round(120 + t * 60);
        const b = Math.round(70 + t * 60);
        return `rgb(${r},${g},${b})`;
      }
      default: return "#4a8c5a";
    }
  }

  function draw() {
    if (!canvas || !map) return;
    const regionW = (map.maxRegionX - map.minRegionX + 1);
    const regionH = (map.maxRegionZ - map.minRegionZ + 1);
    const W = regionW * GRID * CELL;
    const H = regionH * GRID * CELL;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(W * dpr));
    canvas.height = Math.max(1, Math.round(H * dpr));
    canvas.style.width = W + "px";
    canvas.style.height = H + "px";
    cssW = W; cssH = H;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = "#0e0f13";
    ctx.fillRect(0, 0, W, H);

    const [minMod, maxMod] = globalMinMax();

    for (const r of map.regions) {
      const ox = (r.regionX - map.minRegionX) * GRID * CELL;
      const oy = (r.regionZ - map.minRegionZ) * GRID * CELL;
      for (let i = 0; i < r.chunks.length; i++) {
        const cell = r.chunks[i];
        const lx = i % GRID;
        const lz = Math.floor(i / GRID);
        const key = `${r.regionX}:${r.regionZ}:${i}`;
        ctx.fillStyle = chunkColor(cell, colorMode, minMod, maxMod);
        ctx.fillRect(ox + lx * CELL, oy + lz * CELL, CELL - 0.5, CELL - 0.5);
        if (selection.has(key)) {
          ctx.fillStyle = "rgba(255, 90, 95, 0.5)";
          ctx.fillRect(ox + lx * CELL, oy + lz * CELL, CELL, CELL);
          ctx.strokeStyle = "rgba(255, 90, 95, 0.95)";
          ctx.lineWidth = 1;
          ctx.strokeRect(ox + lx * CELL + 0.5, oy + lz * CELL + 0.5, CELL - 1, CELL - 1);
        }
      }
      if (showRegions) {
        ctx.strokeStyle = "rgba(120, 200, 255, 0.35)";
        ctx.lineWidth = 1;
        ctx.strokeRect(ox + 0.5, oy + 0.5, GRID * CELL - 1, GRID * CELL - 1);
      }
    }

    // Draw active drag rectangle
    if (boxSelecting && dragStart && dragCurrent) {
      const x = Math.min(dragStart.x, dragCurrent.x);
      const y = Math.min(dragStart.y, dragCurrent.y);
      const w = Math.abs(dragCurrent.x - dragStart.x);
      const h = Math.abs(dragCurrent.y - dragStart.y);
      ctx.strokeStyle = dragAdd ? "rgba(120, 200, 255, 0.9)" : "rgba(255, 90, 95, 0.9)";
      ctx.fillStyle = dragAdd ? "rgba(120, 200, 255, 0.15)" : "rgba(255, 90, 95, 0.15)";
      ctx.lineWidth = 1;
      ctx.fillRect(x, y, w, h);
      ctx.strokeRect(x + 0.5, y + 0.5, w, h);
    }
  }

  function cellAt(evt: MouseEvent): { rx: number; rz: number; lx: number; lz: number; idx: number; cell: ChunkCell } | null {
    if (!map || !canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const x = evt.clientX - rect.left;
    const y = evt.clientY - rect.top;
    const localX = Math.floor(x / CELL);
    const localZ = Math.floor(y / CELL);
    const rx = map.minRegionX + Math.floor(localX / GRID);
    const rz = map.minRegionZ + Math.floor(localZ / GRID);
    const r = map.regions.find(rr => rr.regionX === rx && rr.regionZ === rz);
    if (!r) return null;
    const lx = localX - (rx - map.minRegionX) * GRID;
    const lz = localZ - (rz - map.minRegionZ) * GRID;
    if (lx < 0 || lx >= GRID || lz < 0 || lz >= GRID) return null;
    const idx = lz * GRID + lx;
    return { rx, rz, lx, lz, idx, cell: r.chunks[idx] };
  }

  function onMove(evt: MouseEvent) {
    const hit = cellAt(evt);
    if (!hit) { hover = null; }
    else {
      hover = {
        rx: hit.rx, rz: hit.rz,
        cx: worldChunkX(hit.rx, hit.lx), cz: worldChunkZ(hit.rz, hit.lz),
        status: statusLabel(hit.cell.status),
        modified: hit.cell.lastModified,
      };
    }
    if (boxSelecting && dragStart) {
      const rect = canvas.getBoundingClientRect();
      dragCurrent = { x: evt.clientX - rect.left, y: evt.clientY - rect.top };
      draw();
    }
  }

  function onClick(evt: MouseEvent) {
    if (boxSelecting || !selecting) return;
    const hit = cellAt(evt);
    if (!hit) return;
    const key = `${hit.rx}:${hit.rz}:${hit.idx}`;
    const next = new Set(selection);
    if (next.has(key)) next.delete(key); else next.add(key);
    selection = next;
    draw();
  }

  function onDown(evt: MouseEvent) {
    if (!boxSelecting) return;
    evt.preventDefault();
    const rect = canvas.getBoundingClientRect();
    dragStart = { x: evt.clientX - rect.left, y: evt.clientY - rect.top };
    dragCurrent = { ...dragStart };
    dragAdd = !(evt.shiftKey || evt.ctrlKey || evt.metaKey); // shift/ctrl = subtract
  }

  function onUp(evt: MouseEvent) {
    if (!boxSelecting || !dragStart || !dragCurrent) { dragStart = null; dragCurrent = null; return; }
    const rect = canvas.getBoundingClientRect();
    const x1 = Math.min(dragStart.x, dragCurrent.x);
    const y1 = Math.min(dragStart.y, dragCurrent.y);
    const xMax = Math.max(dragStart.x, dragCurrent.x);
    const yMax = Math.max(dragStart.y, dragCurrent.y);
    const lx1 = Math.max(0, Math.floor(x1 / CELL));
    const lz1 = Math.max(0, Math.floor(y1 / CELL));
    const lx2 = Math.floor(xMax / CELL);
    const lz2 = Math.floor(yMax / CELL);

    if (!map) { dragStart = null; dragCurrent = null; return; }
    const next = new Set(selection);
    for (const r of map.regions) {
      const baseX = (r.regionX - map.minRegionX) * GRID;
      const baseZ = (r.regionZ - map.minRegionZ) * GRID;
      for (let lz = lz1; lz <= lz2; lz++) {
        for (let lx = lx1; lx <= lx2; lx++) {
          const rx = map.minRegionX + Math.floor(lx / GRID);
          const rz = map.minRegionZ + Math.floor(lz / GRID);
          if (rx !== r.regionX || rz !== r.regionZ) continue;
          const localX = lx - baseX;
          const localZ = lz - baseZ;
          if (localX < 0 || localX >= GRID || localZ < 0 || localZ >= GRID) continue;
          const idx = localZ * GRID + localX;
          const key = `${rx}:${rz}:${idx}`;
          if (dragAdd) next.add(key); else next.delete(key);
        }
      }
    }
    selection = next;
    dragStart = null; dragCurrent = null;
    draw();
  }

  function selectByDate() {
    if (!map) return;
    const from = dateToEpoch(filterFrom);
    const to = filterTo ? dateToEpoch(filterTo) + 86399 : Infinity;
    const next = new Set<string>();
    for (const r of map.regions) {
      r.chunks.forEach((cell, i) => {
        if (!cell.present) return;
        const m = cell.lastModified;
        if (m >= from && m <= to) next.add(`${r.regionX}:${r.regionZ}:${i}`);
      });
    }
    selection = next;
    filterActive = next.size > 0;
    draw();
  }

  function selectAll() {
    if (!map) return;
    const next = new Set<string>();
    for (const r of map.regions) {
      r.chunks.forEach((cell, i) => { if (cell.present) next.add(`${r.regionX}:${r.regionZ}:${i}`); });
    }
    selection = next;
    draw();
  }

  function clearSelection() {
    selection = new Set();
    filterActive = false;
    draw();
  }

  function invertSelection() {
    if (!map) return;
    const next = new Set<string>();
    for (const r of map.regions) {
      r.chunks.forEach((cell, i) => {
        const key = `${r.regionX}:${r.regionZ}:${i}`;
        if (cell.present !== 0 && !selection.has(key)) next.add(key);
      });
    }
    selection = next;
    draw();
  }

  function exportPng() {
    if (!canvas) return;
    const url = canvas.toDataURL("image/png");
    const a = document.createElement("a");
    a.href = url;
    a.download = `worldmap-${worldName || "world"}.png`;
    document.body.appendChild(a);
    a.click();
    a.remove();
  }

  async function deleteSelected() {
    if (!map || selection.size === 0 || !$projectPath || !worldName) return;
    const byRegion = new Map<string, { regionX: number; regionZ: number; indices: number[] }>();
    for (const key of selection) {
      const [rx, rz, idx] = key.split(":").map(Number);
      const k = `${rx}:${rz}`;
      if (!byRegion.has(k)) byRegion.set(k, { regionX: rx, regionZ: rz, indices: [] });
      byRegion.get(k)!.indices.push(idx);
    }
    const payload = Array.from(byRegion.values());
    error = null;
    try {
      const cleared: number = await api.worlds.deleteChunks(worldName, payload, $projectPath);
      selection = new Set();
      filterActive = false;
      await load();
      flash(`Deleted ${cleared} chunks`);
    } catch (e) {
      error = String(e);
    }
  }

  let flashMsg: string | null = null;
  let flashTimer: any;
  function flash(msg: string) {
    flashMsg = msg;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flashMsg = null), 2500);
  }

  $: if (worldName && $projectPath) load();
  $: if (map) requestAnimationFrame(draw);
</script>

<div class="world-map">
  <div class="toolbar">
    <div class="title"><MapIcon size={18} /> 2D map · {worldName}</div>
    <div class="tools">
      <label class="toggle" title="Overlay region boundaries">
        <Layers size={14} /> Regions
        <input type="checkbox" bind:checked={showRegions} on:change={draw} />
      </label>
      <select class="ghost select" bind:value={colorMode} on:change={draw} title="Color mode">
        <option value="status">by status</option>
        <option value="date">by date</option>
      </select>
      <button class="ghost" class:active={selecting} on:click={() => { selecting = !selecting; boxSelecting = false; }} title="Click chunks to toggle">
        <MousePointer2 size={14} /> Click
      </button>
      <button class="ghost" class:active={boxSelecting} on:click={() => { boxSelecting = !boxSelecting; selecting = false; }} title="Drag a rectangle (shift = subtract)">
        <Square size={14} /> Box
      </button>
      <button class="ghost" on:click={exportPng} title="Export map as PNG"><Download size={14} /> PNG</button>
      <button class="ghost danger" on:click={deleteSelected} disabled={selection.size === 0} title="Delete selected chunks (mcaselector-style)">
        <Trash2 size={14} /> Delete {selection.size || ""}
      </button>
      <button class="ghost" on:click={load} disabled={loading} title="Reload">
        <RefreshCw size={14} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  <div class="stats">
    {#if map}
      <span>{map.regionCount} regions</span>
      <span>{map.totalPresent.toLocaleString()} chunks</span>
      <span>RX {map.minRegionX}…{map.maxRegionX}</span>
      <span>RZ {map.minRegionZ}…{map.maxRegionZ}</span>
      <span class="sel">selected: {selection.size}</span>
      {#if flashMsg}<span class="ok">{flashMsg}</span>{/if}
    {:else if error}
      <span class="err">{error}</span>
    {:else if loading}
      <span>loading…</span>
    {:else}
      <span>no world map</span>
    {/if}
  </div>

  <div class="filter-bar">
    <CalendarRange size={14} />
    <span>from</span>
    <input type="date" bind:value={filterFrom} />
    <span>to</span>
    <input type="date" bind:value={filterTo} />
    <button class="mini" on:click={selectByDate} disabled={!map || (!filterFrom && !filterTo)}>Select by date</button>
    <span class="sep" />
    <button class="mini" on:click={selectAll} disabled={!map}><CheckSquare size={12} /> All</button>
    <button class="mini" on:click={invertSelection} disabled={!map}><CheckSquare size={12} /> Invert</button>
    <button class="mini" on:click={clearSelection} disabled={selection.size === 0}><XSquare size={12} /> Clear</button>
    {#if filterActive}<span class="filttag">date filter active</span>{/if}
  </div>

  <div class="map-scroll">
    {#if map}
      <div class="map-wrap">
        <canvas
          bind:this={canvas}
          on:mousemove={onMove}
          on:mouseleave={() => (hover = null)}
          on:click={onClick}
          on:mousedown={onDown}
          on:mouseup={onUp}
        ></canvas>
        {#if hover}
          <div class="hover-tip">
            chunk <code>{hover.cx}, {hover.cz}</code> · region {hover.rx},{hover.rz}<br />
            {hover.status}{#if hover.modified} · {new Date(hover.modified * 1000).toLocaleDateString()}{/if}
          </div>
        {/if}
      </div>
    {:else if error}
      <div class="empty">No map yet — generate the world by running the pack, then refresh.</div>
    {:else if !loading}
      <div class="empty">Open a world to view its 2D map.</div>
    {/if}
  </div>

  <div class="legend">
    <span><i style="background:#15171c"></i> absent</span>
    <span><i style="background:#3b4252"></i> empty</span>
    <span><i style="background:#b08968"></i> partial</span>
    <span><i style="background:#2d8c8c"></i> {colorMode === "date" ? "old→new" : "full (old→new)"}</span>
    <span><i style="background:rgba(255,90,95,0.7)"></i> selected</span>
  </div>
</div>

<style>
  .world-map { display: flex; flex-direction: column; gap: 10px; min-height: 0; }
  .toolbar { display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; }
  .title { display: flex; align-items: center; gap: 8px; font-weight: 700; color: var(--text-primary); }
  .tools { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .toggle { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; color: var(--text-muted); cursor: pointer; }
  .ghost { display: inline-flex; align-items: center; gap: 5px; }
  .ghost.active { background: rgba(120,200,255,0.15); border-color: rgba(120,200,255,0.4); color: #8fd3ff; }
  .ghost.danger:not(:disabled):hover { background: rgba(255,90,95,0.15); border-color: rgba(255,90,95,0.4); color: #ff7a7e; }
  .select { padding: 5px 8px; background: var(--bg-elevated); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); font-size: 12px; }
  .stats { display: flex; gap: 14px; flex-wrap: wrap; font-size: 11px; color: var(--text-muted); }
  .stats .sel { color: #ff7a7e; }
  .stats .ok { color: var(--accent-primary); }
  .stats .err { color: #fca5a5; }
  .filter-bar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 12px; color: var(--text-muted); }
  .filter-bar input[type="date"] { background: var(--bg-elevated); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 4px 6px; font-size: 12px; color-scheme: dark; }
  .filter-bar .sep { width: 1px; height: 18px; background: var(--border-color); margin: 0 4px; }
  .mini { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; padding: 4px 8px; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: transparent; color: var(--text-secondary); cursor: pointer; }
  .mini:hover:not(:disabled) { background: var(--bg-tertiary); }
  .mini:disabled { opacity: .4; cursor: default; }
  .filttag { color: #8fd3ff; font-size: 11px; }
  .map-scroll { overflow: auto; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: #0e0f13; max-height: 60vh; }
  .map-wrap { position: relative; display: inline-block; }
  canvas { display: block; image-rendering: pixelated; cursor: crosshair; }
  .hover-tip { position: fixed; pointer-events: none; z-index: 30; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: 6px; padding: 6px 8px; font-size: 11px; color: var(--text-secondary); box-shadow: 0 4px 16px rgba(0,0,0,.4); }
  .hover-tip code { color: var(--accent-primary); }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; font-size: 11px; color: var(--text-muted); }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .legend i { width: 11px; height: 11px; border-radius: 2px; display: inline-block; border: 1px solid rgba(255,255,255,.1); }
  .empty { padding: 28px; text-align: center; color: var(--text-muted); }
  .spin { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
