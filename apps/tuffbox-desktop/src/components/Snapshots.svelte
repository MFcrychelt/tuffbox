<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { History, Plus, RefreshCw, RotateCcw, Calendar } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  let snapshots: any[] = [];
  let loading = false;
  let newName = "";

  async function load() {
    if (!$projectPath) return;
    loading = true;
    try {
      const dir = await invoke("get_project_dir", { path: $projectPath });
      snapshots = await invoke("list_snapshots", { projectDir: dir });
    } finally {
      loading = false;
    }
  }

  async function create() {
    if (!$projectPath) return;
    loading = true;
    try {
      const dir = await invoke("get_project_dir", { path: $projectPath });
      await invoke("create_snapshot", {
        projectDir: dir,
        name: newName || "manual",
        reason: "Created from UI",
      });
      newName = "";
      await load();
    } finally {
      loading = false;
    }
  }

  $: if ($projectPath) load();
</script>

<div class="snapshots">
  <div class="toolbar">
    <div class="title">
      <History size={18} />
      <span>Snapshots</span>
    </div>
    <div class="actions">
      <input bind:value={newName} placeholder="Snapshot name" />
      <button on:click={create}>
        <Plus size={16} />
        Create
      </button>
      <button class="ghost" on:click={load} title="Refresh">
        <RefreshCw size={16} />
      </button>
    </div>
  </div>

  {#if loading}
    <div class="loading">Loading snapshots...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to manage snapshots.</div>
  {:else if snapshots.length === 0}
    <div class="empty">No snapshots yet.</div>
  {:else}
    <div class="grid">
      {#each snapshots as s}
        <div class="card">
          <div class="card-header">
            <h3>{s.name}</h3>
            <span class="badge">{s.id}</span>
          </div>
          <p class="reason">{s.reason}</p>
          <div class="card-footer">
            <div class="date">
              <Calendar size={14} />
              <span>{s.createdAt}</span>
            </div>
            <button class="ghost rollback">
              <RotateCcw size={14} />
              Rollback
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .snapshots {
    max-width: 1200px;
  }

  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 20px;
    flex-wrap: wrap;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .actions input {
    min-width: 180px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 16px;
  }

  .card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    transition: transform 0.15s ease;
  }

  .card:hover {
    transform: translateY(-2px);
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }

  .card-header h3 {
    font-size: 16px;
    font-weight: 700;
  }

  .badge {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
  }

  .reason {
    color: var(--text-secondary);
    font-size: 13px;
    flex: 1;
  }

  .card-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 12px;
    border-top: 1px solid var(--border-color);
  }

  .date {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .rollback {
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
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
