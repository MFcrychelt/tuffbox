<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Stethoscope, RefreshCw, AlertCircle, AlertTriangle, Info } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  let diagnostics: any[] = [];
  let loading = false;

  async function load() {
    if (!$projectPath) return;
    loading = true;
    try {
      diagnostics = await invoke("get_diagnostics", { path: $projectPath });
    } finally {
      loading = false;
    }
  }

  $: if ($projectPath) load();

  function icon(severity: string) {
    if (severity === "Error") return AlertCircle;
    if (severity === "Warning") return AlertTriangle;
    return Info;
  }
</script>

<div class="diagnostics">
  <div class="toolbar">
    <div class="title">
      <Stethoscope size={18} />
      <span>Health check</span>
    </div>
    <button class="ghost" on:click={load} title="Refresh">
      <RefreshCw size={16} />
    </button>
  </div>

  {#if loading}
    <div class="loading">Loading diagnostics...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to see diagnostics.</div>
  {:else if diagnostics.length === 0}
    <div class="empty success">
      <AlertCircle size={32} color="#1bd96a" />
      <p>No issues found. Project looks healthy.</p>
    </div>
  {:else}
    <div class="list">
      {#each diagnostics as d}
        <div class="card {d.severity.toLowerCase()}">
          <div class="icon">
            <svelte:component this={icon(d.severity)} size={22} />
          </div>
          <div class="body">
            <div class="meta">
              <span class="severity">{d.severity}</span>
              <span class="code">{d.code}</span>
            </div>
            <p class="message">{d.message}</p>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .diagnostics {
    max-width: 1200px;
  }

  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .card {
    display: flex;
    gap: 16px;
    padding: 18px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    border-left: 4px solid var(--text-muted);
  }

  .card.error {
    border-left-color: var(--accent-danger);
  }

  .card.error .icon {
    color: var(--accent-danger);
  }

  .card.warning {
    border-left-color: var(--accent-warning);
  }

  .card.warning .icon {
    color: var(--accent-warning);
  }

  .card.info {
    border-left-color: #60a5fa;
  }

  .card.info .icon {
    color: #60a5fa;
  }

  .icon {
    color: var(--text-muted);
    margin-top: 2px;
  }

  .body {
    flex: 1;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }

  .severity {
    font-size: 12px;
    font-weight: 800;
    text-transform: uppercase;
    color: var(--text-secondary);
  }

  .code {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 2px 8px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
  }

  .message {
    color: var(--text-primary);
    line-height: 1.5;
  }

  .empty,
  .loading {
    color: var(--text-muted);
    padding: 80px;
    text-align: center;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }

  .empty.success {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }

  .empty.success p {
    color: var(--text-secondary);
    font-weight: 500;
  }
</style>
