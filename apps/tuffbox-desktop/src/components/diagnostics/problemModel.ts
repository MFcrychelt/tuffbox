/** Unified Health Check problem model — merge hints, rules, AI, graph, jars. */

export type ProblemSeverity = "critical" | "error" | "warning" | "info";
export type ProblemCategory = "crash" | "dependency" | "conflict" | "runtime" | "config" | "pack";
export type ProblemSource = "hint" | "rules" | "ai" | "graph" | "jar";
export type ProblemRisk = "safe" | "caution" | "destructive";

export type FixAction = {
  kind: string;
  label: string;
  modId: string | null;
};

export type Problem = {
  id: string;
  severity: ProblemSeverity;
  category: ProblemCategory;
  title: string;
  summary: string;
  source: ProblemSource;
  code?: string;
  modIds: string[];
  actions: FixAction[];
  risk: ProblemRisk;
  evidence?: { line?: number; label?: string };
  steps?: string[];
  layer: "crash" | "pack";
};

const SEV_RANK: Record<ProblemSeverity, number> = {
  critical: 0,
  error: 1,
  warning: 2,
  info: 3,
};

const CAT_RANK: Record<ProblemCategory, number> = {
  crash: 0,
  runtime: 1,
  conflict: 2,
  dependency: 3,
  config: 4,
  pack: 5,
};

const CODE_TITLES: Record<string, string> = {
  MISSING_DEPENDENCY: "Missing dependency",
  MOD_CONFLICT: "Two mods conflict",
  DUPLICATE_MOD: "Duplicate mod jars",
  WRONG_LOADER: "Wrong loader jar",
  WRONG_SIDE_IN_PROFILE: "Wrong side in profile",
  PROFILE_INCLUDES_UNKNOWN_MOD: "Unknown mod in profile",
  UNKNOWN_SIDE: "Unknown mod side",
  OUT_OF_MEMORY: "Out of memory",
  MIXIN_APPLY_FAILED: "Mixin conflict",
  CASCADING_CONFIG_ERROR: "Crash masked by another error",
  CLIENT_ONLY_ON_SERVER: "Client-only mod on server",
  HARD_MOD_CONFLICT: "Hard mod conflict",
  DUPLICATE_MODS: "Duplicate mods",
};

export function humanizeDiagnosticCode(code: string): string {
  const key = String(code ?? "").trim().toUpperCase().replace(/-/g, "_");
  if (CODE_TITLES[key]) return CODE_TITLES[key];
  // Title-case leftover SCREAMING_SNAKE
  return key
    .toLowerCase()
    .split("_")
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ") || "Issue";
}

export function normalizeSeverity(raw: string | undefined | null): ProblemSeverity {
  const s = String(raw ?? "").toLowerCase();
  if (s === "critical") return "critical";
  if (s === "error") return "error";
  if (s === "warning") return "warning";
  if (s === "info") return "info";
  // Graph uses PascalCase
  if (raw === "Error") return "error";
  if (raw === "Warning") return "warning";
  if (raw === "Info") return "info";
  return "info";
}

export function riskForActionKind(kind: string): ProblemRisk {
  const k = String(kind ?? "").toLowerCase();
  if (k.includes("remove") || k === "removemod") return "destructive";
  if (
    k.includes("install") ||
    k.includes("raise") ||
    k.includes("memory") ||
    k.includes("eula") ||
    k.includes("java") ||
    k.includes("port")
  ) {
    return "safe";
  }
  return "caution";
}

export function riskForActions(actions: FixAction[]): ProblemRisk {
  if (!actions.length) return "safe";
  let worst: ProblemRisk = "safe";
  for (const a of actions) {
    const r = riskForActionKind(a.kind);
    if (r === "destructive") return "destructive";
    if (r === "caution") worst = "caution";
  }
  return worst;
}

export function categoryForCode(code: string, source: ProblemSource): ProblemCategory {
  const c = String(code ?? "").toUpperCase();
  if (/MISSING_DEPENDENCY|INSTALL/.test(c)) return "dependency";
  if (/CONFLICT|DUPLICATE|WRONG_LOADER/.test(c)) return "conflict";
  if (/MEMORY|OOM|HS_ERR|NATIVE|OPENGL|PERFORMANCE|TICK/.test(c)) return "runtime";
  if (/CONFIG|CASCADING|EULA|PORT/.test(c)) return "config";
  if (source === "graph" || source === "jar") return "pack";
  if (source === "hint" || source === "rules" || source === "ai") return "crash";
  return "pack";
}

export function severityChip(sev: ProblemSeverity): string {
  if (sev === "critical") return "Fix this first";
  if (sev === "error") return "Needs a fix";
  if (sev === "warning") return "Worth checking";
  return "FYI";
}

export function maxSeverity(a: ProblemSeverity, b: ProblemSeverity): ProblemSeverity {
  return SEV_RANK[a] <= SEV_RANK[b] ? a : b;
}

function dedupeKey(p: Problem): string {
  const mods = [...p.modIds].map((m) => m.toLowerCase()).sort().join(",");
  const code = (p.code ?? "").toUpperCase();
  if (code) return `${code}::${mods}`;
  return `${p.title.toLowerCase().trim()}::${mods}`;
}

export function mergeProblems(rows: Problem[]): Problem[] {
  const map = new Map<string, Problem>();
  for (const row of rows) {
    const key = dedupeKey(row);
    const prev = map.get(key);
    if (!prev) {
      map.set(key, { ...row, actions: [...row.actions], modIds: [...row.modIds], steps: row.steps ? [...row.steps] : undefined });
      continue;
    }
    prev.severity = maxSeverity(prev.severity, row.severity);
    if (SEV_RANK[row.severity] < SEV_RANK[prev.severity] || row.summary.length > prev.summary.length) {
      prev.summary = row.summary;
      prev.title = row.title;
    }
    const seen = new Set(prev.actions.map((a) => `${a.kind}:${a.modId}:${a.label}`));
    for (const a of row.actions) {
      const k = `${a.kind}:${a.modId}:${a.label}`;
      if (!seen.has(k)) {
        seen.add(k);
        prev.actions.push(a);
      }
    }
    for (const m of row.modIds) {
      if (!prev.modIds.includes(m)) prev.modIds.push(m);
    }
    if (!prev.evidence && row.evidence) prev.evidence = row.evidence;
    if ((!prev.steps || !prev.steps.length) && row.steps?.length) prev.steps = [...row.steps];
    prev.risk = riskForActions(prev.actions);
    // Prefer crash layer over pack when merged
    if (row.layer === "crash") prev.layer = "crash";
  }
  return [...map.values()].sort((a, b) => {
    const sd = SEV_RANK[a.severity] - SEV_RANK[b.severity];
    if (sd !== 0) return sd;
    const cd = CAT_RANK[a.category] - CAT_RANK[b.category];
    if (cd !== 0) return cd;
    if (a.layer !== b.layer) return a.layer === "crash" ? -1 : 1;
    return a.title.localeCompare(b.title);
  });
}

type HintLike = {
  id: string;
  title: string;
  severity: string;
  detail: string;
  steps?: string[];
  relatedMods?: string[];
  fix?: FixAction | null;
  fixes?: FixAction[];
};

type FindingLike = {
  code?: string;
  title?: string;
  description?: string;
  severity?: string;
  autoFix?: string;
  fixes?: FixAction[];
};

type GraphDiagLike = {
  severity: string;
  code: string;
  message: string;
  relatedNodes?: unknown[];
};

type JarWrong = { fileName: string; reason?: string };
type JarDup = { modId: string; files?: string[]; jars?: { fileName?: string }[] };

export type BuildProblemsInput = {
  hints?: HintLike[];
  findings?: FindingLike[];
  graphDiagnostics?: GraphDiagLike[];
  wrongLoaderJars?: JarWrong[];
  duplicateJarGroups?: JarDup[];
  memoryHint?: string | null;
  worldCoords?: { x: number; y: number; z: number; label: string } | null;
  aiAnalysis?: {
    humanExplanation?: string;
    confidence?: number;
    actions?: any[];
    recommended_actions?: any[];
    recommendedActions?: any[];
  } | null;
};

function actionsFromHint(h: HintLike): FixAction[] {
  if (h.fixes?.length) return h.fixes;
  if (h.fix) return [h.fix];
  return [];
}

function graphActions(g: GraphDiagLike): FixAction[] {
  const code = String(g.code ?? "").toUpperCase();
  const msg = g.message ?? "";
  if (code.includes("MISSING")) {
    const m = msg.match(/mod:([a-z0-9_-]+)/i);
    const id = m?.[1] ?? null;
    return [
      {
        kind: "installDependency",
        label: id ? `Install ${id}` : "Install missing dependency",
        modId: id,
      },
      { kind: "openResolve", label: "Open Resolve", modId: null },
    ];
  }
  if (code.includes("CONFLICT")) {
    return [
      { kind: "openResolve", label: "Open Resolve", modId: null },
    ];
  }
  if (code.includes("DUPLICATE")) {
    return [{ kind: "openResolve", label: "Review duplicates", modId: null }];
  }
  return [{ kind: "openResolve", label: "Open Resolve", modId: null }];
}

function aiPlanActions(analysis: BuildProblemsInput["aiAnalysis"]): any[] {
  if (!analysis) return [];
  return analysis.actions ?? analysis.recommended_actions ?? analysis.recommendedActions ?? [];
}

function aiActionToFix(action: any, idx: number): FixAction {
  const op = String(action?.op ?? action?.action_type ?? action?.actionType ?? "").toLowerCase();
  const modId = action?.mod_id ?? action?.modId ?? action?.id ?? null;
  const labelMap: Record<string, string> = {
    install_mod: "Install",
    install: "Install",
    remove_mod: "Remove",
    remove: "Remove",
    disable_mod: "Disable",
    disable: "Disable",
    update_mod: "Update",
    update: "Update",
    reinstall_mod: "Reinstall",
    reinstall: "Reinstall",
    edit_config: "Edit config",
  };
  const verb = labelMap[op] ?? (op || "Apply");
  const name = String(action?.name ?? action?.mod_name ?? modId ?? "").trim();
  return {
    kind: op.includes("remove")
      ? "removeMod"
      : op.includes("disable")
        ? "disableMod"
        : op.includes("update")
          ? "updateMod"
          : op.includes("install")
            ? "installDependency"
            : "aiPlanAction",
    label: name ? `${verb} ${name}` : verb,
    modId: modId ? String(modId) : null,
  };
}

/** Build a unified, sorted, deduped problem list for the Health UI. */
export function buildUnifiedProblems(input: BuildProblemsInput): Problem[] {
  const rows: Problem[] = [];

  for (const h of input.hints ?? []) {
    if (!h?.id) continue;
    const actions = actionsFromHint(h);
    rows.push({
      id: `hint:${h.id}`,
      severity: normalizeSeverity(h.severity),
      category: categoryForCode(h.id, "hint"),
      title: h.title || "Issue",
      summary: h.detail || "",
      source: "hint",
      code: h.id,
      modIds: [...(h.relatedMods ?? [])],
      actions,
      risk: riskForActions(actions),
      steps: h.steps?.length ? h.steps : undefined,
      layer: "crash",
    });
  }

  for (const f of input.findings ?? []) {
    const code = String(f.code ?? f.title ?? "finding");
    const actions = f.fixes?.length ? f.fixes : [];
    const steps = f.autoFix ? [f.autoFix] : undefined;
    rows.push({
      id: `rules:${code}`,
      severity: normalizeSeverity(f.severity),
      category: categoryForCode(code, "rules"),
      title: f.title || humanizeDiagnosticCode(code),
      summary: f.description || "",
      source: "rules",
      code,
      modIds: [],
      actions,
      risk: riskForActions(actions),
      steps,
      layer: "crash",
    });
  }

  const missingGraph: GraphDiagLike[] = [];
  const otherGraph: GraphDiagLike[] = [];
  for (const g of input.graphDiagnostics ?? []) {
    if (String(g.code ?? "").toUpperCase().includes("MISSING")) missingGraph.push(g);
    else otherGraph.push(g);
  }

  if (missingGraph.length > 1) {
    const actions: FixAction[] = [
      { kind: "installAllMissing", label: `Install ${missingGraph.length} missing`, modId: null },
      { kind: "openResolve", label: "Open Resolve", modId: null },
    ];
    rows.push({
      id: "graph:missing-batch",
      severity: "error",
      category: "dependency",
      title: `${missingGraph.length} missing dependencies`,
      summary: missingGraph.map((g) => g.message).slice(0, 3).join(" · "),
      source: "graph",
      code: "MISSING_DEPENDENCY",
      modIds: [],
      actions,
      risk: "safe",
      layer: "pack",
    });
  } else {
    for (const g of missingGraph) {
      const actions = graphActions(g);
      rows.push({
        id: `graph:${g.code}:${g.message}`,
        severity: normalizeSeverity(g.severity),
        category: "dependency",
        title: humanizeDiagnosticCode(g.code),
        summary: g.message,
        source: "graph",
        code: g.code,
        modIds: [],
        actions,
        risk: riskForActions(actions),
        layer: "pack",
      });
    }
  }

  for (const g of otherGraph) {
    const actions = graphActions(g);
    rows.push({
      id: `graph:${g.code}:${g.message}`,
      severity: normalizeSeverity(g.severity),
      category: categoryForCode(g.code, "graph"),
      title: humanizeDiagnosticCode(g.code),
      summary: g.message,
      source: "graph",
      code: g.code,
      modIds: [],
      actions,
      risk: riskForActions(actions),
      layer: "pack",
    });
  }

  for (const j of input.wrongLoaderJars ?? []) {
    rows.push({
      id: `jar:wrong:${j.fileName}`,
      severity: "error",
      category: "conflict",
      title: "Wrong loader jar",
      summary: j.reason || `${j.fileName} does not match this pack's loader.`,
      source: "jar",
      code: "WRONG_LOADER",
      modIds: [],
      actions: [{ kind: "removeWrongJar", label: `Remove ${j.fileName}`, modId: j.fileName }],
      risk: "destructive",
      layer: "pack",
    });
  }

  for (const g of input.duplicateJarGroups ?? []) {
    const fileCount = g.files?.length ?? g.jars?.length ?? 0;
    rows.push({
      id: `jar:dup:${g.modId}`,
      severity: "error",
      category: "conflict",
      title: "Duplicate mod jars",
      summary: `${g.modId}: ${fileCount} jars — keep one.`,
      source: "jar",
      code: "DUPLICATE_MOD",
      modIds: [g.modId],
      actions: [{ kind: "openResolve", label: "Review duplicates", modId: g.modId }],
      risk: "caution",
      layer: "pack",
    });
  }

  if (input.memoryHint) {
    rows.push({
      id: "runtime:memory",
      severity: "critical",
      category: "runtime",
      title: "Out of memory",
      summary: input.memoryHint,
      source: "hint",
      code: "OUT_OF_MEMORY",
      modIds: [],
      actions: [
        { kind: "raiseMemory", label: "Raise RAM", modId: null },
        { kind: "openSetup", label: "Open Setup", modId: null },
      ],
      risk: "safe",
      layer: "crash",
    });
  }

  if (input.worldCoords) {
    const c = input.worldCoords;
    rows.push({
      id: "runtime:world-coords",
      severity: "warning",
      category: "runtime",
      title: `${c.label} coordinates`,
      summary: `Crash near ${c.x}, ${c.y}, ${c.z}. Restore nearby chunks or teleport away if a ticking entity is stuck.`,
      source: "hint",
      code: "TICKING_ENTITY",
      modIds: [],
      actions: [{ kind: "openEvidence", label: "Open Evidence", modId: null }],
      risk: "safe",
      layer: "crash",
    });
  }

  const aiActs = aiPlanActions(input.aiAnalysis);
  if (aiActs.length > 0 && input.aiAnalysis) {
    const fixes = aiActs.slice(0, 6).map((a, i) => aiActionToFix(a, i));
    rows.push({
      id: "ai:plan",
      severity: "error",
      category: "crash",
      title: "Suggested fix plan",
      summary:
        input.aiAnalysis.humanExplanation?.slice(0, 280) ||
        `${aiActs.length} step(s) suggested — review before applying.`,
      source: "ai",
      code: "AI_PLAN",
      modIds: [],
      actions: [
        { kind: "reviewAiPlan", label: "Review & apply plan", modId: null },
        ...fixes.slice(0, 2),
      ],
      risk: "caution",
      layer: "crash",
    });
  }

  return mergeProblems(rows);
}

export function countBySeverity(problems: Problem[]): {
  critical: number;
  error: number;
  warning: number;
  info: number;
} {
  const out = { critical: 0, error: 0, warning: 0, info: 0 };
  for (const p of problems) out[p.severity]++;
  return out;
}

export function hasBlockingProblems(problems: Problem[]): boolean {
  return problems.some((p) => p.severity === "critical" || p.severity === "error");
}

export function crashProblems(problems: Problem[]): Problem[] {
  return problems.filter((p) => p.layer === "crash");
}

export function packProblems(problems: Problem[]): Problem[] {
  return problems.filter((p) => p.layer === "pack");
}
