<script lang="ts">
  import HeadAvatar from "./HeadAvatar.svelte";
  import { ChevronLeft, ChevronRight } from "@lucide/svelte";
  import type { AccountEntry } from "../lib/store";

  /**
   * Xbox-style account picker: a horizontal rail of skin heads steered with
   * the mouse wheel, arrow keys, or clicks. Navigation only moves a LOCAL
   * selection highlight; the real account switch happens on explicit confirm
   * (Enter or clicking the selected head). Escape cancels back to the active
   * account. The signed-in head wears a theme-colored glow rising from the
   * nick up through the head; a pending pick glows softer. The selected head
   * stays centered; neighbors scale down and fade, rail edges dim via mask.
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
  const SLOT_GAP = 18; // px between slots — wide enough for the rail to breathe
  const SLOT_W = HEAD_W + SLOT_GAP; // one step of the rail
  const WHEEL_STEP = 22; // accumulated deltaY per selection step
  const WHEEL_COOLDOWN_MS = 90; // min gap between wheel-driven steps

  let root = $state<HTMLDivElement | undefined>();
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
  const selected = $derived(accounts[index]);
  // A pick awaiting confirmation: selected head differs from the signed-in one.
  const pending = $derived(count > 1 && !!selected && selected.uuid !== activeUuid);

  // Follow external account switches (AccountManager, login modal, boot);
  // doubles as cancel when the world changes behind a pending pick.
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
    return { scale, opacity };
  }

  /** Move the LOCAL selection; never touches the backend. */
  function select(i: number) {
    const next = Math.min(count - 1, Math.max(0, i));
    if (next === index || busy) return;
    index = next;
    // Focus the group so Enter confirms the pick instead of leaking to
    // whatever the user had focused before they started scrolling.
    root?.focus({ preventScroll: true });
  }

  function move(dir: number) {
    if (count <= 1) return;
    select(index + dir);
  }

  /** Explicit confirmation: the only path that fires the real switch. */
  function confirmSelection() {
    if (busy || !selected || selected.uuid === activeUuid) return;
    onswitch(selected.uuid);
  }

  function cancelSelection() {
    index = activeIndex >= 0 ? activeIndex : 0;
    wheelAcc = 0;
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
    if (count <= 1) return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      move(1);
    } else if (e.key === "Enter") {
      // preventDefault stops a focused slot button from also firing click.
      e.preventDefault();
      confirmSelection();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelSelection();
    }
  }

  /** First click picks a neighbor; clicking the picked head again confirms. */
  function onSlotClick(i: number) {
    if (busy || count <= 1) return;
    const account = accounts[i];
    if (!account || account.uuid === activeUuid) return;
    if (i === index) confirmSelection();
    else select(i);
  }
</script>

<!-- Carousel container: focusable group (WCAG carousel pattern); the user scrolls it
     with the mouse wheel and steers it with arrow keys. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="account-carousel"
  bind:this={root}
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
    <div class="carousel-track" style="gap: {SLOT_GAP}px; transform: translateX({trackOffset}px);">
      {#each accounts as account, i (account.uuid)}
        {@const v = visual(i)}
        <button
          type="button"
          class="carousel-slot"
          class:active={account.uuid === activeUuid}
          class:selected={i === index}
          disabled={busy}
          title={account.name}
          onclick={() => onSlotClick(i)}
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
    gap: 9px;
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

  /* Signed-in head: theme-colored glow rising from the nick up through the
     head (bottom → top), no ring/frame. */
  .carousel-slot.active .slot-head::before {
    content: "";
    position: absolute;
    inset: -14px -20px -26px;
    border-radius: var(--border-radius-md);
    pointer-events: none;
    background:
      radial-gradient(
        ellipse 62% 88% at 50% 108%,
        color-mix(in srgb, var(--accent-primary) 55%, transparent) 0%,
        color-mix(in srgb, var(--accent-primary) 22%, transparent) 46%,
        transparent 78%
      );
    z-index: -1;
    filter: blur(2px);
  }

  /* Pending pick: softer version of the same bottom-up glow. */
  .carousel-slot.selected:not(.active) .slot-head::before {
    content: "";
    position: absolute;
    inset: -10px -16px -22px;
    border-radius: var(--border-radius-md);
    pointer-events: none;
    background:
      radial-gradient(
        ellipse 58% 84% at 50% 108%,
        color-mix(in srgb, var(--accent-primary) 30%, transparent) 0%,
        transparent 72%
      );
    z-index: -1;
    filter: blur(2px);
  }

  .slot-name {
    font-family: var(--font-minecraft);
    font-size: 11px;
    letter-spacing: 0.3px;
    max-width: 120px;
    white-space: nowrap;
    text-align: center;
    line-height: 1.2;
    display: inline-block;
    overflow: visible;
    color: var(--text-secondary);
    transition: color var(--motion-fast) var(--ease-out);
  }

  .carousel-slot.active .slot-name {
    color: var(--text-primary);
    text-shadow: var(--mc-nick-shadow-soft);
  }

  /* Neutralize the global button hover/active chrome on carousel slots —
     the white "button" highlight on the nick was the global button:hover. */
  .carousel-slot:hover:not(:disabled),
  .carousel-slot:active:not(:disabled) {
    background: none;
    border-color: transparent;
    margin-top: 0;
  }
  .carousel-slot:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--accent-primary) 60%, transparent);
    outline-offset: 4px;
    border-radius: var(--border-radius-sm);
  }

  .carousel-arrow {
    position: absolute;
    top: 38px; /* head-row center: pad-top 14px + head 76/2 − arrow 28/2 */
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: var(--border-radius-sm);
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out),
      color var(--motion-fast) var(--ease-out),
      transform var(--motion-fast) var(--ease-spring);
  }

  .carousel-arrow.left { left: 8px; }
  .carousel-arrow.right { right: 8px; }

  .carousel-arrow:hover:not(:disabled) {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    transform: scale(1.1);
  }

  .carousel-arrow:disabled {
    opacity: 0.4;
    cursor: default;
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