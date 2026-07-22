<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Network } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";

  const dispatch = createEventDispatcher<{ enable: void; skip: void }>();
</script>

<div
  class="sw-backdrop"
  role="button"
  tabindex="-1"
  on:click={(e) => e.target === e.currentTarget && dispatch("skip")}
  on:keydown={() => {}}
>
  <div
    class="sw-dialog"
    role="dialog"
    aria-modal="true"
    aria-labelledby="swarm-onboard-title"
    use:trapFocus={{ onEscape: () => dispatch("skip") }}
  >
    <div class="sw-icon"><Network size={28} /></div>
    <h3 id="swarm-onboard-title">Use TuffSwarm network?</h3>
    <p>
      Share crash-fix experience and unlock Creation mode (modpack trends).
      Without the network, those modes stay unavailable. You can change this anytime in Settings.
    </p>
    <div class="sw-actions">
      <button class="ghost" type="button" on:click={() => dispatch("skip")}>Not now</button>
      <button type="button" on:click={() => dispatch("enable")}>Use network</button>
    </div>
  </div>
</div>

<style>
  .sw-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 220;
    backdrop-filter: blur(8px);
  }
  .sw-dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    padding: 28px;
    width: min(440px, 92vw);
    text-align: center;
    box-shadow: var(--shadow-lg);
  }
  .sw-icon {
    margin-bottom: 12px;
    color: var(--accent-primary);
  }
  .sw-dialog h3 {
    font-size: 18px;
    margin-bottom: 8px;
    color: var(--text-primary);
  }
  .sw-dialog p {
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.5;
    margin-bottom: 20px;
  }
  .sw-actions {
    display: flex;
    gap: 10px;
    justify-content: center;
  }
</style>
