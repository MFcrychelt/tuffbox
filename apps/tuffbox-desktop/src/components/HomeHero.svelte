<script module lang="ts">
  export type PosterCoverKind = "gallery" | "icon" | "none";
</script>

<script lang="ts">
  import {
    Play,
    Square,
    Settings,
    Package,
    FolderOpen,
    FolderInput,
    Search,
    MoreHorizontal,
    Pencil,
    Copy,
    Trash2,
    ShieldAlert,
  } from "@lucide/svelte";
  import { fade } from "svelte/transition";
  import type { CrashFixBannerPayload } from "../lib/homeBootstrap";

  let {
    hasSelection = false,
    emptyZero = false,
    meta = "",
    launching = false,
    launchMessage = "",
    launchPercent = null,
    running = false,
    playDisabled = false,
    coverUrl = null,
    coverKind = "none",
    potato = false,
    actionBusy = false,
    overflowOpen = false,
    signedIn = true,
    playerName = "",
    crashBanner = null,
    crashFixBusy = false,
    softVerifyRemainingSecs = null,
    onPlay,
    onStop,
    onSettings,
    onFolder,
    onToggleOverflow,
    onRename,
    onClone,
    onDelete,
    onCreate,
    onImport,
    onBrowse,
    onRollback,
    onDiagnostics,
    onSignIn,
  }: {
    hasSelection?: boolean;
    emptyZero?: boolean;
    meta?: string;
    launching?: boolean;
    launchMessage?: string;
    launchPercent?: number | null;
    running?: boolean;
    playDisabled?: boolean;
    coverUrl?: string | null;
    coverKind?: PosterCoverKind;
    potato?: boolean;
    actionBusy?: boolean;
    overflowOpen?: boolean;
    signedIn?: boolean;
    playerName?: string;
    crashBanner?: CrashFixBannerPayload | null;
    crashFixBusy?: boolean;
    softVerifyRemainingSecs?: number | null;
    onPlay: () => void;
    onStop: () => void;
    onSettings: () => void;
    onFolder: () => void;
    onToggleOverflow: () => void;
    onRename: () => void;
    onClone: () => void;
    onDelete: () => void;
    onCreate: () => void;
    onImport: () => void;
    onBrowse: () => void;
    onRollback: () => void;
    onDiagnostics: () => void;
    onSignIn?: () => void;
  } = $props();

  const showStorefront = $derived(!hasSelection);
  const playStop = $derived(running && !launching);
  const progressPct = $derived(
    launching && launchPercent != null ? Math.max(0, Math.min(100, launchPercent)) : 0,
  );
  const artFadeMs = $derived.by(() => {
    if (potato) return 0;
    if (
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      return 0;
    }
    return 240;
  });

  function onPlayClick() {
    if (playStop) onStop();
    else onPlay();
  }

  function onOverflowPointerDown(e: MouseEvent) {
    if (!overflowOpen) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest?.(".poster-toolbar")) return;
    onToggleOverflow();
  }

  function onOverflowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && overflowOpen) onToggleOverflow();
  }
</script>

<svelte:window onmousedown={onOverflowPointerDown} onkeydown={onOverflowKeydown} />

<section
  class={["poster", { launching, storefront: showStorefront }]}
  aria-label="Play"
>
  <div class="poster-art-stack" aria-hidden="true">
    {#key `${coverKind}:${coverUrl ?? ""}`}
      {#if coverUrl}
        <img
          class={["poster-art", { "icon-fill": coverKind === "icon", "no-blur": potato }]}
          src={coverUrl}
          alt=""
          draggable="false"
          in:fade={{ duration: artFadeMs }}
          out:fade={{ duration: artFadeMs }}
        />
      {:else}
        <div
          class="poster-procedural tex-deepslate"
          in:fade={{ duration: artFadeMs }}
          out:fade={{ duration: artFadeMs }}
        ></div>
      {/if}
    {/key}
  </div>
  <div class="poster-scrim" aria-hidden="true"></div>
  {#if !coverUrl}
    <div class="poster-grass-edge tex-grass" aria-hidden="true"></div>
  {/if}

  {#if launching}
    <div
      class="poster-progress"
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={launchPercent ?? undefined}
      aria-label={launchMessage || "Launching"}
    >
      <span class="poster-progress-fill" style:width={`${progressPct}%`}></span>
    </div>
  {/if}

  {#if hasSelection}
    <div class="poster-toolbar">
      <div class="poster-toolbar-bar">
        <button type="button" class="glass-seg" onclick={onSettings} disabled={actionBusy} title="Instance settings" aria-label="Instance settings">
          <Settings size={14} />
          <span>Instance</span>
        </button>
        <button type="button" class="glass-seg" onclick={onFolder} disabled={actionBusy} title="Open instance folder" aria-label="Open instance folder">
          <FolderOpen size={14} />
          <span>Folder</span>
        </button>
        <div class="poster-overflow">
          <button
            type="button"
            class="glass-seg glass-seg-icon"
            aria-label="More instance actions"
            aria-expanded={overflowOpen}
            aria-haspopup="menu"
            disabled={actionBusy}
            onclick={onToggleOverflow}
          >
            <MoreHorizontal size={15} />
          </button>
        </div>
      </div>
      {#if overflowOpen}
        <div class="poster-overflow-menu" role="menu">
          <button type="button" role="menuitem" disabled={actionBusy} onclick={onRename}>
            <Pencil size={14} />
            Rename
          </button>
          <button type="button" role="menuitem" disabled={actionBusy} onclick={onClone}>
            <Copy size={14} />
            Clone
          </button>
          <button type="button" role="menuitem" class="danger" disabled={actionBusy} onclick={onDelete}>
            <Trash2 size={14} />
            Delete
          </button>
        </div>
      {/if}
    </div>
  {/if}

  {#if showStorefront}
    <div class="poster-storefront">
      {#if emptyZero}
        <p class="storefront-title">No instances yet</p>
        <p class="storefront-hint">Create a blank pack, import one you already have, or browse the library.</p>
      {:else}
        <p class="storefront-title">Select an instance</p>
        <p class="storefront-hint">Choose one from the list on the left, or create a new instance.</p>
      {/if}
      <div class="storefront-ctas">
        <button type="button" class="storefront-primary" onclick={onCreate}>
          <Package size={15} />
          Create
        </button>
        <button type="button" class="glass-seg storefront-ghost" onclick={onImport}>
          <FolderInput size={15} />
          Import
        </button>
        <button type="button" class="glass-seg storefront-ghost" onclick={onBrowse}>
          <Search size={15} />
          Browse
        </button>
      </div>
    </div>
  {:else}
    <div class="poster-bottom">
      {#if crashBanner}
        <div class="crash-fix-banner" role="status">
          <ShieldAlert size={16} />
          <div class="crash-fix-banner-body">
            <strong>Fix applied</strong>
            <span>
              {#if crashBanner.softVerifyStartedUnix}
                Play about {softVerifyRemainingSecs ?? 0}s more to confirm it works.
              {:else}
                Launch the game to confirm the fix. You can restore anytime.
              {/if}
            </span>
          </div>
          <div class="crash-fix-banner-actions">
            <button class="crash-restore" type="button" disabled={crashFixBusy} onclick={onRollback}>
              Restore
            </button>
            <button class="ghost crash-fix-diag" type="button" onclick={onDiagnostics}>
              Diagnostics
            </button>
          </div>
        </div>
      {/if}

      <div class="poster-play-row">
        <button
          class={["play-btn", { stop: playStop }]}
          onclick={onPlayClick}
          disabled={playDisabled || launching}
          aria-busy={launching}
          title={launching ? (launchMessage || "Launching…") : undefined}
        >
          {#if launching}
            <span class="spinner spin" aria-hidden="true"></span>
            <span class="play-text play-phase">{launchMessage || "Launching…"}</span>
            {#if launchPercent != null}
              <span class="play-pct" aria-hidden="true">{launchPercent}%</span>
            {/if}
          {:else if playStop}
            <Square size={24} fill="currentColor" />
            <span class="play-text">Stop</span>
          {:else}
            <Play size={28} fill="currentColor" />
            <span class="play-text">Play</span>
          {/if}
        </button>
        {#if meta}
          <p class="poster-meta">
            {meta}
            <span class="meta-chevron" aria-hidden="true"></span>
          </p>
        {/if}
        {#if playerName}
          <span class="poster-player">{playerName}</span>
        {/if}
        {#if !signedIn && onSignIn && !launching}
          <button type="button" class="poster-signin" onclick={onSignIn}>
            Sign in
          </button>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .poster {
    position: relative;
    isolation: isolate;
    height: 400px;
    border-radius: var(--border-radius-xl);
    border: 1px solid var(--border-color);
    box-shadow:
      var(--shadow-md),
      inset 0 1px 0 var(--glass-highlight, color-mix(in srgb, #fff 14%, transparent));
    overflow: hidden;
    container-type: inline-size;
  }

  .poster.launching .poster-art {
    filter: brightness(0.85);
  }

  .poster.launching .poster-art.icon-fill {
    filter: blur(28px) brightness(0.85);
  }

  .poster.launching .poster-art.no-blur,
  .poster.launching .poster-procedural {
    filter: brightness(0.85);
  }

  :global(html.potato-pc) .poster.launching .poster-art,
  :global(html.potato-pc) .poster.launching .poster-procedural {
    filter: none;
  }

  .poster-art-stack {
    position: absolute;
    inset: 0;
    overflow: hidden;
    z-index: 0;
    background: var(--bg-secondary);
  }

  .poster-art {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    z-index: 1;
  }

  .poster-art.icon-fill {
    inset: -8%;
    width: 116%;
    height: 116%;
    filter: blur(28px);
    transform: scale(1.25);
  }

  .poster-art.no-blur,
  :global(html.potato-pc) .poster-art {
    filter: none;
    inset: 0;
    width: 100%;
    height: 100%;
    transform: none;
  }

  /* Cave scene: deepslate speckle (palette from .tex-deepslate) under a
     vertical light falloff and the accent glows. */
  .poster-procedural {
    position: absolute;
    inset: 0;
    background-color: var(--bg-primary);
    background-image:
      linear-gradient(180deg, rgba(0, 0, 0, 0.30), rgba(0, 0, 0, 0.10) 42%, rgba(0, 0, 0, 0.44)),
      radial-gradient(ellipse 70% 80% at 18% 85%, color-mix(in srgb, var(--accent-primary) 28%, transparent), transparent 55%),
      radial-gradient(ellipse 55% 60% at 92% 8%, color-mix(in srgb, var(--accent-secondary) 22%, transparent), transparent 50%),
      radial-gradient(circle at 22% 30%, var(--tex-speck-a) 0 10%, transparent 11%),
      radial-gradient(circle at 68% 12%, var(--tex-speck-b) 0 12%, transparent 13%),
      radial-gradient(circle at 82% 64%, var(--tex-speck-a) 0 9%, transparent 10%),
      radial-gradient(circle at 38% 78%, var(--tex-speck-b) 0 13%, transparent 14%),
      radial-gradient(circle at 8% 88%, var(--tex-speck-c) 0 8%, transparent 9%),
      repeating-conic-gradient(var(--tex-speck-c) 0% 25%, transparent 0% 50%);
    background-size:
      auto, auto, auto,
      var(--tex-size) var(--tex-size),
      var(--tex-size) var(--tex-size),
      var(--tex-size) var(--tex-size),
      var(--tex-size) var(--tex-size),
      var(--tex-size) var(--tex-size),
      calc(var(--tex-size) / 2) calc(var(--tex-size) / 2);
    image-rendering: pixelated;
  }

  :global(html.potato-pc) .poster-procedural {
    background-image:
      linear-gradient(180deg, rgba(0, 0, 0, 0.30), rgba(0, 0, 0, 0.10) 42%, rgba(0, 0, 0, 0.44)),
      radial-gradient(ellipse 70% 80% at 18% 85%, color-mix(in srgb, var(--accent-primary) 28%, transparent), transparent 55%),
      radial-gradient(ellipse 55% 60% at 92% 8%, color-mix(in srgb, var(--accent-secondary) 22%, transparent), transparent 50%);
  }

  /* Grass ledge framing the top of the fallback scene; ::after hangs a row
     of pixel "drips" (8px checker squares) below the strip. */
  .poster-grass-edge {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 2;
    height: 18px;
    pointer-events: none;
    background-color: color-mix(in srgb, var(--accent-primary) 26%, #33501f);
    box-shadow:
      0 1px 0 rgba(0, 0, 0, 0.4),
      0 8px 20px rgba(0, 0, 0, 0.28);
  }

  .poster-grass-edge::after {
    content: "";
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    height: 8px;
    background-image: repeating-conic-gradient(
      color-mix(in srgb, var(--accent-primary) 26%, #33501f) 0% 25%,
      transparent 0% 50%
    );
    background-size: 16px 16px;
    image-rendering: pixelated;
  }

  .poster-scrim {
    position: absolute;
    inset: 0;
    z-index: 1;
    background: var(--hero-scrim);
    pointer-events: none;
  }

  .poster-progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 2;
    height: 3px;
    background: color-mix(in srgb, var(--hero-fg) 12%, transparent);
  }

  .poster-progress-fill {
    display: block;
    height: 100%;
    background: var(--accent-primary);
    transition: width var(--motion-fast, 160ms) var(--ease-out, ease);
  }

  .poster-toolbar {
    position: absolute;
    top: 20px;
    right: 20px;
    z-index: 4;
    animation: poster-in var(--motion-enter, 320ms) var(--ease-spring, ease) both;
    animation-delay: calc(var(--stagger-step, 48ms) * 2);
  }

  .poster-toolbar-bar {
    display: flex;
    align-items: stretch;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--glass-border);
    background: var(--glass-bg);
    -webkit-backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
    backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
    overflow: hidden;
  }

  .glass-seg {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--hero-fg);
    font-size: 12px;
    font-weight: 600;
    box-shadow: none;
  }

  .glass-seg + .glass-seg,
  .glass-seg + .poster-overflow {
    border-left: 1px solid color-mix(in srgb, #fff 12%, transparent);
  }

  .glass-seg:hover:not(:disabled) {
    background: color-mix(in srgb, #fff 10%, transparent);
    color: var(--hero-fg);
    transform: none;
  }

  .glass-seg:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .glass-seg-icon {
    padding: 8px 10px;
  }

  .poster-overflow {
    position: relative;
    display: flex;
  }

  .poster-overflow-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    min-width: 160px;
    padding: 6px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    box-shadow: var(--shadow-md);
    z-index: 8;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .poster-overflow-menu button {
    justify-content: flex-start;
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-primary);
    font-size: 13px;
    font-weight: 600;
    box-shadow: none;
  }

  .poster-overflow-menu button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
    color: var(--accent-primary);
    transform: none;
  }

  .poster-overflow-menu button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .poster-overflow-menu button.danger {
    color: var(--accent-danger, #e5484d);
  }

  .poster-overflow-menu button.danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-danger, #e5484d) 12%, transparent);
    color: var(--accent-danger, #e5484d);
  }

  .poster-bottom {
    position: absolute;
    left: 20px;
    right: 20px;
    bottom: 20px;
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    max-width: calc(100% - 40px);
  }

  .poster-play-row {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
    min-width: 0;
    width: 100%;
  }

  .play-btn {
    min-width: 160px;
    width: auto;
    max-width: 280px;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-size: 18px;
    border-radius: var(--border-radius-lg);
    background: var(--accent-primary);
    color: var(--on-accent, #000);
    box-shadow: var(--play-glow);
    padding: 0 20px;
    flex-shrink: 0;
    animation: poster-in var(--motion-enter, 320ms) var(--ease-spring, ease) both;
  }

  .play-btn:hover:not(:disabled) {
    box-shadow: var(--play-glow-hover);
    transform: translateY(-1px);
  }

  .play-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    box-shadow: none;
  }

  .play-btn[aria-busy="true"] {
    opacity: 1;
    cursor: progress;
    box-shadow: var(--play-glow);
  }

  .play-btn.stop {
    background: var(--accent-danger, #ef4444);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent-danger, #ef4444) 38%, transparent);
  }

  .play-btn.stop:hover:not(:disabled) {
    background: var(--accent-danger, #ef4444);
    box-shadow: 0 12px 32px color-mix(in srgb, var(--accent-danger, #ef4444) 52%, transparent);
    transform: translateY(-1px);
  }

  :global(html.potato-pc) .play-btn.stop,
  :global(html.potato-pc) .play-btn.stop:hover:not(:disabled) {
    box-shadow: none;
    transform: none;
  }

  .play-text {
    font-weight: 800;
  }

  .play-text.play-phase {
    font-size: 13px;
    font-weight: 700;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 220px;
  }

  .play-pct {
    font-size: 11px;
    font-weight: 700;
    opacity: 0.85;
    flex-shrink: 0;
  }

  .poster-meta {
    margin: 0;
    min-width: 0;
    max-width: min(420px, 100%);
    font-size: 13px;
    font-weight: 600;
    color: var(--hero-fg-muted);
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.45);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    animation: poster-in var(--motion-enter, 320ms) var(--ease-spring, ease) both;
    animation-delay: var(--stagger-step, 48ms);
  }

  .poster-player {
    display: none;
  }

  .meta-chevron {
    display: none;
  }

  .poster-signin {
    padding: 0;
    height: auto;
    background: transparent;
    border: none;
    box-shadow: none;
    color: var(--hero-fg-muted);
    font-size: 12px;
    font-weight: 600;
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .poster-signin:hover:not(:disabled) {
    color: var(--hero-fg);
    background: transparent;
    transform: none;
  }

  .poster-storefront {
    position: absolute;
    inset: 0;
    z-index: 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    text-align: center;
  }

  .storefront-title {
    margin: 0;
    font-size: 18px;
    font-weight: 800;
    color: var(--hero-fg);
  }

  .storefront-hint {
    margin: 0 0 8px;
    max-width: 420px;
    font-size: 13px;
    color: var(--hero-fg-muted);
  }

  .storefront-ctas {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }

  .storefront-primary {
    height: 40px;
    padding: 0 16px;
    border-radius: var(--border-radius-md);
    box-shadow: var(--play-glow);
  }

  .storefront-ghost {
    border-radius: var(--border-radius-md);
    border: 1px solid var(--glass-border);
    background: var(--glass-bg);
    -webkit-backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
    backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
  }

  .crash-fix-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 10px;
    padding: 8px 10px;
    border-radius: var(--border-radius-md);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 35%, var(--glass-border));
    background: var(--glass-bg);
    -webkit-backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
    backdrop-filter: blur(var(--glass-blur, 10px)) saturate(var(--glass-saturate, 100%));
    box-shadow: inset 0 1px 0 var(--glass-highlight);
    max-width: 560px;
  }

  .crash-fix-banner-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 160px;
    font-size: 12px;
    color: var(--hero-fg-muted);
  }

  .crash-fix-banner-body strong {
    color: var(--hero-fg);
    font-size: 13px;
  }

  .crash-fix-banner-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }

  .crash-restore {
    padding: 6px 10px;
    font-size: 12px;
    height: auto;
  }

  .crash-fix-diag {
    padding: 6px 8px;
    font-size: 11px;
    font-weight: 500;
    border-radius: var(--border-radius-sm);
    color: var(--hero-fg-muted);
  }

  .crash-fix-diag:hover {
    color: var(--hero-fg);
    background: color-mix(in srgb, #fff 10%, transparent);
  }

  .spinner {
    width: 20px;
    height: 20px;
    border: 2.5px solid color-mix(in srgb, var(--on-accent, #000) 18%, transparent);
    border-top-color: var(--on-accent, #000);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes poster-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @container (max-width: 640px) {
    .poster-meta {
      flex-basis: 100%;
      max-width: 100%;
    }

    .poster-toolbar span {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .poster-art,
    .poster-toolbar,
    .play-btn,
    .poster-meta {
      filter: none;
      animation: none;
      animation-delay: 0ms;
    }

    .poster-art {
      inset: 0;
      width: 100%;
      height: 100%;
      transform: none;
    }

    .poster.launching .poster-art,
    .poster.launching .poster-procedural {
      filter: none;
    }
  }
</style>
