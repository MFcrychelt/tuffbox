<script lang="ts">
  import { CheckCircle, AlertTriangle, CircleHelp, Loader2 } from "@lucide/svelte";
  import {
    type Problem,
    countBySeverity,
    hasBlockingProblems,
    severityChip,
  } from "./problemModel";

  let {
    problems = [],
    sessionOk = false,
    loading = false,
    analyzing = false,
    sourceLabel = "Game log",
    onScrollToWarnings,
  }: {
    problems?: Problem[];
    sessionOk?: boolean;
    loading?: boolean;
    analyzing?: boolean;
    sourceLabel?: string;
    onScrollToWarnings?: () => void;
  } = $props();

  const counts = $derived(countBySeverity(problems));
  const blocking = $derived(hasBlockingProblems(problems));
  const warnOnly = $derived(!blocking && counts.warning + counts.info > 0);
</script>

<div
  class="dx-status"
  class:ok={sessionOk && !blocking}
  class:warn={blocking}
  class:neutral={!sessionOk && !blocking && !warnOnly}
  class:soft={warnOnly && sessionOk}
>
  <div class="dx-status-icon">
    {#if loading || analyzing}
      <Loader2 size={18} class="spin" />
    {:else if sessionOk && !blocking}
      <CheckCircle size={18} />
    {:else if blocking}
      <AlertTriangle size={18} />
    {:else}
      <CircleHelp size={18} />
    {/if}
  </div>
  <div class="dx-status-body">
    {#if loading && problems.length === 0}
      <strong>Loading…</strong>
      <span>Reading logs and pack graph</span>
    {:else if analyzing}
      <strong>Analyzing…</strong>
      <span>Rules and AI are scanning this source</span>
    {:else if sessionOk && !blocking}
      <strong>Healthy</strong>
      <span>
        Last launch looked fine
        {#if warnOnly}
          ·
          <button type="button" class="linkish" onclick={() => onScrollToWarnings?.()}>
            {counts.warning + counts.info} worth checking
          </button>
        {/if}
        · {sourceLabel}
      </span>
    {:else if blocking}
      <strong>Needs fix</strong>
      <span>
        {#if counts.critical}{counts.critical} critical{/if}
        {#if counts.critical && counts.error} · {/if}
        {#if counts.error}{counts.error} error{counts.error === 1 ? "" : "s"}{/if}
        {#if counts.warning}
          · {counts.warning} warning{counts.warning === 1 ? "" : "s"}
        {/if}
        · {sourceLabel}
      </span>
    {:else if warnOnly}
      <strong>{severityChip("warning")}</strong>
      <span>{counts.warning + counts.info} item(s) · {sourceLabel}</span>
    {:else}
      <strong>No clear signal yet</strong>
      <span>Re-analyze or Test launch · {sourceLabel}</span>
    {/if}
  </div>
</div>

<style>
  .dx-status {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    margin-bottom: 14px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .dx-status.warn {
    border-color: rgba(245, 158, 11, 0.45);
    background: linear-gradient(135deg, rgba(245, 158, 11, 0.1), var(--bg-secondary) 70%);
  }
  .dx-status.ok,
  .dx-status.soft {
    border-color: rgba(27, 217, 106, 0.35);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.08), var(--bg-secondary) 70%);
  }
  .dx-status-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--border-radius-sm);
    background: var(--bg-tertiary);
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .dx-status.warn .dx-status-icon { color: var(--accent-warning); background: rgba(245, 158, 11, 0.14); }
  .dx-status.ok .dx-status-icon,
  .dx-status.soft .dx-status-icon { color: var(--accent-primary); background: rgba(27, 217, 106, 0.14); }
  .dx-status-body { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .dx-status-body strong { font-size: 13px; color: var(--text-primary); }
  .dx-status-body span { font-size: 12px; color: var(--text-muted); }
  .linkish {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent-primary);
    font: inherit;
    font-weight: 700;
    cursor: pointer;
    text-decoration: underline;
  }
  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
