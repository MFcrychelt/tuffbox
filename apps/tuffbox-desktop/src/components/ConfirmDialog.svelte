<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { AlertTriangle, X } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";

  const dispatch = createEventDispatcher<{ confirm: void; cancel: void }>();

  export let title = "Confirm";
  export let message = "Are you sure?";
  export let danger = false;
  export let confirmLabel = "Confirm";
  export let cancelLabel = "Cancel";
</script>

<div class="cd-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && dispatch("cancel")} on:keydown={() => {}}>
  <div class="cd-dialog" role="alertdialog" aria-modal="true" use:trapFocus={{ onEscape: () => dispatch("cancel") }}>
    <div class="cd-icon">
      <AlertTriangle size={28} color={danger ? "#f87171" : "#fbbf24"} />
    </div>
    <h3>{title}</h3>
    <p>{message}</p>
    <div class="cd-actions">
      <button class="ghost" on:click={() => dispatch("cancel")}>{cancelLabel}</button>
      <button class={danger ? "danger" : ""} on:click={() => dispatch("confirm")}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .cd-backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,.55); display: flex;
    align-items: center; justify-content: center; z-index: 200; backdrop-filter: blur(8px);
  }
  .cd-dialog {
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl); padding: 28px; width: 420px;
    text-align: center; box-shadow: var(--shadow-lg);
  }
  .cd-icon { margin-bottom: 12px; }
  .cd-dialog h3 { font-size: 18px; margin-bottom: 8px; color: var(--text-primary); }
  .cd-dialog p { color: var(--text-muted); font-size: 13px; line-height: 1.5; margin-bottom: 20px; }
  .cd-actions { display: flex; gap: 10px; justify-content: center; }
  button.danger { background: #ef4444; color: #fff; }
  button.danger:hover { background: #dc2626; }
</style>
