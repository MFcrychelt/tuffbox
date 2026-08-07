<script lang="ts">
  let {
    isOpen,
    onClose,
  }: {
    isOpen: boolean;
    onClose: () => void;
  } = $props();

  const shortcuts = [
    { category: "General", items: [
      { keys: ["Ctrl", "S"], action: "Save all" },
      { keys: ["Ctrl", "/"], action: "This shortcuts dialog" },
      { keys: ["Ctrl", "Z"], action: "Undo" },
      { keys: ["Ctrl", "Shift", "Z"], action: "Redo" },
      { keys: ["Ctrl", "F"], action: "Search quests" },
      { keys: ["Ctrl", "0"], action: "Zoom to fit" },
      { keys: ["Esc"], action: "Deselect / Close panel" },
    ]},
    { category: "Editing", items: [
      { keys: ["Ctrl", "C"], action: "Copy selected quests" },
      { keys: ["Ctrl", "V"], action: "Paste quests" },
      { keys: ["Ctrl", "A"], action: "Select all quests" },
      { keys: ["Del"], action: "Delete selected quests" },
      { keys: ["Arrow keys"], action: "Nudge selected quests (1px)" },
      { keys: ["Shift", "Arrow"], action: "Nudge selected quests (5px)" },
    ]},
    { category: "Canvas", items: [
      { keys: ["Space", "Drag"], action: "Pan canvas" },
      { keys: ["Scroll"], action: "Zoom in/out" },
      { keys: ["Double-click"], action: "Add new quest" },
      { keys: ["Shift", "Drag"], action: "Link quests (create dependency)" },
      { keys: ["Click + Drag"], action: "Marquee select" },
    ]},
  ];
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_interactive_supports_focus a11y_click_events_have_key_events -->
  <div class="overlay" onclick={onClose} role="dialog" aria-modal="true" tabindex="-1">
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="modal" onclick={(e) => e.stopPropagation()} role="document">
      <div class="modal-header">
        <h2>Keyboard Shortcuts</h2>
        <button type="button" class="close" onclick={onClose}>×</button>
      </div>
      <div class="modal-body">
        {#each shortcuts as section (section.category)}
          <div class="section">
            <h3>{section.category}</h3>
            <div class="shortcut-list">
              {#each section.items as item (item.action)}
                <div class="shortcut-row">
                  <div class="keys">
                    {#each item.keys as key, i (`${item.action}-${i}`)}
                      {#if i > 0}<span class="plus">+</span>{/if}
                      <kbd>{key}</kbd>
                    {/each}
                  </div>
                  <span class="action">{item.action}</span>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .modal {
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 3px;
    width: 480px;
    max-height: 80vh;
    overflow: auto;
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
  }
  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .modal-header h2 {
    font-size: 16px;
    font-weight: 700;
    color: var(--ftbq-title-gold, #f2c94c);
    margin: 0;
  }
  .close {
    border: none;
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 20px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 3px;
  }
  .close:hover {
    background: rgba(255,255,255,0.1);
    color: var(--ftbq-text, #e8e8e8);
  }
  .modal-body {
    padding: 16px 20px;
  }
  .section {
    margin-bottom: 20px;
  }
  .section:last-child {
    margin-bottom: 0;
  }
  .section h3 {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ftbq-accent-teal, #3db8a8);
    margin-bottom: 8px;
    font-weight: 700;
  }
  .shortcut-list {
    display: grid;
    gap: 6px;
  }
  .shortcut-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px;
    border-radius: 3px;
    background: rgba(0,0,0,0.15);
  }
  .shortcut-row:hover {
    background: rgba(0,0,0,0.25);
  }
  .keys {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  kbd {
    display: inline-block;
    padding: 3px 8px;
    font-size: 11px;
    font-family: monospace;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 3px;
    color: var(--ftbq-text, #e8e8e8);
    min-width: 24px;
    text-align: center;
  }
  .plus {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .action {
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
</style>
