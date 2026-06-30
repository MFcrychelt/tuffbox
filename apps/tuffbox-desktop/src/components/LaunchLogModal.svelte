<script lang="ts">
  import { X, Loader2, RotateCcw } from "lucide-svelte";
  import { createEventDispatcher, onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  const dispatch = createEventDispatcher<{ close: void }>();

  export let projectPath: string;

  let log = "";
  let loading = true;
  let interval: ReturnType<typeof setInterval>;

  async function loadLog() {
    try {
      const result = await invoke("get_launch_log", { path: projectPath });
      log = result as string;
    } catch (e) {
      log += `\n[error] ${e}`;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadLog();
    interval = setInterval(loadLog, 1000);
  });

  onDestroy(() => {
    clearInterval(interval);
  });
</script>

<div class="modal-backdrop" on:click={() => dispatch("close")} role="button" tabindex="-1" aria-label="Close">
  <div class="modal" role="dialog" aria-modal="true" on:click|stopPropagation>
    <div class="modal-header">
      <h2>Launch Log</h2>
      <button class="icon-btn" on:click={() => dispatch("close")} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if loading && !log}
        <div class="loader">
          <Loader2 size={20} class="spin" />
          Waiting for log...
        </div>
      {/if}
      <pre class="log">{log || "Waiting for process output..."}</pre>
    </div>

    <div class="modal-footer">
      <button class="ghost" on:click={loadLog}>
        <RotateCcw size={16} />
        Refresh
      </button>
      <button class="ghost" on:click={() => dispatch("close")}>Close</button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 24px;
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    width: 100%;
    max-width: 800px;
    height: 70vh;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .modal-header h2 {
    font-size: 16px;
    font-weight: 800;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--border-radius-md);
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .modal-body {
    flex: 1;
    overflow: auto;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
  }

  .loader {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
    margin-bottom: 12px;
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .log {
    flex: 1;
    margin: 0;
    padding: 12px;
    background: #0b0b0d;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    color: var(--text-secondary);
    font-family: ui-monospace, monospace;
    font-size: 12px;
    white-space: pre-wrap;
    overflow: auto;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--border-color);
  }

  .modal-footer button {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
