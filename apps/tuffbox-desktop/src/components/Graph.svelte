<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { GitGraph, RefreshCw, AlertTriangle, Box, Workflow, Download, X, Loader2 } from "lucide-svelte";
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

  // Dependency preview dialog
  let depPreviewOpen = false;
  let depPreviewLoading = false;
  let depPreviewSlug = "";
  let depPreviewName = "";
  let depPreviewRequired: { target: string; reason?: string | null }[] = [];
  let depPreviewOptional: { target: string; reason?: string | null }[] = [];
  let depPreviewInstallWithOptional = true;

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

  // Deterministic position from string hash (for ghost nodes)
  function hashPos(s: string, baseX: number, baseY: number): {x: number, y: number} {
    let h = 0;
    for (let i = 0; i < s.length; i++) h = ((h << 5) - h) + s.charCodeAt(i);
    const angle = ((h % 360) / 180) * Math.PI;
    const dist = 100 + (Math.abs(h) % 80);
    return { x: baseX + Math.cos(angle) * dist, y: baseY + Math.sin(angle) * dist };
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

  /// Installs a single missing dependency by its slug/id extracted from the
  /// graph edge. The edge.to for a missing dep is the raw Modrinth slug.
  async function installSingleMissingDep(edge: GraphEdge) {
    if (!$projectPath) return;
    const depId = edge.to.startsWith("mod:") ? edge.to.slice(4) : edge.to;
    await previewModrinthDep(depId);
  }

  /// Fetches Modrinth dependency info and shows the preview dialog.
  async function previewModrinthDep(depId: string) {
    if (!$projectPath) return;
    depPreviewSlug = depId;
    depPreviewName = depId;
    depPreviewRequired = [];
    depPreviewOptional = [];
    depPreviewOpen = true;
    depPreviewLoading = true;
    depPreviewInstallWithOptional = true;
    try {
      const preview: any = await invoke("preview_modrinth_install", { path: $projectPath, modId: depId });
      if (preview) {
        depPreviewName = preview.name ?? depId;
        const depsList = preview.dependencies ?? [];
        depsList.forEach((dep: any) => {
          const kind = (dep.type ?? "").toLowerCase();
          const entry = { target: dep.target, reason: dep.reason ?? null };
          if (kind.includes("requires")) {
            depPreviewRequired.push(entry);
          } else {
            depPreviewOptional.push(entry);
          }
        });
      }
    } catch {
      // preview failed — install directly
    } finally {
      depPreviewLoading = false;
    }
  }

  /// Actually install from the dep preview dialog.
  async function confirmDepInstall() {
    if (!$projectPath) return;
    depPreviewOpen = false;
    resolving = true;
    error = null;
    message = null;
    try {
      if (depPreviewInstallWithOptional) {
        await invoke("add_modrinth_mod_with_dependencies", {
          path: $projectPath,
          modId: depPreviewSlug,
          side: "auto",
        });
      } else {
        await invoke("add_modrinth_mod", {
          path: $projectPath,
          modId: depPreviewSlug,
          side: "auto",
        });
      }
      message = `Installed ${depPreviewName}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  /// Heuristic: a mod node is a "dependency" (installed implicitly) if
  /// every edge that points TO it is a "Requires" edge (i.e. it wasn't
  /// explicitly added by the user, it was pulled in as a dep).
  $: depNodeIds = new Set(
    nodes
      .filter((node) => node.kind === "Mod")
      .filter((node) => {
        const incoming = edges.filter((e) => e.to === node.id);
        return incoming.length > 0 && incoming.every((e) => e.kind === "Requires");
      })
      .map((node) => node.id)
  );

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
  /// Group mods by which profile includes them (via IncludedInProfile edges)
  $: modsByProfile = (() => {
    const map = new Map<string, GraphNode[]>();
    const orphaned: GraphNode[] = [];
    for (const mod of modNodes) {
      const profiles = edges
        .filter((e) => e.kind === "IncludedInProfile" && e.to === mod.id)
        .map((e) => nodeById(e.from)?.label ?? e.from);
      if (profiles.length === 0) {
        orphaned.push(mod);
      } else {
        for (const prof of profiles) {
          const list = map.get(prof) ?? [];
          list.push(mod);
          map.set(prof, list);
        }
      }
    }
    return { map, orphaned };
  })();
  $: canvasHeight = Math.max(500, Math.ceil(Math.max(modNodes.length, profileNodes.length, platformNodes.length) / 2) * 96 + 160);
  let positioned: PositionedNode[] = [];
  let simulation: d3.Simulation<PositionedNode, undefined> | null = null;

  // --- Obsidian-style pan & zoom viewport state ---
  // The canvas itself stays a fixed logical size; instead of resizing the
  // SVG we move/scale a "camera" viewBox over it, exactly like Obsidian's
  // graph view: scroll to zoom (toward the cursor), drag empty space to
  // pan, double-click / button to reset.
  const BASE_WIDTH = 1400;
  let viewportEl: SVGSVGElement;
  let viewX = 0;
  let viewY = 0;
  let viewScale = 1;
  let isPanning = false;
  let panStart = { x: 0, y: 0, viewX: 0, viewY: 0 };
  $: viewBoxHeight = canvasHeight / viewScale;
  $: viewBoxWidth = BASE_WIDTH / viewScale;
  $: viewBoxString = `${viewX} ${viewY} ${viewBoxWidth} ${viewBoxHeight}`;

  function clientToSvgPoint(clientX: number, clientY: number) {
    const rect = viewportEl.getBoundingClientRect();
    const relX = (clientX - rect.left) / rect.width;
    const relY = (clientY - rect.top) / rect.height;
    return {
      x: viewX + relX * viewBoxWidth,
      y: viewY + relY * viewBoxHeight,
    };
  }

  function handleWheel(event: WheelEvent) {
    event.preventDefault();
    const zoomFactor = event.deltaY > 0 ? 1.12 : 1 / 1.12;
    const nextScale = Math.min(4, Math.max(0.25, viewScale / zoomFactor));
    const cursor = clientToSvgPoint(event.clientX, event.clientY);
    const nextWidth = BASE_WIDTH / nextScale;
    const nextHeight = canvasHeight / nextScale;
    // Keep the point under the cursor stationary while zooming.
    const ratioX = (cursor.x - viewX) / viewBoxWidth;
    const ratioY = (cursor.y - viewY) / viewBoxHeight;
    viewX = cursor.x - ratioX * nextWidth;
    viewY = cursor.y - ratioY * nextHeight;
    viewScale = nextScale;
  }

  function handleBackgroundMouseDown(event: MouseEvent) {
    if (event.button !== 0) return;
    isPanning = true;
    panStart = { x: event.clientX, y: event.clientY, viewX, viewY };
    window.addEventListener("mousemove", handleBackgroundMouseMove);
    window.addEventListener("mouseup", handleBackgroundMouseUp);
  }

  function handleBackgroundMouseMove(event: MouseEvent) {
    if (!isPanning || !viewportEl) return;
    const rect = viewportEl.getBoundingClientRect();
    const dx = ((event.clientX - panStart.x) / rect.width) * viewBoxWidth;
    const dy = ((event.clientY - panStart.y) / rect.height) * viewBoxHeight;
    viewX = panStart.viewX - dx;
    viewY = panStart.viewY - dy;
  }

  function handleBackgroundMouseUp() {
    isPanning = false;
    window.removeEventListener("mousemove", handleBackgroundMouseMove);
    window.removeEventListener("mouseup", handleBackgroundMouseUp);
  }

  function resetView() {
    viewX = 0;
    viewY = 0;
    viewScale = 1;
  }

  function zoomBy(factor: number) {
    const centerX = viewX + viewBoxWidth / 2;
    const centerY = viewY + viewBoxHeight / 2;
    const nextScale = Math.min(4, Math.max(0.25, viewScale * factor));
    const nextWidth = BASE_WIDTH / nextScale;
    const nextHeight = canvasHeight / nextScale;
    viewX = centerX - nextWidth / 2;
    viewY = centerY - nextHeight / 2;
    viewScale = nextScale;
  }

  $: if (nodes && edges) {
    const initializedNodes = nodes.map((node) => {
      const group = node.kind === "Mod" ? "mod" : node.kind === "Profile" ? "profile" : "runtime";
      let tone: string;
      if (group === "runtime") tone = "runtime";
      else if (group === "profile") tone = "profile";
      else if (depNodeIds.has(node.id)) tone = "dep";
      else tone = String(node.side ?? "unknown").toLowerCase();
      return { ...node, x: 500, y: 300, tone };
    });

    const d3Links = edges.map(e => ({ source: e.from, target: e.to, ...e }));

    if (simulation) simulation.stop();

    simulation = d3.forceSimulation<PositionedNode>(initializedNodes)
      .force("link", d3.forceLink(d3Links).id((d: any) => d.id).distance(200))
      .force("charge", d3.forceManyBody().strength(-500))
      .force("collide", d3.forceCollide().radius(85))
      .force("center", d3.forceCenter(BASE_WIDTH / 2, canvasHeight / 2))
      .force("x", d3.forceX(BASE_WIDTH / 2).strength(0.04))
      .force("y", d3.forceY(canvasHeight / 2).strength(0.04))
      .on("tick", () => {
        positioned = [...initializedNodes];
      });
    resetView();
  }
  $: positionById = new Map(positioned.map((node) => [node.id, node]));

  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
  function handleNodeMouseDown(event: MouseEvent, node: PositionedNode) {
    event.stopPropagation();
    selectedId = node.id;
    if (!simulation) return;
    node.fx = node.x;
    node.fy = node.y;
    const onMouseMove = (ev: MouseEvent) => {
      const p = clientToSvgPoint(ev.clientX, ev.clientY);
      node.fx = p.x;
      node.fy = p.y;
      simulation?.alpha(0.3).restart();
    };
    const onMouseUp = () => {
      node.fx = null;
      node.fy = null;
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  }
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
      <div class="canvas-controls">
        <button class="ghost mini" on:click={() => zoomBy(1.25)} title="Zoom in">+</button>
        <button class="ghost mini" on:click={() => zoomBy(1 / 1.25)} title="Zoom out">−</button>
        <button class="ghost mini" on:click={resetView} title="Reset view (fit)">⤢</button>
        <span class="zoom-readout">{Math.round(viewScale * 100)}%</span>
      </div>
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-missing-attribute -->
      <svg
        bind:this={viewportEl}
        viewBox={viewBoxString}
        role="img"
        aria-label="Dependency graph"
        class:panning={isPanning}
        on:wheel={handleWheel}
        on:mousedown={handleBackgroundMouseDown}
        on:dblclick={resetView}
      >
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
            class:dimmed={selectedId && edge.from !== selectedId && edge.to !== selectedId}
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
            class:dimmed={selectedId && selectedId !== node.id && !selectedEdges.some((e) => e.from === node.id || e.to === node.id)}
            role="button"
            tabindex="0"
            transform={`translate(${node.x}, ${node.y})`}
            on:mousedown={(e) => handleNodeMouseDown(e, node)}
          >
            <circle r="6" />
            <text x="10" y="4">{node.label.length > 22 ? node.label.slice(0, 21) + "…" : node.label}</text>
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
        <h3><Box size={16} /> Mods by profile</h3>
        {#if modNodes.length === 0}
          <div class="muted-box">No mod nodes yet.</div>
        {:else}
          {#each [...modsByProfile.map.entries()] as [profile, mods]}
            <h4 class="profile-group-title">{profile} ({mods.length})</h4>
            <div class="mod-grid">
              {#each mods as node}
                <button class="node-card compact side-{node.side}" class:selected={selectedId === node.id} on:click={() => (selectedId = node.id)}>
                  <span class="node-label">{node.label}</span>
                  <span class="node-meta">{node.version ?? "unknown"} · {node.side}</span>
                </button>
              {/each}
            </div>
          {/each}
          {#if modsByProfile.orphaned.length > 0}
            <h4 class="profile-group-title orphaned">Unassigned ({modsByProfile.orphaned.length})</h4>
            <div class="mod-grid">
              {#each modsByProfile.orphaned as node}
                <button class="node-card compact side-{node.side}" class:selected={selectedId === node.id} on:click={() => (selectedId = node.id)}>
                  <span class="node-label">{node.label}</span>
                  <span class="node-meta">{node.version ?? "unknown"} · {node.side}</span>
                </button>
              {/each}
            </div>
          {/if}
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
        <div class="missing-actions">
          <button class="mini" on:click={installMissingDependencies} disabled={resolving}>
            <Download size={14} /> Install all ({missingEdges.length})
          </button>
        </div>
        {#each missingEdges as edge}
          <button class="missing-row" on:click={() => installSingleMissingDep(edge)} disabled={resolving}>
            <span>{nodeById(edge.from)?.label ?? edge.from}</span>
            <code class="dep-slug">{edge.to.startsWith("mod:") ? edge.to.slice(4) : edge.to}</code>
            <Download size={14} class="dep-icon" />
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="empty">Open a project to view its dependency graph.</div>
  {/if}
</div>

{#if depPreviewOpen}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (depPreviewOpen = false)} on:keydown={(e) => e.key === "Escape" && (depPreviewOpen = false)}>
    <div class="modal" role="dialog" aria-modal="true">
      <div class="modal-header">
        <div>
          <h2>Install dependency: {depPreviewName}</h2>
          <p>This mod has dependencies on Modrinth. Choose what to install.</p>
        </div>
        <button class="icon-btn" on:click={() => (depPreviewOpen = false)}><X size={18} /></button>
      </div>
      <div class="modal-body">
        {#if depPreviewLoading}
          <div class="loading"><Loader2 size={16} class="spin" /> Loading dependency info from Modrinth...</div>
        {:else}
          <div class="dep-list">
            <h4>Required ({depPreviewRequired.length})</h4>
            {#if depPreviewRequired.length === 0}
              <p class="muted">No hard dependencies — installing the mod alone should work.</p>
            {:else}
              {#each depPreviewRequired as dep}
                <div class="dep-entry required">
                  <span class="dep-target">{dep.target}</span>
                  {#if dep.reason}<small>{dep.reason}</small>{/if}
                </div>
              {/each}
            {/if}
          </div>
          <div class="dep-list">
            <h4>Optional ({depPreviewOptional.length})</h4>
            {#if depPreviewOptional.length === 0}
              <p class="muted">No optional dependencies listed.</p>
            {:else}
              {#each depPreviewOptional as dep}
                <div class="dep-entry optional">
                  <span class="dep-target">{dep.target}</span>
                  {#if dep.reason}<small>{dep.reason}</small>{/if}
                </div>
              {/each}
            {/if}
          </div>
          <label class="checkbox-row">
            <input type="checkbox" bind:checked={depPreviewInstallWithOptional} />
            <span>Install optional dependencies too</span>
          </label>
        {/if}
      </div>
      <div class="modal-footer">
        <button class="secondary" on:click={() => (depPreviewOpen = false)}>Cancel</button>
        <button on:click={confirmDepInstall} disabled={depPreviewLoading}>
          <Download size={16} /> Install
        </button>
      </div>
    </div>
  </div>
{/if}

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
    position: relative;
    margin-bottom: 18px;
    background:
      radial-gradient(circle at 78% 18%, rgba(27,217,106,.08), transparent 28%),
      #09090b;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: hidden;
  }

  .graph-canvas svg {
    width: 100%;
    height: 720px;
    display: block;
    cursor: grab;
    touch-action: none;
  }

  .graph-canvas svg.panning {
    cursor: grabbing;
  }

  .canvas-controls {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 6px;
    background: rgba(9, 9, 11, 0.72);
    border: 1px solid var(--border-color);
    border-radius: 10px;
    padding: 4px;
    backdrop-filter: blur(6px);
  }

  .canvas-controls .mini {
    width: 26px;
    height: 26px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 15px;
    line-height: 1;
  }

  .zoom-readout {
    font-size: 11px;
    color: var(--text-muted);
    padding: 0 6px;
    min-width: 38px;
    text-align: center;
  }

  .graph-canvas line {
    stroke: rgba(161, 161, 170, 0.5);
    stroke-width: 1.6;
    transition: opacity 120ms ease;
  }

  .graph-canvas line.dimmed {
    opacity: 0.15;
  }

  .svg-node.dimmed {
    opacity: 0.25;
  }

  .graph-canvas line.danger-edge {
    stroke: rgba(239, 68, 68, 0.8);
    stroke-dasharray: 6 5;
  }

  .svg-node {
    cursor: pointer;
    transition: opacity 120ms ease;
  }

  .svg-node circle {
    fill: #18181b;
    stroke: rgba(255,255,255,.28);
    stroke-width: 1.4;
    transition: stroke 120ms ease, fill 120ms ease;
  }

  .svg-node.selected circle,
  .svg-node:hover circle {
    stroke: rgba(27,217,106,.85);
    fill: rgba(27,217,106,.18);
  }

  .svg-node.tone-client circle { stroke: rgba(139,92,246,.7); }
  .svg-node.tone-server circle { stroke: rgba(59,130,246,.7); }
  .svg-node.tone-both circle { stroke: rgba(27,217,106,.65); }
  .svg-node.tone-runtime circle { stroke: rgba(245,158,11,.7); }
  .svg-node.tone-profile circle { stroke: rgba(96,165,250,.7); }

  .svg-node text {
    fill: #e5e7eb;
    font-size: 12px;
    font-weight: 800;
    pointer-events: none;
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

  .profile-group-title {
    margin: 14px 0 8px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .profile-group-title::before {
    content: "";
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent-primary);
  }

  .profile-group-title.orphaned::before {
    background: var(--text-muted);
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

  .missing-actions {
    margin-bottom: 10px;
  }

  .missing-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 8px;
    width: 100%;
    cursor: pointer;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 10px 12px;
    color: var(--text-secondary);
    text-align: left;
    transform: none;
    transition: border-color .15s;
  }

  .missing-row:hover:not(:disabled) {
    border-color: rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.05);
  }

  .missing-row:disabled {
    opacity: .5;
    cursor: wait;
  }

  .dep-slug {
    font-size: 11px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dep-icon {
    flex-shrink: 0;
    color: var(--accent-primary);
    opacity: .7;
  }

  /* Dep-tone node: amber/orange for auto-installed dependency mods */
  .svg-node.tone-dep circle {
    stroke: rgba(245, 158, 11, 0.7);
    fill: rgba(245, 158, 11, 0.08);
  }

  .ghost-node {
    cursor: pointer;
    transition: all .15s ease;
    animation: ghost-pulse 2s ease-in-out infinite;
  }
  .ghost-node:hover {
    fill: rgba(239,68,68,0.25);
    stroke: rgba(239,68,68,0.9);
    r: 16;
  }
  .ghost-label {
    cursor: pointer;
    transition: fill .15s;
  }
  .ghost-label:hover {
    fill: #fca5a5;
    font-size: 11px;
  }
  @keyframes ghost-pulse {
    0%, 100% { opacity: 0.7; }
    50% { opacity: 1; }
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

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    border-radius: 16px;
    max-width: 520px;
    width: 90%;
    max-height: 80vh;
    overflow-y: auto;
    padding: 0;
  }
  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 20px 24px 12px;
    border-bottom: 1px solid var(--border-color);
  }
  .modal-header h2 { margin: 0; font-size: 18px; }
  .modal-header p { margin: 4px 0 0; font-size: 13px; color: var(--text-muted); }
  .modal-body { padding: 16px 24px; }
  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 24px 20px;
  }
  .icon-btn {
    background: none;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }
  .icon-btn:hover { color: var(--text-primary); background: var(--bg-tertiary); }
  .dep-list { margin-bottom: 14px; }
  .dep-list h4 { font-size: 13px; margin: 0 0 8px; color: var(--text-secondary); }
  .dep-entry {
    padding: 8px 10px;
    border-radius: 8px;
    margin-bottom: 4px;
    font-size: 13px;
    border-left: 3px solid;
  }
  .dep-entry.required { background: rgba(27,217,106,0.08); border-left-color: rgba(27,217,106,0.6); }
  .dep-entry.optional { background: rgba(245,158,11,0.08); border-left-color: rgba(245,158,11,0.6); }
  .dep-target { font-weight: 600; }
  .dep-entry small { display: block; color: var(--text-muted); font-size: 11px; margin-top: 2px; }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 0;
    font-size: 14px;
    cursor: pointer;
  }
  .checkbox-row input { width: auto; }
  .muted { color: var(--text-muted); font-size: 13px; }

  @media (max-width: 1180px) {
    .graph-layout {
      grid-template-columns: 1fr;
    }

    .details {
      position: static;
    }
  }
</style>
