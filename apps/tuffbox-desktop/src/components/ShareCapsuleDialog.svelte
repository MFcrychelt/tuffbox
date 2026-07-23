<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { Share2, Pencil, Check, X } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";
  import { invoke } from "@tauri-apps/api/core";

  export let path = "";
  export let resolutionId: string | null = null;
  /** Optional seed explanation while distill loads */
  export let seedExplanation = "";

  type DistillAction = {
    op: string;
    modId?: string | null;
    projectId?: string | null;
    version?: string | null;
    path?: string | null;
    reason?: string | null;
    risk?: string;
  };

  type DistillPlan = {
    humanExplanation?: string;
    confidence?: number;
    actions?: DistillAction[];
    fingerprintKey?: string;
    distillSource?: string;
    resolutionId?: string;
    beta?: boolean;
  };

  const dispatch = createEventDispatcher<{
    confirm: { humanExplanation: string; actions: DistillAction[]; fingerprintKey: string | null };
    dismiss: void;
  }>();

  let loading = true;
  let error: string | null = null;
  let plan: DistillPlan | null = null;
  let editing = false;
  let editExplanation = "";
  let editActionsJson = "";
  let editError: string | null = null;
  let confirmBusy = false;

  onMount(() => {
    void runDistill();
  });

  async function runDistill() {
    if (!path) {
      loading = false;
      error = "Missing project path";
      return;
    }
    loading = true;
    error = null;
    try {
      plan = await invoke<DistillPlan>("distill_resolved_crash_plan", {
        path,
        resolutionId,
      });
      editExplanation = plan?.humanExplanation ?? seedExplanation ?? "";
      editActionsJson = JSON.stringify(plan?.actions ?? [], null, 2);
    } catch (e) {
      error = String(e);
      plan = {
        humanExplanation: seedExplanation || "Could not distill plan — review before sharing.",
        actions: [],
        distillSource: "fallback_error",
      };
      editExplanation = plan.humanExplanation ?? "";
      editActionsJson = "[]";
    } finally {
      loading = false;
    }
  }

  function actionLabel(a: DistillAction): string {
    const target = a.modId || a.projectId || a.path || "-";
    return `${a.op} ${target}${a.version ? ` → ${a.version}` : ""}`;
  }

  function startEdit() {
    editing = true;
    editError = null;
    editExplanation = plan?.humanExplanation ?? editExplanation;
    editActionsJson = JSON.stringify(plan?.actions ?? [], null, 2);
  }

  function applyEdit() {
    editError = null;
    let actions: DistillAction[];
    try {
      const parsed = JSON.parse(editActionsJson);
      if (!Array.isArray(parsed)) {
        editError = "Actions must be a JSON array";
        return;
      }
      actions = parsed;
    } catch {
      editError = "Invalid JSON for actions";
      return;
    }
    plan = {
      ...(plan ?? {}),
      humanExplanation: editExplanation.trim() || plan?.humanExplanation || "",
      actions,
      distillSource: "user_edited",
    };
    editing = false;
  }

  async function onConfirm() {
    if (!plan || confirmBusy) return;
    confirmBusy = true;
    try {
      dispatch("confirm", {
        humanExplanation: plan.humanExplanation ?? "",
        actions: plan.actions ?? [],
        fingerprintKey: plan.fingerprintKey ?? null,
      });
    } finally {
      confirmBusy = false;
    }
  }
</script>

<div
  class="sc-backdrop"
  role="button"
  tabindex="-1"
  on:click={(e) => e.target === e.currentTarget && dispatch("dismiss")}
  on:keydown={() => {}}
>
  <div
    class="sc-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="share-capsule-title"
    use:trapFocus={{ onEscape: () => dispatch("dismiss") }}
  >
    <div class="sc-icon"><Share2 size={28} /></div>
    <h3 id="share-capsule-title">Share efficient fix with TuffSwarm?</h3>
    <p class="sc-lead">
      Beta: AI distilled your fix path into a minimal plan. Confirm if it looks right, or edit so peers
      do not repeat mistakes.
    </p>

    {#if loading}
      <p class="sc-status">AI analyzing your fix history…</p>
    {:else if error && !(plan?.actions?.length || plan?.humanExplanation)}
      <p class="sc-error">{error}</p>
    {:else}
      {#if plan?.distillSource}
        <p class="sc-meta">
          Source: {plan.distillSource}
          {#if plan.confidence != null}
            · confidence {Math.round(plan.confidence * 100)}%
          {/if}
        </p>
      {/if}

      {#if editing}
        <label class="sc-label" for="distill-explanation">Explanation</label>
        <textarea id="distill-explanation" class="sc-textarea" rows="3" bind:value={editExplanation}></textarea>
        <label class="sc-label" for="distill-actions">Actions (JSON)</label>
        <textarea id="distill-actions" class="sc-textarea sc-code" rows="8" bind:value={editActionsJson}></textarea>
        {#if editError}
          <p class="sc-error">{editError}</p>
        {/if}
        <div class="sc-actions">
          <button class="ghost" type="button" on:click={() => (editing = false)}>Cancel edit</button>
          <button type="button" on:click={applyEdit}><Check size={14} /> Apply edits</button>
        </div>
      {:else}
        <div class="sc-excerpt">{plan?.humanExplanation || seedExplanation}</div>
        {#if (plan?.actions ?? []).length}
          <ul class="sc-actions-list">
            {#each plan?.actions ?? [] as a, i (i)}
              <li>
                <code>{actionLabel(a)}</code>
                {#if a.reason}
                  <span class="sc-reason">{a.reason}</span>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="sc-muted">No structured actions — explanation only will be shared.</p>
        {/if}
        <div class="sc-actions">
          <button class="ghost" type="button" on:click={() => dispatch("dismiss")}>
            <X size={14} /> Not now
          </button>
          <button class="ghost" type="button" on:click={startEdit}>
            <Pencil size={14} /> Edit
          </button>
          <button type="button" disabled={confirmBusy} on:click={onConfirm}>
            <Check size={14} /> {confirmBusy ? "Sharing…" : "Confirm & share"}
          </button>
        </div>
      {/if}
    {/if}

    {#if loading}
      <div class="sc-actions">
        <button class="ghost" type="button" on:click={() => dispatch("dismiss")}>Not now</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .sc-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 220;
    backdrop-filter: blur(8px);
  }
  .sc-dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    padding: 28px;
    width: min(520px, 94vw);
    text-align: center;
    box-shadow: var(--shadow-lg);
    max-height: min(88vh, 720px);
    overflow: auto;
  }
  .sc-icon {
    margin-bottom: 12px;
    color: var(--accent-primary);
  }
  .sc-dialog h3 {
    font-size: 18px;
    margin-bottom: 8px;
    color: var(--text-primary);
  }
  .sc-lead,
  .sc-status,
  .sc-meta,
  .sc-muted {
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.5;
    margin-bottom: 12px;
  }
  .sc-meta {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .sc-error {
    color: var(--danger, #e57373);
    font-size: 13px;
    margin-bottom: 10px;
  }
  .sc-excerpt {
    background: var(--bg-elevated);
    border-radius: 8px;
    padding: 10px;
    font-size: 12px;
    text-align: left;
    color: var(--text-primary);
    margin-bottom: 12px;
    white-space: pre-wrap;
  }
  .sc-actions-list {
    list-style: none;
    padding: 0;
    margin: 0 0 14px;
    text-align: left;
  }
  .sc-actions-list li {
    padding: 8px 10px;
    border: 1px solid var(--border-color);
    border-radius: 8px;
    margin-bottom: 6px;
    font-size: 12px;
  }
  .sc-actions-list code {
    font-size: 12px;
    color: var(--text-primary);
  }
  .sc-reason {
    display: block;
    margin-top: 4px;
    color: var(--text-muted);
  }
  .sc-label {
    display: block;
    text-align: left;
    font-size: 12px;
    color: var(--text-muted);
    margin-bottom: 4px;
  }
  .sc-textarea {
    width: 100%;
    box-sizing: border-box;
    margin-bottom: 10px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-primary);
    padding: 8px 10px;
    font-size: 12px;
    font-family: inherit;
    resize: vertical;
  }
  .sc-code {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  .sc-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: center;
    margin-top: 8px;
  }
  .sc-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
