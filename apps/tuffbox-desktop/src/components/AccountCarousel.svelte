<script lang="ts">
  import HeadAvatar from "./HeadAvatar.svelte";
  import { ChevronLeft, ChevronRight } from "@lucide/svelte";
  import type { AccountEntry } from "../lib/store";

  /**
   * Xbox-style account picker: a horizontal rail of skin heads that the user
   * scrolls with the mouse wheel (or arrows / click). The active head is always
   * centered; neighbors scale down and fade, and the rail edges dim out via a
   * CSS mask so long strips melt into the panel sides.
   */
  let {
    accounts = [],
    skinPaths = {},
    activeUuid = null,
    busy = false,
    onswitch = (_uuid: string) => {},
  }: {
    accounts?: AccountEntry[];
    skinPaths?: Record<string, string | null | undefined>;
    activeUuid?: string | null;
    busy?: boolean;
    onswitch?: (uuid: string) => void;
  } = $props();

  const HEAD_W = 76; // rendered head size (canvas) — CSS scale handles the rest
  const SLOT_GAP = 6; // px between slots
  const SLOT_W = HEAD_W + SLOT_GAP; // one step of the rail
  const WHEEL_STEP = 22; // accumulated deltaY per account step
  const WHEEL_COOLDOWN_MS = 90; // min gap between wheel-driven switches

  let viewport = $state<HTMLDivElement | undefined>();
  let viewportW = $state(288);
  let index = $state(
    (() => {
      const i = accounts.findIndex((a) => a.uuid === activeUuid);
      return i >= 0 ? i : 0;
    })(),
  );
  let wheelAcc = $state(0);
  let lastStepAt = 0;
  let wheelResetTimer: ReturnType<typeof setTimeout> | undefined;

  const count = $derived(accounts.length);
  const activeIndex = $derived(accounts.findIndex((a) => a.uuid === activeUuid));

  // Follow external account switches (AccountManager, login modal, boot).
  $effect(() => {
    index = activeIndex >= 0 ? activeIndex : 0;
  });

  // Keep the rail centered inside the panel as its width changes.
  $effect(() => {
    if (!viewport) return;
    const ro = new ResizeObserver(() => {
      viewportW = viewport?.clientWidth ?? 288;
    });
    ro.observe(viewport);
    return () => ro.disconnect();
  });

  const trackOffset = $derived(viewportW / 2 - HEAD_W / 2 - index * SLOT_W);

  function visual(i: number) {
    const d = Math.abs(i - index);
    const scale = d === 0 ? 1 : d === 1 ? 0.72 : d === 2 ? 0.55 : 0.42;
    const opacity = d === 0 ? 1 : d === 1 ? 0.82 : 0.4;
    return { active: d === 0, scale, opacity };
  }

  function move(dir: number) {
    if (count <= 1 || busy) return;
    const next = Math.min(count - 1, Math.max(0, index + dir));
    if (next === index) return;
    const target = accounts[next];
    if (target) onswitch(target.uuid);
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (count <= 1 || busy) return;
    wheelAcc += e.deltaY;
    const now = performance.now();
    let guard = 0;
    while (Math.abs(wheelAcc) >= WHEEL_STEP && guard++ < 8) {
      if (now - lastStepAt < WHEEL_COOLDOWN_MS) break;
      lastStepAt = now;
      const dir = wheelAcc > 0 ? 1 : -1;
      wheelAcc -= dir * WHEEL_STEP;
      move(dir);
    }
    clearTimeout(wheelResetTimer);
    wheelResetTimer = setTimeout(() => {
      wheelAcc = 0;
    }, 260);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      move(1);
    }
  }
</script>

<!-- Carousel container: focusable group (WCAG carousel pattern); the user scrolls it
     with the mouse wheel and steers it with arrow keys. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="account-carousel"
  tabindex="0"
  role="group"
  aria-label="Minecraft accounts"
  onwheel={onWheel}
  onkeydown={onKeydown}
>
  {#if count > 1}
    <button
      type="button"
      class="carousel-arrow left"
      aria-label="Previous account"
      disabled={busy || index === 0}
      onclick={() => move(-1)}
    >
      <ChevronLeft size={16} />
    </button>
    <button
      type="button"
      class="carousel-arrow right"
      aria-label="Next account"
      disabled={busy || index === count - 1}
      onclick={() => move(1)}
    >
      <ChevronRight size={16} />
    </button>
  {/if}

  <div class="carousel-viewport" bind:this={viewport}>
    <div class="carousel-track" style="transform: translateX({trackOffset}px);">
      {#each accounts as account, i (account.uuid)}
        {@const v = visual(i)}
        <button
          type="button"
          class="carousel-slot"
          class:active={v.active}
          disabled={busy}
          title={account.name}
          onclick={() => onswitch(account.uuid)}
          style="--scale: {v.scale}; --opacity: {v.opacity};"
        >
          <span class="slot-head" aria-hidden="true">
            <HeadAvatar skinSrc={skinPaths[account.uuid] ?? null} size={HEAD_W} alt={account.name} />
          </span>
          <span class="slot-name">{account.name}</span>
        </button>
      {/each}
    </div>
  </div>

  {#if count > 1}
    <div class="carousel-hint">Scroll to switch · ← →</div>
  {/if}
</div>

<style>
  .account-carousel {
    position: relative;
    padding: 14px 0 12px;
    outline: none;
  }

  .account-carousel:focus-visible .carousel-viewport {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-primary) 55%, transparent);
  }

  .carousel-viewport {
    overflow: hidden;
    /* Dim the rail edges so heads melt into the panel sides. */
    -webkit-mask-image: linear-gradient(
      90deg,
      transparent 0%,
      #000 15%,
      #000 85%,
      transparent 100%
    );
    mask-image: linear-gradient(
      90deg,
      transparent 0%,
      #000 15%,
      #000 85%,
      transparent 100%
    );
    border-radius: var(--border-radius-sm);
    transition: box-shadow var(--motion-fast) var(--ease-out);
  }

  .carousel-track {
    display: flex;
    align-items: flex-start;
    width: max-content;
    will-change: transform;
    transition: transform var(--motion-med) var(--ease-spring);
  }

  .carousel-slot {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    width: 76px;
    padding: 0;
    border: none;
    background: none;
    color: inherit;
    cursor: pointer;
    opacity: var(--opacity);
    transform: scale(var(--scale));
    transform-origin: center top;
    transition:
      transform var(--motion-med) var(--ease-spring),
      opacity var(--motion-med) var(--ease-out);
  }

  .carousel-slot:disabled {
    cursor: default;
  }

  .slot-head {
    position: relative;
    width: 76px;
    height: 76px;
    border-radius: 6px;
    overflow: visible;
  }

  .carousel-slot.active .slot-head {
    box-shadow:
      0 0 0 2px var(--accent-primary),
      0 0 18px color-mix(in srgb, var(--accent-primary) 45%, transparent),
      0 10px 24px rgba(0, 0, 0, 0.45);
  }

  .carousel-slot.active .slot-head::after {
    content: "";
    position: absolute;
    left: 50%;
    bottom: -7px;
    width: 46px;
    height: 6px;
    transform: translateX(-50%);
    border-radius: 999px;
    background: var(--accent-primary);
    opacity: 0.85;
    box-shadow: 0 0 12px color-mix(in srgb, var(--accent-primary) 70%, transparent);
  }

  .slot-name {
    font-family: var(--font-minecraft);
    font-size: 8px;
    letter-spacing: 0.3px;
    max-width: 76px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    transition: color var(--motion-fast) var(--ease-out);
  }

  .carousel-slot.active .slot-name {
    color: var(--text-primary);
    text-shadow: var(--mc-nick-shadow-soft);
  }

  .carousel-arrow {
    position: absolute;
    top: 30px;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: var(--border-radius-sm);
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out),
      color var(--motion-fast) var(--ease-out),
      transform var(--motion-fast) var(--ease-spring);
  }

  .carousel-arrow.left { left: 0; }
  .carousel-arrow.right { right: 0; }

  .carousel-arrow:hover:not(:disabled) {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    transform: scale(1.1);
  }

  .carousel-arrow:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .carousel-hint {
    margin-top: 9px;
    text-align: center;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.03em;
    color: var(--text-muted);
    user-select: none;
  }

  /* Weak GPUs / reduced motion: snap instead of sliding. */
  :global(html.potato-pc) .carousel-track,
  :global(html.potato-pc) .carousel-slot {
    transition: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .carousel-track,
    .carousel-slot,
    .carousel-arrow {
      transition: none;
    }
  }
</style>