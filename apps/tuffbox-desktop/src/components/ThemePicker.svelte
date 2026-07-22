<script lang="ts">
  import { Check } from "lucide-svelte";
  import {
    THEMES,
    type ThemeId,
    previewTheme,
    restoreCommittedTheme,
    commitTheme,
  } from "../lib/themes";

  export let value: ThemeId = "tuffbox";
  export let onChange: (id: ThemeId) => void = () => {};

  function select(id: ThemeId) {
    commitTheme(id);
    value = id;
    onChange(id);
  }
</script>

<div class="theme-grid">
  {#each THEMES as theme (theme.id)}
    <button
      type="button"
      class="theme-swatch"
      class:active={value === theme.id}
      style="background: {theme.shades[0]}"
      on:click={() => select(theme.id)}
      on:mouseenter={() => previewTheme(theme.id)}
      on:mouseleave={() => restoreCommittedTheme()}
      on:focus={() => previewTheme(theme.id)}
      on:blur={() => restoreCommittedTheme()}
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
      <span class="label">{theme.label}</span>
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
    width: 148px;
    padding: 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    cursor: pointer;
    color: inherit;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .theme-swatch.active {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 1px var(--accent-primary);
  }

  .mini-ui {
    height: 78px;
    border-radius: 6px;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.06);
  }

  .bar {
    height: 10px;
    opacity: 0.9;
  }

  .body {
    display: flex;
    height: calc(100% - 10px);
  }

  .sidebar {
    width: 22%;
    opacity: 0.85;
  }

  .panel {
    flex: 1;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .line {
    height: 6px;
    border-radius: 3px;
    width: 80%;
  }

  .line.short {
    width: 55%;
  }

  .check {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -70%);
    width: 28px;
    height: 28px;
    border-radius: 999px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #000;
    box-shadow: var(--shadow-md);
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    padding: 0 2px 2px;
  }

  .theme-swatch.active .label {
    color: var(--text-primary);
  }
</style>
