<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { trapFocus } from "../lib/focusTrap";

  export let title = "Select";
  export let message = "";
  export let options: string[] = [];
  export let defaultValue = "";
  export let confirmLabel = "OK";
  export let cancelLabel = "Cancel";
  export let mode: "text" | "select" = "text";

  let value = defaultValue;

  const dispatch = createEventDispatcher<{ confirm: string; cancel: void }>();
</script>

<div class="prompt-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && dispatch("cancel")} on:keydown={() => {}}>
  <div class="prompt-dialog" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => dispatch("cancel") }}>
    <h3>{title}</h3>
    {#if message}
      <p>{message}</p>
    {/if}

    {#if mode === "text"}
      <input
        class="prompt-input"
        type="text"
        bind:value
        on:keydown={(e) => e.key === "Enter" && value.trim() && dispatch("confirm", value)}
      />
    {:else}
      <div class="prompt-options">
        {#each options as option}
          <button
            class="prompt-option"
            class:selected={option === value}
            on:click={() => (value = option)}
          >
            <span class="prompt-option-name">{option}</span>
            {#if option === value}
              <span class="prompt-option-check">&#10003;</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <div class="prompt-actions">
      <button class="ghost" on:click={() => dispatch("cancel")}>{cancelLabel}</button>
      <button disabled={!value.trim()} on:click={() => dispatch("confirm", value)}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .prompt-backdrop {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55);
    display: flex; align-items: center; justify-content: center;
    z-index: 200; backdrop-filter: blur(8px);
  }

  .prompt-dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    padding: 28px; width: 420px;
    box-shadow: var(--shadow-lg);
  }

  .prompt-dialog h3 {
    font-size: 18px; margin: 0 0 4px; color: var(--text-primary);
  }

  .prompt-dialog p {
    color: var(--text-muted); font-size: 13px; line-height: 1.5;
    margin: 0 0 18px;
  }

  .prompt-input {
    width: 100%;
    padding: 10px 14px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    color: var(--text-primary);
    font-size: 14px;
    font-family: inherit;
    outline: none;
    margin-bottom: 18px;
    box-sizing: border-box;
  }

  .prompt-input:focus {
    border-color: var(--accent-primary);
  }

  .prompt-options {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 18px;
    max-height: 260px;
    overflow-y: auto;
  }

  .prompt-option {
    width: 100%;
    padding: 10px 14px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    cursor: pointer;
    text-align: left;
    font-size: 14px;
    color: var(--text-primary);
    transition: all 0.12s ease;
  }

  .prompt-option:hover {
    border-color: var(--text-muted);
    background: var(--bg-hover);
  }

  .prompt-option.selected {
    border-color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
  }

  .prompt-option-name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .prompt-option-check {
    color: var(--accent-primary);
    font-weight: 700;
    flex-shrink: 0;
  }

  .prompt-actions {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .prompt-actions button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  button.ghost {
    background: transparent;
    color: var(--text-muted);
  }

  button.ghost:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }
</style>
