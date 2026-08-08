<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Stethoscope, ArrowRight, X, AlertTriangle } from "@lucide/svelte";
  import { launchWithFeedback } from "../lib/launch";
  import {
    projectPath,
    ideStageRequest,
    ideSuggestedStage,
    ideIssueCount,
    ideNeedsHealth,
    workTrail,
    clearWorkTrail,
    computeIdeNextAction,
    briefDirty,
    tuneDirty,
    questDirty,
    ideNextTrigger,
    ideIssuesRefresh,
    idePlayTrigger,
  } from "../lib/store";

  let {
    onGoStage,
  }: {
    onGoStage?: (stage: string) => void;
  } = $props();

  let refreshing = $state(false);
  let launching = $state(false);

  const next = $derived(
    computeIdeNextAction({
      issueCount: $ideIssueCount,
      needsHealth: $ideNeedsHealth,
      briefDirty: $briefDirty,
      tuneDirty: $tuneDirty,
      questDirty: $questDirty,
    }),
  );

  $effect(() => {
    if (next.stage) ideSuggestedStage.set(next.stage);
  });

  async function refreshIssues() {
    if (!$projectPath || refreshing) return;
    refreshing = true;
    try {
      const diags: { severity?: string; code?: string }[] = await invoke("get_diagnostics", {
        path: $projectPath,
      });
      const blocking = (diags ?? []).filter((d) => {
        const sev = String(d.severity ?? "");
        return sev === "Error" || sev === "error" || sev === "critical";
      });
      ideIssueCount.set(blocking.length);
    } catch {
      /* keep last count */
    } finally {
      refreshing = false;
    }
  }

  $effect(() => {
    if ($projectPath) {
      void refreshIssues();
    } else {
      ideIssueCount.set(0);
      ideNeedsHealth.set(false);
    }
  });

  $effect(() => {
    void $ideNextTrigger;
    if ($ideNextTrigger > 0) runNext();
  });

  $effect(() => {
    void $idePlayTrigger;
    if ($idePlayTrigger > 0) void runPlay();
  });

  $effect(() => {
    void $ideIssuesRefresh;
    if ($ideIssuesRefresh > 0) void refreshIssues();
  });

  function go(stage: string) {
    onGoStage?.(stage);
    ideStageRequest.set(stage);
  }

  function runNext() {
    if (next.stage) go(next.stage);
  }

  async function runPlay() {
    if (!$projectPath || launching) return;
    launching = true;
    try {
      await launchWithFeedback({ path: $projectPath, profile: "client" });
    } finally {
      launching = false;
    }
  }

  function onTrailAction(kind: string, stage?: string) {
    if (kind === "dismiss") {
      clearWorkTrail();
      return;
    }
    if (kind === "play") {
      clearWorkTrail();
      void runPlay();
      return;
    }
    if (kind === "stage" && stage) {
      clearWorkTrail();
      go(stage);
    }
  }

  /** Expose for parent (optional). */
  export function triggerNext() {
    runNext();
  }

  export function refresh() {
    void refreshIssues();
  }
</script>

<div class="ide-next-bar">
  <div class="ide-next-status">
    {#if $ideIssueCount > 0}
      <span class="pill warn">
        <AlertTriangle size={12} />
        {$ideIssueCount} pack issue{$ideIssueCount === 1 ? "" : "s"}
      </span>
    {:else if $ideNeedsHealth}
      <span class="pill warn"><Stethoscope size={12} /> Needs Health check</span>
    {:else}
      <span class="pill ok">Pack graph OK</span>
    {/if}
    {#if next.detail}
      <span class="detail">{next.detail}</span>
    {/if}
  </div>

  <div class="ide-next-main">
    <span class="next-label">Next</span>
    <button type="button" class="next-cta" onclick={runNext} disabled={launching}>
      {next.label}
      <ArrowRight size={14} />
    </button>
  </div>

  <div class="ide-next-actions">
    <button type="button" class="ghost" onclick={() => go("diagnose")} title="Health check">
      <Stethoscope size={14} />
      Health
    </button>
  </div>
</div>

{#if $workTrail}
  <div class="ide-work-trail" role="status">
    <span class="trail-msg">{$workTrail.message}</span>
    <div class="trail-actions">
      {#each $workTrail.actions as act (act.id)}
        <button
          type="button"
          class={act.kind === "dismiss" ? "ghost mini" : "secondary mini"}
          onclick={() => onTrailAction(act.kind, act.stage)}
        >
          {act.label}
        </button>
      {/each}
      <button type="button" class="icon-x" onclick={() => clearWorkTrail()} title="Dismiss" aria-label="Dismiss">
        <X size={14} />
      </button>
    </div>
  </div>
{/if}

<style>
  .ide-next-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-secondary) 88%, transparent);
    flex-shrink: 0;
  }
  .ide-next-status {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
  }
  .pill.ok {
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }
  .pill.warn {
    background: rgba(245, 158, 11, 0.14);
    color: var(--accent-warning);
  }
  .detail {
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ide-next-main {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 140px;
  }
  .next-label {
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .next-cta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--border-radius-sm);
    border: none;
    background: var(--accent-primary);
    color: #000;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .next-cta:disabled { opacity: 0.6; cursor: not-allowed; }
  .ide-next-actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
  .ide-next-actions .ghost {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .ide-next-actions .ghost:hover { color: var(--text-primary); }
  .ide-work-trail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid rgba(27, 217, 106, 0.25);
    background: rgba(27, 217, 106, 0.08);
    font-size: 12px;
    flex-shrink: 0;
  }
  .trail-msg { color: var(--text-primary); font-weight: 600; }
  .trail-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .secondary.mini, .ghost.mini {
    padding: 4px 8px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .secondary.mini {
    background: var(--bg-primary);
    color: var(--text-primary);
  }
  .ghost.mini { background: transparent; color: var(--text-muted); }
  .icon-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 4px;
    cursor: pointer;
    display: inline-flex;
  }
</style>
