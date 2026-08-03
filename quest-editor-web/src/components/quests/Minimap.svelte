<script lang="ts">
  import type { QuestData } from "../../lib/store";

  let {
    quests,
    zoom,
    panX,
    panY,
    viewportWidth,
    viewportHeight,
    base = 24,
  }: {
    quests: QuestData[];
    zoom: number;
    panX: number;
    panY: number;
    viewportWidth: number;
    viewportHeight: number;
    base?: number;
  } = $props();

  const MAP_SIZE = 160;
  const MAP_PADDING = 10;

  let mapBounds = $derived(computeBounds(quests, base));
  let scale = $derived(computeScale(mapBounds, viewportWidth, viewportHeight));

  function computeBounds(quests: QuestData[], base: number) {
    if (quests.length === 0) {
      return { minX: 0, minY: 0, maxX: 10, maxY: 10 };
    }
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const q of quests) {
      const half = (q.size && q.size > 0 ? q.size : 1) / 2;
      minX = Math.min(minX, q.x - half);
      minY = Math.min(minY, q.y - half);
      maxX = Math.max(maxX, q.x + half);
      maxY = Math.max(maxY, q.y + half);
    }
    const pad = 2;
    return { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
  }

  function computeScale(bounds: { minX: number; minY: number; maxX: number; maxY: number }, vw: number, vh: number) {
    const worldW = bounds.maxX - bounds.minX;
    const worldH = bounds.maxY - bounds.minY;
    if (worldW <= 0 || worldH <= 0) return 1;
    return (MAP_SIZE - MAP_PADDING * 2) / Math.max(worldW, worldH);
  }

  function questToMap(q: QuestData) {
    const x = (q.x - mapBounds.minX) * scale + MAP_PADDING;
    const y = (q.y - mapBounds.minY) * scale + MAP_PADDING;
    const size = Math.max(3, (q.size && q.size > 0 ? q.size : 1) * scale * 0.8);
    return { x, y, size };
  }

  function viewportRect() {
    const worldW = mapBounds.maxX - mapBounds.minX;
    const worldH = mapBounds.maxY - mapBounds.minY;
    const viewWorldW = (viewportWidth / base / zoom);
    const viewWorldH = (viewportHeight / base / zoom);
    const viewWorldX = -panX / base / zoom;
    const viewWorldY = -panY / base / zoom;

    const x = (viewWorldX - mapBounds.minX) * scale + MAP_PADDING;
    const y = (viewWorldY - mapBounds.minY) * scale + MAP_PADDING;
    const w = viewWorldW * scale;
    const h = viewWorldH * scale;

    return { x, y, w, h };
  }
</script>

<div class="minimap">
  <svg width={MAP_SIZE} height={MAP_SIZE}>
    <!-- Background -->
    <rect width={MAP_SIZE} height={MAP_SIZE} fill="rgba(0,0,0,0.3)" rx="4" />

    <!-- Quest dots -->
    {#each quests as q (q.id)}
      {@const p = questToMap(q)}
      <rect
        x={p.x - p.size / 2}
        y={p.y - p.size / 2}
        width={p.size}
        height={p.size}
        fill="rgba(61,184,168,0.6)"
        rx="1"
      />
    {/each}

    <!-- Viewport indicator -->
    {#if viewportWidth > 0 && viewportHeight > 0}
      {@const vp = viewportRect()}
      <rect
        x={vp.x}
        y={vp.y}
        width={vp.w}
        height={vp.h}
        fill="none"
        stroke="rgba(255,255,255,0.5)"
        stroke-width="1"
        rx="1"
      />
    {/if}
  </svg>
</div>

<style>
  .minimap {
    position: absolute;
    bottom: 12px;
    right: 12px;
    z-index: 20;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
    box-shadow: 0 2px 8px rgba(0,0,0,0.4);
    opacity: 0.8;
    transition: opacity 0.2s;
  }
  .minimap:hover {
    opacity: 1;
  }
</style>
