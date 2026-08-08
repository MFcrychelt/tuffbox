<script lang="ts">
  import { api } from "../lib/api";

  let {
    skinUrl = null,
    capeUrl = null,
    accountKey = "",
    playerName = "",
    showName = true,
    width = 300,
    height = 400,
  }: {
    skinUrl?: string | null;
    capeUrl?: string | null;
    accountKey?: string;
    playerName?: string;
    showName?: boolean;
    width?: number;
    height?: number;
  } = $props();

  let canvas = $state<HTMLCanvasElement | undefined>(undefined);
  // Plain fields — must not be $state. Reading+writing them inside $effect
  // (Svelte 5 tracks sync reads in callees) causes effect_update_depth_exceeded.
  let viewer: any = null;
  let loading = $state(false);
  let loadError = $state("");
  let lastSkin = "";
  let lastCape = "";
  let lastAccount = "";
  let capeFrames: HTMLCanvasElement[] = [];
  let capeFrameIdx = 0;
  let capeAnimTimer: ReturnType<typeof setInterval> | null = null;
  let initGen = 0;

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
    const myGen = ++initGen;
    loadError = "";

    try {
      const { SkinViewer, WalkingAnimation } = await import("skinview3d");
      if (myGen !== initGen || !canvas) return;

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
        zoom: 0.72,
        fov: 50,
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
      // Aim at mid-torso so the model fills the frame.
      viewer.controls.target.set(0, -4, 0);
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
      loadError = String(e);
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

  /** Draw source into dest stretched to fill (Minecraft cape faces are filled panels). */
  function drawFilled(
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
    ctx.drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh);
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

    // Full-bleed / portrait cape art — pack into classic UV face slots.
    return paintCapeUv((ctx, dx, dy, dw, dh) => {
      drawFilled(ctx, img, sx, sy, sw, sh, dx, dy, dw, dh);
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

        // OptiFine-style animated strip: exactly 64×(32·N), N≥2.
        // Checked before portrait — 64×320 has aspect 0.2 but is a frame strip.
        if (w === 64 && h >= 64 && h % 32 === 0) {
          const frames: HTMLCanvasElement[] = [];
          const n = Math.min(h / 32, 48);
          for (let i = 0; i < n; i++) {
            frames.push(frameToClassicAtlas(img, 0, i * 32, 64, 32));
          }
          resolve(frames.length ? frames : []);
          return;
        }

        // Single cape-face art (~10:16). Must beat strip heuristics like
        // "128×256 = 4× (128×64) 2:1 frames" which shift the artwork.
        if (aspect >= 0.5 && aspect <= 0.9) {
          resolve([frameToClassicAtlas(img, 0, 0, w, h)]);
          return;
        }

        // Tall animated strip of native atlas frames (HD OptiFine cloaks).
        if (h > w) {
          const nativeFh = detectNativeStripFrameHeight(w, h);
          if (nativeFh !== null && h / nativeFh >= 2) {
            const frames: HTMLCanvasElement[] = [];
            const n = Math.min(Math.floor(h / nativeFh), 48);
            for (let i = 0; i < n; i++) {
              frames.push(frameToClassicAtlas(img, 0, i * nativeFh, w, nativeFh));
            }
            resolve(frames.length ? frames : []);
            return;
          }
        }

        // Remaining tall/odd images → UV face pack (safer than false atlas strips).
        if (aspect < 1.25) {
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
    const skin = skinUrl;
    const nextCape = capeKey(capeUrl);
    const skinChanged = !!(skin && skin !== lastSkin);
    const capeChanged = nextCape !== lastCape;
    const hadSkin = !!lastSkin && lastSkin !== "";

    // Quiet cape-only updates: keep the visible skin, no skeleton overlay.
    const showOverlay = skinChanged || !hadSkin;
    if (showOverlay) {
      loading = true;
    }
    loadError = "";
    try {
      const skinPromise =
        skinChanged && skin
          ? toDataUrl(skin)
          : Promise.resolve(null as string | null);
      const capePromise =
        nextCape && capeChanged
          ? toDataUrl(nextCape).then((raw) => extractCapeFrames(raw))
          : Promise.resolve(null as HTMLCanvasElement[] | null);

      const [skinData, capeFramesResult] = await Promise.all([skinPromise, capePromise]);

      if (skinData) {
        await viewer.loadSkin(skinData, { model: "auto-detect" });
        lastSkin = skin!;
      }

      if (nextCape && capeChanged) {
        stopCapeAnim();
        if (capeFramesResult && capeFramesResult.length) {
          startCapeAnim(capeFramesResult);
          lastCape = nextCape;
        } else {
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
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  function retry() {
    lastSkin = "";
    lastCape = "";
    lastAccount = "";
    loadError = "";
    void initViewer();
  }

  // Track texture props; apply when viewer already exists (init also applies).
  $effect(() => {
    const skin = skinUrl;
    const cape = capeUrl;
    const acct = accountKey;
    if (!viewer) return;
    if (skin === lastSkin && capeKey(cape) === lastCape && acct === lastAccount) return;
    if (acct !== lastAccount) {
      lastAccount = acct;
      lastSkin = "";
      lastCape = lastCape || "__pending_clear__";
      stopCapeAnim();
      try {
        viewer.loadCape(null);
      } catch {
        /* ignore */
      }
    }
    void applyTextures();
  });

  // Keep WebGL viewport in sync when width/height props change.
  $effect(() => {
    const w = width;
    const h = height;
    if (!viewer) return;
    try {
      viewer.setSize(w, h);
    } catch {
      /* ignore */
    }
  });

  // Init when canvas binds (Svelte 5: onMount can race bind:this on $state).
  $effect(() => {
    if (!canvas) return;
    void initViewer();
    return () => {
      initGen++;
      stopCapeAnim();
      if (viewer) {
        viewer.dispose();
        viewer = null;
      }
    };
  });
</script>

<div class="skin-3d-wrap" style="width: {width}px;">
  <!-- stopPropagation keeps the page from stealing wheel; OrbitControls still gets the event on canvas -->
  <div
    class="skin-3d-container"
    class:is-loading={loading}
    style="width: {width}px; height: {height}px;"
    onwheel={(e) => e.stopPropagation()}
  >
    <div class="skin-bg" aria-hidden="true"></div>
    <canvas bind:this={canvas} width={width} height={height}></canvas>
    {#if loading}
      <div class="loading-overlay" aria-hidden="true">
        <div class="loading-shimmer">
          <div class="loading-figure skeleton skeleton-block"></div>
        </div>
      </div>
    {/if}
    {#if loadError && !loading}
      <div class="error-overlay">
        <p>Skin failed to load</p>
        <button type="button" class="retry-btn" onclick={retry}>Retry</button>
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
    width: 100%;
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

  .skin-3d-container.is-loading canvas {
    opacity: 0;
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
    background: rgba(18, 20, 22, 0.92);
    pointer-events: none;
    z-index: 2;
  }

  .error-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: rgba(12, 14, 16, 0.72);
    padding: 16px;
    text-align: center;
  }

  .error-overlay p {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .retry-btn {
    padding: 6px 12px;
    border-radius: var(--border-radius-sm, 6px);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated, var(--bg-secondary));
    color: var(--accent-primary);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }

  .retry-btn:hover {
    border-color: var(--accent-primary);
  }

  .loading-shimmer {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    height: 55%;
  }

  .loading-figure {
    width: 72px;
    height: 160px;
    border-radius: 36px 36px 12px 12px;
    opacity: 0.55;
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
</style>
