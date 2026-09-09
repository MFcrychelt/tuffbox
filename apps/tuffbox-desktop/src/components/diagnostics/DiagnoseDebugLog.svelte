<script lang="ts">
  import { Copy, Download, Trash2, X } from "@lucide/svelte";

  let { entries = [], onclose, onclear }: { entries: string[]; onclose: () => void; onclear: () => void } = $props();

  async function copy() {
    await navigator.clipboard?.writeText(entries.join("\n"));
  }

  function download() {
    const blob = new Blob([entries.join("\n") + "\n"], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `tuffbox-diagnose-${new Date().toISOString().replaceAll(":", "-")}.log`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <div class="window" role="dialog" aria-modal="true" aria-label="Diagnose debug log">
    <header>
      <div><strong>Diagnose debug log</strong><span>{entries.length} events · copy this log when reporting a problem</span></div>
      <button class="icon" type="button" aria-label="Close" onclick={onclose}><X size={16} /></button>
    </header>
    <pre>{entries.length ? entries.join("\n") : "Waiting for Diagnose events…"}</pre>
    <footer>
      <button type="button" class="ghost" onclick={onclear}><Trash2 size={14} /> Clear</button>
      <button type="button" class="ghost" onclick={copy}><Copy size={14} /> Copy</button>
      <button type="button" class="primary" onclick={download}><Download size={14} /> Download</button>
    </footer>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 210; display: flex; align-items: center; justify-content: center; padding: 16px; background: rgba(0,0,0,.55); backdrop-filter: blur(5px); }
  .window { display: grid; grid-template-rows: auto minmax(240px, 1fr) auto; width: min(920px, 100%); height: min(720px, 90vh); overflow: hidden; color: var(--text-primary); background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); box-shadow: var(--shadow-lg); }
  header, footer { display: flex; align-items: center; gap: 10px; padding: 12px 14px; border-color: var(--border-color); }
  header { justify-content: space-between; border-bottom: 1px solid var(--border-color); } header div { display: grid; gap: 3px; } header span { color: var(--text-muted); font-size: 11px; }
  .icon { padding: 5px; color: var(--text-muted); background: transparent; border: 0; border-radius: 6px; cursor: pointer; } .icon:hover { color: var(--text-primary); background: var(--bg-hover); }
  pre { min-height: 0; margin: 0; padding: 14px; overflow: auto; color: #dbeafe; background: #090d12; font: 11px/1.55 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  footer { justify-content: flex-end; border-top: 1px solid var(--border-color); } footer button { display: inline-flex; align-items: center; gap: 6px; padding: 7px 10px; border-radius: var(--border-radius-sm); cursor: pointer; font-size: 12px; } .ghost { color: var(--text-secondary); background: transparent; border: 1px solid var(--border-color); } .ghost:hover { background: var(--bg-hover); } .primary { color: var(--on-accent); background: var(--accent-primary); border: 0; font-weight: 700; }
</style>
