<!-- Litube-inspired lite player: large modal + draggable mini window. -->
<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { X, ExternalLink, PictureInPicture2, Maximize2, GripVertical } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";

  export let videoId: string;
  export let title = "";
  /** Card rect on home — fly-open from here to center. */
  export let originRect: DOMRect | null = null;
  /** Start in floating mini window (skips modal). */
  export let startMini = false;

  const dispatch = createEventDispatcher<{ close: void }>();
  const MINI_POS_KEY = "tuffbox-youtube-mini-pos";
  const MINI_W = 440;

  let embedAlive = true;
  let shellEl: HTMLDivElement | null = null;
  let dialogEl: HTMLDivElement | null = null;
  let backdropIn = false;
  let dialogIn = false;
  let backdropOut = false;
  let dialogOut = false;
  let closing = false;
  let mode: "modal" | "mini" = startMini ? "mini" : "modal";

  let miniX = 24;
  let miniY = 24;
  let dragging = false;
  let dragDx = 0;
  let dragDy = 0;

  $: embedSrc = `https://www.youtube-nocookie.com/embed/${videoId}?autoplay=1&rel=0&modestbranding=1&playsinline=1&iv_load_policy=3`;

  function bodyPortal(node: HTMLElement) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }

  function destroyEmbed() {
    embedAlive = false;
  }

  function prefersReducedMotion(): boolean {
    try {
      if (document.documentElement.classList.contains("potato-pc")) return true;
      return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    } catch {
      return false;
    }
  }

  function loadMiniPos() {
    try {
      const raw = localStorage.getItem(MINI_POS_KEY);
      if (!raw) {
        placeMiniDefault();
        return;
      }
      const parsed = JSON.parse(raw) as { x?: number; y?: number };
      if (typeof parsed.x === "number" && typeof parsed.y === "number") {
        miniX = parsed.x;
        miniY = parsed.y;
        clampMiniPos();
        return;
      }
    } catch {
      /* ignore */
    }
    placeMiniDefault();
  }

  function placeMiniDefault() {
    const pad = 20;
    miniX = Math.max(pad, window.innerWidth - MINI_W - pad);
    miniY = Math.max(pad, window.innerHeight - Math.round((MINI_W * 9) / 16) - 72 - pad);
  }

  function clampMiniPos() {
    const w = MINI_W;
    const h = Math.round((MINI_W * 9) / 16) + 48;
    const maxX = Math.max(8, window.innerWidth - w - 8);
    const maxY = Math.max(8, window.innerHeight - h - 8);
    miniX = Math.min(maxX, Math.max(8, miniX));
    miniY = Math.min(maxY, Math.max(8, miniY));
  }

  function saveMiniPos() {
    try {
      localStorage.setItem(MINI_POS_KEY, JSON.stringify({ x: miniX, y: miniY }));
    } catch {
      /* ignore */
    }
  }

  async function playOpenAnimation() {
    if (mode === "mini") {
      loadMiniPos();
      return;
    }
    await tick();
    if (!dialogEl) {
      backdropIn = true;
      dialogIn = true;
      return;
    }

    if (prefersReducedMotion()) {
      backdropIn = true;
      dialogIn = true;
      return;
    }

    backdropIn = true;

    if (originRect && originRect.width > 0 && originRect.height > 0) {
      const end = dialogEl.getBoundingClientRect();
      const dx =
        originRect.left + originRect.width / 2 - (end.left + end.width / 2);
      const dy =
        originRect.top + originRect.height / 2 - (end.top + end.height / 2);
      const sx = Math.max(0.08, originRect.width / end.width);
      const sy = Math.max(0.08, originRect.height / end.height);
      dialogEl.style.transform = `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`;
      dialogEl.style.opacity = "0.35";
      void dialogEl.offsetWidth;
      dialogEl.style.transition =
        "transform 0.42s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.32s ease";
      dialogEl.style.transform = "translate(0, 0) scale(1)";
      dialogEl.style.opacity = "1";
      dialogIn = true;
    } else {
      dialogIn = true;
    }
  }

  function toMini() {
    mode = "mini";
    backdropIn = false;
    dialogIn = true;
    loadMiniPos();
  }

  function toModal() {
    mode = "modal";
    closing = false;
    backdropOut = false;
    dialogOut = false;
    void tick().then(() => {
      backdropIn = true;
      dialogIn = true;
    });
  }

  function close() {
    if (closing) return;
    closing = true;
    destroyEmbed();

    const finish = () => dispatch("close");

    if (mode === "mini" || prefersReducedMotion() || !dialogEl) {
      finish();
      return;
    }

    backdropIn = false;
    backdropOut = true;
    dialogIn = false;
    dialogOut = true;

    if (originRect && originRect.width > 0) {
      const end = dialogEl.getBoundingClientRect();
      const dx =
        originRect.left + originRect.width / 2 - (end.left + end.width / 2);
      const dy =
        originRect.top + originRect.height / 2 - (end.top + end.height / 2);
      const sx = Math.max(0.08, originRect.width / end.width);
      const sy = Math.max(0.08, originRect.height / end.height);
      dialogEl.style.transition =
        "transform 0.28s cubic-bezier(0.4, 0, 1, 1), opacity 0.22s ease";
      dialogEl.style.transform = `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`;
      dialogEl.style.opacity = "0";
    }

    window.setTimeout(finish, 280);
  }

  async function openInBrowser() {
    await open(`https://www.youtube.com/watch?v=${videoId}`);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  function onDragStart(e: PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest("button")) return;
    dragging = true;
    dragDx = e.clientX - miniX;
    dragDy = e.clientY - miniY;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onDragMove(e: PointerEvent) {
    if (!dragging) return;
    miniX = e.clientX - dragDx;
    miniY = e.clientY - dragDy;
    clampMiniPos();
  }

  function onDragEnd(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    } catch {
      /* ignore */
    }
    saveMiniPos();
  }

  function onWinResize() {
    if (mode === "mini") clampMiniPos();
  }

  onMount(() => {
    void playOpenAnimation();
  });

  onDestroy(destroyEmbed);
</script>

<svelte:window on:keydown={onKeydown} on:resize={onWinResize} />

<div
  bind:this={shellEl}
  class="yp-shell"
  class:is-mini={mode === "mini"}
  class:yp-in={backdropIn || mode === "mini"}
  class:yp-out={backdropOut}
  class:dragging
  role={mode === "modal" ? "presentation" : "dialog"}
  aria-label={mode === "mini" ? title || "YouTube mini player" : undefined}
  style={mode === "mini" ? `left: ${miniX}px; top: ${miniY}px; width: ${MINI_W}px;` : undefined}
  use:bodyPortal
  on:click={(e) => mode === "modal" && e.target === e.currentTarget && close()}
  on:keydown={() => {}}
>
  <div
    bind:this={dialogEl}
    class="yp-dialog"
    class:yp-in={dialogIn || mode === "mini"}
    class:yp-out={dialogOut}
    class:mini={mode === "mini"}
    role="dialog"
    aria-modal={mode === "modal" ? "true" : "false"}
    aria-label={title || "YouTube player"}
    use:trapFocus={{ onEscape: close, enabled: mode === "modal" }}
  >
    <div
      class="yp-header"
      class:draggable={mode === "mini"}
      role={mode === "mini" ? "toolbar" : "banner"}
      on:pointerdown={mode === "mini" ? onDragStart : undefined}
      on:pointermove={mode === "mini" ? onDragMove : undefined}
      on:pointerup={mode === "mini" ? onDragEnd : undefined}
      on:pointercancel={mode === "mini" ? onDragEnd : undefined}
    >
      {#if mode === "mini"}
        <span class="yp-grip" aria-hidden="true"><GripVertical size={14} /></span>
      {/if}
      <h3 class="yp-title">{title || "YouTube"}</h3>
      <div class="yp-actions">
        {#if mode === "modal"}
          <button type="button" class="yp-btn" on:click={toMini} title="Watch in a floating mini window while you work">
            <PictureInPicture2 size={16} />
            <span>Mini player</span>
          </button>
          <button type="button" class="yp-btn" on:click={openInBrowser}>
            <ExternalLink size={16} />
            <span>Open in browser</span>
          </button>
        {:else}
          <button type="button" class="yp-icon-btn" on:click={toModal} aria-label="Expand" title="Expand">
            <Maximize2 size={16} />
          </button>
          <button type="button" class="yp-icon-btn" on:click={openInBrowser} aria-label="Open in browser" title="Open in browser">
            <ExternalLink size={16} />
          </button>
        {/if}
        <button type="button" class="yp-icon-btn" on:click={close} aria-label="Close">
          <X size={18} />
        </button>
      </div>
    </div>
    <div class="yp-frame-wrap">
      {#if embedAlive}
        <iframe
          src={embedSrc}
          title={title || "YouTube video"}
          allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
          allowfullscreen
        ></iframe>
      {/if}
    </div>
  </div>
</div>

<style>
  .yp-shell {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: max(12px, 2vh) max(12px, 2vw);
    background:
      repeating-linear-gradient(
        0deg,
        transparent 0,
        transparent 3px,
        rgba(0, 0, 0, 0.12) 3px,
        rgba(0, 0, 0, 0.12) 4px
      ),
      radial-gradient(
        ellipse 70% 55% at 50% 42%,
        color-mix(in srgb, var(--accent-primary) 14%, transparent) 0%,
        transparent 62%
      ),
      radial-gradient(
        ellipse at center,
        color-mix(in srgb, var(--bg-primary) 35%, #000000) 0%,
        color-mix(in srgb, var(--bg-primary) 12%, #000000) 55%,
        rgba(0, 0, 0, 0.88) 100%
      );
    opacity: 0;
    transition: opacity 0.28s ease;
    pointer-events: auto;
  }

  .yp-shell.yp-in {
    opacity: 1;
  }

  .yp-shell.yp-out {
    opacity: 0;
  }

  /* Mini: no dimmed backdrop — developer keeps working underneath */
  .yp-shell.is-mini {
    inset: auto;
    padding: 0;
    background: none;
    opacity: 1;
    z-index: 10050;
    align-items: stretch;
    justify-content: stretch;
    pointer-events: none;
  }

  .yp-shell.is-mini .yp-dialog {
    pointer-events: auto;
    width: 100%;
    max-height: none;
    margin: 0;
    opacity: 1;
    transform: none;
    transition: none;
  }

  .yp-dialog {
    width: min(1400px, calc(100vw - 32px));
    max-height: calc(100vh - 24px);
    margin: auto;
    display: flex;
    flex-direction: column;
    background: var(--bg-secondary);
    border: 2px solid color-mix(in srgb, var(--border-color) 70%, var(--accent-primary) 30%);
    border-radius: var(--border-radius-xl, 12px);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent-primary) 25%, transparent),
      0 24px 80px rgba(0, 0, 0, 0.55),
      var(--shadow-lg);
    overflow: hidden;
    transform-origin: center center;
    will-change: transform, opacity;
    opacity: 0;
    transform: translateY(18px) scale(0.92);
    transition:
      transform 0.42s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.32s ease;
  }

  .yp-dialog.yp-in {
    opacity: 1;
    transform: translate(0, 0) scale(1);
  }

  .yp-dialog.yp-out {
    opacity: 0;
    transform: translateY(12px) scale(0.94);
  }

  .yp-dialog.mini {
    border-radius: var(--border-radius-lg, 14px);
  }

  .yp-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-elevated, var(--bg-tertiary)) 80%, var(--bg-secondary));
    flex-shrink: 0;
  }

  .yp-header.draggable {
    cursor: grab;
    user-select: none;
    touch-action: none;
  }

  .yp-shell.dragging .yp-header.draggable {
    cursor: grabbing;
  }

  .yp-grip {
    display: inline-flex;
    color: var(--text-muted);
    flex-shrink: 0;
    opacity: 0.8;
  }

  .yp-title {
    margin: 0;
    flex: 1;
    min-width: 0;
    font-size: 14px;
    font-weight: 700;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .yp-dialog.mini .yp-title {
    font-size: 12px;
  }

  .yp-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .yp-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: var(--border-radius-md, 8px);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated, var(--bg-primary));
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .yp-btn:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .yp-btn :global(svg) {
    flex-shrink: 0;
  }

  .yp-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: none;
    border-radius: var(--border-radius-md, 8px);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .yp-icon-btn:hover {
    color: var(--text-primary);
    background: var(--bg-elevated, var(--bg-primary));
  }

  .yp-frame-wrap {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    max-height: calc(100vh - 96px);
    background: #000;
    flex: 1 1 auto;
  }

  .yp-dialog.mini .yp-frame-wrap {
    max-height: none;
    flex: 0 0 auto;
  }

  .yp-frame-wrap iframe {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
  }

  :global(.potato-pc) .yp-shell,
  :global(.potato-pc) .yp-dialog {
    transition: none !important;
  }

  @media (prefers-reduced-motion: reduce) {
    .yp-shell,
    .yp-dialog {
      transition: none !important;
    }
  }

  @media (max-width: 720px) {
    .yp-btn span {
      display: none;
    }
  }
</style>
