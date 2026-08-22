<script lang="ts">
  import {
    SvelteFlow,
    Background,
    Controls,
    MiniMap,
    BackgroundVariant,
    useSvelteFlow,
    type Node,
    type Edge,
    type Connection,
  } from "@xyflow/svelte";
  import "@xyflow/svelte/dist/style.css";
  import { tick } from "svelte";
  import { Maximize2, Plus, LayoutGrid, ChevronDown } from "@lucide/svelte";
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
  import { getWorldCoordinates, rectsIntersect } from "./coords";

  let {
    quests,
    chapters = [],
    selectedId = null,
    selectedIds = new Set<string>(),
    issues,
    progressStatuses = {},
    progressOverlay = false,
    emptyHint = "Double-click to add a quest",
    showEmptyAddCta = false,
    onSelect,
    onMove,
    onAddAt,
    onLink,
    onUnlink = undefined,
    onEdgeSelect = undefined,
    onOpenChapter = undefined,
    onSelectMultiple = () => {},
    fitToken = 0,
    addQuestToken = 0,
    questFilter = "",
    filterTotal = 0,
    onQuestFilterChange = undefined,
    onApplyLayout = undefined,
  }: {
    quests: QuestData[];
    chapters?: QuestChapter[];
    selectedId?: string | null;
    selectedIds?: Set<string>;
    issues: QuestValidationIssue[];
    progressStatuses?: Record<string, QuestProgressStatus>;
    progressOverlay?: boolean;
    emptyHint?: string;
    showEmptyAddCta?: boolean;
    onSelect: (q: QuestData | null, e?: MouseEvent) => void;
    onMove: (q: QuestData, x: number, y: number) => void;
    onAddAt: (x: number, y: number) => void;
    onLink: (fromId: string, toDepId: string) => void;
    /** Remove dependency edge: questId lists depId in dependencies. */
    onUnlink?: (questId: string, depId: string) => void;
    /** Fired when a dependency edge is selected or cleared. */
    onEdgeSelect?: (edge: { questId: string; depId: string } | null) => void;
    /** Open another chapter (cross-chapter ghost click). */
    onOpenChapter?: (chapterId: string, questId?: string) => void;
    onSelectMultiple?: (ids: string[]) => void;
    fitToken?: number;
    addQuestToken?: number;
    questFilter?: string;
    filterTotal?: number;
    onQuestFilterChange?: (value: string) => void;
    onApplyLayout?: (kind: "tree" | "grid" | "circle") => void;
  } = $props();

  const BASE = 24;
  const nodeTypes = { quest: QuestNode };

  let issueIds = $derived(new Set(issues.map((i) => i.questId)));
  let iconRevision = $state(0);
  let selectedEdgeId = $state<string | null>(null);
  let layoutMenuOpen = $state(false);
  let lastLayout = $state<"tree" | "grid" | "circle" | null>(null);

  /** Live zoom for the toolbar readout — SvelteFlow moves the viewport via translate/scale. */
  let zoomPercent = $state(100);
  $effect(() => {
    const el = document.querySelector(".svelte-flow__viewport");
    if (!el) return;
    const read = () => {
      const m = /scale\(([\d.]+)\)/.exec(el.getAttribute("style") ?? "");
      zoomPercent = m ? Math.round(parseFloat(m[1]) * 100) : 100;
    };
    read();
    const mo = new MutationObserver(read);
    mo.observe(el, { attributes: true, attributeFilter: ["style"] });
    return () => mo.disconnect();
  });

  $effect(() => {
    if (!layoutMenuOpen) return;
    const onPtr = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target?.closest?.(".layout-pop")) layoutMenuOpen = false;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") layoutMenuOpen = false;
    };
    window.addEventListener("pointerdown", onPtr, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onPtr, true);
      window.removeEventListener("keydown", onKey, true);
    };
  });

  function pickLayout(mode: "tree" | "grid" | "circle") {
    lastLayout = mode;
    layoutMenuOpen = false;
    onApplyLayout?.(mode);
  }

  const { screenToFlowPosition, fitView: flowFitView, getViewport } = useSvelteFlow();

  let nodes = $state<Node[]>([]);
  let edges = $state<Edge[]>([]);
  let viewportEl = $state<HTMLDivElement | null>(null);

  /** Marquee in flow/world px (same space as node.position). */
  let marqueeWorld = $state<null | { x1: number; y1: number; x2: number; y2: number }>(null);
  /** Overlay box in container-local CSS px for drawing. */
  let marqueeScreen = $state<null | { left: number; top: number; width: number; height: number }>(
    null,
  );
  let marqueeOriginScreen = $state<null | { x: number; y: number }>(null);
  let marqueeActive = $state(false);

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
  ): { quest: QuestData; chapterTitle: string; chapterId: string } | null {
    for (const ch of chapters) {
      const direct = ch.quests.find((oq) => oq.id === depId);
      if (direct && !quests.some((q) => q.id === direct.id)) {
        return {
          quest: direct,
          chapterTitle: ch.title || ch.filename || ch.id.slice(0, 8),
          chapterId: ch.id,
        };
      }
      const owner = ch.quests.find((oq) => oq.tasks?.some((t) => t.id === depId));
      if (owner && !quests.some((q) => q.id === owner.id)) {
        return {
          quest: owner,
          chapterTitle: ch.title || ch.filename || ch.id.slice(0, 8),
          chapterId: ch.id,
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
                selectable: true,
                data: {
                  quest: ext.quest,
                  isIssue: false,
                  isSelected: false,
                  baseSize: BASE,
                  progress: null,
                  iconRevision: rev,
                  external: true,
                  chapterTitle: ext.chapterTitle,
                  chapterId: ext.chapterId,
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
          "stroke: var(--ftbq-line, #5c8a9e); stroke-width: 3; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));";
        if (!targetExists || external) {
          style =
            "stroke: var(--ftbq-quest-started); stroke-width: 2.5; stroke-dasharray: 6 4; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));";
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
          selectable: !external,
          data: { depId, dependentId: q.id },
          selected: selectedEdgeId === `e-${depId}-${q.id}`,
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

  let lastAddQuestToken = $state(0);
  $effect(() => {
    if (addQuestToken !== lastAddQuestToken && addQuestToken > 0) {
      lastAddQuestToken = addQuestToken;
      tick().then(() => addAtCenter());
    }
  });

  function snap(v: number) {
    return Math.round(v * 2) / 2;
  }

  function flowContainer(): HTMLElement | null {
    if (!viewportEl) return null;
    return (
      (viewportEl.querySelector(".svelte-flow") as HTMLElement | null) ??
      (viewportEl.querySelector(".react-flow") as HTMLElement | null) ??
      viewportEl
    );
  }

  function isEmptyPaneTarget(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false;
    if (target.closest(".xyflow__node, .svelte-flow__node, .react-flow__node")) return false;
    if (target.closest(".xyflow__edge, .svelte-flow__edge, .react-flow__edge")) return false;
    if (target.closest(".xyflow__controls, .svelte-flow__controls, .react-flow__controls"))
      return false;
    if (target.closest(".xyflow__minimap, .svelte-flow__minimap, button, a, input, textarea"))
      return false;
    return !!(
      target.closest(".xyflow__pane, .svelte-flow__pane, .react-flow__pane") ||
      target.closest(".svelte-flow, .react-flow") ||
      target === viewportEl
    );
  }

  function nodeFlowSize(n: Node): number {
    const q = (n.data as { quest?: QuestData; baseSize?: number } | undefined)?.quest;
    const base = (n.data as { baseSize?: number } | undefined)?.baseSize ?? BASE;
    const scale = q?.size && q.size > 0 ? q.size : 1;
    return base * scale;
  }

  function finishMarquee(additive: boolean) {
    if (!marqueeWorld) {
      marqueeActive = false;
      marqueeScreen = null;
      marqueeOriginScreen = null;
      return;
    }
    const box = marqueeWorld;
    const hit: string[] = [];
    for (const n of nodes) {
      if (n.id.startsWith("ext:")) continue;
      const size = nodeFlowSize(n);
      const nb = {
        x1: n.position.x,
        y1: n.position.y,
        x2: n.position.x + size,
        y2: n.position.y + size,
      };
      if (rectsIntersect(box, nb)) hit.push(n.id);
    }
    marqueeActive = false;
    marqueeWorld = null;
    marqueeScreen = null;
    marqueeOriginScreen = null;
    if (hit.length > 0) {
      onSelectMultiple(hit);
    } else if (!additive) {
      onSelect(null);
    }
  }

  function focusCanvas() {
    viewportEl?.focus({ preventScroll: true });
  }

  function onMarqueePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if (!isEmptyPaneTarget(e.target)) return;
    focusCanvas();
    // Let middle/right pan alone; left on empty pane = marquee.
    const container = flowContainer();
    if (!container) return;
    const { x: panX, y: panY, zoom } = getViewport();
    const world = getWorldCoordinates(e, container, panX, panY, zoom);
    const rect = container.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    marqueeActive = true;
    marqueeWorld = { x1: world.x, y1: world.y, x2: world.x, y2: world.y };
    marqueeOriginScreen = { x: sx, y: sy };
    marqueeScreen = { left: sx, top: sy, width: 0, height: 0 };
    try {
      viewportEl?.setPointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    e.preventDefault();
    e.stopPropagation();
  }

  function onMarqueePointerMove(e: PointerEvent) {
    if (!marqueeActive || !marqueeWorld || !marqueeOriginScreen) return;
    const container = flowContainer();
    if (!container) return;
    const { x: panX, y: panY, zoom } = getViewport();
    const world = getWorldCoordinates(e, container, panX, panY, zoom);
    const rect = container.getBoundingClientRect();
    const sx = e.clientX - rect.left;
    const sy = e.clientY - rect.top;
    const ox = marqueeOriginScreen.x;
    const oy = marqueeOriginScreen.y;
    marqueeWorld = { ...marqueeWorld, x2: world.x, y2: world.y };
    marqueeScreen = {
      left: Math.min(ox, sx),
      top: Math.min(oy, sy),
      width: Math.abs(sx - ox),
      height: Math.abs(sy - oy),
    };
  }

  function onMarqueePointerUp(e: PointerEvent) {
    if (!marqueeActive) return;
    try {
      viewportEl?.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    const dragged =
      marqueeScreen != null && (marqueeScreen.width > 3 || marqueeScreen.height > 3);
    if (dragged) {
      finishMarquee(e.shiftKey || e.ctrlKey || e.metaKey);
    } else {
      marqueeActive = false;
      marqueeWorld = null;
      marqueeScreen = null;
      marqueeOriginScreen = null;
    }
  }

  function onCanvasDblClick(e: MouseEvent) {
    if (!isEmptyPaneTarget(e.target)) return;
    const container = flowContainer();
    if (!container) return;
    e.preventDefault();
    e.stopPropagation();
    const { x: panX, y: panY, zoom } = getViewport();
    const world = getWorldCoordinates(e, container, panX, panY, zoom);
    onAddAt(snap(world.x / BASE), snap(world.y / BASE));
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

  function isValidConnection(connection: Edge | Connection | null | undefined): boolean {
    if (!connection?.source || !connection?.target) return false;
    if (connection.source === connection.target) return false;
    // Dependent (target) must be a local chapter quest — cannot depend "into" a ghost.
    if (connection.target.startsWith("ext:")) return false;
    const src = connection.source.startsWith("ext:")
      ? connection.source.slice(4)
      : connection.source;
    const dependentId = connection.target;
    if (!quests.some((q) => q.id === dependentId)) return false;
    if (src === dependentId) return false;
    const dependent = quests.find((q) => q.id === dependentId);
    if (dependent?.dependencies?.includes(src)) return false;
    return true;
  }

  function clearEdgeSelection() {
    if (!selectedEdgeId) return;
    selectedEdgeId = null;
    edges = edges.map((ed) => (ed.selected ? { ...ed, selected: false } : ed));
    onEdgeSelect?.(null);
  }

  function handleEdgeClick({ edge }: { edge: Edge }) {
    const depId = (edge.data as { depId?: string } | undefined)?.depId;
    const dependentId = (edge.data as { dependentId?: string } | undefined)?.dependentId;
    if (!depId || !dependentId) return;
    selectedEdgeId = edge.id;
    edges = edges.map((ed) => ({
      ...ed,
      selected: ed.id === edge.id,
      style:
        ed.id === edge.id
          ? `${String(ed.style ?? "").replace(/stroke:[^;]+;?/g, "").replace(/stroke-width:[^;]+;?/g, "")} stroke: var(--ftbq-accent-teal); stroke-width: 4;`
          : ed.style,
    }));
    onEdgeSelect?.({ questId: dependentId, depId });
    onSelect(null);
  }

  function handlePaneClick({ event }: { event: MouseEvent }) {
    // Double-click create is handled via ondblclick on the viewport (getWorldCoordinates).
    if (event.detail >= 2) return;
    if (marqueeActive) return;
    clearEdgeSelection();
    focusCanvas();
    onSelect(null, event);
  }

  function handleNodeClick({ node, event }: { node: Node; event: MouseEvent | TouchEvent }) {
    if (node.id.startsWith("ext:")) {
      const chapterId = (node.data as { chapterId?: string } | undefined)?.chapterId;
      const questId = (node.data as { quest?: QuestData } | undefined)?.quest?.id;
      if (chapterId && onOpenChapter) {
        onOpenChapter(chapterId, questId);
      }
      return;
    }
    clearEdgeSelection();
    const q = quests.find((item) => item.id === node.id);
    if (q) {
      focusCanvas();
      onSelect(q, event instanceof MouseEvent ? event : undefined);
    }
  }

  function addAtCenter() {
    const container = flowContainer();
    if (container) {
      const rect = container.getBoundingClientRect();
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      const { x: panX, y: panY, zoom } = getViewport();
      const world = getWorldCoordinates({ clientX: cx, clientY: cy }, container, panX, panY, zoom);
      onAddAt(snap(world.x / BASE), snap(world.y / BASE));
    } else {
      const pos = screenToFlowPosition({
        x: window.innerWidth / 2,
        y: window.innerHeight / 2,
      });
      onAddAt(snap(pos.x / BASE), snap(pos.y / BASE));
    }
  }

  /** Move selection among chapter quests (list order). */
  function selectQuestByIndex(index: number) {
    if (!quests.length) return;
    const clamped = Math.max(0, Math.min(index, quests.length - 1));
    onSelect(quests[clamped]);
  }

  function handleCanvasKeydown(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    switch (e.key) {
      case "Escape": {
        e.preventDefault();
        e.stopPropagation();
        onSelect(null);
        return;
      }
      case "Home": {
        if (!quests.length) return;
        e.preventDefault();
        e.stopPropagation();
        selectQuestByIndex(0);
        return;
      }
      case "End": {
        if (!quests.length) return;
        e.preventDefault();
        e.stopPropagation();
        selectQuestByIndex(quests.length - 1);
        return;
      }
      case "ArrowRight":
      case "ArrowDown": {
        if (!quests.length) return;
        e.preventDefault();
        e.stopPropagation();
        {
          const idx = selectedId ? quests.findIndex((q) => q.id === selectedId) : -1;
          selectQuestByIndex(idx < 0 ? 0 : Math.min(idx + 1, quests.length - 1));
        }
        return;
      }
      case "ArrowLeft":
      case "ArrowUp": {
        if (!quests.length) return;
        e.preventDefault();
        e.stopPropagation();
        {
          const idx = selectedId ? quests.findIndex((q) => q.id === selectedId) : -1;
          selectQuestByIndex(idx < 0 ? quests.length - 1 : Math.max(idx - 1, 0));
        }
        return;
      }
      default:
        return;
    }
  }
</script>

<div class="canvas-wrap ftbq-canvas">
  <div class="canvas-toolbar">
    {#if onQuestFilterChange}
      <input
        type="search"
        class="tb-filter"
        placeholder="Filter…"
        title="Hide nodes that don’t match (canvas filter). Ctrl+F searches fields and jumps."
        aria-label="Filter quests on canvas"
        value={questFilter}
        oninput={(e) => onQuestFilterChange?.((e.currentTarget as HTMLInputElement).value)}
        onkeydown={(e) => {
          if (e.key === "Escape") {
            onQuestFilterChange?.("");
          }
          if (e.key === "Enter") {
            e.preventDefault();
            const first = quests[0];
            if (first) onSelect(first);
          }
        }}
      />
      {#if questFilter}
        <span class="filt-count">{quests.length}/{filterTotal}</span>
      {/if}
    {/if}
    <button type="button" class="tb" title="Fit view" aria-label="Fit view" onclick={() => flowFitView({ padding: 0.2 })}>
      <Maximize2 size={14} class="flex-shrink-0" /> Fit
    </button>
    <span class="zoom-pct" title="Zoom level">{zoomPercent}%</span>
    <button type="button" class="tb" title="Add quest at center (N or double-click)" aria-label="Add quest at center" onclick={addAtCenter}>
      <Plus size={14} class="flex-shrink-0" /> Add quest
    </button>
    {#if onApplyLayout}
      <div class="layout-pop">
        <button
          type="button"
          class="tb"
          class:active={layoutMenuOpen}
          title="Auto-layout current chapter"
          aria-haspopup="menu"
          aria-expanded={layoutMenuOpen}
          onclick={() => (layoutMenuOpen = !layoutMenuOpen)}
        >
          <LayoutGrid size={14} class="flex-shrink-0" /> Layout
          <ChevronDown size={12} class="flex-shrink-0" />
        </button>
        {#if layoutMenuOpen}
          <div class="layout-menu" role="menu">
            <button role="menuitemradio" aria-checked={lastLayout === "tree"} onclick={() => pickLayout("tree")}>
              Tree
            </button>
            <button role="menuitemradio" aria-checked={lastLayout === "grid"} onclick={() => pickLayout("grid")}>
              Grid
            </button>
            <button role="menuitemradio" aria-checked={lastLayout === "circle"} onclick={() => pickLayout("circle")}>
              Circle
            </button>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Focusable canvas widget: arrow/Home/End/Escape selection via existing onSelect -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="viewport"
    role="application"
    tabindex="0"
    aria-label="Quest canvas. Arrow keys select next or previous quest. Home and End jump. Escape clears selection. Shift+Arrow outside canvas nudges selected quests."
    bind:this={viewportEl}
    onpointerdown={onMarqueePointerDown}
    onpointermove={onMarqueePointerMove}
    onpointerup={onMarqueePointerUp}
    onpointercancel={onMarqueePointerUp}
    ondblclick={onCanvasDblClick}
    onkeydown={handleCanvasKeydown}
  >
    {#if quests.length === 0}
      <div class="empty-hint">
        {#if showEmptyAddCta}
          <button type="button" class="empty-add" onclick={(e) => { e.stopPropagation(); addAtCenter(); }}>
            + Add first quest
          </button>
          <span class="empty-sub">Double-click canvas · Press N · Use toolbar button</span>
        {:else}
          <span>{emptyHint}</span>
        {/if}
      </div>
    {/if}

    <SvelteFlow
      bind:nodes
      bind:edges
      {nodeTypes}
      panOnScroll
      selectionOnDrag={false}
      panOnDrag={[1, 2]}
      deleteKey={null}
      onnodeclick={handleNodeClick}
      onpaneclick={handlePaneClick}
      onnodedragstop={handleNodeDragStop}
      onconnect={handleConnect}
      isValidConnection={isValidConnection}
      onedgeclick={handleEdgeClick}
      fitView
      fitViewOptions={{ padding: 0.2 }}
      defaultEdgeOptions={{
        type: "step",
        style:
          "stroke: var(--ftbq-line, #5c8a9e); stroke-width: 3; filter: drop-shadow(0 1px 1px rgba(0,0,0,0.6));",
      }}
    >
      <Background
        variant={BackgroundVariant.Dots}
        gap={20}
        size={1}
        patternColor="rgba(255,255,255,0.07)"
      />
      <Controls />
      <MiniMap
        pannable
        zoomable
        nodeStrokeWidth={2}
        maskColor="rgba(0, 0, 0, 0.45)"
        bgColor="var(--ftbq-bg-panel, #1a1a1e)"
        nodeColor={() => "var(--ftbq-accent-teal)"}
        ariaLabel="Chapter minimap"
      />
    </SvelteFlow>

    {#if marqueeScreen && (marqueeScreen.width > 0 || marqueeScreen.height > 0)}
      <div
        class="marquee-box"
        style="left:{marqueeScreen.left}px; top:{marqueeScreen.top}px; width:{marqueeScreen.width}px; height:{marqueeScreen.height}px;"
      ></div>
    {/if}
    <div class="vignette" aria-hidden="true"></div>
  </div>
</div>

<style>
  .canvas-wrap {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    background: var(--ftbq-bg-canvas);
    border: none;
    border-left: 1px solid var(--ftbq-frame);
    border-right: 1px solid var(--ftbq-frame);
    overflow: hidden;
  }
  .canvas-toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--ftbq-frame);
    background: var(--ftbq-bg-panel);
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .tb-filter {
    min-width: 120px;
    flex: 1;
    max-width: 220px;
    font-size: 12px;
    padding: 4px 8px;
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text);
    border-radius: var(--ftbq-radius-control);
  }
  .canvas-toolbar .tb-filter:focus {
    border-color: var(--ftbq-focus-border);
    box-shadow: 0 0 0 2px var(--ftbq-focus-ring);
  }
  .canvas-toolbar .tb:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: 1px;
  }
  .filt-count {
    font-size: 11px;
    color: var(--ftbq-text-muted);
  }
  .zoom-pct {
    font-size: 10px;
    font-weight: 600;
    color: var(--ftbq-text-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .layout-pop {
    position: relative;
    flex-shrink: 0;
    margin-left: auto;
  }
  .layout-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 40;
    min-width: 130px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.35));
  }
  .layout-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 7px 10px;
    border: none;
    border-radius: var(--ftbq-radius-control);
    background: transparent;
    color: var(--ftbq-text);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-shadow: none;
  }
  .layout-menu button:hover,
  .layout-menu button[aria-checked="true"] {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
  }
  .layout-menu button[aria-checked="true"]::after {
    content: "✓";
    color: var(--ftbq-accent-teal);
  }
  .tb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: var(--ftbq-radius-control);
    border: 1px solid var(--ftbq-frame);
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    color: var(--ftbq-text);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    text-shadow: none;
    box-shadow: none;
  }
  .tb:hover {
    border-color: var(--ftbq-frame);
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text);
  }
  .tb:active {
    background: var(--bg-active, var(--ftbq-btn-hover-bottom));
    box-shadow: none;
  }
  .viewport {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .viewport:focus {
    outline: none;
  }
  .viewport:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: -2px;
  }
  .empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    pointer-events: none;
    z-index: 10;
    color: var(--ftbq-text-muted);
    font-size: 12px;
    font-weight: 600;
    text-shadow: none;
  }
  .empty-add {
    pointer-events: auto;
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 700;
    border: 1px solid var(--ftbq-accent-teal);
    border-radius: var(--ftbq-radius-control);
    background: rgba(61, 184, 168, 0.15);
    color: var(--ftbq-accent-teal);
    cursor: pointer;
    text-shadow: none;
  }
  .empty-add:hover {
    background: rgba(61, 184, 168, 0.28);
  }
  .empty-sub {
    font-size: 11px;
    font-weight: 500;
    color: var(--ftbq-text-muted);
  }
  .vignette {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 5;
    box-shadow: inset 0 0 64px rgba(0, 0, 0, 0.25);
  }

  .marquee-box {
    position: absolute;
    z-index: 8;
    pointer-events: none;
    border: 1px solid color-mix(in srgb, var(--ftbq-accent-teal) 85%, #fff);
    background: color-mix(in srgb, var(--ftbq-accent-teal) 18%, transparent);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
  }

  :global(.flex-shrink-0) {
    flex-shrink: 0;
  }

  :global(.svelte-flow__background) {
    background-color: var(--ftbq-bg-canvas);
  }
  /* Only restyle the visible path — the invisible interaction path (20px hit
     target) must keep its default width or edge clicks stop registering. */
  :global(.svelte-flow__edge .svelte-flow__edge-path) {
    stroke-width: 3;
  }
  :global(.ftbq-canvas .svelte-flow__handle.connectionindicator),
  :global(.ftbq-canvas .svelte-flow__node:hover .svelte-flow__handle) {
    pointer-events: all !important;
  }
  :global(.svelte-flow__edge:hover path) {
    stroke: var(--ftbq-line-hover);
  }
  :global(.svelte-flow__minimap) {
    display: none !important;
  }
  :global(.svelte-flow__controls) {
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-control);
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 4px 10px rgba(0, 0, 0, 0.45);
  }
  :global(.svelte-flow__controls button) {
    background: linear-gradient(180deg, var(--ftbq-border), var(--ftbq-btn-bottom));
    border: none;
    border-bottom: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text);
  }
  :global(.svelte-flow__controls button:hover) {
    background: linear-gradient(180deg, var(--ftbq-btn-hover-top), var(--ftbq-btn-hover-bottom));
  }
  :global(.svelte-flow__controls button svg),
  :global(.ftbq-canvas .tb svg),
  :global(.ftbq-canvas .flex-shrink-0) {
    flex-shrink: 0;
    fill: var(--ftbq-text);
  }
  :global(.svelte-flow__attribution) {
    display: none;
  }
</style>
