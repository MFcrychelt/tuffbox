<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { GitGraph, RefreshCw, AlertTriangle, Box, Workflow, Download, X, Loader2, Maximize2, Minimize2, RotateCw } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import * as d3 from "d3-force";
  import { onDestroy, onMount } from "svelte";

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

  type PositionedNode = GraphNode & { x: number; y: number; fx?: number | null; fy?: number | null; tone: string; ghost?: boolean };
  type DownloadProgress = {
    id: string;
    name: string;
    percent: number;
    status: string;
  };
  type DownloadBatch = {
    phase: string;
    failed?: { modId: string; error: string }[];
    downloaded?: string[];
    alreadyPresent?: string[];
  };

  let graph: GraphModel | null = null;
  let loading = false;
  let error: string | null = null;
  let selectedId: string | null = null;
  let lastLoadedPath: string | null = null;
  let resolving = false;
  let message: string | null = null;
  let changePlan: any | null = null;
  let graphSource = "local";
  let graphGeneratedAt: string | null = null;
  let graphRefreshing = false;
  let refreshError: string | null = null;

  // Dependency preview dialog
  let depPreviewOpen = false;
  let depPreviewLoading = false;
  let depPreviewSlug = "";
  let depPreviewName = "";
  let depPreviewRequired: { target: string; reason?: string | null }[] = [];
  let depPreviewOptional: { target: string; reason?: string | null }[] = [];
  let depPreviewInstallWithOptional = true;
  let depInstallStatus: "idle" | "downloading" | "done" | "failed" = "idle";
  let depInstallMessage = "";
  let depInstallError: string | null = null;
  let depInstallFailedIds: string[] = [];
  let depInstallBatchSeen = false;
  let unlistenDownloadProgress: UnlistenFn | null = null;
  let unlistenDownloadBatch: UnlistenFn | null = null;
  let graphCanvasEl: HTMLElement;
  let fullscreenElement: Element | null = null;
  $: graphFullscreen = fullscreenElement === graphCanvasEl;

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

  function applyGraph(raw: any, resetSelection = false) {
    graph = {
      nodes: (raw.nodes ?? []).map(normalizeNode),
      edges: (raw.edges ?? []).map(normalizeEdge),
    };
    graphSource = raw.source ?? "local";
    graphGeneratedAt = raw.generatedAt ?? null;
    simulationLayoutKey = "";
    if (resetSelection) selectedId = null;
    hydrateMissingIcons().catch(() => {});
    queueMicrotask(() => hydrateGhostNodes().catch(() => {}));
  }

  async function refreshGraph(manual = true) {
    if (!$projectPath || graphRefreshing) return;
    graphRefreshing = true;
    refreshError = null;
    try {
      const raw: any = await invoke("refresh_graph", { path: $projectPath });
      applyGraph(raw);
      await loadChangePlan();
      if (manual) message = "Dependency metadata refreshed.";
    } catch (e) {
      refreshError = String(e);
      if (!graph) error = refreshError;
    } finally {
      graphRefreshing = false;
    }
  }

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && graph) return;
    loading = true;
    error = null;
    brokenIcons = new Set();
    ghostMeta = {};
    simulationLayoutKey = "";
    resetViewOnNextLayout = true;
    try {
      const raw: any = await invoke("get_graph", { path: $projectPath });
      applyGraph(raw, true);
      // Don't pre-select a node — otherwise every unrelated edge is dimmed
      // to near-invisible and the graph looks disconnected.
      await loadChangePlan();
      lastLoadedPath = $projectPath;
      if (raw.source !== "network") {
        queueMicrotask(() => refreshGraph(false));
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function nodeById(id: string) {
    return graph?.nodes.find((n) => n.id === id) ?? null;
  }

  function resolveNodeLabel(id: string): string {
    return nodeById(id)?.label ?? ghostNodes.find((n) => n.id === id)?.label ?? id.replace(/^mod:/, "");
  }

  // Track which nodes have a broken icon so we can fall back to a letter avatar
  let brokenIcons = new Set<string>();
  /// Modrinth metadata for missing (ghost) dependency nodes: name + icon.
  let ghostMeta: Record<string, { name: string; iconUrl?: string | null; projectId?: string }> = {};

  function markIconBroken(id: string) {
    if (!brokenIcons.has(id)) {
      brokenIcons = new Set([...brokenIcons, id]);
    }
  }

  function modIconLookupKey(node: GraphNode): string | null {
    if (node.metadata?.project_id) return node.metadata.project_id;
    if (node.metadata?.source === "modrinth" && node.id.startsWith("mod:")) {
      return node.id.slice(4);
    }
    return null;
  }

  async function resolveIconForNode(node: GraphNode) {
    const key = modIconLookupKey(node);
    if (!key || !graph) return;
    try {
      const url: string | null = await invoke("get_modrinth_project_icon", { projectId: key });
      if (url) {
        node.metadata = { ...(node.metadata ?? {}), icon_url: url };
        graph = { ...graph };
        brokenIcons.delete(node.id);
        brokenIcons = brokenIcons;
      }
    } catch {
      // keep letter-avatar fallback
    }
  }

  async function handleIconError(node: GraphNode) {
    markIconBroken(node.id);
    await resolveIconForNode(node);
  }

  // Hydrate icons for Mod nodes that don't have icon_url in metadata.
  async function hydrateMissingIcons() {
    if (!graph) return;
    const missing = graph.nodes.filter((n) => {
      if (n.kind !== "Mod") return false;
      if (brokenIcons.has(n.id)) return !!modIconLookupKey(n);
      if (n.metadata?.icon_url) return false;
      return !!modIconLookupKey(n);
    });
    if (missing.length === 0) return;
    await Promise.all(missing.map((n) => resolveIconForNode(n)));
  }

  function point(id: string) {
    return simNodes.find((n) => n.id === id) ?? positionById.get(id);
  }

  function edgePath(edge: GraphEdge): string {
    const source = point(edge.from);
    const target = point(edge.to);
    if (!source || !target) return "";
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const distance = Math.hypot(dx, dy);
    if (distance < 1) return "";
    const ux = dx / distance;
    const uy = dy / distance;
    const r1 = nodeSize(source) / 2;
    const r2 = nodeSize(target) / 2;
    const x1 = source.x + ux * (r1 + 2);
    const y1 = source.y + uy * (r1 + 2);
    const x2 = target.x - ux * (r2 + 8);
    const y2 = target.y - uy * (r2 + 8);
    // Match modrinth-extras: straight edges by default; only parallel
    // multi-edges get a light fan so they stay readable.
    const parallel = displayEdges.filter(
      (e) =>
        (e.from === edge.from && e.to === edge.to) ||
        (e.from === edge.to && e.to === edge.from),
    );
    if (parallel.length <= 1) {
      return `M ${x1} ${y1} L ${x2} ${y2}`;
    }
    const idx = parallel.findIndex(
      (e) => e.from === edge.from && e.to === edge.to && e.kind === edge.kind,
    );
    const bend = (idx - (parallel.length - 1) / 2) * 22;
    const mx = (x1 + x2) / 2 + -uy * bend;
    const my = (y1 + y2) / 2 + ux * bend;
    return `M ${x1} ${y1} Q ${mx} ${my} ${x2} ${y2}`;
  }

  function isGhost(id: string) {
    return id.startsWith("__ghost__");
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

  function nodeIconUrl(node: GraphNode | PositionedNode): string | null {
    if (brokenIcons.has(node.id)) return null;
    if (node.metadata?.icon_url) return node.metadata.icon_url;
    const ghost = ghostMeta[node.id];
    if (ghost?.iconUrl) return ghost.iconUrl;
    return null;
  }

  function displayLabel(node: GraphNode | PositionedNode): string {
    return ghostMeta[node.id]?.name ?? node.label;
  }

  function nodeSize(node: PositionedNode): number {
    if (node.kind === "Missing") return 36;
    return depNodeIds.has(node.id) ? 36 : 48;
  }

  async function removeConflictNode(nodeId: string) {
    if (!$projectPath) return;
    const modId = modIdFromNode(nodeId);
    resolving = true;
    error = null;
    message = null;
    try {
      await invoke("remove_project_mod", { path: $projectPath, modId });
      message = `Removed ${modId}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  async function removeModNode(event: MouseEvent | Event, nodeId: string) {
    event.stopPropagation();
    await removeConflictNode(nodeId);
  }

  async function installGhostNode(nodeId: string) {
    if (!$projectPath) return;
    await previewModrinthDep(modIdFromNode(nodeId));
  }

  async function downloadMissingFiles() {
    if (!$projectPath) return;
    resolving = true;
    error = null;
    message = null;
    try {
      const downloaded: string[] = await invoke("download_missing_files", { path: $projectPath });
      message = downloaded.length
        ? `Downloaded ${downloaded.length} file(s): ${downloaded.join(", ")}`
        : "All mod files are already on disk.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      resolving = false;
    }
  }

  function handleNodeClick(node: PositionedNode) {
    selectedId = node.id;
    if (node.kind === "Missing" || node.ghost) {
      installGhostNode(node.id);
    }
  }

  /// Click on a dependency icon (SVG node or card icon). For missing/ghost
  /// nodes this installs the dep. For installed dep nodes that were pulled
  /// in implicitly (not added by the user), clicking the icon re-installs /
  /// re-downloads the file to make sure it's on disk.
  function handleDepIconClick(node: PositionedNode, event?: MouseEvent) {
    event?.stopPropagation();
    selectedId = node.id;
    if (node.kind === "Missing" || node.ghost) {
      installGhostNode(node.id);
    } else if (node.kind === "Mod" && depNodeIds.has(node.id)) {
      // Already installed as a dep — re-download the file in case it's missing
      downloadMissingFiles();
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

  function formatChangeAction(action: Record<string, any>): string {
    if (action.installMod) {
      const version = action.installMod.version;
      return `Install ${action.installMod.project_id}${version ? ` at ${version}` : " (latest compatible version)"}`;
    }
    if (action.removeMod) return `Remove ${action.removeMod.node_id}`;
    if (action.disableMod) return `Disable ${action.disableMod.node_id}`;
    if (action.updateMod) return `Update ${action.updateMod.node_id} to ${action.updateMod.target_version}`;
    if (action.editConfig) return `Edit configuration: ${action.editConfig.path}`;
    return "Apply recommended change";
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
    depInstallStatus = "idle";
    depInstallMessage = "";
    depInstallError = null;
    depInstallFailedIds = [];
    try {
      const preview: any = await invoke("preview_modrinth_install", { path: $projectPath, modId: depId });
      if (preview) {
        depPreviewName = preview.name ?? depId;
        const depsList = preview.dependencies ?? [];
        depsList.forEach((dep: any) => {
          const kind = (dep.type ?? "").toLowerCase();
          const entry = { target: dep.target, reason: dep.reason ?? null };
          if (kind.includes("required") || kind.includes("requires")) {
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
    resolving = true;
    error = null;
    message = null;
    depInstallStatus = "downloading";
    depInstallMessage = `Downloading ${depPreviewName}…`;
    depInstallError = null;
    depInstallFailedIds = [];
    depInstallBatchSeen = false;
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
      if (!depInstallBatchSeen) {
        depInstallStatus = "done";
        depInstallMessage = `${depPreviewName} installed.`;
        message = `Installed ${depPreviewName}.`;
        await load(true);
      }
    } catch (e) {
      depInstallStatus = "failed";
      depInstallError = String(e);
      depInstallMessage = `Could not install ${depPreviewName}.`;
    } finally {
      resolving = false;
    }
  }

  async function retryDepInstall() {
    if (!$projectPath) return;
    if (depInstallFailedIds.length === 0) {
      await confirmDepInstall();
      return;
    }
    resolving = true;
    depInstallStatus = "downloading";
    depInstallMessage = `Retrying ${depPreviewName}…`;
    depInstallError = null;
    depInstallBatchSeen = false;
    try {
      await invoke("retry_failed_mod_downloads", {
        path: $projectPath,
        modIds: depInstallFailedIds,
      });
    } catch (e) {
      depInstallStatus = "failed";
      depInstallError = String(e);
      depInstallMessage = `Retry failed for ${depPreviewName}.`;
    } finally {
      resolving = false;
    }
  }

  /// Installed mods that other mods require/optionally depend on.
  /// These are "downloaded dependencies" and get the third (amber) tone.
  /// Mods with no incoming dep edges are "main" and keep their side color.
  $: depNodeIds = new Set(
    nodes
      .filter((node) => node.kind === "Mod")
      .filter((node) => {
        const incoming = displayEdges.filter(
          (e) => e.to === node.id && (e.kind === "Requires" || e.kind === "Optional")
        );
        return incoming.length > 0;
      })
      .map((node) => node.id)
  );

  $: nodes = graph?.nodes ?? [];
  $: edges = graph?.edges ?? [];
  $: selected = selectedId ? nodeById(selectedId) : null;
  $: selectedEdges = selectedId
    ? displayEdges.filter((edge) => edge.from === selectedId || edge.to === selectedId)
    : [];
  $: missingEdges = displayEdges.filter(
    (edge) => edge.kind === "Requires" && (!nodeById(edge.to) || nodeById(edge.to)?.kind === "Missing")
  );
  $: missingDepsByMod = (() => {
    const map = new Map<string, GraphEdge[]>();
    for (const edge of missingEdges) {
      const list = map.get(edge.from) ?? [];
      list.push(edge);
      map.set(edge.from, list);
    }
    return map;
  })();
  $: conflictEdges = displayEdges.filter(
    (edge) => ["Conflicts", "BreaksWith"].includes(edge.kind)
      && nodeById(edge.from)?.kind !== "Missing"
      && nodeById(edge.to)?.kind !== "Missing"
  );
  $: byKind = nodes.reduce<Record<string, number>>((acc, node) => {
    acc[node.kind] = (acc[node.kind] ?? 0) + 1;
    return acc;
  }, {});
  $: modNodes = nodes.filter((node) => node.kind === "Mod");
  $: platformNodes = nodes.filter((node) => node.kind !== "Mod" && node.kind !== "Profile" && node.kind !== "Missing");
  $: profileNodes = nodes.filter((node) => node.kind === "Profile");

  // Synthesize ghost nodes for any edge endpoint that has no real node. This
  // is the single most important fix: the Rust builder intentionally emits
  // edges to *missing* dependencies, but d3's forceLink throws
  // "missing: <id>" when a link references a node that isn't in the array,
  // which aborted the whole simulation and left the canvas blank. Ghost
  // nodes keep every link resolvable while still being visually distinct
  // (and the missing-dependency panel operates on the real graph).
  const nodeIdSet = (id: string) => nodes.some((n) => n.id === id);
  $: ghostNodes = (() => {
    const out: GraphNode[] = nodes
      .filter((node) => node.kind === "Missing")
      .map((node) => ({
        ...node,
        label: ghostMeta[node.id]?.name ?? node.label,
        metadata: {
          ...(node.metadata ?? {}),
          ...(ghostMeta[node.id]?.iconUrl ? { icon_url: ghostMeta[node.id].iconUrl! } : {}),
          ...(ghostMeta[node.id]?.projectId ? { project_id: ghostMeta[node.id].projectId! } : {}),
          source: "modrinth",
        },
      }));
    const seen = new Set(out.map((node) => node.id));
    for (const e of edges) {
      if (e.kind === "RequiresLoader" || e.kind === "RequiresMinecraft" || e.kind === "RequiresJava" || e.kind === "IncludedInProfile") continue;
      for (const end of [e.from, e.to]) {
        if (!nodeIdSet(end) && !seen.has(end)) {
          seen.add(end);
          const slug = end.replace(/^mod:/, "").replace(/^__ghost__/, "");
          const cached = ghostMeta[end];
          out.push({
            id: end,
            kind: "Missing",
            label: cached?.name ?? slug,
            version: null,
            side: "unknown",
            metadata: {
              ...(cached?.iconUrl ? { icon_url: cached.iconUrl } : {}),
              ...(cached?.projectId ? { project_id: cached.projectId } : {}),
              source: "modrinth",
            },
          });
        }
      }
    }
    return out;
  })();
  $: displayNodes = [
    ...nodes.filter((n) => n.kind === "Mod"),
    ...platformNodes,
    ...ghostNodes,
  ];
  // Mod-to-mod relations plus loader/runtime links (fabric-api, lithium,
  // ferrite-core etc. only depend on the loader, so without these edges they
  // look orphaned). Optional/recommended edges stay hidden — they point
  // outward from hub mods and read like inverted deps in a dense pack.
  $: displayEdges = edges.filter((e) =>
    ["Requires", "Conflicts", "BreaksWith", "Replaces", "RequiresLoader", "RequiresMinecraft", "RequiresJava"].includes(e.kind)
  );

  async function hydrateGhostNodes() {
    if (!$projectPath || !graph) return;
    const missing = ghostNodes.filter((n) => !ghostMeta[n.id]);
    if (missing.length === 0) return;
    const updates: Record<string, { name: string; iconUrl?: string | null; projectId?: string }> = {};
    await Promise.all(
      missing.map(async (node) => {
        const key = modIdFromNode(node.id);
        try {
          const project: any = await invoke("get_modrinth_project", { projectId: key });
          if (project) {
            updates[node.id] = {
              name: project.name ?? key,
              iconUrl: project.iconUrl ?? null,
              projectId: project.id ?? key,
            };
          }
        } catch {
          // keep slug label
        }
      })
    );
    if (Object.keys(updates).length > 0) {
      ghostMeta = { ...ghostMeta, ...updates };
    }
  }

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
  type LayoutNode = PositionedNode & {
    depth: number;
    isHub: boolean;
    component: number;
    hubId: string;
    hubX: number;
    hubY: number;
    vx?: number;
    vy?: number;
    fx?: number | null;
    fy?: number | null;
    index?: number;
  };

  let canvasWidth = 1600;
  let canvasHeight = 900;
  let positioned: PositionedNode[] = [];
  let simulation: any = null;
  let simulationLayoutKey = "";
  let simNodes: LayoutNode[] = [];
  const nodeEls = new Map<string, SVGGElement>();
  const edgeEls = new Map<string, SVGPathElement>();
  let resetViewOnNextLayout = true;

  function layoutCanvasSize(count: number, maxDepth: number) {
    const ring = Math.max(3, maxDepth + 1);
    const span = Math.max(900, ring * 210 * 2 + 280);
    const orphanPad = Math.max(0, count - 20) * 4;
    return {
      width: Math.max(1400, span + orphanPad),
      height: Math.max(900, span * 0.85 + orphanPad),
    };
  }

  /** Undirected BFS depth + hub pick — same idea as modrinth-extras dependency rings. */
  function computeDepthsAndHubs(nodeIds: string[], edges: GraphEdge[]) {
    const adj = new Map<string, string[]>();
    const degree = new Map<string, number>();
    for (const id of nodeIds) {
      adj.set(id, []);
      degree.set(id, 0);
    }
    for (const edge of edges) {
      if (!adj.has(edge.from) || !adj.has(edge.to)) continue;
      adj.get(edge.from)!.push(edge.to);
      adj.get(edge.to)!.push(edge.from);
      degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
      degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
    }

    const remaining = new Set(nodeIds);
    const components: string[][] = [];
    while (remaining.size) {
      const start = remaining.values().next().value as string;
      const queue = [start];
      remaining.delete(start);
      const comp: string[] = [];
      while (queue.length) {
        const cur = queue.shift()!;
        comp.push(cur);
        for (const next of adj.get(cur) ?? []) {
          if (!remaining.has(next)) continue;
          remaining.delete(next);
          queue.push(next);
        }
      }
      components.push(comp);
    }
    components.sort((a, b) => b.length - a.length);

    const depth = new Map<string, number>();
    const hubOf = new Map<string, string>();
    const hubMeta: { id: string; component: number; x: number; y: number }[] = [];

    components.forEach((comp, componentIndex) => {
      const hub = [...comp].sort(
        (a, b) => (degree.get(b) ?? 0) - (degree.get(a) ?? 0) || a.localeCompare(b),
      )[0];
      // Primary component sits at the origin; satellite components orbit it.
      let hx = 0;
      let hy = 0;
      if (componentIndex > 0) {
        const angle = ((componentIndex - 1) / Math.max(1, components.length - 1)) * Math.PI * 2;
        const orbit = 420 + componentIndex * 60;
        hx = Math.cos(angle) * orbit;
        hy = Math.sin(angle) * orbit;
      }
      hubMeta.push({ id: hub, component: componentIndex, x: hx, y: hy });

      const q = [hub];
      depth.set(hub, 0);
      hubOf.set(hub, hub);
      while (q.length) {
        const cur = q.shift()!;
        const d = depth.get(cur) ?? 0;
        for (const next of adj.get(cur) ?? []) {
          if (depth.has(next)) continue;
          depth.set(next, d + 1);
          hubOf.set(next, hub);
          q.push(next);
        }
      }
      for (const id of comp) {
        if (!depth.has(id)) {
          depth.set(id, 1);
          hubOf.set(id, hub);
        }
      }
    });

    return { degree, depth, hubOf, hubMeta, maxDepth: depth.size ? Math.max(...depth.values()) : 0 };
  }

  $: layoutKey = [
    ...displayNodes.map((n) => n.id).sort(),
    ...displayEdges.map((e) => `${e.from}:${e.to}:${e.kind}`).sort(),
  ].join("|");

  function startSimulation() {
    if (!displayNodes.length) return;

    const nodeIds = displayNodes.map((n) => n.id);
    const { degree, depth, hubOf, hubMeta, maxDepth } = computeDepthsAndHubs(
      nodeIds,
      displayEdges,
    );
    const size = layoutCanvasSize(displayNodes.length, maxDepth);
    canvasWidth = size.width;
    canvasHeight = size.height;
    const cx = canvasWidth / 2;
    const cy = canvasHeight / 2;
    const hubPos = new Map(hubMeta.map((h) => [h.id, { x: cx + h.x, y: cy + h.y }]));

    // Seed on concentric rings around each hub (modrinth-extras pattern).
    const ringIndex = new Map<string, number>();
    simNodes = displayNodes.map((node) => {
      const isGhost = node.kind === "Missing";
      let tone: string;
      if (isGhost) tone = "ghost";
      else if (node.kind === "Mod") tone = depNodeIds.has(node.id) ? "dep" : String(node.side ?? "both").toLowerCase();
      else if (node.kind === "Profile") tone = "profile";
      else tone = "runtime";

      const d = depth.get(node.id) ?? 1;
      const hubId = hubOf.get(node.id) ?? node.id;
      const hub = hubPos.get(hubId) ?? { x: cx, y: cy };
      const isHub = hubMeta.some((h) => h.id === node.id);
      const key = `${hubId}:${d}`;
      const idx = ringIndex.get(key) ?? 0;
      ringIndex.set(key, idx + 1);
      const siblings = Math.max(
        1,
        [...depth.entries()].filter(([id, dd]) => dd === d && hubOf.get(id) === hubId).length,
      );
      const angle = (idx / siblings) * Math.PI * 2 + d * 0.35;
      const ring = isHub ? 0 : d * 190 + ((idx % 3) - 1) * 12;

      return {
        ...node,
        label: displayLabel(node),
        x: hub.x + Math.cos(angle) * ring,
        y: hub.y + Math.sin(angle) * ring,
        fx: isHub ? hub.x : null,
        fy: isHub ? hub.y : null,
        tone,
        ghost: isGhost,
        depth: d,
        isHub,
        component: hubMeta.find((h) => h.id === hubId)?.component ?? 0,
        hubId,
        hubX: hub.x,
        hubY: hub.y,
      } as LayoutNode;
    });

    const d3Links = displayEdges.map((e) => ({ source: e.from, target: e.to, ...e }));
    const linkId = (value: any) => (typeof value === "object" && value ? value.id : value);
    const degOf = (id: string) => degree.get(id) ?? 0;
    const centerX = canvasWidth / 2;
    const centerY = canvasHeight / 2;

    // Radial depth rings centered on the canvas (modrinth-extras pattern):
    // each node is pulled toward a radius determined by its dependency depth,
    // so hubs sit in the middle and deeper deps form outer rings instead of
    // collapsing into a knot.
    function radialStrength(node: LayoutNode) {
      if (node.isHub) return 0;
      return maxDepth === 0 ? 0 : 0.3;
    }

    if (simulation) simulation.stop();

    // Physics (modrinth-extras/useForceGraph.ts): local degree-independent
    // repulsion, fixed link gap, hard collide, concentric depth rings pinned
    // to the canvas centre, higher alphaDecay so it settles fast.
    simulation = d3
      .forceSimulation<LayoutNode>(simNodes)
      .force(
        "charge",
        d3.forceManyBody<LayoutNode>().strength(-650).distanceMax(700),
      )
      .force(
        "link",
        d3
          .forceLink(d3Links)
          .id((d: any) => d.id)
          .distance((link: any) => {
            const s = linkId(link.source);
            const t = linkId(link.target);
            const rs = nodeSize({ id: s } as PositionedNode) / 2;
            const rt = nodeSize({ id: t } as PositionedNode) / 2;
            return rs + rt + 70 + Math.max(degOf(s), degOf(t)) * 6;
          }),
        // d3's default link strength (1 / min degree) keeps hubs from collapsing.
      )
      .force(
        "collide",
        d3
          .forceCollide<LayoutNode>()
          .radius((d) => nodeSize(d) / 2 + 26)
          .strength(1)
          .iterations(2),
      )
      .force(
        "radial",
        d3
          .forceRadial<LayoutNode>((n) => Math.max(1, n.depth) * 250, centerX, centerY)
          .strength(radialStrength),
      )
      .alpha(1)
      .alphaDecay(0.1)
      .velocityDecay(0.6)
      .on("tick", () => {
        updateGraphDom();
      })
      .on("end", () => {
        updateGraphDom();
        if (resetViewOnNextLayout) {
          fitToContent();
          resetViewOnNextLayout = false;
        }
      });
    positioned = simNodes.map((n) => ({ ...n }));
    updateGraphDom();
  }

  $: if (displayNodes.length && layoutKey !== simulationLayoutKey) {
    simulationLayoutKey = layoutKey;
    startSimulation();
  }

  // Refresh labels when Modrinth metadata arrives without restarting layout.
  $: if (positioned.length && Object.keys(ghostMeta).length > 0) {
    positioned = positioned.map((node) => ({
      ...node,
      label: displayLabel(node),
    }));
  }

  $: positionById = new Map(positioned.map((node) => [node.id, node]));

  // Reactive edge paths: recomputed whenever `positioned` (and thus
  // `positionById`) changes, so arrows track nodes live during the sim.
  $: edgePaths = displayEdges.map((edge) => ({
    key: `${edge.from}:${edge.to}:${edge.kind}`,
    edge,
    from: edge.from,
    to: edge.to,
    d: edgePath(edge),
    danger: edgeDanger(edge),
  }));

  function edgeKey(edge: GraphEdge): string {
    return `${edge.from}:${edge.to}:${edge.kind}`;
  }

  function registerNode(el: Element, node: PositionedNode) {
    nodeEls.set(node.id, el as SVGGElement);
  }

  function registerEdge(el: Element, edge: GraphEdge) {
    edgeEls.set(edgeKey(edge), el as SVGPathElement);
  }

  // Imperative DOM update (modrinth-extras pattern): mutate transforms and
  // edge `d` attributes directly each tick instead of rebuilding a reactive
  // array, so arrows stay glued to nodes and dragging is smooth.
  function updateGraphDom() {
    for (const node of simNodes) {
      const el = nodeEls.get(node.id);
      if (el) el.style.transform = `translate(${node.x}px, ${node.y}px)`;
    }
    for (const edge of displayEdges) {
      const el = edgeEls.get(edgeKey(edge));
      if (el) el.setAttribute("d", edgePath(edge));
    }
  }

  // --- Obsidian-style pan & zoom viewport state ---
  // The canvas itself stays a fixed logical size; instead of resizing the
  // SVG we move/scale a "camera" viewBox over it, exactly like Obsidian's
  // graph view: scroll to zoom (toward the cursor), drag empty space to
  // pan, double-click / button to reset.
  let viewportEl: SVGSVGElement;
  let viewX = 0;
  let viewY = 0;
  let viewScale = 1;
  let isPanning = false;
  let panStart = { x: 0, y: 0, viewX: 0, viewY: 0 };
  $: viewBoxHeight = canvasHeight / viewScale;
  $: viewBoxWidth = canvasWidth / viewScale;
  $: viewBoxString = `${viewX} ${viewY} ${viewBoxWidth} ${viewBoxHeight}`;
  $: denseGraph = displayEdges.length > 70;

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
    const nextScale = Math.min(8, Math.max(0.15, viewScale / zoomFactor));
    const cursor = clientToSvgPoint(event.clientX, event.clientY);
    const nextWidth = canvasWidth / nextScale;
    const nextHeight = canvasHeight / nextScale;
    // Keep the point under the cursor stationary while zooming.
    const ratioX = (cursor.x - viewX) / viewBoxWidth;
    const ratioY = (cursor.y - viewY) / viewBoxHeight;
    viewX = cursor.x - ratioX * nextWidth;
    viewY = cursor.y - ratioY * nextHeight;
    viewScale = nextScale;
  }

  let panMoved = false;
  function handleBackgroundMouseDown(event: MouseEvent) {
    if (event.button !== 0) return;
    if (isPanning) return;
    isPanning = true;
    panMoved = false;
    panStart = { x: event.clientX, y: event.clientY, viewX, viewY };
    window.addEventListener("mousemove", handleBackgroundMouseMove);
    window.addEventListener("mouseup", handleBackgroundMouseUp);
  }

  function handleBackgroundMouseMove(event: MouseEvent) {
    if (!isPanning || !viewportEl) return;
    const rect = viewportEl.getBoundingClientRect();
    const dx = ((event.clientX - panStart.x) / rect.width) * viewBoxWidth;
    const dy = ((event.clientY - panStart.y) / rect.height) * viewBoxHeight;
    if (Math.abs(event.clientX - panStart.x) > 3 || Math.abs(event.clientY - panStart.y) > 3) {
      panMoved = true;
    }
    viewX = panStart.viewX - dx;
    viewY = panStart.viewY - dy;
  }

  function handleBackgroundMouseUp() {
    isPanning = false;
    window.removeEventListener("mousemove", handleBackgroundMouseMove);
    window.removeEventListener("mouseup", handleBackgroundMouseUp);
  }

  function fitToContent(padding = 80) {
    const fitNodes = simNodes.length ? simNodes : positioned;
    if (!fitNodes.length) {
      viewX = 0;
      viewY = 0;
      viewScale = 1;
      return;
    }
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const node of fitNodes) {
      const half = nodeSize(node) / 2 + 28;
      minX = Math.min(minX, node.x - half);
      minY = Math.min(minY, node.y - half);
      maxX = Math.max(maxX, node.x + half);
      maxY = Math.max(maxY, node.y + half + 18);
    }
    const contentW = Math.max(160, maxX - minX);
    const contentH = Math.max(160, maxY - minY);
    const targetW = contentW + padding * 2;
    const targetH = contentH + padding * 2;
    // Cap like modrinth-extras (max ~1.2–1.5) so we never blow past readable size.
    const scale = Math.min(canvasWidth / targetW, canvasHeight / targetH, 1.35);
    viewScale = Math.min(4, Math.max(0.2, scale));
    const vbW = canvasWidth / viewScale;
    const vbH = canvasHeight / viewScale;
    viewX = (minX + maxX) / 2 - vbW / 2;
    viewY = (minY + maxY) / 2 - vbH / 2;
  }

  function resetView() {
    fitToContent();
  }

  function zoomBy(factor: number) {
    const centerX = viewX + viewBoxWidth / 2;
    const centerY = viewY + viewBoxHeight / 2;
    const nextScale = Math.min(8, Math.max(0.2, viewScale * factor));
    const nextWidth = canvasWidth / nextScale;
    const nextHeight = canvasHeight / nextScale;
    viewX = centerX - nextWidth / 2;
    viewY = centerY - nextHeight / 2;
    viewScale = nextScale;
  }

  async function toggleFullscreen() {
    try {
      if (graphFullscreen) {
        await document.exitFullscreen();
      } else {
        await graphCanvasEl.requestFullscreen();
      }
      resetView();
    } catch (e) {
      error = `Fullscreen mode is unavailable: ${String(e)}`;
    }
  }

  onMount(() => {
    void listen<DownloadProgress>("mod-download-progress", (event) => {
      if (depInstallStatus !== "downloading") return;
      depInstallBatchSeen = true;
      const item = event.payload;
      depInstallMessage = item.status === "downloading"
        ? `Downloading ${item.name}… ${Math.round(item.percent)}%`
        : `${item.name}: ${item.status}`;
    }).then((unlisten) => {
      unlistenDownloadProgress = unlisten;
    });
    void listen<DownloadBatch>("mod-download-batch", (event) => {
      if (depInstallStatus !== "downloading") return;
      depInstallBatchSeen = true;
      if (event.payload.phase !== "done") return;
      const failures = event.payload.failed ?? [];
      if (failures.length > 0) {
        depInstallStatus = "failed";
        depInstallFailedIds = failures.map((failure) => failure.modId);
        depInstallError = failures.map((failure) => `${failure.modId}: ${failure.error}`).join("\n");
        depInstallMessage = `Download failed for ${failures.length} file${failures.length > 1 ? "s" : ""}.`;
      } else {
        depInstallStatus = "done";
        depInstallMessage = `${depPreviewName} and its dependencies are installed.`;
        message = `Installed ${depPreviewName}.`;
        void load(true);
      }
    }).then((unlisten) => {
      unlistenDownloadBatch = unlisten;
    });
  });

  onDestroy(() => {
    unlistenDownloadProgress?.();
    unlistenDownloadBatch?.();
    simulation?.stop();
  });

  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
  function handleNodeMouseDown(event: MouseEvent, node: PositionedNode) {
    event.stopPropagation();
    selectedId = node.id;
    if (!simulation) return;
    const sim = simNodes.find((n) => n.id === node.id);
    if (!sim) return;
    const layoutNode = sim;
    const keepPinned = !!layoutNode.isHub;
    layoutNode.fx = layoutNode.x;
    layoutNode.fy = layoutNode.y;
    const onMouseMove = (ev: MouseEvent) => {
      const p = clientToSvgPoint(ev.clientX, ev.clientY);
      layoutNode.fx = p.x;
      layoutNode.fy = p.y;
      layoutNode.x = p.x;
      layoutNode.y = p.y;
      if (keepPinned) {
        layoutNode.hubX = p.x;
        layoutNode.hubY = p.y;
        for (const other of simNodes) {
          if (other.hubId === layoutNode.hubId) {
            other.hubX = p.x;
            other.hubY = p.y;
          }
        }
      }
      simulation?.alpha(0.1).restart();
    };
    const onMouseUp = () => {
      // Hubs stay pinned (modrinth-extras root behavior); others rejoin the sim.
      if (!keepPinned) {
        layoutNode.fx = null;
        layoutNode.fy = null;
      }
      simulation?.alphaTarget(0);
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
  }
</script>

<svelte:document bind:fullscreenElement />

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
      <button class="ghost" on:click={() => refreshGraph(true)} title="Refresh dependency metadata" disabled={!$projectPath || loading || graphRefreshing}>
        <RefreshCw size={16} class={graphRefreshing ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if message}<div class="notice success">{message}</div>{/if}
  {#if graph}
    <div class="graph-status" class:stale={graphSource === "local"} class:error={!!refreshError}>
      {#if graphRefreshing}
        <Loader2 size={13} class="spin" /> Updating dependencies in the background…
      {:else if refreshError}
        Offline graph shown. Refresh failed: {refreshError}
      {:else if graphSource === "local"}
        Local graph shown; network metadata has not been cached yet.
      {:else}
        {graphSource === "cache" ? "Cached dependency graph" : "Current dependency graph"}
        {graphGeneratedAt ? ` · ${new Date(graphGeneratedAt).toLocaleString()}` : ""}
      {/if}
    </div>
  {/if}

  {#if loading && !graph}
    <div class="loading">Loading graph...</div>
  {:else if error}
    <div class="empty error">{error}</div>
  {:else if graph}
    <div class="stats">
      <div class="stat-card accent">
        <span class="stat-value">{modNodes.length}</span>
        <span class="stat-label">Mods</span>
      </div>
      <div class="stat-card">
        <span class="stat-value">{displayEdges.length}</span>
        <span class="stat-label">Dependencies</span>
      </div>
      <div class="stat-card" class:danger={missingEdges.length > 0}>
        <span class="stat-value">{missingEdges.length}</span>
        <span class="stat-label">Missing</span>
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
            {#each changePlan.actions as action, index (index)}
              <div class="plan-action-row">
                <span class="plan-action-label">{formatChangeAction(action)}</span>
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

    <section bind:this={graphCanvasEl} class="graph-canvas" class:fullscreen={graphFullscreen} aria-label="Dependency graph canvas">
      <div class="canvas-controls">
        <button class="ghost mini" on:click={() => zoomBy(1.25)} title="Zoom in">+</button>
        <button class="ghost mini" on:click={() => zoomBy(1 / 1.25)} title="Zoom out">−</button>
        <button class="ghost mini" on:click={resetView} title="Fit graph to view">⤢</button>
        <button class="ghost mini" on:click={toggleFullscreen} title={graphFullscreen ? "Exit fullscreen" : "Open fullscreen"}>
          {#if graphFullscreen}<Minimize2 size={14} />{:else}<Maximize2 size={14} />{/if}
        </button>
        <span class="zoom-readout">{Math.round(viewScale * 100)}%</span>
      </div>
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
      <svg
        bind:this={viewportEl}
        viewBox={viewBoxString}
        role="img"
        aria-label="Dependency graph"
        class:panning={isPanning}
        on:wheel={handleWheel}
        on:mousedown={handleBackgroundMouseDown}
        on:click={(e) => {
          // Click on empty canvas (not a node — those stopPropagation) clears
          // the current selection, but only if the user wasn't panning.
          if (!panMoved) selectedId = null;
        }}
        on:dblclick={resetView}
        on:keydown={() => {}}
      >
        <defs>
          <marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="3" orient="auto" markerUnits="userSpaceOnUse">
            <path d="M0,0 L0,6 L7,3 z" fill="rgba(161,161,170,.75)" />
          </marker>
          <marker id="arrow-danger" markerWidth="8" markerHeight="8" refX="7" refY="3" orient="auto" markerUnits="userSpaceOnUse">
            <path d="M0,0 L0,6 L7,3 z" fill="rgba(113,113,122,.95)" />
          </marker>
        </defs>
        {#each edgePaths as ep (ep.key)}
          <path
            use:registerEdge={ep.edge}
            class="graph-edge"
            class:dense={denseGraph}
            class:danger-edge={ep.danger}
            class:dimmed={selectedId && ep.from !== selectedId && ep.to !== selectedId}
            d={ep.d}
            marker-end={ep.danger ? "url(#arrow-danger)" : "url(#arrow)"}
          />
        {/each}
        {#each positioned as node (node.id)}
          {@const size = nodeSize(node)}
          {@const half = size / 2}
          {@const icon = nodeIconUrl(node)}
          {@const isGhost = node.kind === "Missing" || node.ghost}
          {@const isInstalledDep = node.kind === "Mod" && depNodeIds.has(node.id)}
          {@const isClickableDep = isGhost || isInstalledDep}
          <g
            use:registerNode={node}
            class="svg-node tone-{node.tone}"
            class:selected={selectedId === node.id}
            class:clickable-dep={isInstalledDep}
            class:dimmed={selectedId && selectedId !== node.id && !selectedEdges.some((e) => e.from === node.id || e.to === node.id)}
            role="button"
            tabindex="0"
            style={`transform: translate(${node.x}px, ${node.y}px)`}
            on:mousedown={(e) => handleNodeMouseDown(e, node)}
            on:click|stopPropagation={() => handleNodeClick(node)}
            on:keydown={(e) => e.key === "Enter" && handleNodeClick(node)}
            aria-label={node.label}
          >
            {#if isClickableDep}
              <g
                class="dep-icon-hit"
                role="button"
                tabindex="0"
                on:click|stopPropagation={() => handleDepIconClick(node)}
                on:keydown={(e) => e.key === "Enter" && handleDepIconClick(node)}
                aria-label={isGhost ? `Install ${node.label}` : `Re-download ${node.label}`}
              >
                <rect x={-half} y={-half} width={size} height={size} rx="8" ry="8" />
                {#if icon}
                  <clipPath id="clip-{node.id.replace(/[^a-zA-Z0-9]/g, '_')}">
                    <rect x={-half + 2} y={-half + 2} width={size - 4} height={size - 4} rx="6" ry="6" />
                  </clipPath>
                  <image
                    href={icon}
                    x={-half + 2}
                    y={-half + 2}
                    width={size - 4}
                    height={size - 4}
                    clip-path={`url(#clip-${node.id.replace(/[^a-zA-Z0-9]/g, '_')})`}
                    preserveAspectRatio="xMidYMid slice"
                    on:error={() => handleIconError(node)}
                  />
                {:else}
                  <text class="fallback-letter" y="5" text-anchor="middle">{node.label?.[0]?.toUpperCase() ?? "?"}</text>
                {/if}
              </g>
            {:else}
              <rect x={-half} y={-half} width={size} height={size} rx="8" ry="8" />
              {#if icon}
                <clipPath id="clip-{node.id.replace(/[^a-zA-Z0-9]/g, '_')}">
                  <rect x={-half + 2} y={-half + 2} width={size - 4} height={size - 4} rx="6" ry="6" />
                </clipPath>
                <image
                  href={icon}
                  x={-half + 2}
                  y={-half + 2}
                  width={size - 4}
                  height={size - 4}
                  clip-path={`url(#clip-${node.id.replace(/[^a-zA-Z0-9]/g, '_')})`}
                  preserveAspectRatio="xMidYMid slice"
                  on:error={() => handleIconError(node)}
                />
              {:else}
                <text class="fallback-letter" y="5" text-anchor="middle">{node.label?.[0]?.toUpperCase() ?? "?"}</text>
              {/if}
            {/if}
            {#if isGhost}
              <text class="ghost-download" y={half + 14} text-anchor="middle">⬇ {node.label.length > 14 ? node.label.slice(0, 13) + "…" : node.label}</text>
            {:else}
              <text class="node-label-text" y={half + 14} text-anchor="middle">{node.label.length > 18 ? node.label.slice(0, 17) + "…" : node.label}</text>
              <g class="remove-btn" role="button" tabindex="-1" aria-label="Remove mod" on:mousedown|stopPropagation={() => {}} on:click|stopPropagation={() => removeConflictNode(node.id)} on:keydown|stopPropagation={(e) => e.key === "Enter" && removeConflictNode(node.id)}>
                <circle cx={half - 2} cy={-half + 2} r="8" />
                <text x={half - 2} y={-half + 6} text-anchor="middle" class="remove-x">×</text>
              </g>
            {/if}
          </g>
        {/each}
      </svg>
    </section>

    <div class="graph-layout">
      <section class="node-column mods-column">
        <h3><Box size={16} /> Mods ({modNodes.length})</h3>
        {#if modNodes.length === 0}
          <div class="muted-box">No mod nodes yet.</div>
        {:else}
          <div class="mod-grid">
            {#each modNodes as node (node.id)}
              {@const icon = !brokenIcons.has(node.id) ? nodeIconUrl(node) : null}
              {@const isClickableDep = depNodeIds.has(node.id)}
              {@const missingDeps = missingDepsByMod.get(node.id) ?? []}
              <div
                class="node-card compact side-{node.side}"
                class:selected={selectedId === node.id}
                class:is-dep={isClickableDep}
                role="button"
                tabindex="0"
                on:click={() => (selectedId = node.id)}
                on:keydown={(event) => (event.key === "Enter" || event.key === " ") && (selectedId = node.id)}
              >
                {#if icon}
                  {#if isClickableDep}
                    <button
                      class="card-icon-btn"
                      title="Click to re-download this dependency"
                      on:click|stopPropagation={downloadMissingFiles}
                    >
                      <img class="card-icon" src={icon} alt="" loading="lazy" on:error={() => handleIconError(node)} />
                    </button>
                  {:else}
                    <img class="card-icon" src={icon} alt="" loading="lazy" on:error={() => handleIconError(node)} />
                  {/if}
                {:else if isClickableDep}
                  <button
                    class="card-icon-btn"
                    title="Click to re-download this dependency"
                    on:click|stopPropagation={downloadMissingFiles}
                  >
                    <span class="card-icon-fallback">{node.label?.[0]?.toUpperCase() ?? "?"}</span>
                  </button>
                {:else}
                  <span class="card-icon-fallback">{node.label?.[0]?.toUpperCase() ?? "?"}</span>
                {/if}
                <div class="card-text">
                  <span class="node-label">{node.label}</span>
                  <span class="node-meta">{node.version ?? "unknown"}{depNodeIds.has(node.id) ? " · dep" : ""}{missingDeps.length > 0 ? ` · ${missingDeps.length} missing` : ""}</span>
                </div>
                {#if missingDeps.length > 0}
                  <button
                    class="card-install-deps"
                    title="Install missing dependencies"
                    on:click|stopPropagation={async () => {
                      for (const edge of missingDeps) {
                        await installSingleMissingDep(edge);
                      }
                    }}
                    disabled={resolving}
                  >
                    <Download size={14} />
                  </button>
                {/if}
                <span class="card-remove" role="button" tabindex="0" title="Remove mod" on:click|stopPropagation={() => removeConflictNode(node.id)} on:keydown|stopPropagation={(e) => e.key === "Enter" && removeConflictNode(node.id)}>
                  <X size={14} />
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </section>

      {#if ghostNodes.length > 0}
        <section class="node-column missing-column">
          <h3><Download size={16} /> Missing dependencies ({ghostNodes.length})</h3>
          <p class="column-hint">Click an icon to install.</p>
          <div class="mod-grid">
            {#each ghostNodes as node (node.id)}
              <button
                class="node-card compact missing-card"
                class:selected={selectedId === node.id}
                on:click={() => installGhostNode(node.id)}
                disabled={resolving}
                title="Click to install {node.label}"
              >
                <span class="card-icon-fallback missing-fallback">⬇</span>
                <div class="card-text">
                  <span class="node-label">{node.label}</span>
                  <span class="node-meta">not installed</span>
                </div>
                <Download size={14} />
              </button>
            {/each}
          </div>
        </section>
      {/if}

      <aside class="details">
        {#if selected}
          <div class="details-header">
            <div>
              <span class="eyebrow">Selected node</span>
              <h2>{selected.label}</h2>
            </div>
            <span class="tag">{selected.kind}</span>
          </div>

          {#if selected.kind === "Missing"}
            <div class="details-actions">
              <button class="install-btn" on:click={() => installGhostNode(selected.id)} disabled={resolving}>
                <Download size={16} />
                {resolving ? "Installing..." : "Install from Modrinth"}
              </button>
            </div>
          {:else}
            <div class="details-actions">
              <button class="remove-btn-panel" on:click={() => removeConflictNode(selected.id)} disabled={resolving}>
                <X size={16} />
                Remove mod
              </button>
            </div>
          {/if}

          <div class="details-grid">
            <div><span>ID</span><code>{selected.id}</code></div>
            <div><span>Version</span><code>{selected.version ?? "—"}</code></div>
            <div><span>Side</span><code>{selected.side ?? "—"}</code></div>
            <div><span>Relations</span><code>{selectedEdges.length}</code></div>
          </div>

          {#if selected.metadata && Object.keys(selected.metadata).length > 0}
            <h3>Metadata</h3>
            <div class="kv">
              {#each Object.entries(selected.metadata) as [key, value] (key)}
                <span>{key}</span><code>{value}</code>
              {/each}
            </div>
          {/if}

          <h3>Relations</h3>
          {#if selectedEdges.length === 0}
            <div class="muted-box">No direct relations.</div>
          {:else}
            <div class="relations">
              {#each selectedEdges as edge (`${edge.from}:${edge.to}:${edge.kind}`)}
                {@const otherId = edge.from === selectedId ? edge.to : edge.from}
                {@const isMissingDep = edge.kind === "Requires" && !nodeById(otherId)}
                <div class="relation" class:incoming={edge.to === selectedId}>
                  <span class="relation-kind">{edge.kind}</span>
                  <span class="relation-text">
                    {edge.from === selectedId ? "requires" : "required by"}
                    <strong>{resolveNodeLabel(otherId)}</strong>
                  </span>
                  {#if edge.reason}<small>{edge.reason}</small>{/if}
                  {#if isMissingDep}
                    <button class="secondary mini" on:click={() => installGhostNode(otherId)} disabled={resolving}>
                      <Download size={12} /> Install
                    </button>
                  {/if}
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
        {#each conflictEdges as edge (`${edge.from}:${edge.to}:${edge.kind}`)}
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
        {#each missingEdges as edge (`${edge.from}:${edge.to}:${edge.kind}`)}
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
              {#each depPreviewRequired as dep (dep.target)}
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
              {#each depPreviewOptional as dep (dep.target)}
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
        {#if depInstallStatus === "downloading"}
          <div class="install-transfer" aria-live="polite">
            <Loader2 size={16} class="spin" />
            <span>{depInstallMessage}</span>
          </div>
        {:else if depInstallStatus === "failed"}
          <div class="install-transfer failed" aria-live="assertive">
            <AlertTriangle size={16} />
            <div><strong>{depInstallMessage}</strong><pre>{depInstallError}</pre></div>
          </div>
        {:else if depInstallStatus === "done"}
          <div class="install-transfer done" aria-live="polite">
            <Download size={16} />
            <span>{depInstallMessage}</span>
          </div>
        {/if}
        <div class="modal-footer-actions">
          <button class="secondary" on:click={() => (depPreviewOpen = false)} disabled={depInstallStatus === "downloading"}>
            {depInstallStatus === "done" ? "Close" : "Cancel"}
          </button>
          {#if depInstallStatus === "failed"}
            <button on:click={retryDepInstall} disabled={resolving}>
              <RotateCw size={16} /> Retry
            </button>
          {:else if depInstallStatus !== "done"}
            <button on:click={confirmDepInstall} disabled={depPreviewLoading || depInstallStatus === "downloading"}>
              <Download size={16} /> {depInstallStatus === "downloading" ? "Downloading…" : "Install"}
            </button>
          {/if}
        </div>
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

  .graph-status {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: -10px 0 14px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .graph-status.stale { color: #fbbf24; }
  .graph-status.error { color: #fca5a5; }

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
  .plan-action-label {
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.35;
  }
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
    height: 560px;
    display: block;
    cursor: grab;
    touch-action: none;
  }

  .graph-canvas:fullscreen {
    margin: 0;
    border: 0;
    border-radius: 0;
    background: #09090b;
  }

  .graph-canvas:fullscreen svg {
    height: 100vh;
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

  .graph-canvas .graph-edge {
    fill: none;
    stroke: rgba(161, 161, 170, 0.55);
    stroke-width: 1.6;
    transition: opacity 120ms ease;
  }

  .graph-canvas .graph-edge.dense {
    stroke: rgba(161, 161, 170, 0.28);
    stroke-width: 1.2;
  }

  .graph-canvas .graph-edge.dimmed {
    opacity: 0.12;
  }

  .svg-node.dimmed {
    opacity: 0.25;
  }

  .graph-canvas .graph-edge.danger-edge {
    stroke: rgba(82, 82, 91, 0.95);
    stroke-dasharray: 6 5;
  }

  .svg-node {
    cursor: pointer;
    transition: opacity 120ms ease;
  }

  /* Installed dependency — third color (amber) */
  .svg-node.clickable-dep rect {
    stroke: rgba(245,166,35,.7);
    stroke-dasharray: 4 3;
  }
  .svg-node.clickable-dep:hover rect {
    stroke: rgba(245,166,35,1);
    fill: rgba(245,166,35,.08);
  }

  .svg-node rect {
    fill: #18181b;
    stroke: rgba(255,255,255,.28);
    stroke-width: 1.6;
    transition: stroke 120ms ease, fill 120ms ease;
  }

  .svg-node.selected rect,
  .svg-node:hover rect {
    stroke: rgba(27,217,106,.85);
    fill: rgba(27,217,106,.18);
  }

  .svg-node.tone-client rect { stroke: rgba(139,92,246,.7); }
  .svg-node.tone-server rect { stroke: rgba(59,130,246,.7); }
  .svg-node.tone-both rect { stroke: rgba(27,217,106,.65); }
  .svg-node.tone-runtime rect { stroke: rgba(245,158,11,.7); }
  .svg-node.tone-profile rect { stroke: rgba(96,165,250,.7); }
  .svg-node.tone-ghost rect {
    stroke: rgba(113, 113, 122, 0.9);
    stroke-dasharray: 4 3;
    fill: rgba(24, 24, 27, 0.95);
  }
  .svg-node.tone-ghost .node-label-text { fill: #a1a1aa; }
  .svg-node.tone-ghost .fallback-letter { fill: #71717a; }

  .svg-node .fallback-letter {
    fill: #e5e7eb;
    font-size: 16px;
    font-weight: 900;
    pointer-events: none;
  }

  .svg-node .node-label-text {
    fill: #d1d5db;
    font-size: 11px;
    font-weight: 700;
    pointer-events: none;
  }

  .svg-node .ghost-download {
    fill: #a1a1aa;
    font-size: 10px;
    font-weight: 700;
    pointer-events: none;
    cursor: pointer;
  }

  .svg-node.tone-ghost {
    cursor: pointer;
  }
  .svg-node.tone-ghost:hover rect {
    stroke: rgba(27,217,106,.85);
    fill: rgba(27,217,106,.12);
  }

  .remove-btn {
    cursor: pointer;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .svg-node:hover .remove-btn,
  .svg-node.selected .remove-btn {
    opacity: 1;
  }
  .remove-btn circle {
    fill: rgba(239,68,68,.85);
    stroke: none;
  }
  .remove-btn:hover circle {
    fill: rgba(239,68,68,1);
  }
  .remove-btn .remove-x {
    fill: #fff;
    font-size: 13px;
    font-weight: 900;
    pointer-events: none;
  }

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
    grid-template-columns: minmax(320px, 1fr) 360px;
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

  .missing-column {
    border-top: 1px solid var(--border-color);
    padding-top: 14px;
    margin-top: 14px;
  }
  .column-hint {
    color: var(--text-muted);
    font-size: 11px;
    margin: 0 0 8px;
    font-style: italic;
  }
  .missing-card {
    border-color: rgba(82, 82, 91, 0.65);
    background: rgba(24, 24, 27, 0.85);
  }
  .missing-card:hover {
    border-color: rgba(27, 217, 106, 0.55);
    background: rgba(27, 217, 106, 0.08);
  }
  .missing-fallback {
    background: linear-gradient(135deg, #27272a, #18181b);
    color: #a1a1aa;
    font-size: 18px;
    font-weight: 800;
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
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }

  .node-card.is-dep {
    opacity: 0.85;
    border-style: dashed;
    border-color: rgba(245, 158, 11, 0.45);
    background: rgba(245, 158, 11, 0.06);
  }

  .card-icon {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    object-fit: cover;
    flex-shrink: 0;
    background: var(--bg-elevated);
  }
  .card-icon-btn {
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
    border-radius: 8px;
    line-height: 0;
    transition: transform 100ms ease, box-shadow 100ms ease;
  }
  .card-icon-btn:hover {
    transform: scale(1.08);
    box-shadow: 0 0 0 2px rgba(245,166,35,.6);
  }

  .card-icon-fallback {
    width: 36px;
    height: 36px;
    border-radius: 8px;
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 900;
    font-size: 16px;
    flex-shrink: 0;
  }

  .card-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .card-remove {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    color: var(--text-muted);
    opacity: 0;
    transition: opacity 120ms ease, color 120ms ease, background 120ms ease;
    flex-shrink: 0;
    cursor: pointer;
  }
  .node-card:hover .card-remove,
  .node-card.selected .card-remove {
    opacity: 1;
  }
  .card-remove:hover {
    color: #ef4444;
    background: rgba(239,68,68,.12);
  }

  .card-install-deps {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    border: 1px solid rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.1);
    color: var(--accent-primary);
    flex-shrink: 0;
  }

  .card-install-deps:hover:not(:disabled) {
    background: rgba(27, 217, 106, 0.18);
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

  .details-actions {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }

  .install-btn,
  .remove-btn-panel {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-radius: 10px;
    font-weight: 700;
    font-size: 13px;
    border: none;
    cursor: pointer;
    transition: filter 120ms ease;
  }
  .install-btn {
    background: var(--accent-primary);
    color: #fff;
    flex: 1;
  }
  .install-btn:hover:not(:disabled) { filter: brightness(1.1); }
  .install-btn:disabled { opacity: .5; cursor: wait; }

  .remove-btn-panel {
    background: rgba(239,68,68,.12);
    color: #ef4444;
    border: 1px solid rgba(239,68,68,.3);
  }
  .remove-btn-panel:hover:not(:disabled) {
    background: rgba(239,68,68,.2);
  }
  .remove-btn-panel:disabled { opacity: .5; cursor: wait; }

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

  /* Dep-tone node: amber for downloaded dependency mods */
  .svg-node.tone-dep rect {
    stroke: rgba(245, 158, 11, 0.7);
    fill: rgba(245, 158, 11, 0.08);
  }

  .ghost-node {
    cursor: pointer;
    transition: all .15s ease;
    animation: ghost-pulse 2s ease-in-out infinite;
  }
  .ghost-node:hover {
    fill: rgba(27,217,106,0.18);
    stroke: rgba(27,217,106,0.85);
    r: 16;
  }
  .ghost-label {
    cursor: pointer;
    transition: fill .15s;
  }
  .ghost-label:hover {
    fill: #d4d4d8;
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
    flex-direction: column;
    gap: 10px;
    padding: 12px 24px 20px;
  }
  .modal-footer-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .install-transfer {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    padding: 10px 12px;
    border: 1px solid rgba(27, 217, 106, 0.28);
    border-radius: 10px;
    color: var(--text-secondary);
    background: rgba(27, 217, 106, 0.06);
    font-size: 13px;
  }
  .install-transfer.failed {
    border-color: rgba(239, 68, 68, 0.35);
    background: rgba(239, 68, 68, 0.08);
    color: #fca5a5;
  }
  .install-transfer.done { color: var(--accent-primary); }
  .install-transfer pre {
    margin: 4px 0 0;
    white-space: pre-wrap;
    color: inherit;
    font: inherit;
    font-size: 12px;
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
