<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    BackgroundVariant,
    useSvelteFlow,
    type Node,
    type Edge,
    type Connection,
  } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { tick } from "svelte";
  import { Maximize2, Plus } from "@lucide/svelte";
  import {
    iconDisplayId,
    type QuestChapter,
    type QuestData,
    type QuestValidationIssue,
    type QuestProgressStatus,
  } from "../../lib/api";
  import { projectPath } from "../../lib/store";
  import { preloadItemIcons } from "./iconCache";
  import QuestNode from "./QuestNode.svelte";

  let {
    quests,
    chapters = [],
    selectedId = null,
    selectedIds = new Set<string>(),
    issues,
    progressStatuses = {},
    progressOverlay = false,
    emptyHint = "Double-click to add a quest",
    onSelect,
    onMove,
    onAddAt,
    onLink,
    onSelectMultiple = () => {},
    fitToken = 0,
  }: {
    quests: QuestData[];
    chapters?: QuestChapter[];
    selectedId?: string | null;
    selectedIds?: Set<string>;
    issues: QuestValidationIssue[];
    progressStatuses?: Record<string, QuestProgressStatus>;
    progressOverlay?: boolean;
    emptyHint?: string;
    onSelect: (q: QuestData | null, e?: MouseEvent) => void;
    onMove: (q: QuestData, x: number, y: number) => void;
    onAddAt: (x: number, y: number) => void;
    onLink: (fromId: string, toDepId: string) => void;
    onSelectMultiple?: (ids: string[]) => void;
    fitToken?: number;
  } = $props();

  const BASE = 24;
  const nodeTypes = { quest: QuestNode };

  let issueIds = $derived(new Set(issues.map((i) => i.questId)));
  let iconRevision = $state(0);

  const { screenToFlowPosition, fitView: flowFitView } = useSvelteFlow();

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);

  $effect(() => {
    if (quests && $projectPath) {
      void preloadChapterIcons(quests);
    }
  });

  async function preloadChapterIcons(list: QuestData[]) {
    const ids = list
      .map((q) => iconDisplayId(q.icon))
      .filter((id): id is string => !!id);
    if (!ids.length || !$projectPath) return;
    await preloadItemIcons(ids, $projectPath);
    iconRevision += 1;
  }

  function progressOf(q: QuestData): QuestProgressStatus | null {
    if (!progressOverlay) return null;
    return progressStatuses[q.id] ?? "unknown";
  }

  function findExternalDep(
    depId: string,
  ): { quest: QuestData; chapterTitle: string } | null {
    for (const ch of chapters) {
      const direct = ch.quests.find((oq) => oq.id === depId);
      if (direct && !quests.some((q) => q.id === direct.id)) {
        return {
          quest: direct,
          chapterTitle: ch.title || ch.filename || ch.id.slice(0, 8),
        };
      }
      const owner = ch.quests.find((oq) => oq.tasks?.some((t) => t.id === depId));
      if (owner && !quests.some((q) => q.id === owner.id)) {
        return {
          quest: owner,
          chapterTitle: ch.title || ch.filename || ch.id.slice(0, 8),
        };
      }
    }
    return null;
  }

  $effect(() => {
    const rev = iconRevision;
    void chapters;
    const newNodes: Node[] = quests.map((q) => ({
      id: q.id,
      type: "quest",
      position: { x: q.x * BASE, y: q.y * BASE },
      data: {
        quest: q,
        isIssue: issueIds.has(q.id),
        isSelected: selectedId === q.id || selectedIds.has(q.id),
        baseSize: BASE,
        progress: progressOf(q),
        iconRevision: rev,
      },
      selected: selectedId === q.id || selectedIds.has(q.id),
    }));

    const ghosts = new Map<string, Node>();
    const newEdges: Edge[] = [];
    for (const q of quests) {
      for (const depId of q.dependencies) {
        // FTB may store a task id as dependency — resolve to owning quest node.
        let sourceId = depId;
        let targetExists = quests.some((oq) => oq.id === depId);
        let external = false;
        if (!targetExists) {
          const owner = quests.find((oq) => oq.tasks?.some((t) => t.id === depId));
          if (owner) {
            sourceId = owner.id;
            targetExists = true;
          }
        }

        if (!targetExists) {
          const ext = findExternalDep(depId);
          if (ext) {
            sourceId = `ext:${ext.quest.id}`;
            targetExists = true;
            external = true;
            if (!ghosts.has(sourceId)) {
              ghosts.set(sourceId, {
                id: sourceId,
                type: "quest",
                position: {
                  x: q.x * BASE - BASE * 3,
                  y: q.y * BASE - BASE * 3,
                },
                draggable: false,
                selectable: false,
                data: {
                  quest: ext.quest,
                  isIssue: false,
                  isSelected: false,
                  baseSize: BASE,
                  progress: null,
                  iconRevision: rev,
                  external: true,
                  chapterTitle: ext.chapterTitle,
                },
              });
            }
          }
        }

        const depDone =
          progressOverlay &&
          (progressStatuses[sourceId.replace(/^ext:/, "")] === "completed" ||
            progressStatuses[depId] === "completed");

        let style =
          "stroke: #5c8a9e; stroke-width: 3; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));";
        if (!targetExists || external) {
          style =
            "stroke: var(--ftbq-quest-started, #f2c94c); stroke-width: 2.5; stroke-dasharray: 6 4; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));";
        } else if (depDone) {
          style =
            "stroke: var(--ftbq-line-done, #55c95a); stroke-width: 3.5; filter: drop-shadow(0 0 3px rgba(85,201,90,0.6));";
        }

        newEdges.push({
          id: `e-${depId}-${q.id}`,
          source: sourceId,
          target: q.id,
          type: "step",
          style,
        });
      }
    }

    nodes = [...newNodes, ...ghosts.values()];
    edges = newEdges;
  });

  // Sync marquee / multi-select from Svelte Flow back to parent
  $effect(() => {
    const selectedNodes = nodes.filter((n) => n.selected && !n.id.startsWith("ext:"));
    if (selectedNodes.length > 1) {
      onSelectMultiple(selectedNodes.map((n) => n.id));
    }
  });

  let lastFitToken = $state(-1);
  $effect(() => {
    if (fitToken !== lastFitToken) {
      lastFitToken = fitToken;
      tick().then(() => flowFitView({ padding: 0.2 }));
    }
  });

  function snap(v: number) {
    return Math.round(v * 2) / 2;
  }

  function handleNodeDragStop({
    targetNode,
    nodes: flowNodes,
  }: {
    targetNode: Node | null;
    nodes: Node[];
    event: MouseEvent | TouchEvent;
  }) {
    // Persist all selected nodes (multi-drag) plus the primary target.
    const toPersist = new Set<string>();
    if (targetNode && !targetNode.id.startsWith("ext:")) toPersist.add(targetNode.id);
    for (const n of flowNodes) {
      if (n.selected && !n.id.startsWith("ext:")) toPersist.add(n.id);
    }
    for (const n of flowNodes) {
      if (!toPersist.has(n.id)) continue;
      const q = quests.find((item) => item.id === n.id);
      if (q) {
        onMove(q, snap(n.position.x / BASE), snap(n.position.y / BASE));
      }
    }
  }

  function handleConnect(connection: Connection) {
    if (connection.source && connection.target) {
      const src = connection.source.startsWith("ext:")
        ? connection.source.slice(4)
        : connection.source;
      const tgt = connection.target.startsWith("ext:")
        ? connection.target.slice(4)
        : connection.target;
      // Edge is prereq (source) → dependent (target); dependent lists prereq in dependencies.
      onLink(tgt, src);
    }
  }

  // No dedicated double-click pane event in @xyflow/svelte 1.x — detect via MouseEvent.detail.
  function handlePaneClick({ event }: { event: MouseEvent }) {
    if (event.detail === 2) {
      const pos = screenToFlowPosition({ x: event.clientX, y: event.clientY });
      onAddAt(snap(pos.x / BASE), snap(pos.y / BASE));
      return;
    }
    onSelect(null, event);
  }

  function handleNodeClick({ node, event }: { node: Node; event: MouseEvent | TouchEvent }) {
    if (node.id.startsWith("ext:")) return;
    const q = quests.find((item) => item.id === node.id);
    if (q) onSelect(q, event instanceof MouseEvent ? event : undefined);
  }

  function addAtCenter() {
    const viewportEl = document.querySelector(".ftbq-canvas .xyflow__viewport");
    if (viewportEl) {
      const rect = viewportEl.getBoundingClientRect();
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      const pos = screenToFlowPosition({ x: cx, y: cy });
      onAddAt(snap(pos.x / BASE), snap(pos.y / BASE));
    } else {
      onAddAt(0, 0);
    }
  }
</script>

<div class="canvas-wrap ftbq-canvas">
  <div class="canvas-toolbar">
    <button type="button" class="tb" title="Fit view" onclick={() => flowFitView({ padding: 0.2 })}>
      <Maximize2 size={14} /> Fit
    </button>
    <button type="button" class="tb" title="Add quest at center" onclick={addAtCenter}>
      <Plus size={14} /> Add quest
    </button>
    <span class="hint">Drag · Scroll zoom · Connect · Dbl-click add · Shift/Ctrl multi · Marquee</span>
  </div>

  <div class="viewport">
    {#if quests.length === 0}
      <div class="empty-hint">{emptyHint}</div>
    {/if}

    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      panOnScroll
      selectionOnDrag
      panOnDrag={[1, 2]}
      deleteKey={null}
      onnodeclick={handleNodeClick}
      onpaneclick={handlePaneClick}
      onnodedragstop={handleNodeDragStop}
      onconnect={handleConnect}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      defaultEdgeOptions={{
        type: "step",
        style: "stroke: #5c8a9e; stroke-width: 3; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));",
      }}
    >
      <Background
        variant={BackgroundVariant.Dots}
        gap={20}
        size={1}
        patternColor="rgba(255,255,255,0.07)"
      />
      <Controls />
    </SvelteFlow>
    <div class="vignette" aria-hidden="true"></div>
  </div>
</div>

<style>
  .canvas-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--ftbq-bg-canvas, #2b2b30);
    border: none;
    border-left: 1px solid #101014;
    border-right: 1px solid #101014;
    overflow: hidden;
  }
  .canvas-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-bottom: 1px solid #101014;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(0, 0, 0, 0.2)),
      var(--ftbq-bg-panel, #212126);
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.05);
    flex-shrink: 0;
  }
  .tb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 3px;
    border: 1px solid #101014;
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    color: var(--ftbq-text, #e8e8e8);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      inset 0 -1px 0 rgba(0, 0, 0, 0.45);
  }
  .tb:hover {
    border-color: #101014;
    background: linear-gradient(180deg, #47503f, #32382d);
    color: #d6f5d0;
  }
  .tb:active {
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  .hint {
    margin-left: auto;
    font-size: 9px;
    color: var(--ftbq-text-muted, #9a9aa0);
    letter-spacing: 0.02em;
  }
  .viewport {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    z-index: 10;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 12px;
    font-weight: 600;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.8);
  }
  .vignette {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
    box-shadow: inset 0 0 64px rgba(0, 0, 0, 0.38);
  }

  :global(.svelte-flow__background) {
    background-color: var(--ftbq-bg-canvas, #2b2b30);
  }
  :global(.svelte-flow__edge path) {
    stroke-width: 3;
  }
  :global(.svelte-flow__edge:hover path) {
    stroke: #7fb3c8;
  }
  :global(.svelte-flow__controls) {
    border: 1px solid #101014;
    border-radius: 3px;
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 4px 10px rgba(0, 0, 0, 0.45);
  }
  :global(.svelte-flow__controls button) {
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    border: none;
    border-bottom: 1px solid #101014;
    color: var(--ftbq-text, #e8e8e8);
  }
  :global(.svelte-flow__controls button:hover) {
    background: linear-gradient(180deg, #46464f, #32323a);
  }
  :global(.svelte-flow__controls button svg) {
    fill: var(--ftbq-text, #e8e8e8);
  }
  :global(.svelte-flow__attribution) {
    display: none;
  }
</style>
