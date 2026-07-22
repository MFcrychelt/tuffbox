<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";

  export let skinUrl: string | null = null;
  export let capeUrl: string | null = null;
  /** Forces a full reload when the active account changes. */
  export let accountKey: string = "";
  export let playerName: string = "";
  /** When false, hide the Minecraft nick under the canvas (parent shows it). */
  export let showName: boolean = true;
  export let width: number = 300;
  export let height: number = 400;

  let canvas: HTMLCanvasElement;
  let viewer: any = null;
  let loading = false;
  let lastSkin = "";
  let lastCape = "";
  let lastAccount = "";
  let capeFrames: HTMLCanvasElement[] = [];
  let capeFrameIdx = 0;
  let capeAnimTimer: ReturnType<typeof setInterval> | null = null;

  function stopCapeAnim() {
    if (capeAnimTimer) {
      clearInterval(capeAnimTimer);
      capeAnimTimer = null;
    }
    capeFrames = [];
    capeFrameIdx = 0;
  }

  function startCapeAnim(frames: HTMLCanvasElement[]) {
    stopCapeAnim();
    if (!viewer || frames.length === 0) return;
    capeFrames = frames;
    viewer.loadCape(frames[0]);
    if (frames.length < 2) return;
    capeAnimTimer = setInterval(() => {
      if (!viewer || capeFrames.length < 2) return;
      capeFrameIdx = (capeFrameIdx + 1) % capeFrames.length;
      try {
        viewer.loadCape(capeFrames[capeFrameIdx]);
      } catch (e) {
        console.warn("[SkinPreview3D] cape frame failed:", e);
      }
    }, 100);
  }

  async function initViewer() {
    if (!canvas) return;

    try {
      const { SkinViewer, WalkingAnimation } = await import("skinview3d");

      if (viewer) {
        stopCapeAnim();
        viewer.dispose();
        viewer = null;
      }

      viewer = new SkinViewer({
        canvas,
        width,
        height,
      });

      // Soft solid backdrop (composer clears are unreliable with CSS-underlay alpha).
      viewer.background = 0x1e2a3a;
      viewer.globalLight.intensity = 1.45;
      viewer.cameraLight.intensity = 1.05;

      viewer.camera.position.set(0, 0, 42);
      viewer.controls.enableRotate = true;
      viewer.controls.enableZoom = true;
      viewer.controls.enablePan = false;
      viewer.controls.rotateSpeed = 0.68;
      viewer.controls.minDistance = 24;
      viewer.controls.maxDistance = 88;
      viewer.controls.autoRotate = false;

      const walk = new WalkingAnimation();
      walk.headBobbing = true;
      walk.speed = 0.55;
      viewer.animation = walk;

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

  /**
   * skinview3d only accepts classic cape aspect ratios (64×32 etc).
   * TLauncher / OptiFine cloaks often ship as HD vertical atlases —
   * split into 64×32 frames so we can animate them.
   */
  async function extractCapeFrames(dataUrl: string): Promise<HTMLCanvasElement[]> {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const w = img.width;
        const h = img.height;

        // Classic single-frame (or exact 2:1) — pass through.
        if (w === 2 * h || (w === 64 && h === 32) || (w === 22 && h === 17) || (w === 46 && h === 22)) {
          const c = document.createElement("canvas");
          c.width = w;
          c.height = h;
          const ctx = c.getContext("2d");
          if (!ctx) {
            resolve([]);
            return;
          }
          ctx.imageSmoothingEnabled = false;
          ctx.drawImage(img, 0, 0);
          resolve([c]);
          return;
        }

        // Animated classic OptiFine: 64 × (32 * N)
        if (w === 64 && h > 32 && h % 32 === 0) {
          const frames: HTMLCanvasElement[] = [];
          const n = h / 32;
          for (let i = 0; i < n; i++) {
            const c = document.createElement("canvas");
            c.width = 64;
            c.height = 32;
            const ctx = c.getContext("2d");
            if (!ctx) continue;
            ctx.imageSmoothingEnabled = false;
            ctx.drawImage(img, 0, i * 32, 64, 32, 0, 0, 64, 32);
            frames.push(c);
          }
          resolve(frames.length ? frames : []);
          return;
        }

        // HD / TLauncher atlas: tall strip — pick frame height near w/2.
        let frameH = Math.max(1, Math.round(w / 2));
        if (h > w) {
          const candidates = [Math.round(w / 2), 32, 64, 17, 272, Math.floor(h / Math.max(1, Math.round(h / (w / 2))))];
          let best = frameH;
          let bestScore = Infinity;
          for (const c of candidates) {
            if (c <= 0 || h % c !== 0) continue;
            const score = Math.abs(w / c - 2);
            if (score < bestScore) {
              bestScore = score;
              best = c;
            }
          }
          // Also try dividing into a reasonable frame count (8–64).
          for (let n = 8; n <= 64; n++) {
            if (h % n !== 0) continue;
            const fh = h / n;
            const score = Math.abs(w / fh - 2);
            if (score < bestScore) {
              bestScore = score;
              best = fh;
            }
          }
          frameH = best;
        }

        const frameCount = Math.max(1, Math.floor(h / frameH));
        const frames: HTMLCanvasElement[] = [];
        const maxFrames = Math.min(frameCount, 64);
        for (let i = 0; i < maxFrames; i++) {
          const c = document.createElement("canvas");
          c.width = 64;
          c.height = 32;
          const ctx = c.getContext("2d");
          if (!ctx) continue;
          ctx.imageSmoothingEnabled = false;
          ctx.drawImage(img, 0, i * frameH, w, frameH, 0, 0, 64, 32);
          frames.push(c);
        }
        resolve(frames.length ? frames : []);
      };
      img.onerror = () => resolve([]);
      img.src = dataUrl;
    });
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
        stopCapeAnim();
        const raw = await toDataUrl(capeUrl);
        const frames = await extractCapeFrames(raw);
        if (frames.length) {
          startCapeAnim(frames);
          lastCape = capeUrl;
        } else {
          lastCape = "";
        }
      } else if (!capeUrl && lastCape) {
        stopCapeAnim();
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
      stopCapeAnim();
    }
    void applyTextures();
  }

  onMount(() => {
    initViewer();
  });

  onDestroy(() => {
    stopCapeAnim();
    if (viewer) {
      viewer.dispose();
      viewer = null;
    }
  });
</script>

<div class="skin-3d-wrap" style="width: {width}px;">
  <div class="skin-3d-container" style="width: {width}px; height: {height}px;">
    <div class="skin-bg" aria-hidden="true"></div>
    <canvas bind:this={canvas} width={width} height={height}></canvas>
    {#if loading}
      <div class="loading-overlay">
        <div class="loading-spinner"></div>
      </div>
    {/if}
  </div>
  {#if showName && playerName}
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
    border: 1px solid rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 0 40px rgba(0, 0, 0, 0.25);
  }

  .skin-bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(ellipse 80% 60% at 50% 20%, rgba(96, 165, 250, 0.18), transparent 55%),
      radial-gradient(ellipse 70% 50% at 50% 100%, rgba(27, 217, 106, 0.1), transparent 50%),
      linear-gradient(165deg, #243044 0%, #151c28 55%, #0f141c 100%);
    pointer-events: none;
  }

  canvas {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    background: transparent;
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
    background: rgba(15, 20, 28, 0.35);
    pointer-events: none;
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
