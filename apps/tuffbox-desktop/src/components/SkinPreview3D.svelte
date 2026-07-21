<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";

  export let skinUrl: string | null = null;
  export let capeUrl: string | null = null;
  /** Forces a full reload when the active account changes. */
  export let accountKey: string = "";
  export let playerName: string = "";
  export let width: number = 300;
  export let height: number = 400;

  let canvas: HTMLCanvasElement;
  let viewer: any = null;
  let loading = false;
  let lastSkin = "";
  let lastCape = "";
  let lastAccount = "";

  async function initViewer() {
    if (!canvas) return;

    try {
      const { SkinViewer } = await import("skinview3d");

      if (viewer) {
        viewer.dispose();
        viewer = null;
      }

      viewer = new SkinViewer({
        canvas,
        width,
        height,
      });

      viewer.background = null;
      viewer.globalLight.intensity = 0.85;
      viewer.cameraLight.intensity = 0.55;

      viewer.camera.position.set(0, 0, 42);
      viewer.controls.enableRotate = true;
      viewer.controls.enableZoom = true;
      viewer.controls.enablePan = false;
      viewer.controls.rotateSpeed = 0.68;
      viewer.controls.minDistance = 24;
      viewer.controls.maxDistance = 88;
      viewer.controls.autoRotate = false;

      lastSkin = "";
      lastCape = "";
      await applyTextures();
    } catch (e) {
      console.error("[SkinPreview3D] init failed:", e);
    }
  }

  async function toDataUrl(url: string): Promise<string> {
    return api.mcAuth.getSkinBase64(url);
  }

  async function applyTextures() {
    if (!viewer) return;
    loading = true;
    try {
      if (skinUrl && skinUrl !== lastSkin) {
        const dataUrl = await toDataUrl(skinUrl);
        await viewer.loadSkin(dataUrl, { model: "auto-detect" });
        lastSkin = skinUrl;
      } else if (!skinUrl && lastSkin) {
        // Keep previous skin if URL cleared briefly during switch.
      }

      if (capeUrl && capeUrl !== lastCape) {
        const dataUrl = await toDataUrl(capeUrl);
        await viewer.loadCape(dataUrl);
        lastCape = capeUrl;
      } else if (!capeUrl && lastCape) {
        viewer.loadCape(null);
        lastCape = "";
      }
    } catch (e) {
      console.error("[SkinPreview3D] load textures failed:", e);
    } finally {
      loading = false;
    }
  }

  $: if (viewer && (skinUrl !== lastSkin || capeUrl !== lastCape || accountKey !== lastAccount)) {
    if (accountKey !== lastAccount) {
      lastAccount = accountKey;
      lastSkin = "";
      lastCape = "";
    }
    void applyTextures();
  }

  onMount(() => {
    initViewer();
  });

  onDestroy(() => {
    if (viewer) {
      viewer.dispose();
      viewer = null;
    }
  });
</script>

<div class="skin-3d-wrap" style="width: {width}px;">
  <div class="skin-3d-container" style="width: {width}px; height: {height}px;">
    <canvas bind:this={canvas} width={width} height={height}></canvas>
    {#if loading}
      <div class="loading-overlay">
        <div class="loading-spinner"></div>
      </div>
    {/if}
  </div>
  {#if playerName}
    <div class="mc-nick" title={playerName}>{playerName}</div>
  {/if}
</div>

<style>
  .skin-3d-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .skin-3d-container {
    position: relative;
    border-radius: var(--border-radius-lg);
    overflow: hidden;
    background: linear-gradient(180deg, rgba(139, 92, 246, 0.06) 0%, rgba(27, 217, 106, 0.04) 100%);
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
  }

  canvas:active {
    cursor: grabbing;
  }

  .loading-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(2px);
  }

  .loading-spinner {
    width: 24px;
    height: 24px;
    border: 3px solid rgba(255, 255, 255, 0.15);
    border-top-color: var(--accent-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .mc-nick {
    font-family: var(--font-minecraft);
    font-size: 12px;
    line-height: 1.4;
    color: #fff;
    text-shadow:
      2px 2px 0 #3f3f3f,
      -1px 0 0 #000,
      1px 0 0 #000,
      0 -1px 0 #000,
      0 1px 0 #000;
    letter-spacing: 0.5px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 0 8px;
    text-align: center;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
