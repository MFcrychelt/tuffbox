<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { GitGraph, RefreshCw, AlertTriangle, Box, Workflow } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import * as d3 from "d3-force";
  import { onMount } from "svelte";

  type GraphNode = {
    id: string;
    kind: string;
    label: string;
    version?: string | null;
    side?: string;
    metadata?: Record<string, string>;
  };

  type GraphEdge = {
    from: string;
    to: string;
    kind: string;
    constraint?: string | null;
    reason?: string | null;
  };

  type GraphModel = {
    nodes: GraphNode[];
    edges: GraphEdge[];
  };

  type PositionedNode = GraphNode & { x: number; y: number; fx?: number | null; fy?: number | null; tone: string };

  let graph: GraphModel | null = null;
  let loading = false;
  let error: string | null = null;
  let selectedId: string | null = null;
  let lastLoadedPath: string | null = null;
  let resolving = false;
  let message: string | null = null;
  let changePlan: any | null = null;

  function normalizeNode(node: any): GraphNode {
    return {
      ...node,
      id: typeof node.id === "string" ? node.id : node.id?.[0] ?? String(node.id),
    };
  }

  function normalizeEdge(edge: any): GraphEdge {
    const from = typeof edge.from === "string" ? edge.from : edge.from?.[0] ?? String(edge.from);
    const to = typeof edge.to === "string" ? edge.to : edge.to?.[0] ?? String(edge.to);
    return { ...edge, from, to };
  }

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && graph) return;
    loading = true;
    error = null;
    try {
      const raw: any = await invoke("get_graph", { path: $projectPath });
      graph = {
        nodes: (raw.nodes ?? []).map(normalizeNode),
        edges: (raw.edges ?? []).map(normalizeEdge),
      };
      selectedId = graph.nodes.find((n) => n.kind === "Mod")?.id ?? graph.nodes[0]?.id ?? null;
      await loadChangePlan();
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function nodeById(id: string) {
    return graph?.nodes.find((n) => n.id === id) ?? null;
  }

  function point(id: string) {
    return positionById.get(id);
  }

  function edgeDanger(edge: GraphEdge) {
    return edge.kind === "Requires" && !nodeById(edge.to);
  }

  function modIdFromNode(nodeId: string) {
    return nodeId.startsWith("mod:") ? nodeId.slice(4) : nodeId;
  }

  async function removeConflictNode(nodeId: string) {
    if (!$projectPath) return;
    const modId = modIdFromNode(nodeId);
    resolving = true;
    error = null;
    message = null;
    try {
      await invoke("remove_project_mod", { path: $projectPath, modId });
      message = `Removed conflicting mod ${modId}. Auto snapshot created.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  async function loadChangePlan() {
    if (!$projectPath) return;
    try {
      changePlan = await invoke("get_resolve_change_plan", { path: $projectPath });
    } catch {
      changePlan = null;
    }
  }

  async function applyAction(index: number) {
    if (!$projectPath || !changePlan) return;
    resolving = true;
    error = null;
    message = null;
    try {
      const applied: string[] = await invoke("apply_resolve_action", { path: $projectPath, actionIndex: index });
      message = applied.length ? `Applied action: ${applied.join(", ")}` : "No deterministic action was applied.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  async function applyChangePlan() {
    if (!$projectPath || !changePlan) return;
    resolving = true;
    error = null;
    message = null;
    try {
      const applied: string[] = await invoke("apply_resolve_change_plan", { path: $projectPath });
      message = applied.length ? `Applied plan: ${applied.join(", ")}` : "No deterministic actions were applied.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  async function installMissingDependencies() {
    if (!$projectPath || missingEdges.length === 0) return;
    resolving = true;
    error = null;
    message = null;
    try {
      const installed: string[] = await invoke("resolve_missing_dependencies", { path: $projectPath });
      message = installed.length ? `Installed dependencies: ${installed.join(", ")}` : "No installable missing dependencies were found.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  $: nodes = graph?.nodes ?? [];
  $: edges = graph?.edges ?? [];
  $: selected = selectedId ? nodeById(selectedId) : null;
  $: selectedEdges = selectedId
    ? edges.filter((edge) => edge.from === selectedId || edge.to === selectedId)
    : [];
  $: missingEdges = edges.filter((edge) => edge.kind === "Requires" && !nodeById(edge.to));
  $: conflictEdges = edges.filter((edge) => ["Conflicts", "BreaksWith"].includes(edge.kind) && nodeById(edge.from) && nodeById(edge.to));
  $: byKind = nodes.reduce<Record<string, number>>((acc, node) => {
    acc[node.kind] = (acc[node.kind] ?? 0) + 1;
    return acc;
  }, {});
  $: modNodes = nodes.filter((node) => node.kind === "Mod");
  $: platformNodes = nodes.filter((node) => node.kind !== "Mod" && node.kind !== "Profile");
  $: profileNodes = nodes.filter((node) => node.kind === "Profile");
  $: canvasHeight = Math.max(360, Math.ceil(Math.max(modNodes.length, profileNodes.length, platformNodes.length) / 2) * 96 + 120);
  let positioned: PositionedNode[] = [];
  let simulation: d3.Simulation<PositionedNode, undefined> | null = null;

  $: if (nodes && edges) {
    const initializedNodes = nodes.map((node) => {
      const group = node.kind === "Mod" ? "mod" : node.kind === "Profile" ? "profile" : "runtime";
      const tone = group === "runtime" ? "runtime" : group === "profile" ? "profile" : String(node.side ?? "unknown").toLowerCase();
      return { ...node, x: 500, y: 300, tone };
    });

    const d3Links = edges.map(e => ({ source: e.from, target: e.to, ...e }));

    if (simulation) simulation.stop();

    simulation = d3.forceSimulation<PositionedNode>(initializedNodes)
      .force("link", d3.forceLink(d3Links).id((d: any) => d.id).distance(150))
      .force("charge", d3.forceManyBody().strength(-400))
      .force("center", d3.forceCenter(560, 300))
      .force("x", d3.forceX(560).strength(0.05))
      .force("y", d3.forceY(300).strength(0.05))
      .on("tick", () => {
        positioned = [...initializedNodes];
      });
  }
  $: positionById = new Map(positioned.map((node) => [node.id, node]));

  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
</script>

<div class="graph">
  <div class="toolbar">
    <div class="title">
      <GitGraph size={18} />
      <span>Dependency graph</span>
    </div>
    <div class="toolbar-actions">
      <button class="secondary" on:click={installMissingDependencies} disabled={!$projectPath || resolving || missingEdges.length === 0}>
        <Workflow size={16} />
        {resolving ? "Resolving..." : "Auto-install dependencies"}
      </button>
      <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if message}<div class="notice success">{message}</div>{/if}

  {#if loading}
    <div class="loading">Loading graph...</div>
  {:else if error}
    <div class="empty error">{error}</div>
  {:else if graph}
    <div class="stats">
      <div class="stat-card">
        <span class="stat-value">{nodes.length}</span>
        <span class="stat-label">Nodes</span>
      </div>
      <div class="stat-card">
        <span class="stat-value">{edges.length}</span>
        <span class="stat-label">Edges</span>
      </div>
      <div class="stat-card" class:danger={missingEdges.length > 0}>
        <span class="stat-value">{missingEdges.length}</span>
        <span class="stat-label">Missing deps</span>
      </div>
      <div class="stat-card accent">
        <span class="stat-value">{byKind.Mod ?? 0}</span>
        <span class="stat-label">Mods</span>
      </div>
      <div class="stat-card" class:danger={conflictEdges.length > 0}>
        <span class="stat-value">{conflictEdges.length}</span>
        <span class="stat-label">Conflicts</span>
      </div>
    </div>

    {#if changePlan}
      <section class="change-plan-panel">
        <div>
          <span class="eyebrow">Change plan</span>
          <h2>{changePlan.summary}</h2>
          <p>Risk: {changePlan.risk} · {changePlan.requiresSnapshot ? "snapshot required" : "no snapshot required"}</p>
        </div>
        {#if changePlan.actions?.length}
          <div class="plan-actions-list">
            {#each changePlan.actions as action, index}
              <div class="plan-action-row">
                <code>{JSON.stringify(action)}</code>
                <button class="secondary mini" on:click={() => applyAction(index)} disabled={resolving}>Apply action</button>
              </div>
            {/each}
            <button on:click={applyChangePlan} disabled={resolving}>Apply full plan</button>
          </div>
        {:else}
          <button class="secondary" on:click={applyChangePlan} disabled={resolving}>Mark reviewed</button>
        {/if}
      </section>
    {/if}

    <section class="graph-canvas" aria-label="Dependency graph canvas">
      <svg viewBox={`0 0 1120 ${canvasHeight}`} role="img">
        <defs>
          <marker id="arrow" markerWidth="8" markerHeight="8" refX="22" refY="3" orient="auto" markerUnits="strokeWidth">
            <path d="M0,0 L0,6 L7,3 z" fill="rgba(161,161,170,.75)" />
          </marker>
          <marker id="arrow-danger" markerWidth="8" markerHeight="8" refX="22" refY="3" orient="auto" markerUnits="strokeWidth">
            <path d="M0,0 L0,6 L7,3 z" fill="rgba(239,68,68,.85)" />
          </marker>
        </defs>
        {#each edges as edge}
          <line
            class:danger-edge={edgeDanger(edge)}
            x1={point(edge.from)?.x ?? 0}
            y1={point(edge.from)?.y ?? 0}
            x2={point(edge.to)?.x ?? 1060}
            y2={point(edge.to)?.y ?? (point(edge.from)?.y ?? 0)}
            marker-end={edgeDanger(edge) ? "url(#arrow-danger)" : "url(#arrow)"}
          />
        {/each}
        {#each positioned as node}
          <g
            class="svg-node tone-{node.tone}"
            class:selected={selectedId === node.id}
            role="button"
            tabindex="0"
            transform={`translate(${node.x}, ${node.y})`}
            on:mousedown={(e) => {
              selectedId = node.id;
              if (simulation) {
                let isDragging = true;
                node.fx = node.x;
                node.fy = node.y;
                const onMouseMove = (ev) => {
                  node.fx = ev.offsetX;
                  node.fy = ev.offsetY;
                  simulation.alpha(0.3).restart();
                };
                const onMouseUp = () => {
                  node.fx = null;
                  node.fy = null;
                  window.removeEventListener('mousemove', onMouseMove);
                  window.removeEventListener('mouseup', onMouseUp);
                };
                window.addEventListener('mousemove', onMouseMove);
                window.addEventListener('mouseup', onMouseUp);
              }
            }}
          >
            <circle r="6" />
            <text x="10" y="4">{node.label}</text>
          </g>
        {/each}
      </svg>
    </section>

    <div class="graph-layout">
      <section class="node-column">
        <h3><Workflow size={16} /> Runtime</h3>
        {#each platformNodes as node}
          <button class="node-card kind-{node.kind}" class:selected={selectedId === node.id} on:click={() => (selectedId = node.id)}>
            <span class="node-label">{node.label}</span>
            <span class="node-meta">{node.kind}{node.version ? ` · ${node.version}` : ""}</span>
          </button>
        {/each}

        <h3><Box size={16} /> Profiles</h3>
        {#each profileNodes as node}
          <button class="node-card kind-{node.kind}" class:selected={selectedId === node.id} on:click={() => (selectedId = node.id)}>
            <span class="node-label">{node.label}</span>
            <span class="node-meta">{node.id}</span>
          </button>
        {/each}
      </section>

      <section class="node-column mods-column">
        <h3><Box size={16} /> Mods</h3>
        {#if modNodes.length === 0}
          <div class="muted-box">No mod nodes yet. Add mods from the Mods tab.</div>
        {:else}
          <div class="mod-grid">
            {#each modNodes as node}
              <button class="node-card compact side-{node.side}" class:selected={selectedId === node.id} on:click={() => (selectedId = node.id)}>
                <span class="node-label">{node.label}</span>
                <span class="node-meta">{node.version ?? "unknown"} · {node.side}</span>
              </button>
            {/each}
          </div>
        {/if}
      </section>

      <aside class="details">
        {#if selected}
          <div class="details-header">
            <div>
              <span class="eyebrow">Selected node</span>
              <h2>{selected.label}</h2>
            </div>
            <span class="tag">{selected.kind}</span>
          </div>
          <div class="details-grid">
            <div><span>ID</span><code>{selected.id}</code></div>
            <div><span>Version</span><code>{selected.version ?? "—"}</code></div>
            <div><span>Side</span><code>{selected.side ?? "—"}</code></div>
            <div><span>Relations</span><code>{selectedEdges.length}</code></div>
          </div>

          {#if selected.metadata && Object.keys(selected.metadata).length > 0}
            <h3>Metadata</h3>
            <div class="kv">
              {#each Object.entries(selected.metadata) as [key, value]}
                <span>{key}</span><code>{value}</code>
              {/each}
            </div>
          {/if}

          <h3>Relations</h3>
          {#if selectedEdges.length === 0}
            <div class="muted-box">No direct relations.</div>
          {:else}
            <div class="relations">
              {#each selectedEdges as edge}
                <div class="relation" class:incoming={edge.to === selectedId}>
                  <span class="relation-kind">{edge.kind}</span>
                  <span class="relation-text">
                    {edge.from === selectedId ? "to" : "from"}
                    <strong>{nodeById(edge.from === selectedId ? edge.to : edge.from)?.label ?? (edge.from === selectedId ? edge.to : edge.from)}</strong>
                  </span>
                  {#if edge.reason}<small>{edge.reason}</small>{/if}
                </div>
              {/each}
            </div>
          {/if}
        {:else}
          <div class="muted-box">Select a node to inspect dependencies.</div>
        {/if}
      </aside>
    </div>

    {#if conflictEdges.length > 0}
      <div class="conflict-panel">
        <h3><AlertTriangle size={16} /> Conflicts</h3>
        {#each conflictEdges as edge}
          <div class="conflict-row">
            <div>
              <strong>{nodeById(edge.from)?.label ?? edge.from}</strong>
              <span>{edge.kind} with</span>
              <strong>{nodeById(edge.to)?.label ?? edge.to}</strong>
              {#if edge.reason}<small>{edge.reason}</small>{/if}
            </div>
            <div class="conflict-actions">
              <button class="ghost mini" on:click={() => removeConflictNode(edge.from)}>Remove left</button>
              <button class="ghost mini" on:click={() => removeConflictNode(edge.to)}>Remove right</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if missingEdges.length > 0}
      <div class="missing-panel">
        <h3><AlertTriangle size={16} /> Missing dependencies</h3>
        {#each missingEdges as edge}
          <div class="missing-row">
            <span>{nodeById(edge.from)?.label ?? edge.from}</span>
            <code>{edge.to}</code>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="empty">Open a project to view its dependency graph.</div>
  {/if}
</div>

<style>
   .graph {
    max-width: none;
    width: 100%;
  }

  .toolbar,
  .toolbar-actions,
  .notice {
    display: flex;
    align-items: center;
  }

  .toolbar {
    justify-content: space-between;
    margin-bottom: 20px;
  }

  .toolbar-actions { gap: 10px; }

  .notice {
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--border-radius-lg);
    margin-bottom: 14px;
    border: 1px solid var(--border-color);
  }

  .notice.success {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
  }

  .title,
  h3 {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  h3 {
    margin: 0 0 12px;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .stats {
    display: flex;
    gap: 16px;
    margin-bottom: 20px;
    flex-wrap: wrap;
  }

  .change-plan-panel {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
    padding: 16px;
    border: 1px solid rgba(27, 217, 106, .28);
    border-radius: var(--border-radius-lg);
    background: radial-gradient(circle at top left, rgba(27,217,106,.09), transparent 42%), var(--bg-secondary);
  }

  .change-plan-panel h2 { margin: 4px 0; font-size: 17px; }
  .change-plan-panel p { color: var(--text-muted); }
  .plan-actions-list { display: flex; flex-direction: column; gap: 6px; max-width: 560px; }
  .plan-action-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 8px; align-items: center; }
  .mini { padding: 5px 8px; font-size: 11px; }

  .graph-canvas {
    margin-bottom: 18px;
    background:
      radial-gradient(circle at 78% 18%, rgba(27,217,106,.08), transparent 28%),
      #09090b;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: auto;
  }

  .graph-canvas svg {
    min-width: 980px;
    width: 100%;
    height: auto;
    display: block;
  }

  .graph-canvas line {
    stroke: rgba(161, 161, 170, 0.5);
    stroke-width: 1.6;
  }

  .graph-canvas line.danger-edge {
    stroke: rgba(239, 68, 68, 0.8);
    stroke-dasharray: 6 5;
  }

  .svg-node {
    cursor: pointer;
  }

  .svg-node rect {
    fill: #18181b;
    stroke: rgba(255,255,255,.11);
    stroke-width: 1;
  }

  .svg-node.selected rect,
  .svg-node:hover rect {
    stroke: rgba(27,217,106,.72);
    fill: rgba(27,217,106,.12);
  }

  .svg-node.tone-client rect { stroke: rgba(139,92,246,.45); }
  .svg-node.tone-server rect { stroke: rgba(59,130,246,.45); }
  .svg-node.tone-both rect { stroke: rgba(27,217,106,.38); }
  .svg-node.tone-runtime rect { stroke: rgba(245,158,11,.42); }
  .svg-node.tone-profile rect { stroke: rgba(96,165,250,.42); }

  .svg-node text {
    fill: #e5e7eb;
    font-size: 12px;
    font-weight: 800;
    pointer-events: none;
  }

  .svg-node text.sub {
    fill: #a1a1aa;
    font-size: 10px;
    font-weight: 600;
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

  .stat-card.accent {
    border-color: rgba(27, 217, 106, 0.32);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.12), var(--bg-secondary));
  }

  .stat-card.danger {
    border-color: rgba(239, 68, 68, 0.35);
    background: linear-gradient(135deg, rgba(239, 68, 68, 0.12), var(--bg-secondary));
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

  .graph-layout {
    display: grid;
    grid-template-columns: 260px minmax(320px, 1fr) 360px;
    gap: 16px;
    align-items: start;
  }

  .node-column,
  .details,
  .missing-panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 16px;
  }

  .mods-column {
    min-height: 480px;
  }

  .mod-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 10px;
  }

  .node-card {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 12px;
    margin-bottom: 10px;
    border-radius: 14px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }

  .node-card.compact {
    margin-bottom: 0;
  }

  .node-card:hover,
  .node-card.selected {
    border-color: rgba(27, 217, 106, 0.45);
    background: rgba(27, 217, 106, 0.08);
    color: var(--text-primary);
  }

  .node-label {
    color: var(--text-primary);
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .node-meta {
    color: var(--text-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .side-client {
    box-shadow: inset 3px 0 rgba(139, 92, 246, 0.75);
  }

  .side-server {
    box-shadow: inset 3px 0 rgba(59, 130, 246, 0.75);
  }

  .side-both {
    box-shadow: inset 3px 0 rgba(27, 217, 106, 0.75);
  }

  .details {
    position: sticky;
    top: 0;
  }

  .details-header {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    margin-bottom: 16px;
  }

  .details-header h2 {
    margin: 4px 0 0;
    font-size: 22px;
  }

  .eyebrow {
    color: var(--text-muted);
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.08em;
  }

  .details-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-bottom: 18px;
  }

  .details-grid div,
  .kv,
  .relation,
  .muted-box,
  .missing-row {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 14px;
    padding: 12px;
  }

  .details-grid span,
  .kv span {
    display: block;
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    margin-bottom: 6px;
  }

  code {
    font-family: ui-monospace, monospace;
    color: var(--text-secondary);
    word-break: break-all;
  }

  .tag,
  .relation-kind {
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 9px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
    font-size: 11px;
    font-weight: 800;
    text-transform: uppercase;
  }

  .kv {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 8px;
    margin-bottom: 18px;
  }

  .relations {
    display: grid;
    gap: 8px;
  }

  .relation {
    display: grid;
    gap: 6px;
  }

  .relation.incoming .relation-kind {
    background: rgba(139, 92, 246, 0.12);
    color: var(--accent-secondary);
  }

  .relation-text {
    color: var(--text-secondary);
  }

  .relation-text strong {
    color: var(--text-primary);
    margin-left: 6px;
  }

  small,
  .muted-box {
    color: var(--text-muted);
    line-height: 1.45;
  }

  .missing-panel {
    margin-top: 16px;
    border-color: rgba(239, 68, 68, 0.3);
  }

  .missing-panel h3 {
    color: #fca5a5;
  }

  .missing-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-top: 8px;
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

  .error {
    color: #fecaca;
    border-color: rgba(239, 68, 68, 0.35);
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (max-width: 1180px) {
    .graph-layout {
      grid-template-columns: 1fr;
    }

    .details {
      position: static;
    }
  }
</style>
