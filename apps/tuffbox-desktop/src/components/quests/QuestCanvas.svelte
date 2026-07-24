<script lang="ts">
  import { onMount, tick } from "svelte";
  import { Maximize2, Plus } from "lucide-svelte";
  import type { QuestData, QuestValidationIssue, QuestProgressStatus } from "../../lib/api";

  export let quests: QuestData[];
  export let selectedId: string | null = null;
  export let issues: QuestValidationIssue[] = [];
  /** questId → progress status when overlay is on */
  export let progressStatuses: Record<string, QuestProgressStatus> = {};
  export let progressOverlay = false;
  export let onSelect: (q: QuestData | null) => void;
  export let onMove: (q: QuestData, x: number, y: number) => void;
  export let onAddAt: (x: number, y: number) => void;
  export let onLink: (fromId: string, toDepId: string) => void;
  /** Bump to force fitView (e.g. chapter change). */
  export let fitToken = 0;

  const BASE = 24;
  const MIN_ZOOM = 0.25;
  const MAX_ZOOM = 4;

  let viewport: HTMLDivElement;
  let zoom = 1;
  let panX = 0;
  let panY = 0;

  let mode: "idle" | "pan" | "drag" | "link" = "idle";
  let panLast: { x: number; y: number } | null = null;
  let dragQuest: QuestData | null = null;
  let dragMoved = false;
  let linkFrom: QuestData | null = null;
  let linkCursor: { x: number; y: number } | null = null;
  let spaceDown = false;
  let lastFitToken = -1;

  $: unit = BASE * zoom;
  $: issueIds = new Set(issues.map((i) => i.questId));
  $: if (fitToken !== lastFitToken && quests) {
    lastFitToken = fitToken;
    void refit();
  }

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
    const rect = viewport.getBoundingClientRect();
    return {
      x: (clientX - rect.left - panX) / unit,
      y: (clientY - rect.top - panY) / unit,
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
    let minX = Infinity,
      minY = Infinity,
      maxX = -Infinity,
      maxY = -Infinity;
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
    const rect = viewport.getBoundingClientRect();
    const sx = clientX - rect.left;
    const sy = clientY - rect.top;
    // Top-most hit (reverse paint order) — icon + label area
    for (let i = quests.length - 1; i >= 0; i--) {
      const q = quests[i];
      const p = screenPos(q);
      const labelH = Math.max(14, p.size * 0.35);
      if (sx >= p.left && sx <= p.left + p.size && sy >= p.top && sy <= p.top + p.size + labelH + 4) {
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
        onSelect(hit);
        viewport.setPointerCapture(e.pointerId);
        e.preventDefault();
        return;
      }
      mode = "drag";
      dragQuest = hit;
      dragMoved = false;
      onSelect(hit);
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
      onMove(dragQuest, w.x, w.y);
      return;
    }
    if (mode === "link" && linkFrom) {
      linkCursor = clientToWorld(e.clientX, e.clientY);
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (mode === "drag" && dragQuest) {
      if (dragMoved) {
        onMove(dragQuest, snap(dragQuest.x), snap(dragQuest.y));
      }
    }
    if (mode === "link" && linkFrom) {
      const hit = questAt(e.clientX, e.clientY);
      if (hit && hit.id !== linkFrom.id) {
        onLink(linkFrom.id, hit.id);
      }
    }
    mode = "idle";
    panLast = null;
    dragQuest = null;
    dragMoved = false;
    linkFrom = null;
    linkCursor = null;
    try {
      viewport.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  }

  function onWheel(e: WheelEvent) {
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
    const direct = quests.find((q) => q.id === depId);
    if (direct) return direct;
    // FTB: dependency may be a task id — resolve to owning quest in this chapter.
    return (
      quests.find((q) => q.tasks?.some((t) => t.id === depId)) ?? null
    );
  }

  function glyph(q: QuestData) {
    const icon = q.icon?.trim();
    if (icon) {
      const leaf = icon.includes(":") ? icon.split(":").pop()! : icon;
      return (leaf[0] || "?").toUpperCase();
    }
    return (q.title[0] || "?").toUpperCase();
  }

  function progressOf(q: QuestData): QuestProgressStatus | null {
    if (!progressOverlay) return null;
    return progressStatuses[q.id] ?? "unknown";
  }

  function nodeShape(q: QuestData): string {
    const s = q.shape?.trim();
    if (s && s !== "none") return s;
    return "rsquare";
  }

  function shapeClass(q: QuestData): string {
    return `shape-${nodeShape(q)}`;
  }
</script>

<div class="canvas-wrap ftbq-canvas">
  <div class="canvas-toolbar">
    <button type="button" class="tb" title="Fit view" on:click={fitView}><Maximize2 size={14} /> Fit</button>
    <button
      type="button"
      class="tb"
      title="Add quest at center"
      on:click={() => {
        const rect = viewport?.getBoundingClientRect();
        if (!rect) {
          onAddAt(0, 0);
          return;
        }
        const w = clientToWorld(rect.left + rect.width / 2, rect.top + rect.height / 2);
        onAddAt(snap(w.x), snap(w.y));
      }}
    >
      <Plus size={14} /> Add quest
    </button>
    <span class="hint">Drag · Space/MMB pan · Wheel zoom · Shift+drag link · Dbl-click add</span>
  </div>

  <div
    class="viewport"
    class:panning={mode === "pan" || spaceDown}
    class:linking={mode === "link"}
    bind:this={viewport}
    on:pointerdown={onPointerDown}
    on:pointermove={onPointerMove}
    on:pointerup={onPointerUp}
    on:pointercancel={onPointerUp}
    on:wheel={onWheel}
    on:dblclick={onDblClick}
    role="application"
    aria-label="Quest canvas"
  >
    <svg class="edges" width="100%" height="100%">
      {#each quests as q (q.id)}
        {#each q.dependencies as depId}
          {@const target = depTarget(depId)}
          {@const from = centerOf(q)}
          {#if target}
            {@const to = centerOf(target)}
            {@const tProg = progressOf(target)}
            <line
              x1={from.x}
              y1={from.y}
              x2={to.x}
              y2={to.y}
              class="dep"
              class:broken={false}
              class:dep-done={progressOverlay && tProg === "completed"}
            />
          {:else}
            <line
              x1={from.x}
              y1={from.y}
              x2={from.x + 40}
              y2={from.y - 30}
              class="dep broken"
            />
          {/if}
        {/each}
      {/each}
      {#if mode === "link" && linkFrom && linkCursor}
        {@const from = centerOf(linkFrom)}
        <line
          x1={from.x}
          y1={from.y}
          x2={panX + linkCursor.x * unit}
          y2={panY + linkCursor.y * unit}
          class="dep link-preview"
        />
      {/if}
    </svg>

    {#each quests as q (q.id)}
      {@const p = screenPos(q)}
      {@const prog = progressOf(q)}
      <div
        class="node-wrap"
        class:sel={selectedId === q.id}
        class:issue={issueIds.has(q.id)}
        style={`left:${p.left}px; top:${p.top}px; width:${p.size}px;`}
        title={q.title}
      >
        <div
          class="node-icon {shapeClass(q)}"
          class:optional={q.optional}
          class:prog-completed={prog === "completed"}
          class:prog-started={prog === "started"}
          class:prog-available={prog === "available"}
          class:prog-locked={prog === "locked"}
          style={`width:${p.size}px; height:${p.size}px;`}
        >
          <span class="node-glyph">{glyph(q)}</span>
          {#if q.optional}<span class="opt">?</span>{/if}
          {#if prog === "completed"}<span class="check" title="Completed">✓</span>{/if}
        </div>
        <span class="node-label">{q.title}</span>
      </div>
    {/each}

    {#if quests.length === 0}
      <div class="empty-hint">Double-click to add a quest</div>
    {/if}
  </div>
</div>

<style>
  .canvas-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--ftbq-bg-canvas, #2b2b30);
    border: none;
    border-left: 1px solid var(--ftbq-border, #3a3a42);
    border-right: 1px solid var(--ftbq-border, #3a3a42);
    overflow: hidden;
  }
  .canvas-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg-panel, #212126);
    flex-shrink: 0;
  }
  .tb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text, #e8e8e8);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
  }
  .tb:hover {
    border-color: var(--ftbq-accent-teal, #3db8a8);
    background: rgba(61, 184, 168, 0.12);
    color: var(--ftbq-text, #e8e8e8);
  }
  .hint {
    margin-left: auto;
    font-size: 9px;
    color: var(--ftbq-text-muted, #9a9aa0);
    letter-spacing: 0.02em;
  }
  .viewport {
    position: relative;
    flex: 1;
    min-height: 360px;
    overflow: hidden;
    cursor: default;
    background-color: var(--ftbq-bg-canvas, #2b2b30);
    background-image:
      repeating-linear-gradient(
        0deg,
        transparent,
        transparent 15px,
        rgba(255, 255, 255, 0.03) 15px,
        rgba(255, 255, 255, 0.03) 16px
      ),
      repeating-linear-gradient(
        90deg,
        transparent,
        transparent 15px,
        rgba(255, 255, 255, 0.03) 15px,
        rgba(255, 255, 255, 0.03) 16px
      );
    touch-action: none;
    user-select: none;
  }
  .viewport.panning {
    cursor: grabbing;
  }
  .viewport.linking {
    cursor: crosshair;
  }
  .edges {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 1;
  }
  .dep {
    stroke: var(--ftbq-line, #5c8a9e);
    stroke-width: 3;
    stroke-linecap: round;
  }
  .dep.dep-done {
    stroke: var(--ftbq-line-done, #55c95a);
    stroke-width: 3.5;
  }
  .dep.broken {
    stroke: var(--ftbq-quest-started, #f2c94c);
    stroke-dasharray: 6 4;
    stroke-width: 2.5;
  }
  .dep.link-preview {
    stroke: var(--ftbq-accent-teal, #3db8a8);
    stroke-dasharray: 6 4;
    stroke-width: 2.5;
    opacity: 0.85;
  }
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
  .node-wrap.sel .node-icon {
    box-shadow:
      0 0 0 2px rgba(61, 184, 168, 0.5),
      0 0 12px rgba(85, 201, 90, 0.35);
  }
  .node-wrap.issue .node-icon {
    border-color: var(--ftbq-quest-started, #f2c94c);
  }
  .node-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 2px solid var(--ftbq-quest-default, #ffffff);
    background: var(--ftbq-node-fill, #18181c);
    color: var(--ftbq-text, #e8e8e8);
    box-shadow:
      inset 0 2px 6px rgba(0, 0, 0, 0.5),
      0 1px 3px rgba(0, 0, 0, 0.4);
  }
  /* FTB quest shapes */
  .node-icon.shape-circle {
    border-radius: 50%;
  }
  .node-icon.shape-square {
    border-radius: 0;
  }
  .node-icon.shape-rsquare {
    border-radius: 4px;
  }
  .node-icon.shape-diamond {
    border-radius: 0;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
  }
  .node-icon.shape-hexagon {
    clip-path: polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%);
    border-radius: 0;
  }
  .node-icon.shape-pentagon {
    clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%);
    border-radius: 0;
  }
  .node-icon.shape-gear {
    border-radius: 3px;
    clip-path: polygon(
      50% 0%,
      61% 8%,
      75% 4%,
      82% 18%,
      96% 25%,
      92% 39%,
      100% 50%,
      92% 61%,
      96% 75%,
      82% 82%,
      75% 96%,
      61% 92%,
      50% 100%,
      39% 92%,
      25% 96%,
      18% 82%,
      4% 75%,
      8% 61%,
      0% 50%,
      8% 39%,
      4% 25%,
      18% 18%,
      25% 4%,
      39% 8%
    );
  }
  .node-icon.optional {
    border-style: dashed;
  }
  .node-glyph {
    font-size: clamp(10px, 45%, 20px);
    font-weight: 800;
    color: var(--ftbq-text, #e8e8e8);
    line-height: 1;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }
  .node-label {
    font-size: clamp(8px, 10px, 11px);
    line-height: 1.15;
    max-width: calc(100% + 24px);
    min-width: 100%;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ftbq-text-muted, #9a9aa0);
    pointer-events: none;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
  }
  .node-wrap.sel .node-label {
    color: var(--ftbq-text, #e8e8e8);
  }
  .opt {
    position: absolute;
    top: -3px;
    right: -3px;
    font-size: 9px;
    color: var(--ftbq-quest-started, #f2c94c);
    font-weight: 900;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.8);
  }
  .check {
    position: absolute;
    bottom: -4px;
    right: -4px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--ftbq-quest-completed, #55c95a);
    color: #0a1a0c;
    font-size: 10px;
    font-weight: 900;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.5);
  }
  /* Progress overlay border colors */
  .node-icon.prog-completed {
    border-color: var(--ftbq-quest-completed, #55c95a);
  }
  .node-icon.prog-started {
    border-color: var(--ftbq-quest-started, #f2c94c);
  }
  .node-icon.prog-available {
    border-color: var(--ftbq-quest-default, #ffffff);
  }
  .node-icon.prog-locked {
    border-color: var(--ftbq-quest-locked, #6b6b6b);
    opacity: 0.55;
    filter: grayscale(0.4);
  }
  .empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 12px;
    pointer-events: none;
    z-index: 0;
  }
</style>
