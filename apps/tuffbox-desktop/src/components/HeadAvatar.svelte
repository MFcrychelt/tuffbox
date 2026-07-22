<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { User } from "lucide-svelte";

  /** Local filesystem path to a skin PNG, or a remote/data URL. */
  export let skinSrc: string | null = null;
  /** When true, treat skinSrc as a local path and convert via convertFileSrc. */
  export let isLocalPath: boolean = true;
  export let size: number = 32;
  export let alt: string = "";

  let canvas: HTMLCanvasElement;
  let ready = false;
  let gen = 0;

  async function drawHead() {
    const myGen = ++gen;
    ready = false;
    if (!canvas || !skinSrc) return;

    const url = isLocalPath ? convertFileSrc(skinSrc) : skinSrc;
    const img = new Image();
    img.decoding = "async";
    try {
      await new Promise<void>((resolve, reject) => {
        img.onload = () => resolve();
        img.onerror = () => reject(new Error("skin load failed"));
        img.src = url;
      });
    } catch {
      if (myGen === gen) ready = false;
      return;
    }
    if (myGen !== gen || !canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const scale = Math.max(1, Math.floor(img.width / 64));
    const s = 8 * scale;
    const faceX = 8 * scale;
    const faceY = 8 * scale;
    const hatX = 40 * scale;
    const hatY = 8 * scale;

    canvas.width = size;
    canvas.height = size;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, size, size);
    ctx.drawImage(img, faceX, faceY, s, s, 0, 0, size, size);
    ctx.drawImage(img, hatX, hatY, s, s, 0, 0, size, size);
    ready = true;
  }

  $: if (skinSrc !== undefined) {
    void drawHead();
  }

  onMount(() => {
    void drawHead();
  });

  onDestroy(() => {
    gen++;
  });
</script>

<div class="head-wrap" style="width: {size}px; height: {size}px;" title={alt}>
  <canvas
    bind:this={canvas}
    class="head-avatar"
    class:ready
    width={size}
    height={size}
    aria-hidden={!ready}
  ></canvas>
  {#if !ready}
    <div class="head-fallback" aria-hidden="true">
      <User size={Math.max(12, Math.floor(size * 0.55))} />
    </div>
  {/if}
</div>

<style>
  .head-wrap {
    position: relative;
    flex-shrink: 0;
    border-radius: 6px;
    overflow: hidden;
  }

  .head-avatar {
    display: block;
    width: 100%;
    height: 100%;
    image-rendering: pixelated;
    opacity: 0;
  }

  .head-avatar.ready {
    opacity: 1;
  }

  .head-fallback {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    color: var(--text-muted);
  }
</style>
