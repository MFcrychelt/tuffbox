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

      // Match CSS backdrop; keep lights bright so the model isn't muddy.
      viewer.background = 0x1c2433;
      viewer.globalLight.intensity = 1.55;
      viewer.cameraLight.intensity = 1.15;

      viewer.camera.position.set(-8, 4, 38);
      viewer.controls.enableRotate = true;
      viewer.controls.enableZoom = true;
      viewer.controls.enablePan = false;
      viewer.controls.rotateSpeed = 0.68;
      viewer.controls.minDistance = 24;
      viewer.controls.maxDistance = 88;
      viewer.controls.autoRotate = false;
      viewer.controls.target.set(0, -4, 0);

      const walk = new WalkingAnimation();
      walk.headBobbing = true;
      walk.speed = 0.5;
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

  /** Classic Minecraft cape UV: front (1,1) 10×16, back (12,1) 10×16. */
  function paintCapeUv(
    draw: (ctx: CanvasRenderingContext2D, dx: number, dy: number, dw: number, dh: number) => void,
  ): HTMLCanvasElement {
    const c = document.createElement("canvas");
    c.width = 64;
    c.height = 32;
    const ctx = c.getContext("2d");
    if (!ctx) return c;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, 64, 32);
    draw(ctx, 1, 1, 10, 16);
    draw(ctx, 12, 1, 10, 16);
    // Thin side/edge strips so the box isn't empty.
    draw(ctx, 0, 1, 1, 16);
    draw(ctx, 11, 1, 1, 16);
    return c;
  }

  function frameToClassicAtlas(
    img: CanvasImageSource,
    sx: number,
    sy: number,
    sw: number,
    sh: number,
  ): HTMLCanvasElement {
    const aspect = sw / Math.max(1, sh);
    // OptiFine / Mojang atlas slice (~2:1) — keep layout, just scale to 64×32.
    if (aspect >= 1.6 && aspect <= 2.4) {
      const c = document.createElement("canvas");
      c.width = 64;
      c.height = 32;
      const ctx = c.getContext("2d");
      if (!ctx) return c;
      ctx.imageSmoothingEnabled = false;
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, 64, 32);
      return c;
    }
    // Full-bleed cape art (common in TLauncher) — place into UV slots so the
    // mesh doesn't sample a magnified corner of a stretched atlas.
    return paintCapeUv((ctx, dx, dy, dw, dh) => {
      ctx.drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh);
    });
  }

  /**
   * skinview3d only accepts classic cape aspect ratios (64×32 etc).
   * TLauncher / OptiFine cloaks often ship as HD vertical atlases —
   * split into correctly UV-mapped 64×32 frames.
   */
  async function extractCapeFrames(dataUrl: string): Promise<HTMLCanvasElement[]> {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const w = img.width;
        const h = img.height;

        if (
          w === 2 * h ||
          (w === 64 && h === 32) ||
          (w === 22 && h === 17) ||
          (w === 46 && h === 22)
        ) {
          resolve([frameToClassicAtlas(img, 0, 0, w, h)]);
          return;
        }

        if (w === 64 && h > 32 && h % 32 === 0) {
          const frames: HTMLCanvasElement[] = [];
          const n = h / 32;
          for (let i = 0; i < n; i++) {
            frames.push(frameToClassicAtlas(img, 0, i * 32, 64, 32));
          }
          resolve(frames.length ? frames : []);
          return;
        }

        // HD / TLauncher atlas: tall strip — prefer frame aspect near 2:1 (OF),
        // else near 10:16 (full-bleed cape panel).
        let frameH = Math.max(1, Math.round(w / 2));
        if (h > w) {
          let best = frameH;
          let bestScore = Infinity;
          const tryH = (fh: number) => {
            if (fh <= 0 || h % fh !== 0) return;
            const a = w / fh;
            const score = Math.min(Math.abs(a - 2), Math.abs(a - 10 / 16) * 2);
            if (score < bestScore) {
              bestScore = score;
              best = fh;
            }
          };
          for (const c of [Math.round(w / 2), Math.round((w * 16) / 10), 32, 64, 17, 272]) {
            tryH(c);
          }
          for (let n = 4; n <= 64; n++) {
            if (h % n === 0) tryH(h / n);
          }
          frameH = best;
        }

        const frameCount = Math.max(1, Math.floor(h / frameH));
        const frames: HTMLCanvasElement[] = [];
        const maxFrames = Math.min(frameCount, 48);
        for (let i = 0; i < maxFrames; i++) {
          frames.push(frameToClassicAtlas(img, 0, i * frameH, w, frameH));
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
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow:
      inset 0 -40px 60px rgba(0, 0, 0, 0.35),
      0 12px 28px rgba(0, 0, 0, 0.35);
  }

  .skin-bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(ellipse 90% 55% at 50% 18%, rgba(125, 180, 255, 0.22), transparent 58%),
      radial-gradient(ellipse 70% 45% at 50% 100%, rgba(27, 217, 106, 0.12), transparent 55%),
      linear-gradient(165deg, #2a3648 0%, #1c2433 48%, #121820 100%);
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
    background: rgba(18, 24, 32, 0.28);
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
