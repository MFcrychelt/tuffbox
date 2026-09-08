<script lang="ts">
  import { Clock, FileText } from "@lucide/svelte";
  import type { Snapshot } from "../lib/api";

  let {
    snapshot,
    selected = false,
    kind,
    title,
    operation,
    preview,
    date,
    changedCount = 0,
    onSelect,
  }: {
    snapshot: Snapshot;
    selected?: boolean;
    kind: "auto" | "manual" | "crash";
    title: string;
    operation: string;
    preview: string;
    date: string;
    changedCount?: number;
    onSelect: () => void;
  } = $props();

  const kindLabel = kind === "crash" ? "Crash fix" : kind === "auto" ? "Auto" : "Manual";
</script>

<button type="button" class="snapshot-item {selected ? `selected ${kind}` : ""}" onclick={onSelect}>
  <span class="snapshot-dot {kind}" aria-hidden="true"></span>
  <div class="snapshot-item-body">
    <div class="snapshot-item-title-row">
      <strong>{title}</strong>
      <span class="snapshot-operation">{operation}</span>
    </div>
    <p>{preview}</p>
    <div class="snapshot-item-meta">
      <span class="kind-tag {kind}">{kindLabel}</span>
      <span class="snapshot-date"><Clock size={12} /> {date}</span>
      {#if changedCount > 0}<span class="snapshot-files"><FileText size={12} /> {changedCount}</span>{/if}
    </div>
  </div>
</button>

<style>
  .snapshot-item {
    width: 100%;
    display: flex;
    gap: 10px;
    text-align: left;
    padding: 12px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: border-color 140ms ease, background 140ms ease, box-shadow 140ms ease;
  }
  .snapshot-item:hover { background: rgba(255,255,255,.055); color: var(--text-primary); }
  .snapshot-item.selected {
    color: var(--text-primary);
    border-color: rgba(16,185,129,.6);
    background: rgba(16,185,129,.09);
    box-shadow: 0 0 15px rgba(16,185,129,.15);
  }
  .snapshot-item.selected.auto { border-color: rgba(96,165,250,.6); box-shadow: 0 0 15px rgba(96,165,250,.12); }
  .snapshot-item.selected.crash { border-color: rgba(245,158,11,.6); box-shadow: 0 0 15px rgba(245,158,11,.12); }
  .snapshot-dot { width: 8px; height: 8px; margin-top: 6px; border-radius: 999px; flex: 0 0 auto; background: #34d399; box-shadow: 0 0 0 3px rgba(52,211,153,.16); }
  .snapshot-dot.auto { background: #93c5fd; box-shadow: 0 0 0 3px rgba(147,197,253,.16); }
  .snapshot-dot.crash { background: #f59e0b; box-shadow: 0 0 0 3px rgba(245,158,11,.16); }
  .snapshot-item-body { min-width: 0; flex: 1; display: grid; gap: 6px; }
  .snapshot-item-title-row { min-width: 0; display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
  .snapshot-item-title-row strong { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13.5px; color: var(--text-primary); }
  .snapshot-operation { max-width: 130px; flex: 0 0 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 10.5px ui-monospace, SFMono-Regular, monospace; color: var(--text-muted); }
  .snapshot-item p { margin: 0; color: var(--text-muted); font-size: 12.5px; line-height: 1.35; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; overflow: hidden; }
  .snapshot-item-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; color: var(--text-muted); font-size: 11.5px; }
  .snapshot-date, .snapshot-files { display: inline-flex; align-items: center; gap: 4px; }
  .kind-tag { padding: 1px 8px; border: 1px solid var(--border-color); border-radius: 999px; font-size: 10.5px; font-weight: 800; text-transform: uppercase; letter-spacing: .04em; }
  .kind-tag.auto { color: #93c5fd; border-color: rgba(147,197,253,.4); }
  .kind-tag.manual { color: #34d399; border-color: rgba(52,211,153,.4); }
  .kind-tag.crash { color: #fbbf24; border-color: rgba(251,191,36,.4); }
</style>
