<script lang="ts">
  import { createEventDispatcher } from "svelte";

  /** ActionPlan review modal (replaces window.confirm) — pure presentation.
   *  Parent (Diagnostics.svelte) owns building/applying the plan; this
   *  component only lets the user pick which rows to apply and acknowledge
   *  destructive plans, then reports back via events. */
  export let open = false;
  export let source: "ai" | "network" = "ai";
  export let explanation = "";
  export let hasDestructive = false;
  export let rows: {
    key: string;
    selected: boolean;
    op: string;
    modId: string | null;
    path: string | null;
    patchPreview: string | null;
    reason: string;
    risk: string;
    diffKind?: "add" | "remove" | "change" | "other";
    destructive?: boolean;
    raw: any;
  }[] = [];
  export let needsAck = false;
  export let acknowledged = false;
  export let canApply = false;
  export let selectedCount = 0;
  export let busy = false;

  const dispatch = createEventDispatcher<{ cancel: void; confirm: void }>();
</script>

{#if open}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    on:click|self={() => dispatch("cancel")}
    on:keydown={() => {}}
  >
    <div class="modal plan-review-modal" role="dialog" aria-modal="true">
      <div class="modal-header">
        <div>
          <h2>{source === "network" ? "Review network ActionPlan" : "Review AI ActionPlan"}</h2>
          <p>Snapshot will be created first. Uncheck actions you do not want applied.</p>
        </div>
        <button class="icon-btn" type="button" on:click={() => dispatch("cancel")} aria-label="Close">×</button>
      </div>
      <p class="plan-review-expl">{explanation}</p>
      {#if source === "network" && hasDestructive}
        <p class="plan-review-warn">
          This plan includes destructive actions (disable/remove). A snapshot will be created first — use Restore on the home screen if something breaks.
        </p>
      {/if}
      <div class="plan-review-list">
        {#each rows as row (row.key)}
          <label class="plan-review-row">
            <input type="checkbox" bind:checked={row.selected} />
            <div class="plan-review-body">
              <div class="plan-review-top">
                <span class="diff-chip {row.diffKind ?? 'other'}">
                  {row.diffKind === "add" ? "+" : row.diffKind === "remove" ? "−" : row.diffKind === "change" ? "~" : "·"}
                </span>
                <strong>{row.op}</strong>
                {#if row.modId}<code>{row.modId}</code>{/if}
                <span class="risk-pill">{row.risk}</span>
              </div>
              {#if row.reason}<p>{row.reason}</p>{/if}
              {#if row.patchPreview}
                <pre class="patch-preview">{row.patchPreview}</pre>
              {/if}
            </div>
          </label>
        {/each}
      </div>
      {#if needsAck}
        <label class="plan-review-ack">
          <input type="checkbox" bind:checked={acknowledged} />
          I reviewed these actions (required — plan flagged needsUserReview)
        </label>
      {/if}
      <div class="plan-review-actions">
        <button class="ghost" type="button" on:click={() => dispatch("cancel")}>Cancel</button>
        <button
          class="primary"
          type="button"
          disabled={!canApply || busy}
          on:click={() => dispatch("confirm")}
        >
          Apply {selectedCount} action{selectedCount === 1 ? "" : "s"} (snapshot first)
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .diff-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }
  .diff-chip.add { color: #86efac; border-color: rgba(34, 197, 94, 0.35); background: rgba(34, 197, 94, 0.1); }
  .diff-chip.remove { color: #fca5a5; border-color: rgba(239, 68, 68, 0.35); background: rgba(239, 68, 68, 0.1); }
  .diff-chip.change { color: #fde68a; border-color: rgba(245, 158, 11, 0.35); background: rgba(245, 158, 11, 0.1); }
  .plan-review-warn {
    margin: 0 0 10px !important;
    padding: 8px 10px;
    border-radius: var(--border-radius-sm);
    font-size: 12px !important;
    color: #fde68a !important;
    background: rgba(245, 158, 11, 0.1);
    border: 1px solid rgba(245, 158, 11, 0.28);
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    padding: 16px;
  }
  .plan-review-modal {
    width: min(560px, 100%);
    max-height: 85vh;
    overflow: auto;
    padding: 16px 18px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .plan-review-modal .modal-header {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .plan-review-modal h2 { margin: 0 0 4px; font-size: 16px; }
  .plan-review-modal p { margin: 0; font-size: 13px; color: var(--text-muted); }
  .plan-review-expl { margin: 0 0 12px !important; color: var(--text-secondary) !important; }
  .plan-review-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px; }
  .plan-review-row {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    cursor: pointer;
  }
  .plan-review-top { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .plan-review-body p { margin: 4px 0 0; font-size: 12px; color: var(--text-secondary); }
  .patch-preview {
    margin: 6px 0 0;
    padding: 8px;
    max-height: 120px;
    overflow: auto;
    font-size: 11px;
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--text-muted);
  }
  .plan-review-ack {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
    margin-bottom: 12px;
  }
  .plan-review-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .risk-pill {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    color: var(--text-muted);
  }
</style>
