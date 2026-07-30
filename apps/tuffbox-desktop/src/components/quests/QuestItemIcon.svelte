<script lang="ts">
  import { projectPath } from "../../lib/store";
  import {
    getCachedIcon,
    glyphFromItemId,
    normalizeItemId,
    preloadItemIcons,
  } from "./iconCache";

  export let itemId: string | null | undefined = null;
  export let fallback = "?";
  export let size = 24;
  /** When parent bumps this after preload, re-read cache. */
  export let revision = 0;

  let src: string | null | undefined = undefined;

  $: normalized = normalizeItemId(itemId);
  $: letter = glyphFromItemId(normalized, fallback);
  $: {
    void revision;
    src = readSrc(normalized);
  }
  $: if (normalized && $projectPath && src === undefined) {
    void loadOne(normalized);
  }

  function readSrc(id: string | null): string | null | undefined {
    return getCachedIcon(id);
  }

  async function loadOne(id: string) {
    if (!$projectPath) return;
    await preloadItemIcons([id], $projectPath);
    src = getCachedIcon(id) ?? null;
  }
</script>

{#if src}
  <img
    class="qii"
    src={src}
    alt=""
    width={size}
    height={size}
    style={`width:${size}px;height:${size}px`}
  />
{:else}
  <span class="qii-ph" style={`width:${size}px;height:${size}px;font-size:${Math.max(9, size * 0.45)}px`}
    >{letter}</span
  >
{/if}

<style>
  .qii {
    image-rendering: pixelated;
    image-rendering: crisp-edges;
    object-fit: contain;
    display: block;
    flex-shrink: 0;
    pointer-events: none;
  }
  .qii-ph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    font-weight: 800;
    color: var(--ftbq-text, #e8e8e8);
    line-height: 1;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    pointer-events: none;
  }
</style>
