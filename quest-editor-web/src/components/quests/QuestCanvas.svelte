<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { QuestData, QuestValidationIssue } from "../../lib/store";
  import Minimap from "./Minimap.svelte";

  let {
    quests,
    selectedId = null,
    selectedIds = new Set(),
    issues,
    emptyHint = "Double-click to add a quest",
    onSelect,
    onMove,
    onAddAt,
    onLink,
    fitToken = 0,
  }: {
    quests: QuestData[];
    selectedId?: string | null;
    selectedIds?: Set<string>;
    issues: QuestValidationIssue[];
    emptyHint?: string;
    onSelect: (q: QuestData | null, e?: MouseEvent) => void;
    onMove: (q: QuestData, x: number, y: number) => void;
    onAddAt: (x: number, y: number) => void;
    onLink: (fromId: string, toDepId: string) => void;
    fitToken?: number;
  } = $props();

  const BASE = 24;
  const MIN_ZOOM = 0.25;
  const MAX_ZOOM = 4;

  let viewport = $state<HTMLDivElement | undefined>(undefined);
  let zoom = $state(1);
  let panX = $state(0);
  let panY = $state(0);

  let mode = $state<"idle" | "pan" | "drag" | "link" | "marquee">("idle");
  let panLast = $state<{ x: number; y: number } | null>(null);
  let dragQuest = $state<QuestData | null>(null);
  let dragMoved = $state(false);
  let dragOffset = $state({ x: 0, y: 0 });
  let dragTick = $state(0);
  let linkFrom = $state<QuestData | null>(null);
  let linkCursor = $state<{ x: number; y: number } | null>(null);
  let spaceDown = $state(false);
  let lastFitToken = $state(-1);

  // Marquee state
  let marqueeStart = $state<{ x: number; y: number } | null>(null);
  let marqueeEnd = $state<{ x: number; y: number } | null>(null);
  let marqueeIds = $state<Set<string>>(new Set());

  let unit = $derived(BASE * zoom);
  let issueIds = $derived(new Set(issues.map((i) => i.questId)));
  let viewportWidth = $state(800);
  let viewportHeight = $state(600);

  $effect(() => {
    if (fitToken !== lastFitToken && quests) {
      lastFitToken = fitToken;
      void refit();
    }
  });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code === "Space" && !(e.target as HTMLElement)?.closest?.("input,textarea,select")) {
        spaceDown = true;
        e.preventDefault();
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.code === "Space") spaceDown = false;
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKeyUp);

    // Track viewport dimensions
    if (viewport) {
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          viewportWidth = entry.contentRect.width;
          viewportHeight = entry.contentRect.height;
        }
      });
      ro.observe(viewport);
      void refit();
      return () => {
        window.removeEventListener("keydown", onKey);
        window.removeEventListener("keyup", onKeyUp);
        ro.disconnect();
      };
    }

    void refit();
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKeyUp);
    };
  });

  async function refit() {
    await tick();
    fitView();
  }

  function snap(v: number) {
    return Math.round(v * 2) / 2;
  }

  function nodeSize(q: QuestData) {
    return BASE * (q.size && q.size > 0 ? q.size : 1) * zoom;
  }

  function screenPos(q: QuestData) {
    const s = nodeSize(q);
    return {
      left: panX + q.x * unit - s / 2,
      top: panY + q.y * unit - s / 2,
      size: s,
    };
  }

  function clientToWorld(clientX: number, clientY: number) {
    if (!viewport) return { x: 0, y: 0 };
    const rect = viewport.getBoundingClientRect();
    return {
      x: (clientX - rect.left - panX) / unit,
      y: (clientY - rect.top - panY) / unit,
    };
  }

  function worldToScreen(wx: number, wy: number) {
    return {
      x: wx * unit + panX,
      y: wy * unit + panY,
    };
  }

  function fitView() {
    if (!viewport) return;
    const vw = viewport.clientWidth || 800;
    const vh = viewport.clientHeight || 500;
    if (quests.length === 0) {
      zoom = 1;
      panX = vw / 2;
      panY = vh / 2;
      return;
    }
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const q of quests) {
      const half = (q.size && q.size > 0 ? q.size : 1) / 2;
      minX = Math.min(minX, q.x - half);
      minY = Math.min(minY, q.y - half);
      maxX = Math.max(maxX, q.x + half);
      maxY = Math.max(maxY, q.y + half);
    }
    const pad = 1.5;
    const w = Math.max(maxX - minX + pad * 2, 4);
    const h = Math.max(maxY - minY + pad * 2, 4);
    zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, Math.min(vw / (w * BASE), vh / (h * BASE)) * 0.9));
    const u = BASE * zoom;
    panX = (vw - (minX + maxX) * u) / 2;
    panY = (vh - (minY + maxY) * u) / 2;
  }

  function questAt(clientX: number, clientY: number): QuestData | null {
    if (!viewport) return null;
    const rect = viewport.getBoundingClientRect();
    const sx = clientX - rect.left;
    const sy = clientY - rect.top;
    for (let i = quests.length - 1; i >= 0; i--) {
      const q = quests[i];
      const p = screenPos(q);
      if (sx >= p.left && sx <= p.left + p.size && sy >= p.top && sy <= p.top + p.size) {
        return q;
      }
    }
    return null;
  }

  function onPointerDown(e: PointerEvent) {
    if (!viewport) return;
    const hit = questAt(e.clientX, e.clientY);
    const wantPan = e.button === 1 || spaceDown || (e.button === 0 && !hit);

    if (wantPan && e.button !== 2) {
      if (!hit) onSelect(null);
      mode = "pan";
      panLast = { x: e.clientX, y: e.clientY };
      viewport.setPointerCapture(e.pointerId);
      e.preventDefault();
      return;
    }

    if (e.button === 0 && hit) {
      if (e.shiftKey) {
        mode = "link";
        linkFrom = hit;
        linkCursor = clientToWorld(e.clientX, e.clientY);
        onSelect(hit, e);
        viewport.setPointerCapture(e.pointerId);
        e.preventDefault();
        return;
      }
      mode = "drag";
      dragQuest = hit;
      dragMoved = false;
      const w = clientToWorld(e.clientX, e.clientY);
      dragOffset = { x: w.x - hit.x, y: w.y - hit.y };
      onSelect(hit, e);
      viewport.setPointerCapture(e.pointerId);
      e.preventDefault();
    }

    // Marquee selection on empty space
    if (e.button === 0 && !hit && !e.shiftKey) {
      mode = "marquee";
      const w = clientToWorld(e.clientX, e.clientY);
      marqueeStart = w;
      marqueeEnd = w;
      marqueeIds = new Set();
      viewport.setPointerCapture(e.pointerId);
      e.preventDefault();
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (mode === "pan" && panLast) {
      panX += e.clientX - panLast.x;
      panY += e.clientY - panLast.y;
      panLast = { x: e.clientX, y: e.clientY };
      return;
    }
    if (mode === "drag" && dragQuest) {
      const w = clientToWorld(e.clientX, e.clientY);
      dragMoved = true;
      dragQuest.x = snap(w.x - dragOffset.x);
      dragQuest.y = snap(w.y - dragOffset.y);
      dragTick += 1;
      return;
    }
    if (mode === "link" && linkFrom) {
      linkCursor = clientToWorld(e.clientX, e.clientY);
    }
    if (mode === "marquee" && marqueeStart) {
      marqueeEnd = clientToWorld(e.clientX, e.clientY);
      // Calculate which quests are in the marquee
      const x1 = Math.min(marqueeStart.x, marqueeEnd.x);
      const x2 = Math.max(marqueeStart.x, marqueeEnd.x);
      const y1 = Math.min(marqueeStart.y, marqueeEnd.y);
      const y2 = Math.max(marqueeStart.y, marqueeEnd.y);

      const newIds = new Set<string>();
      for (const q of quests) {
        if (q.x >= x1 && q.x <= x2 && q.y >= y1 && q.y <= y2) {
          newIds.add(q.id);
        }
      }
      marqueeIds = newIds;
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (mode === "drag" && dragQuest) {
      if (dragMoved) onMove(dragQuest, snap(dragQuest.x), snap(dragQuest.y));
    }
    if (mode === "link" && linkFrom) {
      const hit = questAt(e.clientX, e.clientY);
      if (hit && hit.id !== linkFrom.id) onLink(linkFrom.id, hit.id);
    }
    if (mode === "marquee" && marqueeStart) {
      // Select all quests in marquee
      if (marqueeIds.size > 0) {
        for (const id of marqueeIds) {
          const q = quests.find((q) => q.id === id);
          if (q) onSelect(q, e);
        }
      } else {
        onSelect(null);
      }
      marqueeStart = null;
      marqueeEnd = null;
      marqueeIds = new Set();
    }
    mode = "idle";
    panLast = null;
    dragQuest = null;
    dragMoved = false;
    linkFrom = null;
    linkCursor = null;
    try { viewport?.releasePointerCapture(e.pointerId); } catch { /* ignore */ }
  }

  function onWheel(e: WheelEvent) {
    if (!viewport) return;
    e.preventDefault();
    const rect = viewport.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const before = { x: (sx - panX) / unit, y: (sy - panY) / unit };
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom * factor));
    const u = BASE * zoom;
    panX = sx - before.x * u;
    panY = sy - before.y * u;
  }

  function onDblClick(e: MouseEvent) {
    if (questAt(e.clientX, e.clientY)) return;
    const w = clientToWorld(e.clientX, e.clientY);
    onAddAt(snap(w.x), snap(w.y));
  }

  function centerOf(q: QuestData) {
    return { x: panX + q.x * unit, y: panY + q.y * unit };
  }

  function depTarget(depId: string) {
    return quests.find((q) => q.id === depId) ?? quests.find((q) => q.tasks?.some((t) => t.id === depId)) ?? null;
  }

  function glyph(q: QuestData) {
    const icon = q.icon?.trim();
    if (icon) {
      const leaf = icon.includes(":") ? icon.split(":").pop()! : icon;
      return (leaf[0] || "?").toUpperCase();
    }
    return (q.title[0] || "?").toUpperCase();
  }

  function nodeShape(q: QuestData): string {
    const s = q.shape?.trim();
    if (s && s !== "none") return s;
    return "rsquare";
  }

  function marqueeRect() {
    if (!marqueeStart || !marqueeEnd) return null;
    const x1 = Math.min(marqueeStart.x, marqueeEnd.x) * unit + panX;
    const y1 = Math.min(marqueeStart.y, marqueeEnd.y) * unit + panY;
    const x2 = Math.max(marqueeStart.x, marqueeEnd.x) * unit + panX;
    const y2 = Math.max(marqueeStart.y, marqueeEnd.y) * unit + panY;
    return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
  }
</script>

<div class="canvas-wrap">
  <div class="canvas-toolbar">
    <button type="button" class="tb" onclick={fitView}>⊞ Fit</button>
    <button
      type="button"
      class="tb"
      onclick={() => {
        const rect = viewport?.getBoundingClientRect();
        if (!rect) { onAddAt(0, 0); return; }
        const w = clientToWorld(rect.left + rect.width / 2, rect.top + rect.height / 2);
        onAddAt(snap(w.x), snap(w.y));
      }}
    >+ Add</button>
    <span class="hint">Drag · Space pan · Wheel zoom · Shift+drag link · Dbl-click add · Marquee select</span>
  </div>

  <div
    class="viewport"
    class:panning={mode === "pan" || spaceDown}
    class:linking={mode === "link"}
    class:marquee={mode === "marquee"}
    bind:this={viewport}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    onwheel={onWheel}
    ondblclick={onDblClick}
    role="application"
    aria-label="Quest canvas"
  >
    <svg class="edges" width="100%" height="100%">
      {#each quests as q (q.id)}
        {@const _e = dragTick}
        {#each q.dependencies as depId}
          {@const target = depTarget(depId)}
          {@const from = centerOf(q)}
          {#if target}
            {@const to = centerOf(target)}
            <line x1={from.x} y1={from.y} x2={to.x} y2={to.y} class="dep" />
          {:else}
            <line x1={from.x} y1={from.y} x2={from.x + 40} y2={from.y - 30} class="dep broken" />
          {/if}
        {/each}
      {/each}
      {#if mode === "link" && linkFrom && linkCursor}
        {@const from = centerOf(linkFrom)}
        <line x1={from.x} y1={from.y} x2={panX + linkCursor.x * unit} y2={panY + linkCursor.y * unit} class="dep link-preview" />
      {/if}
    </svg>

    {#each quests as q (q.id)}
      {@const _drag = dragTick}
      {@const p = screenPos(q)}
      <div
        class="node-wrap"
        class:sel={selectedId === q.id || selectedIds.has(q.id)}
        class:issue={issueIds.has(q.id)}
        style="left:{p.left}px; top:{p.top}px; width:{p.size}px;"
        title={q.title}
      >
        <div class="node-icon shape-{nodeShape(q)}" class:optional={q.optional} style="width:{p.size}px; height:{p.size}px;">
          <div class="node-face shape-{nodeShape(q)}">
            <span class="glyph" style="font-size:{Math.max(10, Math.floor(p.size * 0.5))}px">{glyph(q)}</span>
          </div>
          {#if q.optional}<span class="opt">?</span>{/if}
        </div>
        <span class="node-label">{q.title}</span>
      </div>
    {/each}

    {#if marqueeRect()}
      <div
        class="marquee-rect"
        style="left:{marqueeRect()!.x}px; top:{marqueeRect()!.y}px; width:{marqueeRect()!.width}px; height:{marqueeRect()!.height}px;"
      ></div>
    {/if}

    {#if quests.length === 0}
      <div class="empty-hint">{emptyHint}</div>
    {/if}
  </div>

  {#if quests.length > 0}
    <Minimap
      {quests}
      {zoom}
      {panX}
      {panY}
      {viewportWidth}
      {viewportHeight}
      base={BASE}
    />
  {/if}
</div>

<style>
  .canvas-wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-tertiary);
    overflow: hidden;
  }
  .canvas-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-secondary);
    flex-shrink: 0;
  }
  .tb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 2px;
    border: 1px solid var(--border);
    background: rgba(0,0,0,0.25);
    color: var(--text-primary);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .tb:hover { border-color: var(--accent); background: rgba(61,184,168,0.12); }
  .hint { margin-left: auto; font-size: 9px; color: var(--text-muted); }
  .viewport {
    position: relative;
    flex: 1;
    overflow: hidden;
    cursor: default;
    background-color: var(--bg-canvas);
    background-image:
      repeating-linear-gradient(0deg, transparent, transparent 15px, var(--grid-line) 15px, var(--grid-line) 16px),
      repeating-linear-gradient(90deg, transparent, transparent 15px, var(--grid-line) 15px, var(--grid-line) 16px);
    touch-action: none;
    user-select: none;
  }
  .viewport.panning { cursor: grabbing; }
  .viewport.linking { cursor: crosshair; }
  .viewport.marquee { cursor: crosshair; }
  .edges { position: absolute; inset: 0; pointer-events: none; z-index: 1; }
  .dep { stroke: #5c8a9e; stroke-width: 3; stroke-linecap: round; }
  .dep.broken { stroke: var(--warning); stroke-dasharray: 6 4; stroke-width: 2.5; }
  .dep.link-preview { stroke: var(--accent); stroke-dasharray: 6 4; stroke-width: 2.5; opacity: 0.85; }
  .node-wrap {
    position: absolute;
    z-index: 2;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    pointer-events: none;
    cursor: grab;
  }
  .node-wrap.sel .node-icon { outline: 2px solid var(--node-selected); outline-offset: 1px; }
  .node-wrap.issue .node-icon { border-color: var(--warning); }
  .node-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 2px solid var(--node-border);
    background: transparent;
    box-shadow: 0 1px 3px rgba(0,0,0,0.4);
  }
  .node-icon.optional { border-style: dashed; }
  .node-icon.shape-circle, .node-face.shape-circle { border-radius: 50%; }
  .node-icon.shape-square, .node-face.shape-square { border-radius: 0; }
  .node-icon.shape-rsquare, .node-face.shape-rsquare { border-radius: 4px; }
  .node-face {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--node-bg);
    box-shadow: inset 0 2px 6px rgba(0,0,0,0.5);
  }
  .node-face.shape-diamond { border-radius: 0; clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%); }
  .node-face.shape-hexagon { clip-path: polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%); }
  .node-face.shape-pentagon { clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%); }
  .glyph { color: var(--text-primary); font-weight: 800; }
  .node-label {
    font-size: clamp(8px, 10px, 11px);
    line-height: 1.15;
    max-width: calc(100% + 24px);
    min-width: 100%;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    pointer-events: none;
    text-shadow: 0 1px 2px rgba(0,0,0,0.8);
  }
  .node-wrap.sel .node-label { color: var(--text-primary); }
  .opt {
    position: absolute;
    top: -3px;
    right: -3px;
    font-size: 9px;
    color: var(--warning);
    font-weight: 900;
  }
  .marquee-rect {
    position: absolute;
    border: 1px solid var(--accent);
    background: rgba(61, 184, 168, 0.1);
    pointer-events: none;
    z-index: 10;
  }
  .empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: 12px;
    pointer-events: none;
  }
</style>
