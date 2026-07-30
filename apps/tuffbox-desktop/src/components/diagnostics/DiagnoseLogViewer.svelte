<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Terminal, Search, Maximize2, Copy, Share2 } from "lucide-svelte";

  /** Crash/game log viewer: search, syntax coloring, virtualized render window,
   *  error cycling. Parent (Diagnostics.svelte) owns log source selection,
   *  copy/share (network) actions, and the "jump to error" API shared with
   *  the verdict hero / tools strip / triage panel — this component exposes
   *  `scrollToLine()` so the parent can drive scrolling into this viewer's
   *  own (otherwise private) render window. */
  export let logDisplayText = "";
  export let currentLogTextLength = 0;
  export let sourceLabel = "log";
  export let signalLineMap: Map<number, string> = new Map();
  export let errorHits: number[] = [];
  export let activeErrorHit = -1;
  export let sharingLog = false;
  export let hasLogText = false;
  /** Changes whenever the user switches log source — resets the truncation window. */
  export let sourceKey = "";

  const dispatch = createEventDispatcher<{ jumpNextError: void; copy: void; share: void }>();

  let logQuery = "";
  let logMatches: { line: number }[] = [];
  let activeMatch = 0;
  let logWrap = true;
  let logExpanded = false;
  let logPreEl: HTMLElement | null = null;

  const LOG_RENDER_CAP = 400;
  let logShowAll = false;
  let lastSourceKey = "";
  $: if (sourceKey !== lastSourceKey) {
    lastSourceKey = sourceKey;
    logShowAll = false;
  }

  $: logLines = logDisplayText ? logDisplayText.split("\n") : [];
  $: logLineCount = logLines.length;
  $: recomputeLogMatches(logDisplayText);
  $: logRenderOffset =
    logShowAll || logLineCount <= LOG_RENDER_CAP ? 0 : logLineCount - LOG_RENDER_CAP;
  $: visibleLogLines = logLines.slice(logRenderOffset);
  $: hiddenLogLineCount = logRenderOffset;

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  function markQuery(html: string, query: string): string {
    if (!query) return html;
    const q = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return html.replace(new RegExp(`(${q})`, "gi"), "<mark>$1</mark>");
  }

  /** Minecraft / log4j-ish line coloring (safe: escapes first). */
  function colorizeLogLine(line: string, query: string): string {
    let html = escapeHtml(line);
    if (/^\s+at\s+\S/.test(line) || /^\s+\.\.\.\s+\d+\s+more/.test(line)) {
      return `<span class="tok-stack">${markQuery(html, query)}</span>`;
    }
    if (/^Caused by:/i.test(line) || /^Suppressed:/i.test(line)) {
      return `<span class="tok-caused">${markQuery(html, query)}</span>`;
    }
    if (/^-{5,}|^----\s*Minecraft Crash Report/i.test(line) || /^\/\/ /i.test(line)) {
      return `<span class="tok-section">${markQuery(html, query)}</span>`;
    }
    html = html.replace(
      /^((?:\[[^\]]+\]\s*){1,3}|\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:\s+\[[^\]]+\])?)/,
      '<span class="tok-time">$1</span>',
    );
    html = html.replace(/\b(FATAL|ERROR|SEVERE)\b/g, '<span class="tok-error">$1</span>');
    html = html.replace(/\b(WARN(?:ING)?)\b/g, '<span class="tok-warn">$1</span>');
    html = html.replace(/\b(INFO|DEBUG|TRACE)\b/g, '<span class="tok-info">$1</span>');
    html = html.replace(
      /\b([a-z0-9_.-]+\.(?:Exception|Error|Throwable))\b/gi,
      '<span class="tok-exc">$1</span>',
    );
    html = html.replace(
      /\b(mod\s+[a-z0-9_-]+|[a-z0-9_-]+:[a-z0-9_./-]+)\b/gi,
      '<span class="tok-mod">$1</span>',
    );
    return markQuery(html, query);
  }

  function signalClass(kind: string | undefined): string {
    if (!kind) return "";
    const k = kind.toLowerCase();
    if (k.includes("entry") || k.includes("error") || k.includes("crash")) return "sig-error";
    if (k.includes("warn") || k.includes("mismatch") || k.includes("perf")) return "sig-warn";
    return "sig-info";
  }

  function recomputeLogMatches(text: string) {
    if (!logQuery) {
      logMatches = [];
      activeMatch = 0;
      return;
    }
    const lower = text.toLowerCase();
    const q = logQuery.toLowerCase();
    const found: { line: number }[] = [];
    let from = 0;
    while (true) {
      const idx = lower.indexOf(q, from);
      if (idx < 0) break;
      const line = text.slice(0, idx).split("\n").length - 1;
      found.push({ line });
      from = idx + q.length;
    }
    logMatches = found;
    activeMatch = found.length ? Math.min(activeMatch, found.length - 1) : 0;
  }

  function jumpToMatch(dir: 1 | -1) {
    if (!logMatches.length) return;
    activeMatch = (activeMatch + dir + logMatches.length) % logMatches.length;
    scrollToLine(logMatches[activeMatch].line);
  }

  /** Imperative API used by the parent's shared "jump to error" handlers
   *  (verdict hero / tools strip / triage panel jumpLine event). */
  export function scrollToLine(line: number) {
    if (!logPreEl) return;
    const oneBased = line + 1;
    if (oneBased <= logRenderOffset) {
      logShowAll = true;
      requestAnimationFrame(() => {
        const target = logPreEl?.querySelector(`div.log-line[data-ln="${oneBased}"]`) as HTMLElement | undefined;
        target?.scrollIntoView({ block: "center", behavior: "smooth" });
      });
      return;
    }
    const target = logPreEl.querySelector(`div.log-line[data-ln="${oneBased}"]`) as HTMLElement | undefined;
    target?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
</script>

<!-- Log viewer (after verdict, plan, and evidence) -->
<section class="log-viewer panel" class:expanded={logExpanded}>
  <div class="log-viewer-head">
    <div class="log-viewer-title">
      <Terminal size={16} />
      <strong>Log</strong>
      <span class="log-meta">
        {sourceLabel}
        · {logLineCount.toLocaleString()} lines
        {#if currentLogTextLength > logDisplayText.length}
          · last {(logDisplayText.length / 1024).toFixed(0)} KB
        {/if}
      </span>
    </div>
    <div class="log-viewer-actions">
      <label class="log-search">
        <Search size={13} />
        <input
          type="search"
          placeholder="Find in log…"
          bind:value={logQuery}
          on:keydown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              jumpToMatch(e.shiftKey ? -1 : 1);
            }
          }}
        />
        {#if logQuery}
          <span class="log-match-count">
            {logMatches.length ? `${activeMatch + 1}/${logMatches.length}` : "0"}
          </span>
          <button type="button" class="ghost mini" on:click={() => jumpToMatch(-1)} disabled={!logMatches.length}>↑</button>
          <button type="button" class="ghost mini" on:click={() => jumpToMatch(1)} disabled={!logMatches.length}>↓</button>
        {/if}
      </label>
      <button type="button" class="ghost mini" class:active={logWrap} on:click={() => (logWrap = !logWrap)} title="Toggle wrap">
        Wrap
      </button>
      <button type="button" class="ghost mini" on:click={() => (logExpanded = !logExpanded)} title="Toggle height">
        <Maximize2 size={13} /> {logExpanded ? "Compact" : "Tall"}
      </button>
      <button
        type="button"
        class="ghost mini"
        on:click={() => dispatch("jumpNextError")}
        disabled={!errorHits.length}
        title={errorHits.length ? `Next error (${errorHits.length})` : "No error lines"}
      >
        Error{errorHits.length ? ` ${(activeErrorHit < 0 ? 0 : activeErrorHit) + 1}/${errorHits.length}` : ""}
      </button>
      <button type="button" class="ghost mini" on:click={() => dispatch("copy")} disabled={!hasLogText}><Copy size={13} /></button>
      <button type="button" class="ghost mini" on:click={() => dispatch("share")} disabled={sharingLog || !hasLogText}>
        <Share2 size={13} />
      </button>
    </div>
  </div>

  {#if logLines.length}
    {#if hiddenLogLineCount > 0 && !logShowAll}
      <div class="log-trunc-banner">
        <span>Showing last {LOG_RENDER_CAP.toLocaleString()} of {logLineCount.toLocaleString()} lines</span>
        <button type="button" class="secondary small" on:click={() => (logShowAll = true)}>
          Show {hiddenLogLineCount.toLocaleString()} earlier lines
        </button>
      </div>
    {/if}
    <div
      class="log-stage"
      class:nowrap={!logWrap}
      bind:this={logPreEl}
      role="log"
      aria-label="Crash or game log"
    >
      {#each visibleLogLines as line, vi (logRenderOffset + vi)}
        {@const lineIndex = logRenderOffset + vi}
        <div
          class="log-line {signalClass(signalLineMap.get(lineIndex + 1))}"
          class:active-match={logMatches[activeMatch]?.line === lineIndex}
          data-ln={lineIndex + 1}
        >
          <span class="ln">{lineIndex + 1}</span>
          <span class="ll">{@html colorizeLogLine(line, logQuery)}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="muted-box">No log yet — pick latest.log or a crash report above, then Refresh.</div>
  {/if}
</section>

<style>
  .panel { padding: 16px; min-width: 0; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .muted-box { padding: 12px; border-radius: 10px; border: 1px dashed var(--border-color); color: var(--text-muted); font-size: 12px; }
  .log-viewer {
    margin-bottom: 14px;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: var(--bg-secondary);
  }
  .log-viewer-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .log-viewer-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    color: var(--text-primary);
    font-size: 13px;
  }
  .log-meta { color: var(--text-muted); font-size: 11px; font-weight: 500; }
  .log-viewer-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .log-search {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 4px 8px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-muted);
    font-weight: 500;
  }
  .log-search input {
    width: min(220px, 36vw);
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    outline: none;
  }
  .log-match-count { font-size: 11px; color: var(--text-muted); min-width: 36px; text-align: center; }
  .log-viewer-actions .ghost.mini.active {
    border-color: rgba(27, 217, 106, 0.45);
    color: var(--accent-primary);
  }
  .log-stage {
    flex: 1;
    min-height: 200px;
    max-height: min(62vh, 720px);
    overflow: auto;
    background: #0a0a0c;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
  }
  .log-viewer.expanded .log-stage {
    max-height: min(86vh, 1100px);
  }
  .log-trunc-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-color);
    background: rgba(250, 204, 21, 0.06);
    font-size: 12px;
    color: var(--text-muted);
  }
  .log-line {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 0;
    padding: 0 10px 0 0;
    border-left: 2px solid transparent;
  }
  .log-line:hover { background: rgba(255, 255, 255, 0.03); }
  .log-line.active-match { background: rgba(250, 204, 21, 0.12); }
  .log-line.sig-error { border-left-color: #f87171; background: rgba(248, 113, 113, 0.06); }
  .log-line.sig-warn { border-left-color: #fbbf24; background: rgba(251, 191, 36, 0.05); }
  .log-line.sig-info { border-left-color: #60a5fa; }
  .ln {
    user-select: none;
    text-align: right;
    padding: 0 10px 0 8px;
    color: #52525b;
    background: #111114;
    border-right: 1px solid #1f1f23;
  }
  .ll {
    padding: 0 8px;
    color: #d4d4d8;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .log-stage.nowrap .ll {
    white-space: pre;
    overflow-wrap: normal;
  }
  .log-stage :global(mark) {
    background: rgba(250, 204, 21, 0.45);
    color: #fff;
    border-radius: 2px;
    padding: 0 1px;
  }
  .log-stage :global(.tok-time) { color: #71717a; }
  .log-stage :global(.tok-error) { color: #f87171; font-weight: 700; }
  .log-stage :global(.tok-warn) { color: #fbbf24; font-weight: 700; }
  .log-stage :global(.tok-info) { color: #38bdf8; }
  .log-stage :global(.tok-stack) { color: #a1a1aa; }
  .log-stage :global(.tok-caused) { color: #fb7185; font-weight: 700; }
  .log-stage :global(.tok-section) { color: #c4b5fd; font-weight: 700; }
  .log-stage :global(.tok-exc) { color: #f472b6; }
  .log-stage :global(.tok-mod) { color: #4ade80; }
</style>
