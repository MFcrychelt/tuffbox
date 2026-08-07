<script lang="ts">
  import { unifiedDiffLines, type DiffLine } from "../../lib/snbtDiff";
  import { trapFocus } from "../../lib/focusTrap";

  let {
    open = false,
    title = "Review SNBT changes",
    leftLabel = "Disk",
    rightLabel = "Editor",
    leftText = "",
    rightText = "",
    confirmLabel = "Save",
    onConfirm,
    onCancel,
  }: {
    open?: boolean;
    title?: string;
    leftLabel?: string;
    rightLabel?: string;
    leftText?: string;
    rightText?: string;
    confirmLabel?: string;
    onConfirm?: () => void;
    onCancel?: () => void;
  } = $props();

  let lines = $derived.by<DiffLine[]>(() => {
    if (!open) return [];
    return unifiedDiffLines(leftText, rightText);
  });

  let stats = $derived.by(() => {
    let added = 0;
    let removed = 0;
    for (const l of lines) {
      if (l.kind === "add") added++;
      else if (l.kind === "del") removed++;
    }
    return { added, removed };
  });

  function lineClass(kind: DiffLine["kind"]): string {
    if (kind === "add") return "added";
    if (kind === "del") return "removed";
    return "same";
  }

  function prefix(kind: DiffLine["kind"]): string {
    if (kind === "add") return "+";
    if (kind === "del") return "-";
    return " ";
  }
</script>

{#if open}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={(e) => e.target === e.currentTarget && onCancel?.()}
    onkeydown={() => {}}
  >
    <div
      class="modal snbt-diff-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="snbt-diff-title"
      use:trapFocus={{ onEscape: () => onCancel?.() }}
    >
      <div class="modal-header">
        <div>
          <h2 id="snbt-diff-title">{title}</h2>
          <p>
            {leftLabel} → {rightLabel}
            · +{stats.added} / −{stats.removed}
          </p>
        </div>
        <button class="icon-btn" type="button" onclick={() => onCancel?.()} aria-label="Close">×</button>
      </div>
      <pre class="diff-body"><code
        >{#each lines as line, i (`${i}-${line.kind}-${line.text.slice(0, 24)}`)}<span
            class={lineClass(line.kind)}>{prefix(line.kind)}{line.text}
</span>{/each}</code
      ></pre>
      <div class="modal-actions">
        <button class="ghost" type="button" onclick={() => onCancel?.()}>Cancel</button>
        <button class="primary" type="button" onclick={() => onConfirm?.()}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    padding: 16px;
  }
  .snbt-diff-modal {
    width: min(920px, 96vw);
    max-height: min(88vh, 900px);
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--bg-secondary, #1a1a1e);
    border: 1px solid var(--border-color, #3a3a42);
    border-radius: var(--border-radius-lg);
    padding: 16px;
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: flex-start;
  }
  .modal-header h2 {
    margin: 0;
    font-size: 16px;
    color: var(--text-primary, #e8e8e8);
  }
  .modal-header p {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted, #9a9aa0);
  }
  .icon-btn {
    border: 0;
    background: transparent;
    color: var(--text-muted, #9a9aa0);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
    padding: 2px 6px;
  }
  .diff-body {
    flex: 1;
    min-height: 200px;
    max-height: 58vh;
    overflow: auto;
    margin: 0;
    padding: 12px;
    border-radius: var(--border-radius-md);
    background: #09090b;
    border: 1px solid var(--border-color, #3a3a42);
    font-family: ui-monospace, "Cascadia Code", "Fira Code", monospace;
    font-size: 11px;
    line-height: 1.45;
    white-space: pre;
  }
  .diff-body code {
    display: block;
  }
  .same {
    display: block;
    color: #a1a1aa;
  }
  .added {
    display: block;
    color: #86efac;
    background: rgba(27, 217, 106, 0.08);
  }
  .removed {
    display: block;
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.08);
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .modal-actions button {
    padding: 8px 14px;
    border-radius: var(--border-radius-sm);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid var(--border-color, #3a3a42);
  }
  .modal-actions .ghost {
    background: transparent;
    color: var(--text-secondary, #c4c4c8);
  }
  .modal-actions .primary {
    background: var(--accent, #3db8a8);
    border-color: transparent;
    color: #0b1210;
  }
</style>
