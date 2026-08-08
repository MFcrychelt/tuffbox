<script lang="ts">
  import { Handle, Position } from "@xyflow/svelte";
  import { iconDisplayId, type QuestData, type QuestProgressStatus } from "../../lib/api";
  import QuestItemIcon from "./QuestItemIcon.svelte";

  let {
    data,
  }: {
    data: {
      quest: QuestData;
      isIssue: boolean;
      isSelected: boolean;
      baseSize: number;
      progress: QuestProgressStatus | null;
      iconRevision: number;
      external?: boolean;
      chapterTitle?: string;
    };
  } = $props();

  let q = $derived(data.quest);
  let size = $derived(data.baseSize * (q.size && q.size > 0 ? q.size : 1));
  let prog = $derived(data.progress);
  let external = $derived(!!data.external);

  function glyph(quest: QuestData): string {
    const icon = iconDisplayId(quest.icon);
    if (icon) {
      const leaf = icon.includes(":") ? icon.split(":").pop()! : icon;
      return (leaf[0] || "?").toUpperCase();
    }
    return (quest.title[0] || "?").toUpperCase();
  }

  let iconId = $derived(iconDisplayId(q.icon));

  function nodeShape(quest: QuestData): string {
    const s = quest.shape?.trim();
    if (s && s !== "none") return s;
    return "rsquare";
  }

  let shape = $derived(nodeShape(q));
  let clipped = $derived(
    shape === "diamond" || shape === "hexagon" || shape === "pentagon" || shape === "gear",
  );
</script>

<Handle type="target" position={Position.Top} style="opacity: 0; width: 6px; height: 6px;" />
<Handle type="source" position={Position.Bottom} style="opacity: 0; width: 6px; height: 6px;" />

<div
  class="node-wrap"
  class:sel={data.isSelected}
  class:issue={data.isIssue}
  class:external
  title={external && data.chapterTitle ? `${q.title} (${data.chapterTitle})` : q.title}
>
  <div
    class="node-icon shape-{shape}"
    class:clipped
    class:optional={q.optional || external}
    class:prog-completed={prog === "completed"}
    class:prog-started={prog === "started"}
    class:prog-available={prog === "available"}
    class:prog-locked={prog === "locked"}
    style="width:{size}px; height:{size}px;"
  >
    <div class="node-face shape-{shape}">
      <QuestItemIcon
        itemId={iconId}
        fallback={glyph(q)}
        size={Math.max(12, Math.floor(size * 0.62))}
        revision={data.iconRevision}
      />
    </div>
    {#if q.optional}<span class="opt">?</span>{/if}
    {#if prog === "completed"}<span class="check" title="Completed">✓</span>{/if}
  </div>
  <span class="node-label">{q.title}</span>
  {#if external && data.chapterTitle}
    <span class="ch-badge">{data.chapterTitle}</span>
  {/if}
</div>

<style>
  .node-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    cursor: grab;
    user-select: none;
    transition: transform 0.12s ease;
  }
  .node-wrap.external {
    cursor: default;
    opacity: 0.72;
  }
  .node-wrap:hover:not(.external) {
    transform: scale(1.06);
  }
  .node-wrap:active:not(.external) {
    cursor: grabbing;
  }
  .node-icon {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    border: 2px solid var(--ftbq-quest-default, #ffffff);
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.65),
      0 3px 8px rgba(0, 0, 0, 0.55);
  }
  .node-wrap.external .node-icon:not(.clipped) {
    border-style: dashed;
    border-color: var(--ftbq-quest-locked, #6b6b6b);
  }
  .node-wrap.sel .node-icon {
    border-color: var(--ftbq-title-gold, #f2c94c);
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.65),
      0 0 12px 2px rgba(242, 201, 76, 0.5),
      0 3px 8px rgba(0, 0, 0, 0.55);
    animation: sel-pulse 1.6s ease-in-out infinite;
  }
  @keyframes sel-pulse {
    0%,
    100% {
      box-shadow:
        0 0 0 2px rgba(0, 0, 0, 0.65),
        0 0 8px 1px rgba(242, 201, 76, 0.35),
        0 3px 8px rgba(0, 0, 0, 0.55);
    }
    50% {
      box-shadow:
        0 0 0 2px rgba(0, 0, 0, 0.65),
        0 0 14px 3px rgba(242, 201, 76, 0.6),
        0 3px 8px rgba(0, 0, 0, 0.55);
    }
  }
  .node-wrap.issue .node-icon {
    border-color: #f87171;
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.65),
      0 0 10px 2px rgba(248, 113, 113, 0.45),
      0 3px 8px rgba(0, 0, 0, 0.55);
  }
  .node-icon.clipped {
    border-color: transparent;
    box-shadow: none;
    background: transparent;
    filter: drop-shadow(0 0 2px var(--ftbq-quest-default, #ffffff)) drop-shadow(0 3px 4px rgba(0, 0, 0, 0.6));
  }
  .node-wrap.sel .node-icon.clipped {
    border-color: transparent;
    animation: none;
    filter: drop-shadow(0 0 3px var(--ftbq-title-gold, #f2c94c)) drop-shadow(0 3px 4px rgba(0, 0, 0, 0.6));
  }
  .node-icon.clipped.prog-completed {
    border-color: transparent;
    filter: drop-shadow(0 0 3px var(--ftbq-quest-completed, #55c95a)) drop-shadow(0 3px 4px rgba(0, 0, 0, 0.6));
  }
  .node-icon.clipped.prog-started {
    border-color: transparent;
    filter: drop-shadow(0 0 3px var(--ftbq-quest-started, #f2c94c)) drop-shadow(0 3px 4px rgba(0, 0, 0, 0.6));
  }
  .node-icon.clipped.prog-locked {
    border-color: transparent;
    filter: drop-shadow(0 0 2px var(--ftbq-quest-locked, #6b6b6b)) drop-shadow(0 3px 4px rgba(0, 0, 0, 0.6));
  }
  .node-face {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background:
      radial-gradient(circle at 50% 32%, rgba(255, 255, 255, 0.07), transparent 62%),
      var(--ftbq-node-fill);
    box-shadow:
      inset 2px 2px 0 rgba(0, 0, 0, 0.55),
      inset -1px -1px 0 rgba(255, 255, 255, 0.09);
  }
  .node-icon.shape-circle,
  .node-face.shape-circle {
    border-radius: 50%;
  }
  .node-icon.shape-square,
  .node-face.shape-square {
    border-radius: 0;
  }
  .node-icon.shape-rsquare,
  .node-face.shape-rsquare {
    border-radius: 4px;
  }
  .node-face.shape-diamond {
    border-radius: 0;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
  }
  .node-face.shape-hexagon {
    clip-path: polygon(25% 0%, 75% 0%, 100% 50%, 75% 100%, 25% 100%, 0% 50%);
    border-radius: 0;
  }
  .node-face.shape-pentagon {
    clip-path: polygon(50% 0%, 100% 38%, 82% 100%, 18% 100%, 0% 38%);
    border-radius: 0;
  }
  .node-face.shape-gear {
    border-radius: 3px;
    clip-path: polygon(
      50% 0%,
      61% 8%,
      75% 4%,
      82% 18%,
      96% 25%,
      92% 39%,
      100% 50%,
      92% 61%,
      96% 75%,
      82% 82%,
      75% 96%,
      61% 92%,
      50% 100%,
      39% 92%,
      25% 96%,
      18% 82%,
      4% 75%,
      8% 61%,
      0% 50%,
      8% 39%,
      4% 25%,
      18% 18%,
      25% 4%,
      39% 8%
    );
  }
  .node-icon.optional:not(.clipped) {
    border-style: dashed;
  }
  .node-face :global(.qii),
  .node-face :global(.qii-ph) {
    max-width: 70%;
    max-height: 70%;
  }
  .node-label {
    font-size: clamp(8px, 10px, 11px);
    line-height: 1.15;
    max-width: calc(100% + 24px);
    min-width: 100%;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ftbq-text-muted, #9a9aa0);
    pointer-events: none;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.9);
  }
  .ch-badge {
    font-size: 8px;
    line-height: 1.1;
    max-width: calc(100% + 28px);
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ftbq-accent-teal, #3db8a8);
    pointer-events: none;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.9);
    padding: 1px 4px;
    border: 1px dashed var(--ftbq-border);
    border-radius: 2px;
    background: rgba(0, 0, 0, 0.35);
  }
  .node-wrap:hover .node-label {
    color: var(--ftbq-text, #e8e8e8);
  }
  .node-wrap.sel .node-label {
    color: var(--ftbq-title-gold, #f2c94c);
    font-weight: 700;
  }
  .opt {
    position: absolute;
    top: -3px;
    right: -3px;
    font-size: 9px;
    color: var(--ftbq-quest-started, #f2c94c);
    font-weight: 900;
    text-shadow: 0 0 3px rgba(0, 0, 0, 0.8);
  }
  .check {
    position: absolute;
    bottom: -5px;
    right: -5px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: linear-gradient(180deg, #6fdd74, #3da344);
    border: 1px solid #0d2a10;
    color: #07230b;
    font-size: 10px;
    font-weight: 900;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    box-shadow:
      0 0 0 1px rgba(0, 0, 0, 0.6),
      0 0 6px rgba(85, 201, 90, 0.55),
      inset 0 1px 0 rgba(255, 255, 255, 0.35);
  }
  .node-icon.prog-completed {
    border-color: var(--ftbq-quest-completed, #55c95a);
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.65),
      0 0 10px 2px rgba(85, 201, 90, 0.4),
      0 3px 8px rgba(0, 0, 0, 0.55);
  }
  .node-icon.prog-started {
    border-color: var(--ftbq-quest-started, #f2c94c);
    box-shadow:
      0 0 0 2px rgba(0, 0, 0, 0.65),
      0 0 10px 2px rgba(242, 201, 76, 0.4),
      0 3px 8px rgba(0, 0, 0, 0.55);
  }
  .node-icon.prog-available {
    border-color: var(--ftbq-quest-default, #ffffff);
  }
  .node-icon.prog-locked {
    border-color: var(--ftbq-quest-locked, #6b6b6b);
    opacity: 0.5;
    filter: grayscale(0.5);
  }
</style>
