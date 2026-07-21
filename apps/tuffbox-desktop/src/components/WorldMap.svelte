<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    Map as MapIcon, RefreshCw, Trash2, MousePointer2, Square, Layers, Download,
    CalendarRange, CheckSquare, XSquare, Copy, Scissors, Clipboard, Circle,
    ZoomIn, ZoomOut, Minimize2, Eraser, ArrowLeftRight, Filter, FolderOutput, FolderInput,
    FileDown, FileUp, Wrench, Pencil,
  } from "lucide-svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { projectPath } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import ChunkNbtEditor from "./ChunkNbtEditor.svelte";
  import { api } from "../lib/api";
  import type {
    WorldMap as WorldMapData,
    ChunkCell,
    ChunkClipboard,
    NbtChangeRequest,
    AdvancedChunkFilter,
  } from "../lib/api";

  const STATUS_EMPTY = 1;
  const STATUS_PARTIAL = 2;
  const STATUS_FULL = 3;

  type ColorMode = "status" | "date" | "inhabited" | "biome" | "height";
  type Tool = "pan" | "click" | "box" | "radius" | "region";

  export let worldName: string = "";

  let map: WorldMapData | null = null;
  let loading = false;
  let error: string | null = null;

  let dimensions: string[] = ["overworld"];
  let dimension = "overworld";

  let showRegions = true;
  let colorMode: ColorMode = "status";
  let tool: Tool = "box";
  let selection = new Set<string>();
  let statusFilter: "all" | "empty" | "partial" | "full" = "all";

  let hover: {
    rx: number; rz: number; cx: number; cz: number;
    status: string; modified: number;
    inhabitedTime: number; dataVersion: number;
    biomeId?: number; surfaceY?: number;
    entityCount?: number; structureCount?: number;
  } | null = null;
  let tipX = 0;
  let tipY = 0;

  let clipboard: ChunkClipboard | null = null;
  let pasteOffsetX = 0;
  let pasteOffsetZ = 0;

  let filterFrom = "";
  let filterTo = "";
  let filterActive = false;
  let radiusChunks = 8;

  let inhabitedMin = "";
  let inhabitedMax = "";
  let dataVersionMin = "";
  let dataVersionMax = "";
  let xposMin = "";
  let xposMax = "";
  let zposMin = "";
  let zposMax = "";
  let borderEmpty = "";
  let entityCountMin = "";
  let structureCountMin = "";
  let filtEntityNames = "";
  let filtStructureNames = "";
  let filtPaletteNames = "";
  let filterQuery = "";
  let importOverwrite = true;
  let importIntoSelection = false;
  let importYOffset = 0;
  let importSections = "";

  let chgInhabited = "";
  let chgStatus = "";
  let chgDataVersion = "";
  let chgLightPopulated = "";
  let chgBiome = "";
  let chgDeleteSections = "";
  let chgReplaceBlocks = "";
  let chgDeleteStructureRefs = "";
  let chgPreventRetrogen = false;
  let chgForceBlend = false;
  let chgDeleteEntities = false;
  let chgFixStatus = false;
  let chgForce = false;
  let nbtPanelOpen = true;

  let editorOpen = false;
  let editorRx = 0;
  let editorRz = 0;
  let editorIdx = 0;

  let csvInput: HTMLInputElement;

  const CELL = 4;
  const GRID = 32;
  let canvas: HTMLCanvasElement;
  let viewport: HTMLDivElement;

  // View transform (screen = world * zoom + pan)
  let zoom = 1;
  let panX = 0;
  let panY = 0;
  let panning = false;
  let panLast: { x: number; y: number } | null = null;

  let dragStart: { x: number; y: number } | null = null;
  let dragCurrent: { x: number; y: number } | null = null;
  let dragAdd = true;

  let flashMsg: string | null = null;
  let flashTimer: ReturnType<typeof setTimeout>;

  function statusLabel(code: number): string {
    return code === STATUS_EMPTY ? "empty"
      : code === STATUS_PARTIAL ? "partial"
      : code === STATUS_FULL ? "full" : "unknown";
  }

  function worldChunkX(rx: number, local: number) { return rx * GRID + local; }
  function worldChunkZ(rz: number, local: number) { return rz * GRID + local; }

  function dimLabel(d: string): string {
    if (d === "nether") return "Nether";
    if (d === "end") return "The End";
    return "Overworld";
  }

  function flash(msg: string) {
    flashMsg = msg;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flashMsg = null), 2500);
  }

  function parseOptNum(s: string): number | null {
    if (s === "" || s == null) return null;
    const n = Number(s);
    return Number.isFinite(n) ? n : null;
  }

  function cycleColorMode() {
    colorMode = colorMode === "status" ? "date"
      : colorMode === "date" ? "inhabited"
      : colorMode === "inhabited" ? "biome"
      : colorMode === "biome" ? "height"
      : "status";
    draw();
  }

  async function loadDimensions() {
    if (!$projectPath || !worldName) return;
    try {
      dimensions = await api.worlds.dimensions(worldName, $projectPath);
      if (!dimensions.includes(dimension)) {
        dimension = dimensions[0] || "overworld";
      }
    } catch {
      dimensions = ["overworld"];
    }
  }

  async function load() {
    if (!$projectPath || !worldName) return;
    loading = true;
    error = null;
    selection = new Set();
    filterActive = false;
    try {
      await loadDimensions();
      map = await api.worlds.map(worldName, dimension, $projectPath);
      fitView();
    } catch (e) {
      map = null;
      error = String(e);
    } finally {
      loading = false;
      requestAnimationFrame(draw);
    }
  }

  function fitView() {
    if (!map || !viewport) return;
    const regionW = map.maxRegionX - map.minRegionX + 1;
    const regionH = map.maxRegionZ - map.minRegionZ + 1;
    const W = regionW * GRID * CELL;
    const H = regionH * GRID * CELL;
    const vw = viewport.clientWidth || 800;
    const vh = viewport.clientHeight || 400;
    zoom = Math.max(0.25, Math.min(4, Math.min(vw / Math.max(W, 1), vh / Math.max(H, 1)) * 0.92));
    panX = (vw - W * zoom) / 2;
    panY = (vh - H * zoom) / 2;
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
      if (r.present > 0) {
        min = Math.min(min, r.minModified);
        max = Math.max(max, r.maxModified);
      }
    }
    if (!isFinite(min)) return [0, 1];
    return [min, max];
  }

  function globalInhabitedMinMax(): [number, number] {
    if (!map) return [0, 1];
    let min = Infinity, max = 0;
    for (const r of map.regions) {
      for (const cell of r.chunks) {
        if (!cell.present) continue;
        const v = cell.inhabitedTime ?? 0;
        min = Math.min(min, v);
        max = Math.max(max, v);
      }
    }
    if (!isFinite(min)) return [0, 1];
    return [min, max];
  }

  function globalSurfaceMinMax(): [number, number] {
    if (!map) return [0, 1];
    let min = Infinity, max = -Infinity;
    for (const r of map.regions) {
      for (const cell of r.chunks) {
        if (!cell.present) continue;
        const y = cell.surfaceY;
        if (y == null || y === -9999) continue;
        min = Math.min(min, y);
        max = Math.max(max, y);
      }
    }
    if (!isFinite(min) || !isFinite(max)) return [0, 1];
    return [min, max];
  }

  function heatColor(t: number): string {
    const r = Math.round(40 + t * 200);
    const g = Math.round(90 + t * 120);
    const b = Math.round(200 - t * 170);
    return `rgb(${r},${g},${b})`;
  }

  function biomeHue(id: number): number {
    let h = Math.imul(id ^ 0x9e3779b9, 0x85ebca6b) >>> 0;
    return h % 360;
  }

  function chunkColor(
    cell: ChunkCell,
    mode: ColorMode,
    minMod: number,
    maxMod: number,
    minInh: number,
    maxInh: number,
    minSurf: number,
    maxSurf: number,
  ): string {
    if (!cell.present) return "#15171c";
    if (mode === "date") {
      const span = Math.max(1, maxMod - minMod);
      const t = Math.max(0, Math.min(1, (cell.lastModified - minMod) / span));
      return heatColor(t);
    }
    if (mode === "inhabited") {
      const span = Math.max(1, maxInh - minInh);
      const v = cell.inhabitedTime ?? 0;
      const t = Math.max(0, Math.min(1, (v - minInh) / span));
      return heatColor(t);
    }
    if (mode === "biome") {
      const id = cell.biomeId ?? -1;
      if (id < 0) return "#1a1c22";
      return `hsl(${biomeHue(id)}, 55%, 42%)`;
    }
    if (mode === "height") {
      const y = cell.surfaceY ?? -9999;
      if (y === -9999) return "#15171c";
      const span = Math.max(1, maxSurf - minSurf);
      const t = Math.max(0, Math.min(1, (y - minSurf) / span));
      return heatColor(t);
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

  function mapSize(): { W: number; H: number } {
    if (!map) return { W: 0, H: 0 };
    const regionW = map.maxRegionX - map.minRegionX + 1;
    const regionH = map.maxRegionZ - map.minRegionZ + 1;
    return { W: regionW * GRID * CELL, H: regionH * GRID * CELL };
  }

  function draw() {
    if (!canvas || !map) return;
    const { W, H } = mapSize();
    const dpr = window.devicePixelRatio || 1;
    const viewW = Math.max(1, Math.round((viewport?.clientWidth || W) * dpr));
    const viewH = Math.max(1, Math.round((viewport?.clientHeight || H) * dpr));
    canvas.width = viewW;
    canvas.height = viewH;
    canvas.style.width = (viewport?.clientWidth || W) + "px";
    canvas.style.height = (viewport?.clientHeight || H) + "px";

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const cssW = viewport?.clientWidth || W;
    const cssH = viewport?.clientHeight || H;
    ctx.clearRect(0, 0, cssW, cssH);
    ctx.fillStyle = "#0e0f13";
    ctx.fillRect(0, 0, cssW, cssH);

    ctx.save();
    ctx.translate(panX, panY);
    ctx.scale(zoom, zoom);

    const [minMod, maxMod] = globalMinMax();
    const [minInh, maxInh] = globalInhabitedMinMax();
    const [minSurf, maxSurf] = globalSurfaceMinMax();

    for (const r of map.regions) {
      const ox = (r.regionX - map.minRegionX) * GRID * CELL;
      const oy = (r.regionZ - map.minRegionZ) * GRID * CELL;
      for (let i = 0; i < r.chunks.length; i++) {
        const cell = r.chunks[i];
        const lx = i % GRID;
        const lz = Math.floor(i / GRID);
        const key = `${r.regionX}:${r.regionZ}:${i}`;
        ctx.fillStyle = chunkColor(cell, colorMode, minMod, maxMod, minInh, maxInh, minSurf, maxSurf);
        ctx.fillRect(ox + lx * CELL, oy + lz * CELL, CELL - 0.5, CELL - 0.5);
        if (selection.has(key)) {
          ctx.fillStyle = "rgba(255, 90, 95, 0.45)";
          ctx.fillRect(ox + lx * CELL, oy + lz * CELL, CELL, CELL);
        }
      }
      if (showRegions) {
        ctx.strokeStyle = "rgba(120, 200, 255, 0.35)";
        ctx.lineWidth = 1 / zoom;
        ctx.strokeRect(ox + 0.5, oy + 0.5, GRID * CELL - 1, GRID * CELL - 1);
      }
    }

    if (tool === "box" && dragStart && dragCurrent) {
      const x = Math.min(dragStart.x, dragCurrent.x);
      const y = Math.min(dragStart.y, dragCurrent.y);
      const w = Math.abs(dragCurrent.x - dragStart.x);
      const h = Math.abs(dragCurrent.y - dragStart.y);
      ctx.strokeStyle = dragAdd ? "rgba(120, 200, 255, 0.9)" : "rgba(255, 90, 95, 0.9)";
      ctx.fillStyle = dragAdd ? "rgba(120, 200, 255, 0.15)" : "rgba(255, 90, 95, 0.15)";
      ctx.lineWidth = 1 / zoom;
      ctx.fillRect(x, y, w, h);
      ctx.strokeRect(x + 0.5, y + 0.5, w, h);
    }

    if (tool === "radius" && dragStart && dragCurrent) {
      const dx = dragCurrent.x - dragStart.x;
      const dy = dragCurrent.y - dragStart.y;
      const r = Math.sqrt(dx * dx + dy * dy);
      ctx.beginPath();
      ctx.arc(dragStart.x, dragStart.y, r, 0, Math.PI * 2);
      ctx.fillStyle = dragAdd ? "rgba(120, 200, 255, 0.12)" : "rgba(255, 90, 95, 0.12)";
      ctx.fill();
      ctx.strokeStyle = dragAdd ? "rgba(120, 200, 255, 0.9)" : "rgba(255, 90, 95, 0.9)";
      ctx.lineWidth = 1 / zoom;
      ctx.stroke();
    }

    ctx.restore();
  }

  function screenToWorld(clientX: number, clientY: number): { x: number; y: number } | null {
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    return {
      x: (clientX - rect.left - panX) / zoom,
      y: (clientY - rect.top - panY) / zoom,
    };
  }

  function cellAtWorld(wx: number, wy: number): { rx: number; rz: number; lx: number; lz: number; idx: number; cell: ChunkCell } | null {
    if (!map) return null;
    const localX = Math.floor(wx / CELL);
    const localZ = Math.floor(wy / CELL);
    const rx = map.minRegionX + Math.floor(localX / GRID);
    const rz = map.minRegionZ + Math.floor(localZ / GRID);
    const r = map.regions.find((rr) => rr.regionX === rx && rr.regionZ === rz);
    if (!r) return null;
    const lx = localX - (rx - map.minRegionX) * GRID;
    const lz = localZ - (rz - map.minRegionZ) * GRID;
    if (lx < 0 || lx >= GRID || lz < 0 || lz >= GRID) return null;
    const idx = lz * GRID + lx;
    return { rx, rz, lx, lz, idx, cell: r.chunks[idx] };
  }

  function cellAtChunk(cx: number, cz: number): ChunkCell | null {
    if (!map) return null;
    const rx = Math.floor(cx / GRID);
    const rz = Math.floor(cz / GRID);
    const lx = cx - rx * GRID;
    const lz = cz - rz * GRID;
    const r = map.regions.find((rr) => rr.regionX === rx && rr.regionZ === rz);
    if (!r) return null;
    return r.chunks[lz * GRID + lx] || null;
  }

  function isEmptyNeighbor(cell: ChunkCell | null): boolean {
    return !cell || !cell.present || cell.status === STATUS_EMPTY;
  }

  function emptyNeighborCount(cx: number, cz: number): number {
    let n = 0;
    if (isEmptyNeighbor(cellAtChunk(cx + 1, cz))) n++;
    if (isEmptyNeighbor(cellAtChunk(cx - 1, cz))) n++;
    if (isEmptyNeighbor(cellAtChunk(cx, cz + 1))) n++;
    if (isEmptyNeighbor(cellAtChunk(cx, cz - 1))) n++;
    return n;
  }

  function cellAt(evt: MouseEvent) {
    const w = screenToWorld(evt.clientX, evt.clientY);
    if (!w) return null;
    return cellAtWorld(w.x, w.y);
  }

  function onMove(evt: MouseEvent) {
    tipX = evt.clientX + 12;
    tipY = evt.clientY + 12;
    const hit = cellAt(evt);
    if (!hit) hover = null;
    else {
      hover = {
        rx: hit.rx, rz: hit.rz,
        cx: worldChunkX(hit.rx, hit.lx), cz: worldChunkZ(hit.rz, hit.lz),
        status: statusLabel(hit.cell.status),
        modified: hit.cell.lastModified,
        inhabitedTime: hit.cell.inhabitedTime ?? 0,
        dataVersion: hit.cell.dataVersion ?? 0,
        biomeId: hit.cell.biomeId,
        surfaceY: hit.cell.surfaceY,
        entityCount: hit.cell.entityCount,
        structureCount: hit.cell.structureCount,
      };
    }

    if (panning && panLast) {
      panX += evt.clientX - panLast.x;
      panY += evt.clientY - panLast.y;
      panLast = { x: evt.clientX, y: evt.clientY };
      draw();
      return;
    }

    if ((tool === "box" || tool === "radius") && dragStart) {
      const w = screenToWorld(evt.clientX, evt.clientY);
      if (w) {
        dragCurrent = w;
        draw();
      }
    }
  }

  function selectRegion(rx: number, rz: number, add: boolean) {
    if (!map) return;
    const r = map.regions.find((rr) => rr.regionX === rx && rr.regionZ === rz);
    if (!r) return;
    const next = new Set(selection);
    r.chunks.forEach((cell, i) => {
      if (!cell.present) return;
      const key = `${rx}:${rz}:${i}`;
      if (add) next.add(key); else next.delete(key);
    });
    selection = next;
    draw();
  }

  function onClick(evt: MouseEvent) {
    if (tool === "region") {
      const hit = cellAt(evt);
      if (!hit) return;
      selectRegion(hit.rx, hit.rz, !(evt.shiftKey || evt.ctrlKey || evt.metaKey));
      return;
    }
    if (tool !== "click") return;
    const hit = cellAt(evt);
    if (!hit) return;
    const key = `${hit.rx}:${hit.rz}:${hit.idx}`;
    const next = new Set(selection);
    if (next.has(key)) next.delete(key); else next.add(key);
    selection = next;
    draw();
  }

  function onDown(evt: MouseEvent) {
    if (evt.button === 1 || tool === "pan" || (evt.button === 0 && evt.altKey)) {
      evt.preventDefault();
      panning = true;
      panLast = { x: evt.clientX, y: evt.clientY };
      return;
    }
    if (tool !== "box" && tool !== "radius") return;
    if (evt.button !== 0) return;
    evt.preventDefault();
    const w = screenToWorld(evt.clientX, evt.clientY);
    if (!w) return;
    dragStart = w;
    dragCurrent = { ...w };
    dragAdd = !(evt.shiftKey || evt.ctrlKey || evt.metaKey);
  }

  function onUp(_evt: MouseEvent) {
    if (panning) {
      panning = false;
      panLast = null;
      return;
    }
    if (!dragStart || !dragCurrent || !map) {
      dragStart = null;
      dragCurrent = null;
      return;
    }

    const next = new Set(selection);

    if (tool === "box") {
      const x1 = Math.min(dragStart.x, dragCurrent.x);
      const y1 = Math.min(dragStart.y, dragCurrent.y);
      const xMax = Math.max(dragStart.x, dragCurrent.x);
      const yMax = Math.max(dragStart.y, dragCurrent.y);
      const lx1 = Math.floor(x1 / CELL);
      const lz1 = Math.floor(y1 / CELL);
      const lx2 = Math.floor(xMax / CELL);
      const lz2 = Math.floor(yMax / CELL);
      for (let lz = lz1; lz <= lz2; lz++) {
        for (let lx = lx1; lx <= lx2; lx++) {
          const wx = lx * CELL + CELL / 2;
          const wy = lz * CELL + CELL / 2;
          const hit = cellAtWorld(wx, wy);
          if (!hit || !hit.cell.present) continue;
          if (!matchesStatus(hit.cell)) continue;
          const key = `${hit.rx}:${hit.rz}:${hit.idx}`;
          if (dragAdd) next.add(key); else next.delete(key);
        }
      }
    } else if (tool === "radius") {
      const dx = dragCurrent.x - dragStart.x;
      const dy = dragCurrent.y - dragStart.y;
      let rPx = Math.sqrt(dx * dx + dy * dy);
      if (rPx < CELL) rPx = radiusChunks * CELL;
      const rCells = rPx / CELL;
      const cx = dragStart.x / CELL;
      const cz = dragStart.y / CELL;
      const minLx = Math.floor(cx - rCells);
      const maxLx = Math.ceil(cx + rCells);
      const minLz = Math.floor(cz - rCells);
      const maxLz = Math.ceil(cz + rCells);
      for (let lz = minLz; lz <= maxLz; lz++) {
        for (let lx = minLx; lx <= maxLx; lx++) {
          const ddx = lx + 0.5 - cx;
          const ddz = lz + 0.5 - cz;
          if (ddx * ddx + ddz * ddz > rCells * rCells) continue;
          const hit = cellAtWorld(lx * CELL + CELL / 2, lz * CELL + CELL / 2);
          if (!hit || !hit.cell.present) continue;
          if (!matchesStatus(hit.cell)) continue;
          const key = `${hit.rx}:${hit.rz}:${hit.idx}`;
          if (dragAdd) next.add(key); else next.delete(key);
        }
      }
    }

    selection = next;
    dragStart = null;
    dragCurrent = null;
    draw();
  }

  function matchesStatus(cell: ChunkCell): boolean {
    if (statusFilter === "all") return true;
    if (statusFilter === "empty") return cell.status === STATUS_EMPTY;
    if (statusFilter === "partial") return cell.status === STATUS_PARTIAL;
    if (statusFilter === "full") return cell.status === STATUS_FULL;
    return true;
  }

  function matchesChunkFilter(cell: ChunkCell, rx: number, rz: number, idx: number): boolean {
    if (!cell.present) return false;
    if (!matchesStatus(cell)) return false;

    const from = dateToEpoch(filterFrom);
    const to = filterTo ? dateToEpoch(filterTo) + 86399 : Infinity;
    if (filterFrom || filterTo) {
      const m = cell.lastModified;
      if (m < from || m > to) return false;
    }

    const inh = cell.inhabitedTime ?? 0;
    const inhMin = parseOptNum(inhabitedMin);
    const inhMax = parseOptNum(inhabitedMax);
    if (inhMin != null && inh < inhMin) return false;
    if (inhMax != null && inh > inhMax) return false;

    const dv = cell.dataVersion ?? 0;
    const dvMin = parseOptNum(dataVersionMin);
    const dvMax = parseOptNum(dataVersionMax);
    if (dvMin != null && dv < dvMin) return false;
    if (dvMax != null && dv > dvMax) return false;

    const lx = idx % GRID;
    const lz = Math.floor(idx / GRID);
    const cx = worldChunkX(rx, lx);
    const cz = worldChunkZ(rz, lz);

    const xMin = parseOptNum(xposMin);
    const xMax = parseOptNum(xposMax);
    const zMin = parseOptNum(zposMin);
    const zMax = parseOptNum(zposMax);
    if (xMin != null && cx < xMin) return false;
    if (xMax != null && cx > xMax) return false;
    if (zMin != null && cz < zMin) return false;
    if (zMax != null && cz > zMax) return false;

    const borderN = parseOptNum(borderEmpty);
    if (borderN != null && emptyNeighborCount(cx, cz) < borderN) return false;

    const entMin = parseOptNum(entityCountMin);
    if (entMin != null && (cell.entityCount ?? 0) < entMin) return false;

    const structMin = parseOptNum(structureCountMin);
    if (structMin != null && (cell.structureCount ?? 0) < structMin) return false;

    return true;
  }

  function onWheel(evt: WheelEvent) {
    evt.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = evt.clientX - rect.left;
    const my = evt.clientY - rect.top;
    const beforeX = (mx - panX) / zoom;
    const beforeY = (my - panY) / zoom;
    const factor = evt.deltaY < 0 ? 1.12 : 1 / 1.12;
    zoom = Math.max(0.15, Math.min(8, zoom * factor));
    panX = mx - beforeX * zoom;
    panY = my - beforeY * zoom;
    draw();
  }

  function zoomBy(factor: number) {
    if (!viewport) return;
    const mx = viewport.clientWidth / 2;
    const my = viewport.clientHeight / 2;
    const beforeX = (mx - panX) / zoom;
    const beforeY = (my - panY) / zoom;
    zoom = Math.max(0.15, Math.min(8, zoom * factor));
    panX = mx - beforeX * zoom;
    panY = my - beforeY * zoom;
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
        if (!matchesStatus(cell)) return;
        const m = cell.lastModified;
        if (m >= from && m <= to) next.add(`${r.regionX}:${r.regionZ}:${i}`);
      });
    }
    selection = next;
    filterActive = next.size > 0;
    draw();
  }

  function applyChunkFilter() {
    if (!map) return;
    const next = new Set<string>();
    for (const r of map.regions) {
      r.chunks.forEach((cell, i) => {
        if (matchesChunkFilter(cell, r.regionX, r.regionZ, i)) {
          next.add(`${r.regionX}:${r.regionZ}:${i}`);
        }
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
      r.chunks.forEach((cell, i) => {
        if (cell.present && matchesStatus(cell)) next.add(`${r.regionX}:${r.regionZ}:${i}`);
      });
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
        if (cell.present && matchesStatus(cell) && !selection.has(key)) next.add(key);
      });
    }
    selection = next;
    draw();
  }

  function invertSelectedRegions() {
    if (!map) return;
    const touched = new Set<string>();
    for (const key of selection) {
      const parts = key.split(":");
      touched.add(`${parts[0]}:${parts[1]}`);
    }
    if (touched.size === 0) return;
    const next = new Set(selection);
    for (const r of map.regions) {
      const rk = `${r.regionX}:${r.regionZ}`;
      if (!touched.has(rk)) continue;
      r.chunks.forEach((cell, i) => {
        if (!cell.present) return;
        const key = `${r.regionX}:${r.regionZ}:${i}`;
        if (next.has(key)) next.delete(key); else next.add(key);
      });
    }
    selection = next;
    draw();
  }

  /** Expand selection by Chebyshev radius `radiusChunks` (only present map cells). */
  function expandSelection() {
    if (!map || selection.size === 0) return;
    const r = Math.max(0, Math.trunc(Number(radiusChunks)) || 0);
    if (r <= 0) {
      flash(`Selection: ${selection.size} chunk(s)`);
      return;
    }
    const next = new Set(selection);
    for (const key of selection) {
      const parts = key.split(":");
      const rx = Number(parts[0]);
      const rz = Number(parts[1]);
      const idx = Number(parts[2]);
      const lx = idx % GRID;
      const lz = Math.floor(idx / GRID);
      const cx = worldChunkX(rx, lx);
      const cz = worldChunkZ(rz, lz);
      for (let dz = -r; dz <= r; dz++) {
        for (let dx = -r; dx <= r; dx++) {
          if (dx === 0 && dz === 0) continue;
          const ncx = cx + dx;
          const ncz = cz + dz;
          // rem_euclid-style region/local for negatives (Math.floor + remainder)
          const nrx = Math.floor(ncx / GRID);
          const nrz = Math.floor(ncz / GRID);
          const nlx = ncx - nrx * GRID;
          const nlz = ncz - nrz * GRID;
          const region = map.regions.find((rr) => rr.regionX === nrx && rr.regionZ === nrz);
          if (!region) continue;
          const nidx = nlz * GRID + nlx;
          const cell = region.chunks[nidx];
          if (!cell?.present) continue;
          next.add(`${nrx}:${nrz}:${nidx}`);
        }
      }
    }
    selection = next;
    draw();
    flash(`Expanded to ${selection.size} chunk(s)`);
  }

  function selectionPayload() {
    const byRegion = new Map<string, { regionX: number; regionZ: number; indices: number[] }>();
    for (const key of selection) {
      const parts = key.split(":");
      const rx = Number(parts[0]);
      const rz = Number(parts[1]);
      const idx = Number(parts[2]);
      const k = `${rx}:${rz}`;
      if (!byRegion.has(k)) byRegion.set(k, { regionX: rx, regionZ: rz, indices: [] });
      byRegion.get(k)!.indices.push(idx);
    }
    return Array.from(byRegion.values());
  }

  function exportSelectionCsv() {
    if (!map || selection.size === 0) return;
    const byRegion = new Map<string, { rx: number; rz: number; indices: Set<number>; present: number }>();
    for (const r of map.regions) {
      byRegion.set(`${r.regionX}:${r.regionZ}`, {
        rx: r.regionX, rz: r.regionZ, indices: new Set(), present: r.present,
      });
    }
    for (const key of selection) {
      const parts = key.split(":");
      const rk = `${parts[0]}:${parts[1]}`;
      const entry = byRegion.get(rk);
      if (entry) entry.indices.add(Number(parts[2]));
    }
    const lines: string[] = [];
    for (const entry of byRegion.values()) {
      if (entry.indices.size === 0) continue;
      if (entry.present > 0 && entry.indices.size === entry.present) {
        lines.push(`${entry.rx};${entry.rz}`);
      } else {
        for (const idx of entry.indices) {
          const lx = idx % GRID;
          const lz = Math.floor(idx / GRID);
          lines.push(`${entry.rx};${entry.rz};${lx};${lz}`);
        }
      }
    }
    const blob = new Blob([lines.join("\n") + "\n"], { type: "text/csv;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `selection-${worldName || "world"}-${dimension}.csv`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    flash(`Exported ${selection.size} chunk(s) to CSV`);
  }

  function triggerCsvImport() {
    csvInput?.click();
  }

  async function onCsvImport(evt: Event) {
    const input = evt.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file || !map) return;
    try {
      const text = await file.text();
      const lines = text.split(/\r?\n/).map((l) => l.trim()).filter((l) => l.length > 0);
      let doInvert = false;
      const next = new Set<string>();
      for (const line of lines) {
        if (line.toLowerCase() === "inverted") {
          doInvert = true;
          continue;
        }
        const parts = line.split(";").map((p) => p.trim());
        if (parts.length === 2) {
          const rx = Number(parts[0]);
          const rz = Number(parts[1]);
          if (!Number.isFinite(rx) || !Number.isFinite(rz)) continue;
          const r = map.regions.find((rr) => rr.regionX === rx && rr.regionZ === rz);
          if (!r) continue;
          r.chunks.forEach((cell, i) => {
            if (cell.present) next.add(`${rx}:${rz}:${i}`);
          });
        } else if (parts.length >= 4) {
          const rx = Number(parts[0]);
          const rz = Number(parts[1]);
          const lx = Number(parts[2]);
          const lz = Number(parts[3]);
          if (![rx, rz, lx, lz].every(Number.isFinite)) continue;
          if (lx < 0 || lx >= GRID || lz < 0 || lz >= GRID) continue;
          const idx = lz * GRID + lx;
          const r = map.regions.find((rr) => rr.regionX === rx && rr.regionZ === rz);
          if (!r || !r.chunks[idx]?.present) continue;
          next.add(`${rx}:${rz}:${idx}`);
        }
      }
      if (doInvert) {
        const inverted = new Set<string>();
        for (const r of map.regions) {
          r.chunks.forEach((cell, i) => {
            if (!cell.present) return;
            const key = `${r.regionX}:${r.regionZ}:${i}`;
            if (!next.has(key)) inverted.add(key);
          });
        }
        selection = inverted;
      } else {
        selection = next;
      }
      filterActive = selection.size > 0;
      draw();
      flash(`Imported ${selection.size} chunk(s) from CSV`);
    } catch (e) {
      error = String(e);
    }
  }

  async function exportSelectedFolder() {
    if (!map || selection.size === 0 || !$projectPath || !worldName) return;
    error = null;
    try {
      const dest = await open({ directory: true, multiple: false });
      if (!dest || typeof dest !== "string") return;
      const n = await api.worlds.exportChunks(
        worldName,
        selectionPayload(),
        dest,
        dimension,
        $projectPath,
      );
      flash(`Exported ${n} chunk(s) to folder`);
    } catch (e) {
      error = String(e);
    }
  }

  async function importFromFolder() {
    if (!map || !$projectPath || !worldName) return;
    error = null;
    try {
      const src = await open({
        directory: true,
        multiple: false,
        title: "Import chunks from world / export folder",
      });
      if (!src || typeof src !== "string") return;
      const n = await api.worlds.importChunks(
        worldName,
        src,
        {
          offsetX: Number(pasteOffsetX) || 0,
          offsetZ: Number(pasteOffsetZ) || 0,
          overwrite: importOverwrite,
          yOffset: Number(importYOffset) || 0,
          sections: importSections.trim() || undefined,
          targetSelections:
            importIntoSelection && selection.size > 0 ? selectionPayload() : undefined,
          dimension,
          sourceDimension: dimension,
        },
        $projectPath,
      );
      await load();
      flash(`Imported ${n} chunk entries (Δ ${pasteOffsetX},${pasteOffsetZ}, Ysec ${importYOffset})`);
    } catch (e) {
      error = String(e);
    }
  }

  async function exportFullMapPng() {
    if (!map || !$projectPath || !worldName) return;
    error = null;
    try {
      const dest = await save({
        defaultPath: `worldmap-${worldName}-${dimension}.png`,
        filters: [{ name: "PNG", extensions: ["png"] }],
      });
      if (!dest || typeof dest !== "string") return;
      const [w, h] = await api.worlds.renderMapPng(
        worldName,
        dest,
        {
          colorMode,
          scale: 4,
          selections: selection.size > 0 ? selectionPayload() : [],
          dimension,
        },
        $projectPath,
      );
      flash(`Saved map PNG ${w}×${h}${selection.size ? " (selection)" : ""}`);
    } catch (e) {
      error = String(e);
    }
  }

  async function applyQuerySelect() {
    if (!map || !$projectPath || !worldName) return;
    const q = filterQuery.trim();
    if (!q) {
      flash("Enter a filter query (e.g. InhabitedTime < 100)");
      return;
    }
    error = null;
    try {
      const hits = await api.worlds.selectByQuery(worldName, q, dimension, $projectPath);
      const next = new Set<string>();
      for (const h of hits) {
        next.add(`${h.regionX}:${h.regionZ}:${h.index}`);
      }
      selection = next;
      filterActive = true;
      draw();
      flash(`Query: ${selection.size} chunk(s)`);
    } catch (e) {
      error = String(e);
    }
  }

  async function swapTwoSelected() {
    if (!map || selection.size !== 2 || !$projectPath || !worldName) return;
    const keys = Array.from(selection);
    const parseKey = (key: string) => {
      const parts = key.split(":");
      return {
        regionX: Number(parts[0]),
        regionZ: Number(parts[1]),
        indices: [Number(parts[2])],
      };
    };
    const a = parseKey(keys[0]);
    const b = parseKey(keys[1]);
    error = null;
    try {
      await api.worlds.swapChunks(worldName, a, b, dimension, $projectPath);
      selection = new Set();
      await load();
      flash("Swapped 2 chunks");
    } catch (e) {
      error = String(e);
    }
  }

  function exportPng() {
    if (!canvas) return;
    const url = canvas.toDataURL("image/png");
    const a = document.createElement("a");
    a.href = url;
    a.download = `worldmap-${worldName || "world"}-${dimension}.png`;
    document.body.appendChild(a);
    a.click();
    a.remove();
  }

  async function deleteSelected() {
    if (!map || selection.size === 0 || !$projectPath || !worldName) return;
    const n = selection.size;
    if (!confirm(`Delete ${n} selected chunk(s) in ${dimLabel(dimension)}?\n\nThis cannot be undone (make a Backup first).`)) {
      return;
    }
    error = null;
    try {
      const cleared = await api.worlds.deleteChunks(worldName, selectionPayload(), dimension, $projectPath);
      selection = new Set();
      filterActive = false;
      await load();
      flash(`Deleted ${cleared} chunks`);
    } catch (e) {
      error = String(e);
    }
  }

  async function purgeRegions() {
    if (!$projectPath || !worldName) return;
    if (!confirm(`Purge/compact region files in ${dimLabel(dimension)}?\n\nRemoves empty sectors after deletes and deletes empty .mca files (region + entities + poi).`)) {
      return;
    }
    error = null;
    try {
      const n = await api.worlds.purge(worldName, dimension, $projectPath);
      await load();
      flash(`Purged ${n} region file(s)`);
    } catch (e) {
      error = String(e);
    }
  }

  async function clearMapCache() {
    if (!$projectPath || !worldName) return;
    error = null;
    try {
      const n = await api.worlds.clearCache(worldName, dimension, $projectPath);
      await load();
      flash(`Cleared ${n} map cache file(s)`);
    } catch (e) {
      error = String(e);
    }
  }

  async function warmMapCache() {
    if (!$projectPath || !worldName) return;
    error = null;
    try {
      const n = await api.worlds.warmCache(worldName, dimension, $projectPath);
      flash(`Warmed cache for ${n} region(s)`);
    } catch (e) {
      error = String(e);
    }
  }

  async function copySelected() {
    if (!map || selection.size === 0 || !$projectPath || !worldName) return;
    error = null;
    try {
      clipboard = await api.worlds.copyChunks(worldName, selectionPayload(), dimension, $projectPath);
      pasteOffsetX = 0;
      pasteOffsetZ = 0;
      const ents = clipboard.entities?.length ?? 0;
      const pois = clipboard.poi?.length ?? 0;
      let msg = `Copied ${clipboard.chunks.length} chunks`;
      if (ents || pois) {
        const parts: string[] = [];
        if (ents) parts.push(`${ents} entities`);
        if (pois) parts.push(`${pois} poi`);
        msg += ` (+${parts.join(", ")})`;
      }
      flash(msg);
    } catch (e) {
      error = String(e);
    }
  }

  async function cutSelected() {
    await copySelected();
    if (clipboard && $projectPath && worldName) {
      // cut: delete without second confirm (already confirmed by copy success)
      if (!confirm(`Cut: also delete ${selection.size} chunks from the world?`)) return;
      try {
        await api.worlds.deleteChunks(worldName, selectionPayload(), dimension, $projectPath);
        selection = new Set();
        await load();
        flash("Cut complete");
      } catch (e) {
        error = String(e);
      }
    }
  }

  async function pasteFromClipboard() {
    if (!clipboard || !$projectPath || !worldName) return;
    error = null;
    try {
      const pasted = await api.worlds.pasteChunks(
        worldName,
        clipboard,
        Number(pasteOffsetX) || 0,
        Number(pasteOffsetZ) || 0,
        dimension,
        $projectPath,
      );
      await load();
      flash(`Pasted ${pasted} chunks (offset ${pasteOffsetX}, ${pasteOffsetZ})`);
    } catch (e) {
      error = String(e);
    }
  }

  function clearClipboard() {
    clipboard = null;
  }

  function openChunkEditor() {
    if (selection.size !== 1) return;
    const key = Array.from(selection)[0];
    const parts = key.split(":");
    editorRx = Number(parts[0]);
    editorRz = Number(parts[1]);
    editorIdx = Number(parts[2]);
    editorOpen = true;
  }

  function closeChunkEditor() {
    editorOpen = false;
  }

  function buildNbtChange(): NbtChangeRequest {
    const change: NbtChangeRequest = {};
    if (chgInhabited !== "") {
      const n = Number(chgInhabited);
      if (Number.isFinite(n)) change.inhabitedTime = n;
    }
    if (chgStatus !== "") change.status = chgStatus;
    if (chgDataVersion !== "") {
      const n = Number(chgDataVersion);
      if (Number.isFinite(n)) change.dataVersion = Math.trunc(n);
    }
    if (chgLightPopulated !== "") {
      const n = Number(chgLightPopulated);
      if (Number.isFinite(n)) change.lightPopulated = Math.trunc(n);
    }
    if (chgBiome !== "") change.biome = chgBiome;
    if (chgDeleteSections !== "") change.deleteSections = chgDeleteSections;
    if (chgReplaceBlocks !== "") change.replaceBlocks = chgReplaceBlocks;
    if (chgDeleteStructureRefs !== "") change.deleteStructureRefs = chgDeleteStructureRefs;
    if (chgPreventRetrogen) change.preventRetrogen = true;
    if (chgForceBlend) change.forceBlend = true;
    if (chgDeleteEntities) change.deleteEntities = true;
    if (chgFixStatus) change.fixStatus = true;
    if (chgForce) change.force = true;
    return change;
  }

  function buildAdvancedFilter(): AdvancedChunkFilter | null {
    const filter: AdvancedChunkFilter = {};
    if (filtEntityNames !== "") filter.entityNames = filtEntityNames;
    if (filtStructureNames !== "") filter.structureNames = filtStructureNames;
    if (filtPaletteNames !== "") filter.paletteNames = filtPaletteNames;
    const minEnt = parseOptNum(entityCountMin);
    if (minEnt != null) filter.minEntities = minEnt;
    if (
      filter.entityNames == null &&
      filter.structureNames == null &&
      filter.paletteNames == null &&
      filter.minEntities == null
    ) {
      return null;
    }
    return filter;
  }

  async function applyContentFilter() {
    if (!$projectPath || !worldName) return;
    const filter = buildAdvancedFilter();
    if (!filter) {
      flash("Fill entity/structure/palette names or ents min");
      return;
    }
    error = null;
    try {
      const refs = await api.worlds.filterAdvanced(
        worldName,
        filter,
        selection.size ? selectionPayload() : undefined,
        dimension,
        $projectPath,
      );
      const next = new Set<string>();
      for (const ref of refs) {
        next.add(`${ref.regionX}:${ref.regionZ}:${ref.index}`);
      }
      selection = next;
      filterActive = next.size > 0;
      draw();
      flash(`Content filter: ${next.size} chunk(s)`);
    } catch (e) {
      error = String(e);
    }
  }

  async function applyNbtChange() {
    if (!map || selection.size === 0 || !$projectPath || !worldName) return;
    const change = buildNbtChange();
    const n = selection.size;
    if (!confirm(`Apply NBT change to ${n} selected chunk(s) in ${dimLabel(dimension)}?\n\nThis writes chunk data (make a Backup first).`)) {
      return;
    }
    error = null;
    try {
      const changed = await api.worlds.changeChunks(
        worldName,
        selectionPayload(),
        change,
        dimension,
        $projectPath,
      );
      flash(`Changed ${changed} chunk(s)`);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key === "c") { e.preventDefault(); copySelected(); }
    else if (ctrl && e.key === "x") { e.preventDefault(); cutSelected(); }
    else if (ctrl && e.key === "v") { e.preventDefault(); pasteFromClipboard(); }
    else if (ctrl && e.key === "a") { e.preventDefault(); selectAll(); }
    else if (e.key === "Escape") { clearClipboard(); clearSelection(); }
    else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); deleteSelected(); }
    else if (e.key === "+" || e.key === "=") zoomBy(1.15);
    else if (e.key === "-") zoomBy(1 / 1.15);
    else if (e.key === "0") { fitView(); draw(); }
    else if (e.key === "n" || e.key === "N") { e.preventDefault(); cycleColorMode(); }
  }

  function onLeave(evt: MouseEvent) {
    hover = null;
    onUp(evt);
  }

  function onResize() {
    draw();
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    window.addEventListener("resize", onResize);
  });

  onDestroy(() => {
    window.removeEventListener("keydown", handleKeydown);
    window.removeEventListener("resize", onResize);
    clearTimeout(flashTimer);
  });

  $: if (worldName && $projectPath) load();
  $: if (map) requestAnimationFrame(draw);
  $: canvasCursor = tool === "pan" ? "grab" : (tool === "click" || tool === "region") ? "pointer" : "crosshair";
</script>

<div class="world-map">
  <div class="toolbar">
    <div class="title"><MapIcon size={18} /> MCA map · {worldName}</div>
    <div class="tools">
      <select class="ghost select" bind:value={dimension} on:change={load} title="Dimension">
        {#each dimensions as d (d)}
          <option value={d}>{dimLabel(d)}</option>
        {/each}
      </select>
      <label class="toggle" title="Overlay region boundaries">
        <Layers size={14} /> Regions
        <input type="checkbox" bind:checked={showRegions} on:change={draw} />
      </label>
      <select class="ghost select" bind:value={colorMode} on:change={draw} title="Color mode (N to cycle)">
        <option value="status">by status</option>
        <option value="date">by date</option>
        <option value="inhabited">by inhabited</option>
        <option value="biome">by biome</option>
        <option value="height">by height</option>
      </select>
      <button class="ghost" class:active={tool === "pan"} on:click={() => (tool = "pan")} title="Pan (or Alt+drag / middle mouse)">
        <Minimize2 size={14} /> Pan
      </button>
      <button class="ghost" class:active={tool === "click"} on:click={() => (tool = "click")} title="Click chunks to toggle">
        <MousePointer2 size={14} /> Click
      </button>
      <button class="ghost" class:active={tool === "box"} on:click={() => (tool = "box")} title="Drag rectangle (Shift = subtract)">
        <Square size={14} /> Box
      </button>
      <button class="ghost" class:active={tool === "radius"} on:click={() => (tool = "radius")} title="Drag radius (or click with default radius)">
        <Circle size={14} /> Radius
      </button>
      <button class="ghost" class:active={tool === "region"} on:click={() => (tool = "region")} title="Select whole region (Shift = deselect)">
        <Layers size={14} /> Region
      </button>
      <button class="ghost" on:click={() => zoomBy(1.2)} title="Zoom in"><ZoomIn size={14} /></button>
      <button class="ghost" on:click={() => zoomBy(1 / 1.2)} title="Zoom out"><ZoomOut size={14} /></button>
      <button class="ghost" on:click={() => { fitView(); draw(); }} title="Fit map">Fit</button>
      <button class="ghost" on:click={copySelected} disabled={selection.size === 0} title="Copy (Ctrl+C)">
        <Copy size={14} /> Copy
      </button>
      <button class="ghost" on:click={cutSelected} disabled={selection.size === 0} title="Cut (Ctrl+X)">
        <Scissors size={14} /> Cut
      </button>
      <button class="ghost" on:click={pasteFromClipboard} disabled={!clipboard} title="Paste (Ctrl+V)">
        <Clipboard size={14} /> Paste {clipboard ? `(${clipboard.chunks.length})` : ""}
      </button>
      <button class="ghost" on:click={swapTwoSelected} disabled={selection.size !== 2} title="Swap two selected chunks">
        <ArrowLeftRight size={14} /> Swap
      </button>
      <button class="ghost" on:click={openChunkEditor} disabled={selection.size !== 1} title="Edit NBT of selected chunk">
        <Pencil size={14} /> Edit NBT
      </button>
      <button class="ghost" on:click={exportSelectedFolder} disabled={selection.size === 0} title="Export selected chunks to folder">
        <FolderOutput size={14} /> Folder
      </button>
      <button class="ghost" on:click={importFromFolder} disabled={!map} title="Import chunks from another world/export folder (uses paste ΔX/ΔZ)">
        <FolderInput size={14} /> Import
      </button>
      <button class="ghost" on:click={exportSelectionCsv} disabled={selection.size === 0} title="Export selection CSV">
        <FileDown size={14} /> CSV
      </button>
      <button class="ghost" on:click={triggerCsvImport} disabled={!map} title="Import selection CSV">
        <FileUp size={14} /> CSV
      </button>
      <button class="ghost" on:click={exportPng} title="Export viewport PNG"><Download size={14} /> PNG</button>
      <button class="ghost" on:click={exportFullMapPng} disabled={!map} title="Save full dimension/selection map PNG (current color mode)">
        <Download size={14} /> Map PNG
      </button>
      <button class="ghost" on:click={purgeRegions} title="Compact region files after deletes">
        <Eraser size={14} /> Purge
      </button>
      <button class="ghost" on:click={warmMapCache} disabled={!map} title="Warm region metadata cache">
        Cache
      </button>
      <button class="ghost" on:click={clearMapCache} disabled={!map} title="Clear region metadata cache for this dimension">
        Clear cache
      </button>
      <button class="ghost danger" on:click={deleteSelected} disabled={selection.size === 0} title="Delete selected (Del)">
        <Trash2 size={14} /> Delete {selection.size || ""}
      </button>
      <button class="ghost" on:click={load} disabled={loading} title="Reload">
        <RefreshCw size={14} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  <input
    bind:this={csvInput}
    type="file"
    accept=".csv,text/csv,text/plain"
    style="display:none"
    on:change={onCsvImport}
  />

  <div class="stats">
    {#if map}
      <span>{dimLabel(dimension)}</span>
      <span>{map.regionCount} regions</span>
      <span>{map.totalPresent.toLocaleString()} chunks</span>
      <span>RX {map.minRegionX}…{map.maxRegionX}</span>
      <span>RZ {map.minRegionZ}…{map.maxRegionZ}</span>
      <span>zoom {(zoom * 100).toFixed(0)}%</span>
      <span class="sel">selected: {selection.size}</span>
      {#if clipboard}
        <span class="clip">clipboard: {clipboard.chunks.length}</span>
      {/if}
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
    <span>status</span>
    <select class="mini-select" bind:value={statusFilter} title="Only select chunks with this status">
      <option value="all">all</option>
      <option value="empty">empty</option>
      <option value="partial">partial</option>
      <option value="full">full</option>
    </select>
    <span class="sep" />
    <span>inh</span>
    <input class="num" type="text" bind:value={inhabitedMin} placeholder="min" title="Min inhabitedTime" />
    <input class="num" type="text" bind:value={inhabitedMax} placeholder="max" title="Max inhabitedTime" />
    <span>dv</span>
    <input class="num" type="text" bind:value={dataVersionMin} placeholder="min" title="Min dataVersion" />
    <input class="num" type="text" bind:value={dataVersionMax} placeholder="max" title="Max dataVersion" />
    <span>X</span>
    <input class="num" type="text" bind:value={xposMin} placeholder="min" title="Min chunk X" />
    <input class="num" type="text" bind:value={xposMax} placeholder="max" title="Max chunk X" />
    <span>Z</span>
    <input class="num" type="text" bind:value={zposMin} placeholder="min" title="Min chunk Z" />
    <input class="num" type="text" bind:value={zposMax} placeholder="max" title="Max chunk Z" />
    <span>border</span>
    <input class="num" type="text" bind:value={borderEmpty} placeholder="≥N" title="Empty neighbor count ≥ N (4-neighbors)" />
    <span>ents</span>
    <input class="num" type="text" bind:value={entityCountMin} placeholder="min" title="Min entityCount / minEntities" />
    <span>structs</span>
    <input class="num" type="text" bind:value={structureCountMin} placeholder="min" title="Min structureCount" />
    <span>ent names</span>
    <input class="num wide" type="text" bind:value={filtEntityNames} placeholder="zombie,…" title="Entity id names (comma-separated)" />
    <span>struct names</span>
    <input class="num wide" type="text" bind:value={filtStructureNames} placeholder="village,…" title="Structure names (comma-separated)" />
    <span>palette</span>
    <input class="num wide" type="text" bind:value={filtPaletteNames} placeholder="stone,…" title="Block palette names (comma-separated)" />
    <button class="mini" on:click={applyChunkFilter} disabled={!map} title="Select chunks matching all filters">
      <Filter size={12} /> Select by filter
    </button>
    <button class="mini" on:click={applyContentFilter} disabled={!map || !$projectPath} title="Scan MCA for entity/structure/palette content (empty selection = whole dimension)">
      <Filter size={12} /> Content filter
    </button>
    <span class="sep" />
    <span>r</span>
    <input class="num" type="number" min="1" max="128" bind:value={radiusChunks} title="Default radius (chunks)" />
    <span class="sep" />
    <span>paste ΔX</span>
    <input class="num" type="number" bind:value={pasteOffsetX} title="Paste chunk X offset" />
    <span>ΔZ</span>
    <input class="num" type="number" bind:value={pasteOffsetZ} title="Paste chunk Z offset" />
    <label class="chk" title="Overwrite existing chunks on import/paste destinations">
      <input type="checkbox" bind:checked={importOverwrite} /> overwrite
    </label>
    <label class="chk" title="Only import into current selection">
      <input type="checkbox" bind:checked={importIntoSelection} /> into sel
    </label>
    <span>Ysec</span>
    <input class="num" type="number" bind:value={importYOffset} title="Import vertical section offset (×16 blocks)" />
    <span>secs</span>
    <input class="num wide" type="text" bind:value={importSections} placeholder="all / :-4 / 0:4" title="Import only these sections" />
    <span class="sep" />
    <span>query</span>
    <input
      class="num wide"
      type="text"
      bind:value={filterQuery}
      placeholder='InhabitedTime < 100 AND Status = full'
      title="MCA-style map filter query"
    />
    <button class="mini" on:click={applyQuerySelect} disabled={!map || !$projectPath} title="Select by filter query">
      <Filter size={12} /> Query
    </button>
    <span class="sep" />
    <button class="mini" on:click={selectAll} disabled={!map}><CheckSquare size={12} /> All</button>
    <button class="mini" on:click={invertSelection} disabled={!map}><CheckSquare size={12} /> Invert</button>
    <button class="mini" on:click={invertSelectedRegions} disabled={!map || selection.size === 0} title="Invert only regions that have selection">
      <CheckSquare size={12} /> Invert regions
    </button>
    <button class="mini" on:click={clearSelection} disabled={selection.size === 0}><XSquare size={12} /> Clear</button>
    <button
      class="mini"
      on:click={expandSelection}
      disabled={!map || selection.size === 0}
      title="Expand selection by Chebyshev ±r chunks (present cells only; uses r above)"
    >
      Expand ±r
    </button>
    {#if filterActive}<span class="filttag">filter active</span>{/if}
  </div>

  <div class="nbt-bar">
    <button class="mini" on:click={() => (nbtPanelOpen = !nbtPanelOpen)} title="Toggle NBT changer">
      <Wrench size={12} /> NBT Changer
    </button>
    {#if nbtPanelOpen}
      <span>inhabited</span>
      <input class="num" type="text" bind:value={chgInhabited} placeholder="ticks" title="Set InhabitedTime" />
      <span>status</span>
      <input class="num wide" type="text" bind:value={chgStatus} placeholder="e.g. full" title="Set Status string" />
      <span>dataVersion</span>
      <input class="num" type="text" bind:value={chgDataVersion} placeholder="dv" title="Set DataVersion" />
      <span>light</span>
      <input class="num" type="text" bind:value={chgLightPopulated} placeholder="0/1" title="Set isLightOn / LightPopulated" />
      <span>biome</span>
      <input class="num wide" type="text" bind:value={chgBiome} placeholder="plains" title="Set biome id/name" />
      <span>del secs</span>
      <input class="num wide" type="text" bind:value={chgDeleteSections} placeholder="all / :-4" title="Delete sections (e.g. all, :-4, 0:4)" />
      <span>replace</span>
      <input class="num wide" type="text" bind:value={chgReplaceBlocks} placeholder="stone=deepslate; oak_log[axis=y]=stripped_oak_log[axis=y]" title="Replace blocks (name or name[prop=val]; comma/semicolon separated)" />
      <span>del structs</span>
      <input class="num wide" type="text" bind:value={chgDeleteStructureRefs} placeholder="names" title="Delete structure references" />
      <label class="chk" title="Prevent retrogen">
        <input type="checkbox" bind:checked={chgPreventRetrogen} /> no retrogen
      </label>
      <label class="chk" title="Force blend (is_blending / blending_data)">
        <input type="checkbox" bind:checked={chgForceBlend} /> force blend
      </label>
      <label class="chk" title="Delete entities in selected chunks">
        <input type="checkbox" bind:checked={chgDeleteEntities} /> del ents
      </label>
      <label class="chk" title="Fix Status from chunk contents">
        <input type="checkbox" bind:checked={chgFixStatus} /> fix status
      </label>
      <label class="chk" title="Force write even if unchanged">
        <input type="checkbox" bind:checked={chgForce} /> force
      </label>
      <button class="mini" on:click={applyNbtChange} disabled={selection.size === 0} title="Apply NBT change to selection">
        <Wrench size={12} /> NBT Change
      </button>
    {/if}
  </div>

  <div class="map-scroll" bind:this={viewport}>
    {#if map}
      <canvas
        bind:this={canvas}
        style="cursor: {canvasCursor}"
        on:mousemove={onMove}
        on:click={onClick}
        on:mousedown={onDown}
        on:mouseup={onUp}
        on:mouseleave={onLeave}
        on:wheel|preventDefault={onWheel}
        on:contextmenu|preventDefault
      ></canvas>
      {#if hover}
        <div class="hover-tip" style="left: {tipX}px; top: {tipY}px">
          chunk <code>{hover.cx}, {hover.cz}</code> · region {hover.rx},{hover.rz}<br />
          {hover.status}{#if hover.modified} · {new Date(hover.modified * 1000).toLocaleDateString()}{/if}<br />
          inhabited {hover.inhabitedTime} · dataVersion {hover.dataVersion}
          {#if hover.biomeId != null}<br />biome {hover.biomeId}{/if}
          {#if hover.surfaceY != null}<br />surfaceY {hover.surfaceY}{/if}
          {#if hover.entityCount != null}<br />entities {hover.entityCount}{/if}
          {#if hover.structureCount != null}<br />structures {hover.structureCount}{/if}
        </div>
      {/if}
    {:else if error}
      <EmptyState icon={MapIcon} title="No map yet" description="Generate the world by running the pack, then refresh. Switch dimension if you explored Nether/End." />
    {:else if !loading}
      <EmptyState icon={MapIcon} title="No world selected" description="Open a world to view its 2D map." />
    {/if}
  </div>

  <div class="legend">
    <span><i style="background:#15171c"></i> absent</span>
    <span><i style="background:#3b4252"></i> empty</span>
    <span><i style="background:#b08968"></i> partial</span>
    <span><i style="background:#2d8c8c"></i> {colorMode === "date" || colorMode === "inhabited" || colorMode === "height" ? "old→new / low→high" : colorMode === "biome" ? "biome hue" : "full (old→new)"}</span>
    <span><i style="background:rgba(255,90,95,0.7)"></i> selected</span>
    <span class="hint">Wheel zoom · Alt/middle pan · Shift subtract · N color · Del delete</span>
  </div>
</div>

{#if editorOpen}
  <ChunkNbtEditor
    {worldName}
    {dimension}
    regionX={editorRx}
    regionZ={editorRz}
    index={editorIdx}
    onClose={closeChunkEditor}
    onSaved={load}
  />
{/if}

<style>
  .world-map { display: flex; flex-direction: column; gap: 10px; min-height: 0; flex: 1; }
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
  .stats .clip { color: #8fd3ff; }
  .stats .ok { color: var(--accent-primary); }
  .stats .err { color: #fca5a5; }
  .filter-bar, .nbt-bar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 12px; color: var(--text-muted); }
  .filter-bar input[type="date"] { background: var(--bg-elevated); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 4px 6px; font-size: 12px; color-scheme: dark; }
  .filter-bar .num, .nbt-bar .num { width: 64px; background: var(--bg-elevated); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 4px 6px; font-size: 12px; }
  .filter-bar .num.wide, .nbt-bar .num.wide { width: 88px; }
  .nbt-bar .chk { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; cursor: pointer; }
  .filter-bar .sep { width: 1px; height: 18px; background: var(--border-color); margin: 0 4px; }
  .mini-select { background: var(--bg-elevated); color: var(--text-primary); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); padding: 4px 6px; font-size: 11px; }
  .mini { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; padding: 4px 8px; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: transparent; color: var(--text-secondary); cursor: pointer; }
  .mini:hover:not(:disabled) { background: var(--bg-tertiary); }
  .mini:disabled { opacity: .4; cursor: default; }
  .filttag { color: #8fd3ff; font-size: 11px; }
  .map-scroll { position: relative; overflow: hidden; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: #0e0f13; height: min(60vh, 560px); min-height: 280px; }
  canvas { display: block; width: 100%; height: 100%; image-rendering: pixelated; }
  .hover-tip { position: fixed; pointer-events: none; z-index: 30; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: 6px; padding: 6px 8px; font-size: 11px; color: var(--text-secondary); box-shadow: 0 4px 16px rgba(0,0,0,.4); }
  .hover-tip code { color: var(--accent-primary); }
  .legend { display: flex; gap: 14px; flex-wrap: wrap; font-size: 11px; color: var(--text-muted); align-items: center; }
  .legend span { display: inline-flex; align-items: center; gap: 5px; }
  .legend i { width: 11px; height: 11px; border-radius: 2px; display: inline-block; border: 1px solid rgba(255,255,255,.1); }
  .legend .hint { opacity: 0.7; margin-left: auto; }
</style>
