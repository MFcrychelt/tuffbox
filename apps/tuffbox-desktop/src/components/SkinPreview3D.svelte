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
        // Use skinview3d zoom (drives adjustCameraDistance). Manual
        // camera.position fights OrbitControls and breaks wheel zoom.
        zoom: 0.55,
        fov: 55,
      });

      // Transparent WebGL clear — CSS .skin-bg paints the backdrop.
      viewer.background = null;
      viewer.globalLight.intensity = 1.55;
      viewer.cameraLight.intensity = 1.15;

      viewer.controls.enableRotate = true;
      viewer.controls.enableZoom = true;
      viewer.controls.enablePan = false;
      viewer.controls.rotateSpeed = 0.7;
      viewer.controls.zoomSpeed = 1.2;
      // Keep library defaults (10..256) so dolly isn't clamped at mid-range.
      viewer.controls.minDistance = 10;
      viewer.controls.maxDistance = 256;
      viewer.controls.autoRotate = false;
      // Aim at torso so head + feet fit in frame.
      viewer.controls.target.set(0, -6, 0);
      viewer.controls.update();

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

  /** skinview-utils / loadCapeToCanvas accepted atlas ratios. */
  function isNativeCapeAtlas(w: number, h: number): boolean {
    return w === 2 * h || w * 17 === h * 22 || w * 11 === h * 23;
  }

  function copyImageToCanvas(
    img: CanvasImageSource,
    w: number,
    h: number,
    sx = 0,
    sy = 0,
    sw = w,
    sh = h,
  ): HTMLCanvasElement {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = c.getContext("2d");
    if (!ctx) return c;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, sx, sy, sw, sh, 0, 0, w, h);
    return c;
  }

  function scaleSliceTo64x32(
    img: CanvasImageSource,
    sx: number,
    sy: number,
    sw: number,
    sh: number,
  ): HTMLCanvasElement {
    return copyImageToCanvas(img, 64, 32, sx, sy, sw, sh);
  }

  /** Draw source into dest with aspect-preserving contain (letterbox). */
  function drawContained(
    ctx: CanvasRenderingContext2D,
    img: CanvasImageSource,
    sx: number,
    sy: number,
    sw: number,
    sh: number,
    dx: number,
    dy: number,
    dw: number,
    dh: number,
  ) {
    if (sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0) return;
    const srcAspect = sw / sh;
    const dstAspect = dw / dh;
    let ddx = dx;
    let ddy = dy;
    let ddw = dw;
    let ddh = dh;
    if (srcAspect > dstAspect) {
      ddh = dw / srcAspect;
      ddy = dy + (dh - ddh) / 2;
    } else {
      ddw = dh * srcAspect;
      ddx = dx + (dw - ddw) / 2;
    }
    ctx.drawImage(img, sx, sy, sw, sh, ddx, ddy, ddw, ddh);
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

    // Exact native atlas slice — copy / normalize without UV remapping.
    if (isNativeCapeAtlas(sw, sh)) {
      if (sw === 2 * sh) {
        return scaleSliceTo64x32(img, sx, sy, sw, sh);
      }
      // 22×17 / 46×22 family — keep original size for skinview-utils.
      return copyImageToCanvas(img, sw, sh, sx, sy, sw, sh);
    }

    // Near-2:1 atlas (non-integer dims) — scale whole frame to 64×32 as atlas.
    if (aspect >= 1.85 && aspect <= 2.15) {
      return scaleSliceTo64x32(img, sx, sy, sw, sh);
    }

    // Full-bleed / portrait cape art — pack into classic UV slots with contain
    // (letterbox) so 10:16 panels aren't stretched.
    return paintCapeUv((ctx, dx, dy, dw, dh) => {
      drawContained(ctx, img, sx, sy, sw, sh, dx, dy, dw, dh);
    });
  }

  /** Prefer frame heights where each slice is a native cape atlas. */
  function detectNativeStripFrameHeight(w: number, h: number): number | null {
    const candidates = new Set<number>();
    if (w % 2 === 0) candidates.add(w / 2); // 2:1
    const h22 = Math.round((w * 17) / 22);
    if (w * 17 === h22 * 22) candidates.add(h22);
    const h23 = Math.round((w * 11) / 23);
    if (w * 11 === h23 * 23) candidates.add(h23);
    // Also try exact divisors that satisfy native ratios.
    for (let n = 1; n <= Math.min(64, h); n++) {
      if (h % n !== 0) continue;
      const fh = h / n;
      if (isNativeCapeAtlas(w, fh)) candidates.add(fh);
    }
    let best: number | null = null;
    for (const fh of candidates) {
      if (fh <= 0 || h % fh !== 0) continue;
      if (!isNativeCapeAtlas(w, fh)) continue;
      if (best === null || fh < best) best = fh; // prefer more frames when tied
    }
    return best;
  }

  /**
   * skinview3d only accepts classic cape aspect ratios (64×32 etc).
   * TLauncher / OptiFine cloaks often ship as HD panels or vertical atlases —
   * normalize to UV-correct frames without stretching.
   */
  async function extractCapeFrames(dataUrl: string): Promise<HTMLCanvasElement[]> {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const w = img.width;
        const h = img.height;
        const aspect = w / Math.max(1, h);

        // Whole image is a native atlas — copy as-is (2:1 → 64×32).
        if (isNativeCapeAtlas(w, h)) {
          resolve([frameToClassicAtlas(img, 0, 0, w, h)]);
          return;
        }

        // Near-2:1 whole image that isn't exact integers — atlas scale, no UV paint.
        if (aspect >= 1.85 && aspect <= 2.15) {
          resolve([scaleSliceTo64x32(img, 0, 0, w, h)]);
          return;
        }

        // Tall animated strip of native atlas frames.
        if (h > w) {
          const nativeFh = detectNativeStripFrameHeight(w, h);
          if (nativeFh !== null) {
            const frames: HTMLCanvasElement[] = [];
            const n = Math.min(Math.floor(h / nativeFh), 48);
            for (let i = 0; i < n; i++) {
              frames.push(frameToClassicAtlas(img, 0, i * nativeFh, w, nativeFh));
            }
            resolve(frames.length ? frames : []);
            return;
          }

          // Classic 64×N strip of 32px frames.
          if (w === 64 && h % 32 === 0) {
            const frames: HTMLCanvasElement[] = [];
            const n = Math.min(h / 32, 48);
            for (let i = 0; i < n; i++) {
              frames.push(frameToClassicAtlas(img, 0, i * 32, 64, 32));
            }
            resolve(frames.length ? frames : []);
            return;
          }
        }

        // Portrait / full-bleed single cape art → UV slots with contain.
        if (aspect < 1.2) {
          resolve([frameToClassicAtlas(img, 0, 0, w, h)]);
          return;
        }

        // Fallback: treat as one frame (atlas-ish or UV pack via frameToClassicAtlas).
        resolve([frameToClassicAtlas(img, 0, 0, w, h)]);
      };
      img.onerror = () => resolve([]);
      img.src = dataUrl;
    });
  }

  function capeKey(url: string | null | undefined): string {
    return url ?? "";
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

      const nextCape = capeKey(capeUrl);
      if (nextCape && nextCape !== lastCape) {
        stopCapeAnim();
        const raw = await toDataUrl(nextCape);
        const frames = await extractCapeFrames(raw);
        if (frames.length) {
          startCapeAnim(frames);
          lastCape = nextCape;
        } else {
          stopCapeAnim();
          viewer.loadCape(null);
          lastCape = "";
        }
      } else if (!nextCape && lastCape) {
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

  $: if (
    viewer &&
    (skinUrl !== lastSkin || capeKey(capeUrl) !== lastCape || accountKey !== lastAccount)
  ) {
    if (accountKey !== lastAccount) {
      lastAccount = accountKey;
      lastSkin = "";
      // Force cape clear branch even when previous account also had no cape.
      lastCape = lastCape || "__pending_clear__";
      stopCapeAnim();
      try {
        viewer.loadCape(null);
      } catch {
        /* ignore */
      }
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
  <!-- stopPropagation keeps the page from stealing wheel; OrbitControls still gets the event on canvas -->
  <div
    class="skin-3d-container"
    style="width: {width}px; height: {height}px;"
    on:wheel|stopPropagation
  >
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
    touch-action: none;
    overscroll-behavior: contain;
  }

  .skin-bg {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(ellipse 55% 42% at 50% 38%, rgba(210, 195, 170, 0.14), transparent 62%),
      radial-gradient(ellipse 80% 50% at 50% 100%, rgba(0, 0, 0, 0.55), transparent 58%),
      radial-gradient(ellipse 100% 80% at 50% 50%, transparent 40%, rgba(0, 0, 0, 0.45) 100%),
      linear-gradient(180deg, #2a2c30 0%, #1a1c1f 52%, #121314 100%);
    pointer-events: none;
  }

  canvas {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    background: transparent;
    touch-action: none;
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
    background: rgba(18, 20, 22, 0.28);
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
