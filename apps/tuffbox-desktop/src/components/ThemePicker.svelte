<script lang="ts">
  import { Check } from "@lucide/svelte";
  import {
    THEMES,
    type ThemeId,
    previewTheme,
    restoreCommittedTheme,
    commitTheme,
  } from "../lib/themes";

  let {
    value = "tuffbox",
    onChange = () => {},
  }: {
    value?: ThemeId;
    onChange?: (id: ThemeId) => void;
  } = $props();

  const SHARP_THEMES = new Set<ThemeId>(["aether", "frost", "pixelato", "win95"]);

  function select(id: ThemeId) {
    commitTheme(id);
    onChange(id);
  }

  function badgeFor(id: ThemeId): string | null {
    if (id === "tuffbox-light") return "Light";
    if (SHARP_THEMES.has(id)) return "Sharp";
    return null;
  }
</script>

<div class="theme-grid">
  {#each THEMES as theme (theme.id)}
    {@const badge = badgeFor(theme.id)}
    <button
      type="button"
      class="theme-swatch"
      class:active={value === theme.id}
      class:sharp={SHARP_THEMES.has(theme.id)}
      style="background: {theme.shades[0]}"
      onclick={() => select(theme.id)}
      onmouseenter={() => previewTheme(theme.id)}
      onmouseleave={() => restoreCommittedTheme()}
      onfocus={() => previewTheme(theme.id)}
      onblur={() => restoreCommittedTheme()}
    >
      <div class="mini-ui" aria-hidden="true">
        <div class="bar" style="background: {theme.shades[1]}"></div>
        <div class="body">
          <div class="sidebar" style="background: {theme.shades[1]}"></div>
          <div class="panel" style="background: {theme.shades[1]}">
            <span class="dot" style="background: {theme.shades[2]}"></span>
            <span class="line" style="background: {theme.shades[2]}; opacity: 0.45"></span>
            <span class="line short" style="background: {theme.shades[2]}; opacity: 0.25"></span>
          </div>
        </div>
      </div>
      {#if value === theme.id}
        <div class="check" style="background: {theme.shades[2]}">
          <Check size={14} />
        </div>
      {/if}
      <span class="label-row">
        <span class="label">{theme.label}</span>
        {#if badge}<span class="badge">{badge}</span>{/if}
      </span>
    </button>
  {/each}
</div>

<style>
  .theme-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
  }

  .theme-swatch {
    position: relative;
    width: 168px;
    padding: 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    cursor: pointer;
    color: inherit;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 10px;
    transition:
      border-color var(--motion-fast, 160ms) ease,
      box-shadow var(--motion-fast, 160ms) ease,
      transform var(--motion-fast, 160ms) ease;
  }

  .theme-swatch.sharp {
    border-radius: 0;
  }

  .theme-swatch:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 45%, var(--border-color));
  }

  .theme-swatch.active {
    border-color: var(--accent-primary);
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--accent-primary) 55%, transparent),
      var(--shadow-md);
  }

  .mini-ui {
    height: 92px;
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .theme-swatch.sharp .mini-ui {
    border-radius: 0;
  }

  .bar {
    height: 12px;
    opacity: 0.9;
  }

  .body {
    display: flex;
    height: calc(100% - 12px);
  }

  .sidebar {
    width: 22%;
    opacity: 0.85;
  }

  .panel {
    flex: 1;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .dot {
    width: 11px;
    height: 11px;
    border-radius: 50%;
  }

  .theme-swatch.sharp .dot {
    border-radius: 0;
  }

  .line {
    height: 6px;
    border-radius: 3px;
    width: 80%;
  }

  .theme-swatch.sharp .line {
    border-radius: 0;
  }

  .line.short {
    width: 55%;
  }

  .check {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -70%);
    width: 30px;
    height: 30px;
    border-radius: 999px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #000;
    box-shadow: var(--shadow-md);
  }

  .theme-swatch.sharp .check {
    border-radius: 0;
  }

  .label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 0 2px 2px;
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-muted);
    border: 1px solid var(--border-color);
    border-radius: 999px;
    padding: 1px 6px;
    white-space: nowrap;
  }

  .theme-swatch.sharp .badge {
    border-radius: 0;
  }

  .theme-swatch.active .label {
    color: var(--text-primary);
  }

  .theme-swatch.active .badge {
    color: var(--accent-primary);
    border-color: color-mix(in srgb, var(--accent-primary) 40%, var(--border-color));
  }
</style>
