<script lang="ts">
  import {
    type QuestChapter,
    chapterToSnbtJson,
    stripLocaleOverlay,
  } from "../../lib/api";
  import { serializeSnbt, type SnbtValue } from "../../lib/snbtSerialize";

  interface Props {
    chapter: QuestChapter;
    onDiffVsDisk?: () => void;
  }

  let { chapter, onDiffVsDisk }: Props = $props();

  let copied = $state(false);

  let raw = $derived(
    serializeSnbt(stripLocaleOverlay(chapterToSnbtJson(chapter)) as SnbtValue)
  );

  function copyToClipboard() {
    navigator.clipboard.writeText(raw).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 1500);
    });
  }
</script>

<div class="raw-view">
  <div class="raw-header">
    <div class="raw-header-text">
      <span class="raw-title">{chapter.title} — Raw SNBT</span>
      <span class="raw-note">Locale title/description stripped (match save). Copy button.</span>
    </div>
    <div class="raw-actions">
      {#if onDiffVsDisk}
        <button type="button" class="btn ghost small" onclick={() => onDiffVsDisk()}>
          Diff vs disk
        </button>
      {/if}
      <button type="button" class="btn ghost small" onclick={copyToClipboard}>
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  </div>
  <pre class="raw-code"><code>{raw}</code></pre>
</div>

<style>
  .raw-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--ftbq-bg-panel, #212126);
  }
  .raw-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    flex-shrink: 0;
  }
  .raw-header-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .raw-title {
    font-size: 11px;
    color: var(--ftbq-title-gold, #f2c94c);
    font-weight: 600;
  }
  .raw-note {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .raw-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .raw-code {
    flex: 1;
    overflow: auto;
    padding: 12px;
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
    background: var(--ftbq-bg, #1a1a1e);
    color: var(--ftbq-text, #e8e8e8);
    white-space: pre;
    tab-size: 2;
  }
  .btn {
    padding: 4px 8px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0,0,0,0.25);
    color: var(--ftbq-text, #e8e8e8);
    font-size: 11px;
    font-weight: 600;
    border-radius: 2px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .btn:hover { border-color: var(--ftbq-accent-teal, #3db8a8); }
  .btn.ghost { background: transparent; }
  .btn.small { padding: 2px 6px; font-size: 10px; }
</style>
