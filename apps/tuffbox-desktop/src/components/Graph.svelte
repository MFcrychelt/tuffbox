<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { GitGraph, RefreshCw } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  let graph: any = null;
  let loading = false;

  async function load() {
    if (!$projectPath) return;
    loading = true;
    try {
      graph = await invoke("get_graph", { path: $projectPath });
    } finally {
      loading = false;
    }
  }

  $: if ($projectPath) load();
</script>

<div class="graph">
  <div class="toolbar">
    <div class="title">
      <GitGraph size={18} />
      <span>Graph overview</span>
    </div>
    <button class="ghost" on:click={load} title="Refresh">
      <RefreshCw size={16} />
    </button>
  </div>

  {#if loading}
    <div class="loading">Loading graph...</div>
  {:else if graph}
    <div class="stats">
      <div class="stat-card">
        <span class="stat-value">{graph.nodes.length}</span>
        <span class="stat-label">Nodes</span>
      </div>
      <div class="stat-card">
        <span class="stat-value">{graph.edges.length}</span>
        <span class="stat-label">Edges</span>
      </div>
    </div>
    <div class="panel">
      <pre>{JSON.stringify(graph, null, 2)}</pre>
    </div>
  {:else}
    <div class="empty">Open a project to view its dependency graph.</div>
  {/if}
</div>

<style>
  .graph {
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

  .stats {
    display: flex;
    gap: 16px;
    margin-bottom: 20px;
  }

  .stat-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 18px 24px;
    min-width: 130px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .stat-value {
    font-size: 28px;
    font-weight: 800;
  }

  .stat-label {
    font-size: 12px;
    text-transform: uppercase;
    color: var(--text-muted);
    letter-spacing: 0.05em;
  }

  .panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: hidden;
  }

  pre {
    padding: 20px;
    overflow: auto;
    max-height: 600px;
    font-size: 12px;
    color: var(--text-secondary);
    font-family: ui-monospace, monospace;
    line-height: 1.6;
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
</style>
