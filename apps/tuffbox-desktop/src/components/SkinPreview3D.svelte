<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";

  export let skinUrl: string | null = null;
  export let width: number = 300;
  export let height: number = 400;

  let canvas: HTMLCanvasElement;
  let viewer: any = null;
  let loading = false;
  let animFrame: number | null = null;

  async function initViewer() {
    if (!canvas) return;

    try {
      const { SkinViewer, IdleAnimation, WalkingAnimation, RunningAnimation } = await import("skinview3d");

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
      viewer.headFollowCursor = true;

      // Set up camera
      viewer.camera.position.set(0, 0, 42);
      viewer.controls.enableRotate = true;
      viewer.controls.enableZoom = true;
      viewer.controls.enablePan = false;
      viewer.controls.rotateSpeed = 0.68;
      viewer.controls.minDistance = 24;
      viewer.controls.maxDistance = 88;
      viewer.controls.autoRotate = false;

      // Load skin
      if (skinUrl) {
        await loadSkin(skinUrl);
      }
    } catch (e) {
      console.error("[SkinPreview3D] init failed:", e);
    }
  }

  async function loadSkin(url: string) {
    if (!viewer || !url) return;
    loading = true;
    try {
      // Fetch as base64 to avoid CORS issues
      const dataUrl = await api.mcAuth.getSkinBase64(url);
      await viewer.loadSkin(dataUrl, { model: "auto-detect" });
    } catch (e) {
      console.error("[SkinPreview3D] load skin failed:", e);
    } finally {
      loading = false;
    }
  }

  $: if (viewer && skinUrl) {
    loadSkin(skinUrl);
  }

  onMount(() => {
    initViewer();
  });

  onDestroy(() => {
    if (animFrame) cancelAnimationFrame(animFrame);
    if (viewer) {
      viewer.dispose();
      viewer = null;
    }
  });
</script>

<div class="skin-3d-container" style="width: {width}px; height: {height}px;">
  <canvas bind:this={canvas} width={width} height={height}></canvas>
  {#if loading}
    <div class="loading-overlay">
      <div class="loading-spinner"></div>
    </div>
  {/if}
</div>

<style>
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

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
