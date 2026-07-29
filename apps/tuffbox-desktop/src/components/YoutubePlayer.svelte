<!-- Litube-inspired lite player: lazy youtube-nocookie embed (no WebView until click). -->
<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount, tick } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { X, ExternalLink } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";

  export let videoId: string;
  export let title = "";
  /** Card rect on home — fly-open from here to center. */
  export let originRect: DOMRect | null = null;

  const dispatch = createEventDispatcher<{ close: void }>();

  let embedAlive = true;
  let dialogEl: HTMLDivElement | null = null;
  let backdropIn = false;
  let dialogIn = false;
  let backdropOut = false;
  let dialogOut = false;
  let closing = false;

  $: embedSrc = `https://www.youtube-nocookie.com/embed/${videoId}?autoplay=1&rel=0&modestbranding=1&playsinline=1&iv_load_policy=3`;

  /** Mount overlay on document.body so .fade-slide-in transform / .content overflow cannot clip it. */
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

  async function playOpenAnimation() {
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

  function close() {
    if (closing) return;
    closing = true;
    destroyEmbed();

    const finish = () => dispatch("close");

    if (prefersReducedMotion() || !dialogEl) {
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

  onMount(() => {
    void playOpenAnimation();
  });

  onDestroy(destroyEmbed);
</script>

<svelte:window on:keydown={onKeydown} />

<div
  class="yp-backdrop"
  class:yp-in={backdropIn}
  class:yp-out={backdropOut}
  role="button"
  tabindex="-1"
  use:bodyPortal
  on:click={(e) => e.target === e.currentTarget && close()}
  on:keydown={() => {}}
>
  <div
    bind:this={dialogEl}
    class="yp-dialog"
    class:yp-in={dialogIn}
    class:yp-out={dialogOut}
    role="dialog"
    aria-modal="true"
    aria-label={title || "YouTube player"}
    use:trapFocus={{ onEscape: close }}
  >
    <div class="yp-header">
      <h3 class="yp-title">{title || "YouTube"}</h3>
      <div class="yp-actions">
        <button type="button" class="yp-btn" on:click={openInBrowser}>
          <ExternalLink size={16} />
          <span>Open in browser</span>
        </button>
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
  .yp-backdrop {
    position: fixed;
    inset: 0;
    z-index: 10000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: max(16px, 3vh) max(16px, 3vw);
    /* Minecraft pause-menu vibe: dirt vignette + scanlines, tinted by theme */
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
  }

  .yp-backdrop.yp-in {
    opacity: 1;
  }

  .yp-backdrop.yp-out {
    opacity: 0;
  }

  .yp-dialog {
    width: min(960px, 100%);
    margin: auto;
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

  .yp-header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-elevated, var(--bg-tertiary)) 80%, var(--bg-secondary));
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

  .yp-actions {
    display: flex;
    align-items: center;
    gap: 8px;
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
    background: #000;
  }

  .yp-frame-wrap iframe {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
  }

  :global(.potato-pc) .yp-backdrop,
  :global(.potato-pc) .yp-dialog {
    transition: none !important;
  }

  @media (prefers-reduced-motion: reduce) {
    .yp-backdrop,
    .yp-dialog {
      transition: none !important;
    }
  }
</style>
