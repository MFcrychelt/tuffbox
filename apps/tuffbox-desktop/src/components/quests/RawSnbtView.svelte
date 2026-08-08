<script lang="ts">
  import {
    type QuestChapter,
    chapterToSnbtJson,
    stripLocaleOverlay,
  } from "../../lib/api";
  import { serializeSnbt, type SnbtValue } from "../../lib/snbtSerialize";

  interface Props {
    chapter: QuestChapter;
    selectedQuestId?: string | null;
    onDiffVsDisk?: () => void;
  }

  let { chapter, selectedQuestId = null, onDiffVsDisk }: Props = $props();

  let copied = $state(false);
  let findQuery = $state("");
  let matchIndex = $state(0);
  let preEl = $state<HTMLPreElement | null>(null);

  let raw = $derived(
    serializeSnbt(stripLocaleOverlay(chapterToSnbtJson(chapter)) as SnbtValue)
  );

  let lines = $derived(raw.split("\n"));

  let matchLineIndexes = $derived.by(() => {
    const q = findQuery.trim().toLowerCase();
    if (!q) return [] as number[];
    const hits: number[] = [];
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.toLowerCase().includes(q)) hits.push(i);
    }
    return hits;
  });

  $effect(() => {
    void findQuery;
    matchIndex = 0;
  });

  $effect(() => {
    const hits = matchLineIndexes;
    if (!hits.length || !preEl) return;
    const line = hits[Math.min(matchIndex, hits.length - 1)]!;
    const lineHeight = 16.5; // approx 11px * 1.5
    preEl.scrollTop = Math.max(0, line * lineHeight - 40);
  });

  function copyToClipboard() {
    navigator.clipboard.writeText(raw).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 1500);
    });
  }

  function jumpToQuest() {
    if (!selectedQuestId || !preEl) return;
    const needle = selectedQuestId.toLowerCase();
    const idx = lines.findIndex((ln) => ln.toLowerCase().includes(needle));
    if (idx < 0) return;
    findQuery = selectedQuestId;
    matchIndex = 0;
    const lineHeight = 16.5;
    preEl.scrollTop = Math.max(0, idx * lineHeight - 40);
  }

  function nextMatch(dir: 1 | -1) {
    const n = matchLineIndexes.length;
    if (!n) return;
    matchIndex = (matchIndex + dir + n) % n;
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
      <button
        type="button"
        class="btn ghost small"
        disabled={!selectedQuestId}
        onclick={jumpToQuest}
        title={selectedQuestId ? `Jump to ${selectedQuestId}` : "Select a quest first"}
      >
        Jump to quest
      </button>
      <button type="button" class="btn ghost small" onclick={copyToClipboard}>
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  </div>
  <div class="raw-find">
    <input
      type="search"
      placeholder="Find in SNBT…"
      bind:value={findQuery}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          nextMatch(e.shiftKey ? -1 : 1);
        }
      }}
    />
    <span class="find-count">
      {matchLineIndexes.length
        ? `${Math.min(matchIndex + 1, matchLineIndexes.length)}/${matchLineIndexes.length}`
        : findQuery.trim()
          ? "0"
          : ""}
    </span>
    <button type="button" class="btn ghost small" disabled={!matchLineIndexes.length} onclick={() => nextMatch(-1)}>Prev</button>
    <button type="button" class="btn ghost small" disabled={!matchLineIndexes.length} onclick={() => nextMatch(1)}>Next</button>
  </div>
  <pre class="raw-code" bind:this={preEl}><code>{raw}</code></pre>
</div>

<style>
  .raw-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--ftbq-bg-panel);
  }
  .raw-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border);
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
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .raw-find {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--ftbq-border);
    flex-shrink: 0;
  }
  .raw-find input {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    font-size: 11px;
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
    border-radius: 2px;
    color: var(--ftbq-text, #e8e8e8);
  }
  .find-count {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    min-width: 2.5em;
    text-align: right;
  }
  .raw-code {
    flex: 1;
    overflow: auto;
    padding: 12px;
    margin: 0;
    font-size: 11px;
    line-height: 1.5;
    font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace;
    background: var(--ftbq-bg);
    color: var(--ftbq-text, #e8e8e8);
    white-space: pre;
    tab-size: 2;
  }
  .btn {
    padding: 4px 8px;
    border: 1px solid var(--ftbq-border);
    background: rgba(0,0,0,0.25);
    color: var(--ftbq-text, #e8e8e8);
    font-size: 11px;
    font-weight: 600;
    border-radius: 2px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .btn:hover:not(:disabled) { border-color: var(--ftbq-accent-teal, #3db8a8); }
  .btn:disabled { opacity: 0.45; cursor: default; }
  .btn.ghost { background: transparent; }
  .btn.small { padding: 2px 6px; font-size: 10px; }
</style>
