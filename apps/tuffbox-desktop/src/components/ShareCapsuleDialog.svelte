<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Share2 } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";

  const dispatch = createEventDispatcher<{ share: void; dismiss: void }>();

  export let explanation = "";
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
    <h3 id="share-capsule-title">Share fix with the network?</h3>
    <p>
      Your last crash fix worked. Publish an ExperienceCapsule (no raw logs) so peers can reuse the plan.
    </p>
    {#if explanation}
      <p class="sc-excerpt">{explanation}</p>
    {/if}
    <div class="sc-actions">
      <button class="ghost" type="button" on:click={() => dispatch("dismiss")}>Not now</button>
      <button type="button" on:click={() => dispatch("share")}>Share with network</button>
    </div>
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
    width: min(440px, 92vw);
    text-align: center;
    box-shadow: var(--shadow-lg);
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
  .sc-dialog p {
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.5;
    margin-bottom: 12px;
  }
  .sc-excerpt {
    background: var(--bg-elevated);
    border-radius: 8px;
    padding: 10px;
    font-size: 12px;
    text-align: left;
    max-height: 120px;
    overflow: auto;
  }
  .sc-actions {
    display: flex;
    gap: 10px;
    justify-content: center;
    margin-top: 16px;
  }
</style>
