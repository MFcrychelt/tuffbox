<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Sparkles, Check, X } from "lucide-svelte";
  import type { QuestPlan, QuestPlanMergeResult } from "../../lib/api";

  export let merge: QuestPlanMergeResult;
  export let needsReviewAck = false;

  const dispatch = createEventDispatcher<{
    apply: { chapterKeys: string[]; questKeys: string[] };
    discard: void;
  }>();

  let chapterOn: Record<string, boolean> = {};
  let questOn: Record<string, boolean> = {};
  let reviewAck = false;

  $: plan = merge?.plan as QuestPlan | undefined;
  $: initSelection(plan);

  function chKey(ch: { id?: string | null; title: string }, i: number) {
    return ch.id || ch.title || `ch-${i}`;
  }
  function qKey(q: { id?: string | null; title: string }, i: number) {
    return q.id || q.title || `q-${i}`;
  }

  function initSelection(p: QuestPlan | undefined) {
    if (!p) return;
    const nextCh: Record<string, boolean> = {};
    const nextQ: Record<string, boolean> = {};
    p.chapters.forEach((ch, ci) => {
      nextCh[chKey(ch, ci)] = true;
      ch.quests.forEach((q, qi) => {
        nextQ[qKey(q, qi)] = true;
      });
    });
    chapterOn = nextCh;
    questOn = nextQ;
  }

  function selectedChapterKeys(): string[] {
    return Object.entries(chapterOn)
      .filter(([, on]) => on)
      .map(([k]) => k);
  }
  function selectedQuestKeys(): string[] {
    return Object.entries(questOn)
      .filter(([, on]) => on)
      .map(([k]) => k);
  }

  function applyAll() {
    if (needsReviewAck && !reviewAck) return;
    dispatch("apply", {
      chapterKeys: selectedChapterKeys(),
      questKeys: selectedQuestKeys(),
    });
  }

  $: questCount =
    plan?.chapters.reduce((n, ch) => n + (ch.quests?.length ?? 0), 0) ?? 0;
</script>

{#if plan}
  <div class="review">
    <div class="review-h">
      <Sparkles size={14} />
      <strong>Review plan</strong>
      <span class="meta"
        >{questCount} quests · {(plan.confidence * 100).toFixed(0)}% · {plan.source ?? "ai"}</span
      >
    </div>
    <p class="expl">{plan.humanExplanation}</p>
    {#if merge.notes?.length}
      <ul class="notes">{#each merge.notes.slice(0, 8) as n, i (`n-${i}`)}<li>{n}</li>{/each}</ul>
    {/if}
    {#if merge.validation?.warnings?.length}
      <ul class="warns"
        >{#each merge.validation.warnings.slice(0, 6) as w, i (`w-${i}`)}<li>{w}</li>{/each}</ul
      >
    {/if}
    {#if merge.validation?.errors?.length}
      <ul class="errs"
        >{#each merge.validation.errors as e, i (`e-${i}`)}<li>{e}</li>{/each}</ul
      >
    {/if}

    <div class="tree">
      {#each plan.chapters as ch, ci (chKey(ch, ci))}
        <label class="ch">
          <input type="checkbox" bind:checked={chapterOn[chKey(ch, ci)]} />
          <span>{ch.title}</span>
          <small>{ch.quests?.length ?? 0}</small>
        </label>
        {#each ch.quests as q, qi (qKey(q, qi))}
          <label class="q">
            <input type="checkbox" bind:checked={questOn[qKey(q, qi)]} />
            <span>{q.title}</span>
            <small
              >{(q.description?.filter((d) => d.trim()).length ?? 0)} lore · {q.tasks?.length ?? 0}t · {q
                .rewards?.length ?? 0}r</small
            >
          </label>
        {/each}
      {/each}
    </div>

    {#if needsReviewAck || plan.needsUserReview}
      <label class="ack">
        <input type="checkbox" bind:checked={reviewAck} />
        I reviewed uncertain items / dependencies
      </label>
    {/if}

    <div class="actions">
      <button
        type="button"
        disabled={!merge.validation?.valid || ((needsReviewAck || plan.needsUserReview) && !reviewAck)}
        on:click={applyAll}
      >
        <Check size={14} /> Apply selected
      </button>
      <button type="button" class="ghost" on:click={() => dispatch("discard")}>
        <X size={14} /> Discard
      </button>
    </div>
  </div>
{/if}

<style>
  .review {
    border: 1px solid var(--border-color, #3a3a42);
    border-radius: 10px;
    padding: 10px;
    background: var(--bg-elevated, #212126);
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 42vh;
    overflow: auto;
  }
  .review-h {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .meta,
  .expl {
    color: var(--text-muted, #9a9aa0);
    font-size: 12px;
    margin: 0;
  }
  .tree {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ch,
  .q,
  .ack {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .q {
    padding-left: 18px;
    color: var(--text-secondary, #c8c8c8);
  }
  .ch small,
  .q small {
    margin-left: auto;
    color: var(--text-muted, #9a9aa0);
  }
  .notes,
  .warns,
  .errs {
    margin: 0;
    padding-left: 18px;
    font-size: 11px;
  }
  .warns {
    color: #fbbf24;
  }
  .errs {
    color: #f87171;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
</style>
