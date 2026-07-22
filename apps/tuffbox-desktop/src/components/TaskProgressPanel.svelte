<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onDestroy, onMount } from "svelte";
  import { X, Loader2, CheckCircle2, AlertTriangle } from "lucide-svelte";

  type BackgroundTask = {
    id: string;
    title: string;
    status: "running" | "succeeded" | "failed" | "dismissed";
    progress?: number | null;
    detail?: string | null;
    error?: string | null;
  };

  let tasks: BackgroundTask[] = [];
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      tasks = await invoke<BackgroundTask[]>("list_background_tasks");
    } catch {
      tasks = [];
    }
  }

  async function dismiss(id: string) {
    try {
      await invoke("dismiss_background_task", { id });
      await refresh();
    } catch {
      /* ignore */
    }
  }

  onMount(() => {
    void refresh();
    timer = setInterval(() => {
      void refresh();
    }, 800);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  $: visible = tasks.filter((t) => t.status === "running" || t.status === "failed");
</script>

{#if visible.length}
  <aside class="task-panel" aria-label="Background tasks">
    {#each visible as t (t.id)}
      <div class="task" class:failed={t.status === "failed"} class:running={t.status === "running"}>
        <div class="row">
          {#if t.status === "running"}
            <Loader2 size={14} class="spin" />
          {:else if t.status === "failed"}
            <AlertTriangle size={14} />
          {:else}
            <CheckCircle2 size={14} />
          {/if}
          <strong>{t.title}</strong>
          <button type="button" class="ghost" title="Dismiss" on:click={() => dismiss(t.id)}>
            <X size={14} />
          </button>
        </div>
        {#if t.status === "running" && t.progress != null}
          <div class="bar"><div class="fill" style={`width: ${Math.round((t.progress || 0) * 100)}%`}></div></div>
        {/if}
        {#if t.detail}
          <small>{t.detail}</small>
        {/if}
        {#if t.error}
          <small class="err">{t.error}</small>
        {/if}
      </div>
    {/each}
  </aside>
{/if}

<style>
  .task-panel {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 80;
    width: min(320px, calc(100vw - 32px));
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
  }
  .task {
    pointer-events: auto;
    background: var(--surface, #1a1a1e);
    border: 1px solid var(--border, #333);
    border-radius: 8px;
    padding: 10px 12px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  }
  .task.failed {
    border-color: #c44;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .row strong {
    flex: 1;
    font-size: 13px;
  }
  .ghost {
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    opacity: 0.7;
    padding: 2px;
  }
  .bar {
    margin-top: 8px;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent, #6cf);
    transition: width 0.2s ease;
  }
  :global(html.potato-pc) .fill {
    transition: none;
  }
  small {
    display: block;
    margin-top: 4px;
    opacity: 0.75;
    font-size: 11px;
  }
  .err {
    color: #f88;
  }
  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }
  :global(html.potato-pc) :global(.spin) {
    animation: none;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
