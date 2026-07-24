<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { launchWithFeedback } from "../lib/launch";
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import {
    MessageCircle,
    Search,
    Stethoscope,
    Play,
    CheckCircle,
    FolderOpen,
    ArrowUpCircle,
    RefreshCw,
    AlertCircle,
    AlertTriangle,
    Info,
    Lightbulb,
    ListChecks,
    FileText,
    Terminal,
    History,
    Wrench,
    Bug,
    Download,
    Trash2,
    GitMerge,
    Database,
    Zap,
    Copy,
    ChevronDown,
    BadgeCheck,
    CircleHelp,
    Ban,
    Bot,
    BookMarked,
    Share2,
    Maximize2,
    ArrowDownToLine,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import { shareCrashLogWithFeedback } from "../lib/mclogs";
  import EmptyState from "./EmptyState.svelte";
  import AiConnectionModal from "./AiConnectionModal.svelte";
  import { open as openShell } from "@tauri-apps/plugin-shell";

  type Diagnostic = {
    severity: string;
    code: string;
    message: string;
    relatedNodes: any[];
  };

  type Snapshot = {
    id: string;
    name: string;
    createdAt: string;
    reason: string;
    changedFiles?: string[];
  };

  type Evidence = {
    source: string;
    lineNumber: number;
    kind: string;
    text: string;
  };

  type SuspectedMod = {
    id: string;
    name: string;
    version?: string | null;
    fileName?: string | null;
    knownInManifest: boolean;
    confidence: number;
    evidence: Evidence[];
    authors?: string[];
    blameRole?: "primary" | "secondary" | "related" | string;
    matchSources?: string[];
  };

  type CrashReportSummary = {
    id: string;
    name: string;
    path: string;
    size: number;
    modified?: number | null;
  };

  type CrashReportAnalysis = {
    summary: CrashReportSummary;
    content: string;
    sections?: { title: string; startLine: number; endLine: number; preview: string }[];
    modEntries?: { id: string; name?: string | null; version?: string | null; raw: string }[];
    signals: Evidence[];
    suspectedMods: SuspectedMod[];
  };

  type LatestLogAnalysis = {
    path: string;
    exists: boolean;
    tail: string;
    signals: Evidence[];
    suspectedMods: SuspectedMod[];
    hints: DiagnosisHint[];
  };

  type DiagnosisHint = {
    id: string;
    title: string;
    severity: string;
    detail: string;
    steps: string[];
    relatedMods: string[];
    fix: FixAction | null;
    fixes: FixAction[];
  };

  type FixAction = {
    kind: string;
    label: string;
    modId: string | null;
  };

  type CrashDiagnosis = {
    reports: CrashReportSummary[];
    selectedReport?: CrashReportAnalysis | null;
    latestLog: LatestLogAnalysis;
    launcherLog: LatestLogAnalysis;
    suspectedMods: SuspectedMod[];
    hints: DiagnosisHint[];
    recentSnapshots: Snapshot[];
    graphDiagnostics: Diagnostic[];
    fixPlan: any;
    analysisSource?: string;
    crashReportStale?: boolean;
    sessionHealthy?: boolean;
  };

  let diagnosis: CrashDiagnosis | null = null;
  let selectedReportId = "";
  let preferLatestLog = true;
  /// Sentinel: force latest.log analysis (never auto-pick a crash file).
  const LATEST_LOG_SOURCE = "__latest_log__";
  let analysisBusy = false;
  /** Detail panel under the verdict: rules findings vs AI explanation. */
  let detailTab: "rules" | "ai" = "rules";
  let aiSoftError: string | null = null;
  let sharingLog = false;
  let loading = false;
  let planning = false;
  let applying = false;
  let applyingHintId: string | null = null;
  let launching = false;
  let fixingIdx: number | null = null;
  let disablingModId: string | null = null;
  let error: string | null = null;
  let message: string | null = null;
  let plan: any | null = null;
  let lastLoadedPath: string | null = null;

  function onSourceChange(e: Event) {
    const el = e.currentTarget;
    if (!(el instanceof HTMLSelectElement)) return;
    if (el.value === LATEST_LOG_SOURCE) chooseLatestLog();
    else chooseReport(el.value);
  }

  async function shareCurrentLog() {
    if (!$projectPath || sharingLog) return;
    sharingLog = true;
    try {
      const name = selectedReport?.summary?.name;
      await shareCrashLogWithFeedback($projectPath, preferLatestLog ? "latest.log" : name ?? null);
    } finally {
      sharingLog = false;
    }
  }

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && diagnosis) return;
    loading = true;
    error = null;
    const requestedLatest = preferLatestLog;
    try {
      const reportId = preferLatestLog ? LATEST_LOG_SOURCE : selectedReportId || null;
      const data: CrashDiagnosis = await invoke("get_crash_diagnosis", {
        path: $projectPath,
        reportId,
      });
      diagnosis = data;
      if (requestedLatest || data.analysisSource === "latest_log") {
        preferLatestLog = true;
        selectedReportId = "";
      } else {
        preferLatestLog = false;
        selectedReportId = data.selectedReport?.summary.id ?? selectedReportId;
      }
      plan = null;
      detectWrongLoaderMods();
      detectDuplicateModJars();
      if (data.sessionHealthy && preferLatestLog) {
        crashFindings = [];
        crashMcreator = [];
        crashClassFinder = [];
        aiAnalysis = null;
        aiContext = null;
        aiSoftError = null;
        void invoke("confirm_crash_resolution_from_diagnose", { path: $projectPath }).catch(() => {});
      } else {
        void runUnifiedAnalysis();
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function onProjectPathChange(path: string | null) {
    if (!path || path === lastLoadedPath) return;
    lastLoadedPath = path;
    preferLatestLog = true;
    selectedReportId = "";
    void load(true);
  }

  async function chooseReport(reportId: string) {
    preferLatestLog = false;
    selectedReportId = reportId;
    await load(true);
  }

  async function chooseLatestLog() {
    preferLatestLog = true;
    selectedReportId = "";
    await load(true);
  }

  function activeReportId(): string | null {
    return preferLatestLog ? LATEST_LOG_SOURCE : selectedReportId || null;
  }

  async function createFixPlan() {
    if (!$projectPath) return;
    planning = true;
    error = null;
    try {
      plan = await invoke("create_crash_fix_plan", {
        path: $projectPath,
        reportId: activeReportId(),
      });
    } catch (e) {
      error = String(e);
    } finally {
      planning = false;
    }
  }

  /// Per-diagnostic fix: install a missing mod dependency via Modrinth.
  async function fixMissingDependency(modId: string, idx: number) {
    if (!$projectPath) return;
    fixingIdx = idx;
    error = null;
    message = null;
    try {
      const summary: string = await invoke("apply_fix_action", {
        path: $projectPath,
        action: { kind: "installDependency", label: `Install ${modId}`, modId },
      });
      message = `${summary}. Reloading...`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      fixingIdx = null;
    }
  }

  /// Soft-disable a mod by renaming jar → `.jar.disabled` (keeps manifest entry).
  async function fixDisableMod(modId: string, idx: number | null = null) {
    if (!$projectPath) return;
    if (idx !== null) fixingIdx = idx;
    disablingModId = modId;
    error = null;
    message = null;
    try {
      const summary: string = await invoke("apply_fix_action", {
        path: $projectPath,
        action: { kind: "disableMod", label: `Disable ${modId}`, modId },
      });
      message = `${summary}. Rerun the Test profile to verify.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      fixingIdx = null;
      disablingModId = null;
    }
  }

  async function fixEnableMod(modId: string) {
    if (!$projectPath) return;
    disablingModId = modId;
    error = null;
    message = null;
    try {
      const result: any = await invoke("enable_project_mod", {
        path: $projectPath,
        modId,
      });
      message = `Re-enabled ${result?.name ?? modId}.`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      disablingModId = null;
    }
  }

  /// One-click update of the top suspect to the latest compatible version.
  async function applyTopSuspectUpdate() {
    if (!$projectPath || !topSuspect) return;
    fixingIdx = -1;
    error = null;
    message = null;
    try {
      const summary: string = await invoke("apply_fix_action", {
        path: $projectPath,
        action: { kind: "updateMod", label: `Update ${topSuspect.name}`, modId: topSuspect.id },
      });
      message = summary || `Updated ${topSuspect.name}`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      fixingIdx = null;
    }
  }

  /// Per-diagnostic fix: remove a conflicting mod from the project.
  async function fixRemoveMod(modId: string, idx: number) {
    if (!$projectPath) return;
    fixingIdx = idx;
    error = null;
    message = null;
    try {
      const summary: string = await invoke("apply_fix_action", {
        path: $projectPath,
        action: { kind: "removeMod", label: `Remove ${modId}`, modId },
      });
      message = `${summary}. Reloading...`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      fixingIdx = null;
    }
  }

  /// Per-diagnostic fix: keep newest jar for this mod id when disk duplicates exist.
  async function fixDeduplicate(idx: number) {
    fixingIdx = idx;
    const d = graphDiagnostics[idx];
    const fromMsg = (d?.message ?? "").match(/Duplicate mod node:\s*(.+)$/i)?.[1]?.trim();
    const group =
      duplicateJarGroups.find((g) => g.modId === fromMsg) ??
      (fromMsg
        ? duplicateJarGroups.find((g) => g.modId.toLowerCase() === fromMsg.toLowerCase())
        : undefined);
    try {
      if (group?.keepCandidate) {
        await keepOneDuplicateJar(group.modId, group.keepCandidate);
      } else {
        await detectDuplicateModJars();
        message = duplicateJarGroups.length
          ? "Duplicate jars listed under Conflicts & jars — pick which jar to keep."
          : "No duplicate jars on disk for this graph warning.";
      }
    } finally {
      fixingIdx = null;
    }
  }

  // --- Wrong-loader jar detection ---
  type WrongLoaderJar = {
    fileName: string;
    detectedLoader: string;
    projectLoader: string;
    recommendation: string;
    reason: string;
  };
  let wrongLoaderJars: WrongLoaderJar[] = [];
  let wrongLoaderLoading = false;
  let wrongLoaderFixing: string | null = null;

  type DupJar = {
    fileName: string;
    modId: string;
    mtimeMs: number;
    size: number;
    inManifest: boolean;
  };
  type DupJarGroup = { modId: string; keepCandidate: string; jars: DupJar[] };
  let duplicateJarGroups: DupJarGroup[] = [];
  let duplicateJarLoading = false;
  let duplicateJarFixing: string | null = null;

  // Ore generation scanner state
  let oreFindings: any[] = [];
  let oreLoading = false;

  // Duplicate items / unification state
  let duplicateFindings: any[] = [];
  let duplicateLoading = false;
  let unifyConfigResult: any = null;
  let unifyLoading = false;

  // Crash Assistant state
  let crashLoading = false;
  let crashFindings: any[] = [];
  let crashMcreator: string[] = [];
  let crashClassFinder: any[] = [];

  async function runCrashAssistant() {
    if (!$projectPath) return;
    crashLoading = true;
    try {
      const result: any = await invoke("run_crash_assistant_full", {
        path: $projectPath,
        reportId: activeReportId(),
      });
      crashFindings = result.findings ?? [];
      crashMcreator = result.mcreatorMods ?? [];
      crashClassFinder = result.classFinderResults ?? [];
      enrichCrashFindingsWithAi();
    } catch (e) {
      error = String(e);
    } finally {
      crashLoading = false;
    }
  }

  /** Crash Assistant first, then AI — equal analysis cards. */
  async function runUnifiedAnalysis() {
    if (!$projectPath || analysisBusy) return;
    analysisBusy = true;
    aiSoftError = null;
    try {
      await runCrashAssistant();
      try {
        await runAiExplain({ quiet: true });
      } catch (aiErr) {
        aiSoftError = String(aiErr);
        console.warn("[Diagnose] AI explain soft-fail:", aiErr);
      }
    } finally {
      analysisBusy = false;
    }
  }

  function enrichCrashFindingsWithAi() {
    if (!aiAnalysis || !crashFindings.length) return;
    const actions = aiPlanActions(aiAnalysis);
    const suspected = new Set(
      (aiAnalysis.suspectedMods ?? aiAnalysis.suspected_mods ?? []).map((m: string) =>
        String(m).toLowerCase(),
      ),
    );
    crashFindings = crashFindings.map((f: any) => {
      const fixIds = (f.fixes ?? [])
        .map((x: any) => String(x.modId ?? "").toLowerCase())
        .filter(Boolean);
      const blob = `${f.title ?? ""} ${f.description ?? ""} ${f.code ?? ""}`.toLowerCase();
      const matched = actions.find((a: any) => {
        const mid = String(a.modId ?? a.mod_id ?? "").toLowerCase();
        if (!mid) return false;
        return fixIds.includes(mid) || blob.includes(mid) || suspected.has(mid);
      });
      if (!matched && !fixIds.some((id: string) => suspected.has(id))) {
        return { ...f, aiAgree: false, aiHint: null };
      }
      return {
        ...f,
        aiAgree: true,
        aiHint:
          matched?.reason ??
          matched?.description ??
          (aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation ?? null),
      };
    });
  }

  // AI context state
  let aiLoading = false;
  let aiContext: any = null;
  let aiPrompt: string = "";
  let aiShowPrompt = false;
  let aiAnalysis: any = null;
  let aiFeedbackBusy = false;
  let aiFeedbackMsg: string | null = null;
  let aiModalOpen = false;
  let aiApplyBusy = false;
  let pendingPlan: any = null;
  let pendingBusy = false;
  let swarmEnabled = false;

  // Author KB case form (pack author — crash + resolution)
  let authorOpen = false;
  let authorBusy = false;
  let authorMsg: string | null = null;
  let authorId = "";
  let authorSolution = "";
  let authorSymptoms = "";
  let authorSuspected = "";
  let authorNotes = "";
  let authorActionsJson = "[]";
  let authorFingerprint: any = null;
  let authorCases: any[] = [];
  let authorExportPreview = "";

  async function runAiExplain(opts: { quiet?: boolean } = {}) {
    if (!$projectPath) return;
    aiLoading = true;
    if (!opts.quiet) error = null;
    aiSoftError = null;
    aiFeedbackMsg = null;
    try {
      try {
        const prep = await invoke<{ ok?: boolean; model?: string; skipped?: boolean }>(
          "ensure_ollama_model",
        );
        if (!opts.quiet) {
          if (prep?.model) message = `AI ready (${prep.model}). Analyzing crash…`;
          else message = "Preparing local AI…";
        }
      } catch (prepErr) {
        console.warn("[AI] ensure_ollama_model:", prepErr);
      }
      const reportId = activeReportId();
      const context: any = await invoke("build_ai_crash_context", {
        path: $projectPath,
        reportId,
      });
      aiContext = context;
      aiPrompt = context.prompt ?? "";
      aiShowPrompt = false;
      aiAnalysis = await invoke("analyze_crash_with_ai", {
        path: $projectPath,
        reportId,
      });
      swarmEnabled = !!aiAnalysis?.swarmEnabled;
      enrichCrashFindingsWithAi();
      await loadPendingPlan();
      if (!opts.quiet) {
        const similar = context.similarCaseCount ?? 0;
        const model = context.aiModel ?? aiAnalysis?.model ?? "AI";
        message = `AI analysis ready (${model}${similar ? `, ${similar} KB hit(s)` : ""}). Review before applying.`;
      }
    } catch (e) {
      const msg = String(e);
      aiAnalysis = null;
      if (opts.quiet) {
        aiSoftError = msg;
        throw e;
      }
      if (/not installed|Install model|no model|Settings → AI/i.test(msg)) {
        error = `${msg} Open Settings → Integrations → Configure AI to install a model.`;
        aiModalOpen = true;
      } else if (/model.*(not found)|pull|download/i.test(msg)) {
        error = `Local AI model missing: ${msg}`;
      } else if (/ollama|connection refused|failed to fetch|tcp|unreachable/i.test(msg)) {
        error = `Ollama unavailable — install from https://ollama.com, set the path in Settings → AI, then install a model there. ${msg}`;
      } else {
        error = msg;
      }
    } finally {
      aiLoading = false;
    }
  }

  async function sendAiFeedback(helped: boolean) {
    if (!$projectPath || !aiAnalysis) return;
    aiFeedbackBusy = true;
    aiFeedbackMsg = null;
    try {
      const path = await invoke<string>("record_crash_ai_feedback", {
        path: $projectPath,
        feedback: {
          helped,
          fingerprintKey: aiAnalysis.fingerprintKey ?? aiContext?.fingerprintKey ?? null,
          humanExplanation: aiAnalysis.human_explanation ?? aiAnalysis.humanExplanation ?? null,
          suspectedMods: aiAnalysis.suspected_mods ?? aiAnalysis.suspectedMods ?? [],
          recommendedActions: aiAnalysis.recommended_actions ?? aiAnalysis.recommendedActions ?? [],
          reportId: activeReportId(),
        },
      });
      aiFeedbackMsg = helped
        ? `Thanks — saved to knowledge base (${path}).`
        : `Marked as unhelpful — recorded in KB (${path}).`;
    } catch (e) {
      error = String(e);
    } finally {
      aiFeedbackBusy = false;
    }
  }

  function aiPlanActions(analysis: any): any[] {
    return analysis?.actions ?? analysis?.recommended_actions ?? analysis?.recommendedActions ?? [];
  }

  function aiActionLabel(action: any): string {
    const op = String(action?.op ?? action?.action_type ?? action?.actionType ?? "").toLowerCase();
    switch (op) {
      case "install_mod":
      case "install":
        return "Install";
      case "remove_mod":
      case "remove":
        return "Remove";
      case "disable_mod":
      case "disable":
        return "Disable";
      case "update_mod":
      case "update":
        return "Update";
      case "change_mod_version":
        return "Change version";
      case "reinstall_mod":
      case "reinstall":
        return "Reinstall";
      case "edit_config":
      case "config_change":
        return "Edit config";
      default:
        return op || "Action";
    }
  }

  function aiActionVersion(action: any): string | null {
    const v = String(action?.version ?? "").trim();
    if (!v) return null;
    const fake = new Set(["1.2.3", "0.0.0", "x.y.z", "latest", "version", "unknown", "null", "string"]);
    if (fake.has(v.toLowerCase()) || v === "X.Y.Z" || v === "<version>" || v === "{{version}}") return null;
    return v;
  }

  type MergedRec = {
    id: string;
    source: "rules" | "ai";
    label: string;
    detail: string;
    risk: string;
    modId: string | null;
    apply: () => void;
  };

  $: mergedRecommendations = buildMergedRecommendations(crashFindings, aiAnalysis);
  $: primaryRec = mergedRecommendations[0] ?? null;
  $: sessionOk = !!(diagnosis?.sessionHealthy && preferLatestLog);

  function buildMergedRecommendations(findings: any[], analysis: any): MergedRec[] {
    const out: MergedRec[] = [];
    const seen = new Set<string>();
    for (const f of findings ?? []) {
      for (const fix of f.fixes ?? []) {
        const key = `rules:${fix.kind}:${fix.modId ?? fix.label}`;
        if (seen.has(key)) continue;
        seen.add(key);
        out.push({
          id: key,
          source: "rules",
          label: fix.label ?? fix.kind,
          detail: f.aiHint ? `${f.title} — AI: ${f.aiHint}` : f.title ?? f.code ?? "",
          risk: f.severity === "error" || f.severity === "critical" ? "high" : "medium",
          modId: fix.modId ?? null,
          apply: () => void applyCrashFindingFix(f, fix),
        });
      }
    }
    for (const a of aiPlanActions(analysis)) {
      const mid = a.modId ?? a.mod_id ?? null;
      const op = a.op ?? a.action_type ?? a.actionType ?? "action";
      const key = `ai:${op}:${mid ?? a.reason ?? ""}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({
        id: key,
        source: "ai",
        label: `${aiActionLabel(a)}${mid ? ` ${mid}` : ""}`,
        detail: a.reason ?? a.description ?? "",
        risk: a.risk ?? "medium",
        modId: mid,
        apply: () => void applyAiPlan(),
      });
    }
    return out.slice(0, 12);
  }

  async function applyAiPlan() {
    if (!$projectPath || !aiAnalysis) return;
    const actions = aiPlanActions(aiAnalysis);
    if (!actions.length) {
      error = "No actions in the AI plan to apply.";
      return;
    }
    if (aiAnalysis.validation && aiAnalysis.validation.ok === false) {
      error = `Plan invalid: ${(aiAnalysis.validation.errors ?? []).join("; ")}`;
      return;
    }
    const ok = confirm(
      `Apply ${actions.length} action(s) from the AI/KB plan?\nA snapshot will be created first.`,
    );
    if (!ok) return;
    aiApplyBusy = true;
    error = null;
    try {
      const plan = {
        schemaVersion: aiAnalysis.schemaVersion ?? 1,
        humanExplanation: aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation ?? "",
        confidence: aiAnalysis.confidence ?? 0.5,
        suspectedMods: aiAnalysis.suspectedMods ?? aiAnalysis.suspected_mods ?? [],
        needsUserReview: aiAnalysis.needsUserReview ?? aiAnalysis.needs_user_review ?? true,
        source: aiAnalysis.source ?? null,
        matchedCaseIds: aiAnalysis.matchedCaseIds ?? [],
        actions: (aiAnalysis.actions ?? []).map((a: any) => ({
          op: a.op ?? a.action_type ?? a.actionType,
          modId: a.modId ?? a.mod_id ?? null,
          provider: a.provider ?? null,
          projectId: a.projectId ?? a.project_id ?? null,
          version: a.version ?? null,
          path: a.path ?? null,
          patchType: a.patchType ?? a.patch_type ?? null,
          patch: a.patch ?? null,
          reason: a.reason ?? a.description ?? null,
          risk: a.risk ?? "medium",
        })),
        additionalContext: aiAnalysis.additionalContext ?? aiAnalysis.additional_context ?? null,
      };
      // If only legacy recommended_actions exist, map them.
      if (!plan.actions.length) {
        plan.actions = (aiAnalysis.recommended_actions ?? aiAnalysis.recommendedActions ?? []).map(
          (a: any) => ({
            op: a.action_type ?? a.actionType ?? "unknown",
            modId: a.mod_id ?? a.modId ?? null,
            provider: null,
            projectId: null,
            version: null,
            path: null,
            patchType: null,
            patch: null,
            reason: a.description ?? null,
            risk: a.risk ?? "medium",
          }),
        );
      }
      const result: any = await invoke("apply_action_plan", {
        path: $projectPath,
        plan,
        fingerprintKey: aiAnalysis.fingerprintKey ?? aiContext?.fingerprintKey ?? null,
      });
      const applied = result?.applied ?? [];
      const errs = result?.errors ?? [];
      message = `Applied ${applied.length} action(s).${errs.length ? ` Errors: ${errs.join("; ")}` : ""}`;
      if (errs.length) error = errs.join("; ");
      await load(true);
      // Prefill author form from the plan that just worked.
      await openAuthorForm({ fromAnalysis: true });
    } catch (e) {
      error = String(e);
    } finally {
      aiApplyBusy = false;
    }
  }

  async function loadPendingPlan() {
    pendingPlan = null;
    if (!$projectPath) return;
    try {
      const swarm = await invoke<{ enabled?: boolean }>("get_swarm_settings");
      swarmEnabled = !!swarm?.enabled;
      if (!swarmEnabled) return;
      pendingPlan = await invoke("get_pending_action_plan", { path: $projectPath });
    } catch {
      pendingPlan = null;
    }
  }

  async function applyPendingNetworkFix() {
    if (!$projectPath || !pendingPlan) return;
    if (!swarmEnabled) {
      error = "Enable TuffSwarm network in Settings to apply network fixes.";
      return;
    }
    const actions = pendingPlan.actions ?? [];
    if (!actions.length) {
      error = "Pending network plan has no actions.";
      return;
    }
    const ok = confirm(
      `Apply network fix with ${actions.length} action(s)?\nA snapshot will be created first. Nothing runs without this confirm.`,
    );
    if (!ok) return;
    pendingBusy = true;
    error = null;
    try {
      const result: any = await invoke("apply_action_plan", {
        path: $projectPath,
        plan: pendingPlan,
        fingerprintKey: pendingPlan.matchedCaseIds?.[0] ?? null,
      });
      const applied = result?.applied ?? [];
      const errs = result?.errors ?? [];
      message = `Network fix applied (${applied.length}).${errs.length ? ` Errors: ${errs.join("; ")}` : ""}`;
      if (errs.length) error = errs.join("; ");
      await invoke("clear_pending_network_plan", { path: $projectPath });
      pendingPlan = null;
    } catch (e) {
      error = String(e);
    } finally {
      pendingBusy = false;
    }
  }

  function parseAuthorActions(): any[] {
    try {
      const parsed = JSON.parse(authorActionsJson || "[]");
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      throw new Error("Actions JSON is invalid — expect an array of {op, modId, …}");
    }
  }

  async function refreshAuthorCases() {
    if (!$projectPath) return;
    try {
      authorCases = await invoke("list_authored_crash_cases", { path: $projectPath });
    } catch {
      authorCases = [];
    }
  }

  async function openAuthorForm(opts?: { fromAnalysis?: boolean }) {
    if (!$projectPath) return;
    authorOpen = true;
    authorMsg = null;
    authorExportPreview = "";
    authorBusy = true;
    try {
      const draft: any = await invoke("draft_authored_crash_case", {
        path: $projectPath,
        reportId: activeReportId(),
      });
      authorFingerprint = draft.fingerprint;
      authorSymptoms = (draft.symptoms ?? []).join("\n");
      if (opts?.fromAnalysis && aiAnalysis) {
        authorSolution =
          aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation ?? authorSolution;
        authorSuspected = (
          aiAnalysis.suspectedMods ??
          aiAnalysis.suspected_mods ??
          []
        ).join(", ");
        const actions = aiPlanActions(aiAnalysis).map((a: any) => ({
          op: a.op ?? a.action_type ?? a.actionType,
          modId: a.modId ?? a.mod_id ?? null,
          provider: a.provider ?? null,
          projectId: a.projectId ?? a.project_id ?? null,
          version: a.version ?? null,
          path: a.path ?? null,
          patchType: a.patchType ?? a.patch_type ?? null,
          patch: a.patch ?? null,
          reason: a.reason ?? a.description ?? null,
          risk: a.risk ?? "medium",
        }));
        authorActionsJson = JSON.stringify(actions, null, 2);
        if (!authorId) {
          const ex = (draft.fingerprint?.exception ?? "case")
            .replace(/[^a-zA-Z0-9-]+/g, "-")
            .slice(0, 40)
            .toLowerCase();
          authorId = `authored-${ex || "case"}`;
        }
      } else if (!authorActionsJson || authorActionsJson === "[]") {
        authorActionsJson = JSON.stringify(
          [
            {
              op: "disable_mod",
              modId: "examplemod",
              reason: "Describe the fix",
              risk: "low",
            },
          ],
          null,
          2,
        );
      }
      await refreshAuthorCases();
    } catch (e) {
      error = String(e);
    } finally {
      authorBusy = false;
    }
  }

  async function saveAuthorCase() {
    if (!$projectPath || !authorFingerprint) return;
    authorBusy = true;
    authorMsg = null;
    error = null;
    try {
      const launcherActions = parseAuthorActions();
      const result: any = await invoke("save_authored_crash_case", {
        path: $projectPath,
        input: {
          id: authorId.trim() || null,
          fingerprint: authorFingerprint,
          solution: authorSolution.trim(),
          symptoms: authorSymptoms
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean),
          suspectedMods: authorSuspected
            .split(/[,;\n]/)
            .map((s) => s.trim())
            .filter(Boolean),
          launcherActions,
          actions: [],
          notes: authorNotes.trim() || null,
        },
      });
      authorMsg = `Saved «${result.caseId}» → KB + export ${result.exportPath}`;
      authorExportPreview = JSON.stringify(
        {
          id: result.case?.id,
          fingerprint: result.case?.fingerprint,
          solution: result.case?.solution,
          actions: result.case?.launcherActions ?? result.case?.launcher_actions,
        },
        null,
        2,
      );
      await refreshAuthorCases();
      message = `KB case saved: ${result.caseId}`;
    } catch (e) {
      error = String(e);
    } finally {
      authorBusy = false;
    }
  }

  async function copyAuthorExport(caseId?: string) {
    if (!$projectPath) return;
    try {
      let text = authorExportPreview;
      if (caseId) {
        text = await invoke<string>("get_authored_case_export", {
          path: $projectPath,
          caseId,
        });
      }
      if (!text) throw new Error("Nothing to copy");
      await navigator.clipboard.writeText(text);
      authorMsg = "Export JSON copied (notes stripped — safe for remote KB).";
    } catch (e) {
      error = String(e);
    }
  }

  async function openAuthorExportFolder() {
    if (!$projectPath) return;
    try {
      await invoke("open_authored_kb_folder", { path: $projectPath });
    } catch (e) {
      error = String(e);
    }
  }

  async function copyAiPrompt() {
    try { await navigator.clipboard.writeText(aiPrompt); message = "AI prompt copied."; }
    catch { message = "Failed to copy prompt."; }
  }

  async function scanOreGen() {
    if (!$projectPath) return;
    oreLoading = true;
    try {
      oreFindings = await invoke("scan_ore_generation", { path: $projectPath });
    } catch (e) {
      error = String(e);
    } finally {
      oreLoading = false;
    }
  }

  async function scanDuplicateItems() {
    if (!$projectPath) return;
    duplicateLoading = true;
    try {
      duplicateFindings = await invoke("detect_duplicate_items", { path: $projectPath });
    } catch (e) {
      error = String(e);
    } finally {
      duplicateLoading = false;
    }
  }

  async function generateUnify() {
    if (!$projectPath) return;
    unifyLoading = true;
    try {
      unifyConfigResult = await invoke("generate_unify_config", { path: $projectPath, save: true });
      message = `Unify config saved with ${unifyConfigResult.materials?.length ?? 0} materials.`;
    } catch (e) {
      error = String(e);
    } finally {
      unifyLoading = false;
    }
  }

  async function detectWrongLoaderMods() {
    if (!$projectPath) return;
    wrongLoaderLoading = true;
    try {
      wrongLoaderJars = await invoke("detect_wrong_loader_mods", { path: $projectPath });
    } catch {
      wrongLoaderJars = [];
    } finally {
      wrongLoaderLoading = false;
    }
  }

  async function detectDuplicateModJars() {
    if (!$projectPath) return;
    duplicateJarLoading = true;
    try {
      duplicateJarGroups = await invoke("detect_duplicate_mod_jars", { path: $projectPath });
    } catch {
      duplicateJarGroups = [];
    } finally {
      duplicateJarLoading = false;
    }
  }

  async function keepOneDuplicateJar(modId: string, keepFileName: string) {
    if (!$projectPath) return;
    duplicateJarFixing = `${modId}::${keepFileName}`;
    error = null;
    try {
      const result: string = await invoke("keep_one_duplicate_mod_jar", {
        path: $projectPath,
        modId,
        keepFileName,
      });
      message = result;
      await detectDuplicateModJars();
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      duplicateJarFixing = null;
    }
  }

  async function disableWrongJar(fileName: string) {
    if (!$projectPath) return;
    wrongLoaderFixing = fileName;
    error = null;
    try {
      const result: string = await invoke("disable_wrong_loader_jar", { path: $projectPath, fileName });
      message = result;
      await detectWrongLoaderMods();
    } catch (e) {
      error = String(e);
    } finally {
      wrongLoaderFixing = null;
    }
  }

  async function removeWrongJar(fileName: string) {
    if (!$projectPath) return;
    wrongLoaderFixing = fileName;
    error = null;
    try {
      const result: string = await invoke("remove_loose_jar", { path: $projectPath, fileName });
      message = result;
      await detectWrongLoaderMods();
    } catch (e) {
      error = String(e);
    } finally {
      wrongLoaderFixing = null;
    }
  }

  function icon(severity: string) {
    if (severity === "Error") return AlertCircle;
    if (severity === "Warning") return AlertTriangle;
    return Info;
  }

  function formatBytes(size: number) {
    if (size < 1024) return `${size} B`;
    if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
    return `${(size / 1024 / 1024).toFixed(1)} MB`;
  }

  function formatDate(seconds?: number | null) {
    if (!seconds) return "unknown time";
    return new Date(seconds * 1000).toLocaleString();
  }

  function actionLabel(action: any) {
    if (!action || typeof action !== "object") return String(action);
    const [kind, payload] = Object.entries(action)[0] ?? ["Action", {}];
    return `${kind}: ${JSON.stringify(payload)}`;
  }

  function hypothesisForGroup(title: string) {
    if (title === "Entrypoint") return "Likely a mod initialization failure. Check the provided-by mod first, then its required libraries and loader-compatible version.";
    if (title === "Loader mismatch") return "Likely a wrong loader/API bridge or incompatible dependency version. Check Fabric/Forge/NeoForge API ports and update matching libraries.";
    if (title === "Render/OpenGL") return "Likely render pipeline conflict. Disable shaders and test render mods such as Sodium/Iris/Voxy/ETF/MCEF/Litematica in groups.";
    if (title === "Performance") return "Likely overload, not a crash root cause. Lower view distance, profile heavy entities/worldgen and rerun the test.";
    return "Review this signal group and compare it with recent snapshots.";
  }

  $: topFinding =
    [...(crashFindings ?? [])].sort((a, b) => {
      const rank = (s: string) =>
        s === "critical" ? 4 : s === "error" ? 3 : s === "warning" ? 2 : 1;
      return rank(String(b.severity ?? "")) - rank(String(a.severity ?? ""));
    })[0] ?? null;

  function severityChip(sev: string): string {
    if (sev === "critical") return "Fix this first";
    if (sev === "error") return "Needs a fix";
    if (sev === "warning") return "Worth checking";
    return "FYI";
  }

  $: selectedReport = diagnosis?.selectedReport ?? null;
  $: suspected = diagnosis?.suspectedMods ?? [];
  $: primarySuspects = suspected.filter((m) => m.blameRole === "primary");
  $: topSuspect = primarySuspects[0] ?? suspected[0] ?? null;
  $: heroCulpritLabel =
    primarySuspects.length > 1
      ? primarySuspects.map((m) => m.name).join(" + ")
      : topSuspect?.name ?? "";
  $: strongestEvidence = topSuspect?.evidence?.[0] ?? null;
  $: providedByEvidence = topSuspect?.evidence?.find((item) =>
    item.text.toLowerCase().includes("provided by"),
  ) ?? null;

  /// Actually applies the crash-diagnosis fix plan on the backend (snapshot
  /// + update/disable suspected mod / install missing dependency) and
  /// reports what really happened. Previously this only faked a success
  /// message in the UI without calling into the backend, so "fixing" a
  /// conflict did nothing to the project.
  async function applyFix() {
    if (!$projectPath) return;
    applying = true;
    error = null;
    message = null;
    try {
      const applied: string[] = await invoke("apply_crash_fix_plan", {
        path: $projectPath,
        reportId: activeReportId(),
      });
      message = applied.length
        ? `Applied: ${applied.join(", ")}`
        : "No deterministic action was available for this plan. Review the notes manually.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
  }

  /// Applies a machine-actionable fix from a diagnosis hint (raise memory,
  /// accept EULA, change port, update/reinstall/disable a mod, etc.).
  async function applyHintFix(hint: DiagnosisHint) {
    if (hint.fix) await applyHintFixAction(hint, hint.fix);
  }

  /// Applies a specific fix action (used for per-mod buttons on a hint).
  async function applyHintFixAction(hint: DiagnosisHint, action: FixAction) {
    if (!$projectPath) return;
    applyingHintId = hint.id;
    error = null;
    message = null;
    try {
      const summary: string = await invoke("apply_fix_action", {
        path: $projectPath,
        action,
      });
      message = summary || `Applied fix: ${hint.title}`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      applyingHintId = null;
    }
  }

  /// One-by-one fix from a Crash Assistant finding card.
  async function applyCrashFindingFix(finding: any, action: FixAction) {
    await applyHintFixAction(
      {
        id: `ca:${finding.code}`,
        title: finding.title,
        severity: finding.severity,
        detail: finding.description,
        steps: finding.autoFix ? [finding.autoFix] : [],
        relatedMods: [],
        fix: null,
        fixes: finding.fixes ?? [],
      },
      action,
    );
  }

  /// Launches the client (Test) profile so the user can reproduce a crash,
  /// then refreshes the diagnosis once it stops.
  async function runTest() {
    if (!$projectPath || launching) return;
    launching = true;
    error = null;
    message = "Launching Test profile — reproduce the crash, then come back.";
    const result = await launchWithFeedback(
      { path: $projectPath, profile: "client" },
      { onStarted: () => { message = "Test launch started. Re-run Diagnose after it crashes/closes."; } },
    );
    if (result) {
      message = "Test launch started. Re-run Diagnose after it crashes/closes.";
    }
    launching = false;
  }

  /// Opens the project folder in the OS file manager (quick access to
  /// crash-reports / logs without leaving Diagnose).
  async function openFolder() {
    if (!$projectPath) return;
    try {
      await invoke("open_project_folder", { path: $projectPath });
    } catch (e) {
      error = String(e);
    }
  }

  $: allHints = [
    ...(diagnosis?.hints ?? []),
    ...(diagnosis?.latestLog?.hints ?? []),
    ...(diagnosis?.launcherLog?.hints ?? []),
  ];
  $: dedupedHints = Array.from(
    new Map(allHints.filter((h) => h && h.id).map((h) => [h.id, h])).values()
  );

  // Per-line detection highlights for the open crash report: lineNumber -> kind.
  // Drives the inline signal marker so crashes are visible at a glance.
  $: signalLineMap = (() => {
    const m = new Map<number, string>();
    const signals = preferLatestLog
      ? (diagnosis?.latestLog?.signals ?? [])
      : (selectedReport?.signals ?? []);
    for (const s of signals) {
      if (s.lineNumber && s.lineNumber > 0) {
        const prev = m.get(s.lineNumber);
        m.set(s.lineNumber, prev ?? s.kind);
      }
    }
    return m;
  })();

  // --- Inline log search + syntax-colored viewer ---
  let logQuery = "";
  let logMatches: { line: number }[] = [];
  let activeMatch = 0;
  let logPreEl: HTMLElement | null = null;
  let logWrap = true;
  let logExpanded = false;

  $: currentLogText = preferLatestLog
    ? (diagnosis?.latestLog?.tail ?? "")
    : (selectedReport?.content ?? "");
  $: logDisplayText =
    currentLogText.length > 160_000 ? currentLogText.slice(currentLogText.length - 160_000) : currentLogText;
  $: logLines = logDisplayText ? logDisplayText.split("\n") : [];
  $: logLineCount = logLines.length;
  $: if (diagnosis) recomputeLogMatches(logDisplayText);

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  function markQuery(html: string, query: string): string {
    if (!query) return html;
    const q = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return html.replace(new RegExp(`(${q})`, "gi"), "<mark>$1</mark>");
  }

  /** Minecraft / log4j-ish line coloring (safe: escapes first). */
  function colorizeLogLine(line: string, query: string): string {
    let html = escapeHtml(line);
    if (/^\s+at\s+\S/.test(line) || /^\s+\.\.\.\s+\d+\s+more/.test(line)) {
      return `<span class="tok-stack">${markQuery(html, query)}</span>`;
    }
    if (/^Caused by:/i.test(line) || /^Suppressed:/i.test(line)) {
      return `<span class="tok-caused">${markQuery(html, query)}</span>`;
    }
    if (/^-{5,}|^----\s*Minecraft Crash Report/i.test(line) || /^\/\/ /i.test(line)) {
      return `<span class="tok-section">${markQuery(html, query)}</span>`;
    }
    html = html.replace(
      /^((?:\[[^\]]+\]\s*){1,3}|\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:\s+\[[^\]]+\])?)/,
      '<span class="tok-time">$1</span>',
    );
    html = html.replace(/\b(FATAL|ERROR|SEVERE)\b/g, '<span class="tok-error">$1</span>');
    html = html.replace(/\b(WARN(?:ING)?)\b/g, '<span class="tok-warn">$1</span>');
    html = html.replace(/\b(INFO|DEBUG|TRACE)\b/g, '<span class="tok-info">$1</span>');
    html = html.replace(
      /\b([a-z0-9_.-]+\.(?:Exception|Error|Throwable))\b/gi,
      '<span class="tok-exc">$1</span>',
    );
    html = html.replace(
      /\b(mod\s+[a-z0-9_-]+|[a-z0-9_-]+:[a-z0-9_./-]+)\b/gi,
      '<span class="tok-mod">$1</span>',
    );
    return markQuery(html, query);
  }

  function signalClass(kind: string | undefined): string {
    if (!kind) return "";
    const k = kind.toLowerCase();
    if (k.includes("entry") || k.includes("error") || k.includes("crash")) return "sig-error";
    if (k.includes("warn") || k.includes("mismatch") || k.includes("perf")) return "sig-warn";
    return "sig-info";
  }

  function recomputeLogMatches(text: string) {
    if (!logQuery) {
      logMatches = [];
      activeMatch = 0;
      return;
    }
    const lower = text.toLowerCase();
    const q = logQuery.toLowerCase();
    const found: { line: number }[] = [];
    let from = 0;
    while (true) {
      const idx = lower.indexOf(q, from);
      if (idx < 0) break;
      const line = text.slice(0, idx).split("\n").length - 1;
      found.push({ line });
      from = idx + q.length;
    }
    logMatches = found;
    activeMatch = found.length ? Math.min(activeMatch, found.length - 1) : 0;
  }

  function jumpToMatch(dir: 1 | -1) {
    if (!logMatches.length) return;
    activeMatch = (activeMatch + dir + logMatches.length) % logMatches.length;
    scrollLogToLine(logMatches[activeMatch].line);
  }

  function scrollLogToLine(line: number) {
    if (!logPreEl) return;
    const lines = logPreEl.querySelectorAll("div.log-line");
    const target = lines[Math.min(line, lines.length - 1)] as HTMLElement | undefined;
    target?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  const LOG_ERROR_RE = /\b(FATAL|ERROR|SEVERE)\b|Exception|Caused by:|Crash Report/i;
  let activeErrorHit = -1;

  $: errorHits = (logDisplayText ? logDisplayText.split("\n") : [])
    .map((l, i) => (LOG_ERROR_RE.test(l) ? i : -1))
    .filter((i) => i >= 0);

  /** Cycle through every ERROR/FATAL/Exception line (wraps). */
  function jumpToNextError() {
    if (!errorHits.length) {
      message = "No ERROR/FATAL/Exception lines found in this log view.";
      return;
    }
    activeErrorHit = ((activeErrorHit + 1) % errorHits.length + errorHits.length) % errorHits.length;
    const idx = errorHits[activeErrorHit];
    scrollLogToLine(idx);
    message = `Error ${activeErrorHit + 1}/${errorHits.length} · line ${idx + 1}`;
  }

  async function copyCurrentLog() {
    const text = currentLogText;
    if (!text) {
      error = "Nothing to copy — load a log first.";
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      message = `Copied ${text.length.toLocaleString()} characters.`;
    } catch {
      error = "Clipboard copy failed.";
    }
  }

  function projectDir(): string {
    const p = $projectPath ?? "";
    const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
    return i >= 0 ? p.slice(0, i) : p;
  }

  async function openSubdir(name: string) {
    if (!$projectPath) return;
    try {
      await openShell(`${projectDir()}\\${name}`.replace(/\//g, "\\"));
    } catch {
      try {
        await openShell(`${projectDir()}/${name}`);
      } catch (e) {
        error = String(e);
      }
    }
  }

  // --- Unified Problems panel (IDE "Problems" tool window) ---
  type ProblemRow = {
    id: string;
    severity: "critical" | "error" | "warning" | "info";
    title: string;
    detail: string;
    actions: FixAction[];
    source: string;
  };

  $: problems = buildProblems(diagnosis);
  function buildProblems(d: CrashDiagnosis | null): ProblemRow[] {
    if (!d) return [];
    const rows: ProblemRow[] = [];
    for (const h of d.hints) {
      rows.push({
        id: `hint:${h.id}`,
        severity: h.severity === "critical" ? "critical" : h.severity === "error" ? "error" : h.severity === "warning" ? "warning" : "info",
        title: h.title,
        detail: h.detail,
        actions: h.fixes && h.fixes.length ? h.fixes : h.fix ? [h.fix] : [],
        source: "Diagnosis",
      });
    }
    for (const g of d.graphDiagnostics) {
      rows.push({
        id: `graph:${g.code}`,
        severity: g.severity === "Error" ? "error" : g.severity === "Warning" ? "warning" : "info",
        title: g.code,
        detail: g.message,
        actions: [],
        source: "Graph",
      });
    }
    return rows;
  }

  $: graphDiagnostics = diagnosis?.graphDiagnostics ?? [];
  $: allSignals = diagnosis?.sessionHealthy && preferLatestLog
    ? []
    : [
        ...(diagnosis?.selectedReport?.signals ?? []),
        ...(diagnosis?.latestLog?.signals ?? []),
        ...(diagnosis?.launcherLog?.signals ?? []),
      ];
  $: signalGroups = [
    { title: "Entrypoint", hint: "Fabric/Quilt entrypoint failures", items: allSignals.filter((s) => s.kind === "Entrypoint") },
    { title: "Loader mismatch", hint: "Wrong loader/API/version bridge, NoSuchMethod/NoSuchField", items: allSignals.filter((s) => s.kind === "LoaderMismatch") },
    { title: "Render/OpenGL", hint: "Renderer, shader or GPU pipeline signals", items: allSignals.filter((s) => s.kind === "OpenGl") },
    { title: "Performance", hint: "Tick stalls and overload warnings", items: allSignals.filter((s) => s.kind === "Performance") },
  ].filter((group) => group.items.length > 0);

  $: errorCount = graphDiagnostics.filter((d) => d.severity === "Error").length;
  $: warningCount = graphDiagnostics.filter((d) => d.severity === "Warning").length;
  $: onProjectPathChange($projectPath);

  onMount(() => {
    // Refresh whenever the Diagnose tab is (re)opened so the user always sees
    // fresh crash-report / log data rather than a stale snapshot from a
    // previous visit. Without this the panel could appear "stuck" / empty.
    const reload = () => {
      lastLoadedPath = null;
      void load(true);
    };
    window.addEventListener("tuffbox:open-diagnostics", reload);
    if ($projectPath) {
      void load(true);
      void loadPendingPlan();
    }
    return () => window.removeEventListener("tuffbox:open-diagnostics", reload);
  });
</script>


<div class="diagnostics">
  <div class="toolbar">
    <div class="title">
      <Stethoscope size={18} />
      <span>Diagnose</span>
      {#if analysisBusy || crashLoading || aiLoading}
        <span class="analyzing-pill">Analyzing…</span>
      {/if}
    </div>
  </div>

  <!-- Tools first — always visible action strip -->
  <section class="tools-strip panel">
    <div class="tools-group">
      <span class="tools-label">Analyze</span>
      <button class="primary" on:click={runTest} disabled={!$projectPath || launching || loading}>
        <Play size={15} class={launching ? "spin" : ""} />
        {launching ? "Launching…" : "Test launch"}
      </button>
      <button
        class="secondary"
        on:click={() => runUnifiedAnalysis()}
        disabled={!$projectPath || analysisBusy || loading || sessionOk}
        title="Re-run Crash Assistant + AI"
      >
        <RefreshCw size={15} class={analysisBusy ? "spin" : ""} />
        {analysisBusy ? "Analyzing…" : "Re-analyze"}
      </button>
      <button class="ghost" on:click={() => load(true)} disabled={!$projectPath || loading} title="Reload crash reports & logs">
        <RefreshCw size={15} class={loading ? "spin" : ""} /> Refresh
      </button>
      <button class="ghost" on:click={() => runAiExplain()} disabled={!$projectPath || aiLoading || sessionOk}>
        <Bot size={15} /> AI explain
      </button>
    </div>
    <div class="tools-group">
      <span class="tools-label">Log</span>
      <button class="ghost" on:click={shareCurrentLog} disabled={!$projectPath || sharingLog || !currentLogText}>
        <Share2 size={15} /> {sharingLog ? "Sharing…" : "Share mclo.gs"}
      </button>
      <button class="ghost" on:click={copyCurrentLog} disabled={!currentLogText} title="Copy the full raw log to clipboard">
        <Copy size={15} /> Copy log
      </button>
      <button
        class="ghost"
        on:click={jumpToNextError}
        disabled={!errorHits.length}
        title={errorHits.length ? `Cycle errors (${errorHits.length})` : "No error lines in this log"}
      >
        <ArrowDownToLine size={15} />
        Error{errorHits.length ? ` ${(activeErrorHit < 0 ? 0 : activeErrorHit) + 1}/${errorHits.length}` : ""}
      </button>
    </div>
    <div class="tools-group">
      <span class="tools-label">Folders</span>
      <button class="ghost" on:click={openFolder} disabled={!$projectPath} title="Open instance folder">
        <FolderOpen size={15} /> Instance
      </button>
      <button class="ghost" on:click={() => openSubdir("logs")} disabled={!$projectPath}>
        <FileText size={15} /> logs/
      </button>
      <button class="ghost" on:click={() => openSubdir("crash-reports")} disabled={!$projectPath}>
        <Bug size={15} /> crashes/
      </button>
    </div>
    <div class="tools-group">
      <span class="tools-label">Scanners</span>
      <button class="ghost" on:click={createFixPlan} disabled={!$projectPath || planning}>{planning ? "…" : "Fix plan"}</button>
      <button class="ghost" on:click={scanOreGen} disabled={!$projectPath || oreLoading}>{oreLoading ? "…" : "Ore gen"}</button>
      <button class="ghost" on:click={scanDuplicateItems} disabled={!$projectPath || duplicateLoading}>{duplicateLoading ? "…" : "Duplicates"}</button>
      <button class="ghost" on:click={generateUnify} disabled={!$projectPath || unifyLoading}>{unifyLoading ? "…" : "Unify"}</button>
      <button class="ghost" on:click={() => detectWrongLoaderMods()} disabled={!$projectPath || wrongLoaderLoading}>Wrong jars</button>
      <button class="ghost" on:click={() => detectDuplicateModJars()} disabled={!$projectPath || duplicateJarLoading}>
        {duplicateJarLoading ? "Dupes…" : "Dup jars"}
      </button>
      <button class="ghost" on:click={() => openAuthorForm({ fromAnalysis: !!aiAnalysis })} disabled={!$projectPath || authorBusy}>
        <BookMarked size={15} /> Save KB
      </button>
      <button class="ghost" on:click={() => (aiModalOpen = true)}><Bot size={15} /> AI settings</button>
      {#if aiPrompt}
        <button class="ghost" on:click={() => (aiShowPrompt = !aiShowPrompt)}>{aiShowPrompt ? "Hide" : "Show"} AI prompt</button>
      {/if}
    </div>
  </section>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}
  {#if aiSoftError}
    <div class="notice warning">
      AI unavailable — rules still work.
      <button class="ghost mini" type="button" on:click={() => (aiModalOpen = true)}>AI settings</button>
    </div>
  {/if}
  {#if pendingPlan && swarmEnabled}
    <div class="notice warning">
      Network fix ready ({(pendingPlan.actions ?? []).length} action(s)).
      <button class="secondary small" on:click={applyPendingNetworkFix} disabled={pendingBusy}>
        {pendingBusy ? "Applying…" : "Apply"}
      </button>
    </div>
  {/if}

  {#if loading && !diagnosis}
    <div class="loading">Loading crash diagnosis…</div>
  {:else if !$projectPath}
    <EmptyState icon={Stethoscope} title="Pick a pack first" description="Open a project — we'll read the crash log and tell you what to click next." />
  {:else if diagnosis}
    <!-- 1. Source + status (compact) -->
    <section class="dx-source panel">
      <label class="dx-source-label" for="dx-source-select">Looking at</label>
      <select
        id="dx-source-select"
        class="dx-source-select"
        value={preferLatestLog ? LATEST_LOG_SOURCE : selectedReportId}
        on:change={onSourceChange}
      >
        <option value={LATEST_LOG_SOURCE}>
          latest.log{diagnosis.latestLog.exists ? ` · ${diagnosis.latestLog.signals.length} signals` : " · missing"}
        </option>
        {#each diagnosis.reports as report (report.id)}
          <option value={report.id}>{report.name} · {formatBytes(report.size)}</option>
        {/each}
      </select>
      {#if diagnosis.crashReportStale}
        <p class="muted-inline">Crash report is older than latest.log — live log is preferred.</p>
      {/if}
    </section>

    <!-- Verdict first (answer before the scary log) -->
    <section class="dx-verdict" class:ok={sessionOk} class:warn={!sessionOk && !!(topSuspect || topFinding)} class:neutral={!sessionOk && !topSuspect && !topFinding}>
      <div class="dx-verdict-icon">
        {#if sessionOk}
          <CheckCircle size={22} />
        {:else if topSuspect || topFinding}
          <AlertTriangle size={22} />
        {:else}
          <CircleHelp size={22} />
        {/if}
      </div>
      <div class="dx-verdict-body">
        {#if sessionOk}
          <span class="eyebrow">You're good</span>
          <h1>Last launch looked fine</h1>
          <p class="dx-verdict-copy">No crash to chase right now. Pack graph warnings below still matter if any show up.</p>
        {:else if topFinding && (!topSuspect || (topFinding.severity === "critical" || topFinding.severity === "error"))}
          <span class="eyebrow">{severityChip(topFinding.severity)}</span>
          <h1>{topFinding.title}</h1>
          <p class="dx-verdict-copy">{topFinding.description}</p>
          {#if topFinding.autoFix}
            <p class="dx-next-step"><strong>Try this:</strong> {topFinding.autoFix}</p>
          {/if}
          {#if topSuspect}
            <p class="dx-verdict-copy muted-inline">
              Suspect mod: <code>{topSuspect.id}</code>
              · {topSuspect.confidence}%
            </p>
          {/if}
        {:else if topSuspect}
          <span class="eyebrow">Looks like this broke it</span>
          <h1>{heroCulpritLabel || topSuspect.name}</h1>
          <p class="dx-verdict-copy">
            <code>{topSuspect.id}</code>
            · {topSuspect.confidence}% confidence
            {#if topSuspect.blameRole}· {topSuspect.blameRole}{/if}
          </p>
          {#if strongestEvidence}
            <p class="dx-evidence"><code>{strongestEvidence}</code></p>
          {/if}
        {:else}
          <span class="eyebrow">Still figuring it out</span>
          <h1>No clear culprit yet</h1>
          <p class="dx-verdict-copy">
            {analysisBusy
              ? "Scanning the log…"
              : "Hit Re-analyze, or jump to the first error in the log below."}
          </p>
        {/if}

        <div class="dx-cta">
          {#if !sessionOk && primaryRec}
            <button
              class="primary"
              type="button"
              on:click={primaryRec.apply}
              disabled={aiApplyBusy || applyingHintId !== null}
            >
              <Wrench size={15} />
              {primaryRec.label}
            </button>
            {#if mergedRecommendations.length > 1}
              <span class="dx-cta-more">{mergedRecommendations.length - 1} more below</span>
            {/if}
          {:else if !sessionOk && topSuspect?.knownInManifest}
            <button class="primary" on:click={() => fixDisableMod(topSuspect.id)} disabled={disablingModId === topSuspect.id}>
              {disablingModId === topSuspect.id ? "Disabling…" : `Disable ${topSuspect.name}`}
            </button>
            <button class="ghost" on:click={() => applyTopSuspectUpdate()} disabled={fixingIdx === -1}>Update</button>
          {/if}
          {#if !sessionOk && aiAnalysis && aiPlanActions(aiAnalysis).length > 1}
            <button
              class="secondary"
              on:click={applyAiPlan}
              disabled={aiApplyBusy || (aiAnalysis.validation && aiAnalysis.validation.ok === false)}
            >
              {aiApplyBusy ? "Applying…" : "Apply full AI plan"}
            </button>
          {/if}
          {#if !sessionOk}
            <button class="ghost" type="button" on:click={jumpToFirstError} disabled={!logDisplayText}>
              <ArrowDownToLine size={15} /> Jump to error
            </button>
          {/if}
        </div>
      </div>
    </section>

    <!-- Extra actions (only if more than the primary) -->
    {#if !sessionOk && mergedRecommendations.length > 1}
      <section class="dx-more-actions panel">
        <h2><Lightbulb size={16} /> Other ways to fix it</h2>
        <ul class="merged-list compact">
          {#each mergedRecommendations.slice(1) as rec (rec.id)}
            <li class="merged-item {rec.source}">
              <span class="src-tag">{rec.source === "ai" ? "AI" : "Rules"}</span>
              <div class="merged-body">
                <strong>{rec.label}</strong>
                {#if rec.detail}<span>{rec.detail}</span>{/if}
              </div>
              <button class="secondary small" type="button" on:click={rec.apply} disabled={aiApplyBusy || applyingHintId !== null}>
                Do it
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Log viewer (details after the answer) -->
    <section class="log-viewer panel" class:expanded={logExpanded}>
      <div class="log-viewer-head">
        <div class="log-viewer-title">
          <Terminal size={16} />
          <strong>Log</strong>
          <span class="log-meta">
            {preferLatestLog ? "latest.log" : (selectedReport?.summary?.name ?? "log")}
            · {logLineCount.toLocaleString()} lines
            {#if currentLogText.length > logDisplayText.length}
              · last {(logDisplayText.length / 1024).toFixed(0)} KB
            {/if}
          </span>
        </div>
        <div class="log-viewer-actions">
          <label class="log-search">
            <Search size={13} />
            <input
              type="search"
              placeholder="Find in log…"
              bind:value={logQuery}
              on:keydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  jumpToMatch(e.shiftKey ? -1 : 1);
                }
              }}
            />
            {#if logQuery}
              <span class="log-match-count">
                {logMatches.length ? `${activeMatch + 1}/${logMatches.length}` : "0"}
              </span>
              <button type="button" class="ghost mini" on:click={() => jumpToMatch(-1)} disabled={!logMatches.length}>↑</button>
              <button type="button" class="ghost mini" on:click={() => jumpToMatch(1)} disabled={!logMatches.length}>↓</button>
            {/if}
          </label>
          <button type="button" class="ghost mini" class:active={logWrap} on:click={() => (logWrap = !logWrap)} title="Toggle wrap">
            Wrap
          </button>
          <button type="button" class="ghost mini" on:click={() => (logExpanded = !logExpanded)} title="Toggle height">
            <Maximize2 size={13} /> {logExpanded ? "Compact" : "Tall"}
          </button>
          <button
            type="button"
            class="ghost mini"
            on:click={jumpToNextError}
            disabled={!errorHits.length}
            title={errorHits.length ? `Next error (${errorHits.length})` : "No error lines"}
          >
            Error{errorHits.length ? ` ${(activeErrorHit < 0 ? 0 : activeErrorHit) + 1}/${errorHits.length}` : ""}
          </button>
          <button type="button" class="ghost mini" on:click={copyCurrentLog} disabled={!currentLogText}><Copy size={13} /></button>
          <button type="button" class="ghost mini" on:click={shareCurrentLog} disabled={sharingLog || !currentLogText}>
            <Share2 size={13} />
          </button>
        </div>
      </div>

      {#if logLines.length}
        <div
          class="log-stage"
          class:nowrap={!logWrap}
          bind:this={logPreEl}
          role="log"
          aria-label="Crash or game log"
        >
          {#each logLines as line, i (i)}
            <div
              class="log-line {signalClass(signalLineMap.get(i + 1))}"
              class:active-match={logMatches[activeMatch]?.line === i}
              data-ln={i + 1}
            >
              <span class="ln">{i + 1}</span>
              <span class="ll">{@html colorizeLogLine(line, logQuery)}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="muted-box">No log yet — pick latest.log or a crash report above, then Refresh.</div>
      {/if}
    </section>

    <!-- 3. Analysis as tabs (not side-by-side) -->
    <section class="dx-tabs panel">
      <div class="dx-tabbar" role="tablist">
        <button
          type="button"
          role="tab"
          class="dx-tab"
          class:active={detailTab === "rules"}
          aria-selected={detailTab === "rules"}
          on:click={() => (detailTab = "rules")}
        >
          <Zap size={14} /> Rules
          {#if crashFindings.length}<span class="count">{crashFindings.length}</span>{/if}
          {#if crashLoading}<span class="analyzing-pill">…</span>{/if}
        </button>
        <button
          type="button"
          role="tab"
          class="dx-tab"
          class:active={detailTab === "ai"}
          aria-selected={detailTab === "ai"}
          on:click={() => (detailTab = "ai")}
        >
          <MessageCircle size={14} /> AI
          {#if aiAnalysis?.source}<span class="ai-source-badge">{aiAnalysis.source}</span>{/if}
          {#if aiLoading}<span class="analyzing-pill">…</span>{/if}
        </button>
      </div>

      {#key detailTab}
        {#if detailTab === "rules"}
          <div class="dx-tabpanel" role="tabpanel" in:fly={{ x: -12, duration: 280, opacity: 0, easing: quintOut }}>
            {#if crashFindings.length === 0 && !crashLoading}
              <div class="muted-box">No rule-based findings for this source.</div>
            {:else}
              <div class="findings-stack">
                {#each crashFindings.slice(0, 10) as f, fIdx (f.code + f.title + fIdx)}
                  <article class="finding-card {f.severity}" class:ai-agree={f.aiAgree}>
                    <header>
                      <span class="sev-chip {f.severity}">{severityChip(f.severity)}</span>
                      <strong>{f.title}</strong>
                      {#if f.aiAgree}<span class="ai-agree-badge" title={f.aiHint ?? ""}>AI agrees</span>{/if}
                    </header>
                    <p>{f.description}</p>
                    {#if f.aiHint}<p class="ai-hint">AI: {f.aiHint}</p>{/if}
                    {#if f.autoFix}<p class="auto-fix"><strong>Try this:</strong> {f.autoFix}</p>{/if}
                    {#if f.fixes?.length}
                      <div class="finding-actions">
                        {#each f.fixes.slice(0, 3) as action (action.kind + (action.modId ?? "") + action.label)}
                          <button class="secondary small" on:click={() => applyCrashFindingFix(f, action)} disabled={applyingHintId !== null}>
                            {action.label}
                          </button>
                        {/each}
                      </div>
                    {/if}
                  </article>
                {/each}
              </div>
            {/if}
          </div>
        {:else}
          <div class="dx-tabpanel" role="tabpanel" in:fly={{ x: 12, duration: 280, opacity: 0, easing: quintOut }}>
            {#if aiLoading && !aiAnalysis}
              <div class="muted-box">AI is reading this crash…</div>
            {:else if !aiAnalysis}
              <div class="muted-box">
                {aiSoftError ? "AI failed — use Rules, or fix Ollama." : "No AI result yet."}
                <button class="ghost mini" type="button" on:click={() => runAiExplain()}>Retry AI</button>
              </div>
            {:else}
              <p class="ai-human">{aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation}</p>
              <div class="ai-stats compact">
                <div class="ai-stat"><strong>{Math.round((aiAnalysis.confidence ?? 0) * 100)}%</strong> conf</div>
                <div class="ai-stat"><strong>{aiPlanActions(aiAnalysis).length}</strong> actions</div>
                {#if aiAnalysis.model}<div class="ai-stat"><strong>{aiAnalysis.model}</strong></div>{/if}
              </div>
              {#if aiAnalysis.normalizeNotes?.length}
                <div class="notice warning tight">Adjusted: {aiAnalysis.normalizeNotes.join("; ")}</div>
              {/if}
              {#if aiAnalysis.additionalContext ?? aiAnalysis.additional_context}
                <div class="notice warning tight">{aiAnalysis.additionalContext ?? aiAnalysis.additional_context}</div>
              {/if}
              {#if (aiAnalysis.suspectedMods ?? aiAnalysis.suspected_mods)?.length}
                <div class="ai-list">
                  <strong>Suspected</strong>
                  <div class="crash-tags">
                    {#each (aiAnalysis.suspectedMods ?? aiAnalysis.suspected_mods) as modId (modId)}
                      <code>{modId}</code>
                    {/each}
                  </div>
                </div>
              {/if}
              {#if aiPlanActions(aiAnalysis).length}
                <div class="ai-list">
                  <strong>Plan</strong>
                  <ul>
                    {#each aiPlanActions(aiAnalysis) as action, aIdx (aIdx)}
                      <li>
                        <strong>{aiActionLabel(action)}</strong>
                        {#if action.modId ?? action.mod_id}<code>{action.modId ?? action.mod_id}</code>{/if}
                        {#if aiActionVersion(action)}<span class="ai-ver">v{aiActionVersion(action)}</span>{/if}
                        <span>{action.reason ?? action.description ?? ""}</span>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
              <div class="ai-feedback">
                <button class="secondary small" disabled={aiApplyBusy || (aiAnalysis.validation && aiAnalysis.validation.ok === false)} on:click={applyAiPlan}>
                  {aiApplyBusy ? "Applying…" : "Apply plan"}
                </button>
                <button class="ghost mini" disabled={aiFeedbackBusy} on:click={() => sendAiFeedback(true)}>Helped</button>
                <button class="ghost mini" disabled={aiFeedbackBusy} on:click={() => sendAiFeedback(false)}>Wrong</button>
                {#if aiFeedbackMsg}<small>{aiFeedbackMsg}</small>{/if}
              </div>
            {/if}
          </div>
        {/if}
      {/key}
    </section>

    <!-- 4. Evidence (secondary) -->
    <details class="panel collapsible-block dx-evidence-block" open={graphDiagnostics.length > 0 || wrongLoaderJars.length > 0 || duplicateJarGroups.length > 0}>
      <summary>
        <span><GitMerge size={16} /> Conflicts & jars</span>
        <span class="tools-hint">
          {graphDiagnostics.length} conflict{graphDiagnostics.length === 1 ? "" : "s"}
          {#if wrongLoaderJars.length} · {wrongLoaderJars.length} wrong jar{/if}
          {#if duplicateJarGroups.length} · {duplicateJarGroups.length} dup{/if}
          <ChevronDown size={14} />
        </span>
      </summary>
      <div class="dx-evidence-body">
        {#if graphDiagnostics.length === 0 && !wrongLoaderJars.length && !duplicateJarGroups.length}
          <div class="muted-box">No graph conflicts or jar issues.</div>
        {/if}
        {#if graphDiagnostics.length > 0}
          <div class="diag-list">
            {#each graphDiagnostics as d, idx (d.code + d.message + idx)}
              <div class="diag-row {String(d.severity).toLowerCase()}">
                <div>
                  <strong>{d.code}</strong>
                  <p>{d.message}</p>
                </div>
                <div class="diag-actions">
                  {#if /MISSING|DEPEND/i.test(d.code + d.message)}
                    {@const mid = (d.message.match(/['"`]?([a-z0-9_-]{3,})['"`]?\s*$/i) || [])[1]}
                    {#if mid}
                      <button class="secondary small" on:click={() => fixMissingDependency(mid, idx)} disabled={fixingIdx === idx}>
                        Install {mid}
                      </button>
                    {/if}
                  {/if}
                  {#if /DUPLICATE/i.test(d.code)}
                    <button class="secondary small" on:click={() => fixDeduplicate(idx)} disabled={fixingIdx === idx || duplicateJarFixing !== null}>
                      Keep one jar
                    </button>
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
        {#if duplicateJarGroups.length > 0}
          <h3 class="dx-subhead"><AlertTriangle size={14} /> Duplicate mod jars</h3>
          <p class="tools-hint" style="margin: 0 0 8px">Same mod id in more than one jar — keep one, delete the rest.</p>
          {#each duplicateJarGroups as group (group.modId)}
            <div class="diag-row warning">
              <div>
                <strong>{group.modId}</strong>
                <p>{group.jars.length} jars · suggested keep: <code>{group.keepCandidate}</code></p>
                <ul class="dup-jar-list">
                  {#each group.jars as jar (jar.fileName)}
                    <li>
                      <code>{jar.fileName}</code>
                      {#if jar.inManifest}<span class="pill">manifest</span>{/if}
                      {#if jar.fileName === group.keepCandidate}<span class="pill">newest</span>{/if}
                      <button
                        class="ghost mini"
                        disabled={duplicateJarFixing !== null}
                        on:click={() => keepOneDuplicateJar(group.modId, jar.fileName)}
                        title="Keep this jar, delete the other copies"
                      >
                        {duplicateJarFixing === `${group.modId}::${jar.fileName}` ? "…" : "Keep this"}
                      </button>
                    </li>
                  {/each}
                </ul>
              </div>
              <div class="diag-actions">
                <button
                  class="secondary small"
                  disabled={duplicateJarFixing !== null}
                  on:click={() => keepOneDuplicateJar(group.modId, group.keepCandidate)}
                >
                  Keep newest
                </button>
              </div>
            </div>
          {/each}
        {/if}
        {#if wrongLoaderJars.length > 0}
          <h3 class="dx-subhead"><AlertTriangle size={14} /> Wrong-loader jars</h3>
          {#each wrongLoaderJars as jar (jar.fileName)}
            <div class="diag-row warning">
              <div>
                <strong>{jar.fileName}</strong>
                <p>{jar.reason ?? jar.detectedLoader ?? "Wrong loader"}</p>
              </div>
              <div class="diag-actions">
                <button class="ghost mini" on:click={() => disableWrongJar(jar.fileName)} disabled={wrongLoaderFixing === jar.fileName}>Disable</button>
                <button class="ghost mini danger" on:click={() => removeWrongJar(jar.fileName)} disabled={wrongLoaderFixing === jar.fileName}>Remove</button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </details>

    <!-- Scanner results / KB authoring (tools live in the top strip) -->
    {#if plan || oreFindings?.length || duplicateFindings?.length || unifyConfigResult || authorOpen || aiShowPrompt}
      <section class="panel tools-results">
        <h2><Wrench size={16} /> Tool results</h2>
        {#if aiShowPrompt && aiPrompt}
          <pre class="log-pre">{aiPrompt.slice(0, 20000)}</pre>
        {/if}
        {#if plan}
          <div class="plan-card">
            <h3>Fix plan</h3>
            <p>{plan.summary}</p>
            <button class="primary" on:click={applyFix} disabled={applying}>{applying ? "Applying…" : "Apply fix plan"}</button>
          </div>
        {/if}
        {#if oreFindings?.length}
          <div class="muted-box"><strong>Ore gen</strong>: {oreFindings.length} finding(s)</div>
        {/if}
        {#if duplicateFindings?.length}
          <div class="muted-box"><strong>Duplicates</strong>: {duplicateFindings.length} finding(s)</div>
        {/if}
        {#if unifyConfigResult}
          <div class="muted-box"><pre>{JSON.stringify(unifyConfigResult, null, 2).slice(0, 4000)}</pre></div>
        {/if}
        {#if authorOpen}
          <div class="author-form">
            <h3>Save KB case</h3>
            <label>Solution<textarea bind:value={authorSolution} rows="3"></textarea></label>
            <label>Suspected (comma)<input bind:value={authorSuspected} /></label>
            <div class="actions">
              <button class="primary" on:click={saveAuthorCase} disabled={authorBusy}>Save</button>
              <button class="ghost" on:click={() => (authorOpen = false)}>Close</button>
            </div>
            {#if authorMsg}<p class="muted-inline">{authorMsg}</p>{/if}
          </div>
        {/if}
      </section>
    {/if}
  {:else}
    <div class="empty">Press Refresh to load diagnosis.</div>
  {/if}
</div>

<AiConnectionModal bind:open={aiModalOpen} />

<style>
  .diagnostics { max-width: min(1280px, 100%); width: 100%; margin: 0 auto; }
  .toolbar, .actions, .title, .primary-actions, .panel-header, .suspect-head, .meta, .plan-meta { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 10px; flex-wrap: wrap; }
  .title, h2 { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions .primary, .primary-actions .secondary, .primary-actions .ghost { cursor: pointer; }
  .ghost.icon-only { padding: 8px; min-width: 36px; justify-content: center; }

  .tools-strip {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    margin-bottom: 14px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .tools-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .tools-label {
    min-width: 64px;
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .tools-group button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    padding: 6px 10px;
  }

  .log-viewer {
    margin-bottom: 14px;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: var(--bg-secondary);
  }
  .log-viewer-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .log-viewer-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    color: var(--text-primary);
    font-size: 13px;
  }
  .log-meta { color: var(--text-muted); font-size: 11px; font-weight: 500; }
  .log-viewer-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .log-search {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 4px 8px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-muted);
    font-weight: 500;
  }
  .log-search input {
    width: min(220px, 36vw);
    border: none;
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    outline: none;
  }
  .log-match-count { font-size: 11px; color: var(--text-muted); min-width: 36px; text-align: center; }
  .log-viewer-actions .ghost.mini.active {
    border-color: rgba(27, 217, 106, 0.45);
    color: var(--accent-primary);
  }
  .log-stage {
    height: min(62vh, 720px);
    overflow: auto;
    background: #0a0a0c;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
  }
  .log-viewer.expanded .log-stage {
    height: min(86vh, 1100px);
  }
  .log-line {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 0;
    padding: 0 10px 0 0;
    border-left: 2px solid transparent;
  }
  .log-line:hover { background: rgba(255, 255, 255, 0.03); }
  .log-line.active-match { background: rgba(250, 204, 21, 0.12); }
  .log-line.sig-error { border-left-color: #f87171; background: rgba(248, 113, 113, 0.06); }
  .log-line.sig-warn { border-left-color: #fbbf24; background: rgba(251, 191, 36, 0.05); }
  .log-line.sig-info { border-left-color: #60a5fa; }
  .ln {
    user-select: none;
    text-align: right;
    padding: 0 10px 0 8px;
    color: #52525b;
    background: #111114;
    border-right: 1px solid #1f1f23;
  }
  .ll {
    padding: 0 8px;
    color: #d4d4d8;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .log-stage.nowrap .ll {
    white-space: pre;
    overflow-wrap: normal;
  }
  .log-stage :global(mark) {
    background: rgba(250, 204, 21, 0.45);
    color: #fff;
    border-radius: 2px;
    padding: 0 1px;
  }
  .log-stage :global(.tok-time) { color: #71717a; }
  .log-stage :global(.tok-error) { color: #f87171; font-weight: 700; }
  .log-stage :global(.tok-warn) { color: #fbbf24; font-weight: 700; }
  .log-stage :global(.tok-info) { color: #38bdf8; }
  .log-stage :global(.tok-stack) { color: #a1a1aa; }
  .log-stage :global(.tok-caused) { color: #fb7185; font-weight: 700; }
  .log-stage :global(.tok-section) { color: #c4b5fd; font-weight: 700; }
  .log-stage :global(.tok-exc) { color: #f472b6; }
  .log-stage :global(.tok-mod) { color: #4ade80; }

  .tools-results { margin-bottom: 14px; padding: 14px; }
  .tools-results h2, .tools-results h3 { margin: 0 0 10px; display: flex; align-items: center; gap: 8px; }

  .dx-source { display: flex; flex-direction: column; gap: 8px; margin-bottom: 12px; }
  .dx-source-label { color: var(--text-muted); font-size: 11px; font-weight: 800; letter-spacing: .06em; text-transform: uppercase; }
  .dx-source-select {
    width: 100%;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 13px;
  }
  .dx-verdict {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 14px;
    padding: 18px;
    margin-bottom: 14px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .dx-verdict.warn {
    border-color: rgba(245, 158, 11, 0.42);
    background: linear-gradient(135deg, rgba(245, 158, 11, 0.11), var(--bg-secondary) 65%);
  }
  .dx-verdict.ok {
    border-color: rgba(27, 217, 106, 0.35);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.08), var(--bg-secondary) 65%);
  }
  .dx-verdict-icon {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 12px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
  }
  .dx-verdict.warn .dx-verdict-icon { color: var(--accent-warning); background: rgba(245, 158, 11, 0.13); }
  .dx-verdict.ok .dx-verdict-icon { color: var(--accent-primary); background: rgba(27, 217, 106, 0.13); }
  .dx-verdict-body { min-width: 0; }
  .dx-verdict-body h1 { margin: 0; color: var(--text-primary); font-size: 20px; line-height: 1.3; }
  .dx-verdict-copy { margin: 6px 0 0; color: var(--text-secondary); font-size: 13px; line-height: 1.45; }
  .dx-verdict-copy code { font-size: 12px; color: var(--text-muted); }
  .dx-next-step {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-radius: 8px;
    background: rgba(27, 217, 106, 0.08);
    border: 1px solid rgba(27, 217, 106, 0.22);
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.4;
  }
  .sev-chip {
    display: inline-flex;
    align-items: center;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    padding: 2px 7px;
    border-radius: 999px;
    color: var(--text-muted);
    background: rgba(148, 163, 184, 0.15);
  }
  .sev-chip.critical { color: #fecaca; background: rgba(239, 68, 68, 0.18); }
  .sev-chip.error { color: #fed7aa; background: rgba(249, 115, 22, 0.16); }
  .sev-chip.warning { color: #fde68a; background: rgba(245, 158, 11, 0.14); }
  .sev-chip.info { color: #bae6fd; background: rgba(56, 189, 248, 0.12); }
  .dx-evidence {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-left: 3px solid var(--accent-warning);
    border-radius: 0 10px 10px 0;
    background: var(--bg-tertiary);
    font-size: 12px;
    color: var(--text-secondary);
    word-break: break-word;
  }
  .dx-cta { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 14px; }
  .dx-cta-more { color: var(--text-muted); font-size: 12px; }
  .dx-more-actions { margin-bottom: 14px; }
  .dx-tabs { padding: 0; overflow: hidden; margin-bottom: 14px; }
  .dx-tabbar {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .dx-tab {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 11px 16px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    transition:
      color var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-med) var(--ease-spring);
  }
  .dx-tab.active {
    color: var(--text-primary);
    border-bottom-color: var(--accent-primary);
    background: var(--bg-secondary);
  }
  .dx-tabpanel { padding: 14px 16px 16px; }
  .dx-evidence-block .dx-evidence-body { padding: 0 12px 12px; }
  .dx-subhead {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 0 8px;
    color: var(--text-secondary);
    font-size: 13px;
  }
  .merged-list.compact { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 8px; }
  .analysis-tools { margin-bottom: 16px; border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); background: var(--bg-secondary); }
  .analysis-tools > summary,
  .collapsible-block > summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    color: var(--text-secondary);
    cursor: pointer;
    list-style: none;
    font-size: 12px;
    font-weight: 700;
  }
  .analysis-tools > summary::-webkit-details-marker,
  .collapsible-block > summary::-webkit-details-marker { display: none; }
  .analysis-tools > summary span,
  .collapsible-block > summary span { display: flex; align-items: center; gap: 7px; }
  .analysis-tools .tools-hint,
  .collapsible-block .tools-hint { color: var(--text-muted); font-weight: 500; }
  .analysis-tools[open] .tools-hint :global(svg),
  .collapsible-block[open] .tools-hint :global(svg),
  .collapsible-block[open] > summary :global(svg:last-child) { transform: rotate(180deg); }
  .analysis-tools .actions { padding: 0 12px 12px; border-top: 1px solid var(--border-color); padding-top: 12px; }
  .collapsible-block { margin-bottom: 12px; padding: 0; }
  .log-reader-body { padding: 0 12px 12px; display: flex; flex-direction: column; gap: 10px; }
  h2 { display: flex; font-size: 14px; margin: 0 0 12px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .eyebrow { display: block; margin-bottom: 4px; color: var(--text-muted); font-size: 11px; font-weight: 800; letter-spacing: .08em; text-transform: uppercase; }
  .ghost.danger { color: #fca5a5; }
  .ghost.danger:hover { color: #fecaca; }
  .stat-card, .panel, .empty, .loading { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .muted-box, .report-card span, .log-status, .snapshot-row span, .snapshot-row small, .suspect-head span { color: var(--text-muted); font-size: 12px; }
  .panel { padding: 16px; min-width: 0; }
  .muted-inline { margin: 0; color: var(--text-muted); font-size: 12px; }
  .analyzing-pill {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
    font-size: 11px;
    font-weight: 700;
  }
  .count {
    display: inline-flex;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    place-items: center;
    border-radius: 999px;
    background: var(--bg-tertiary);
    font-size: 11px;
  }
  .findings-stack { display: flex; flex-direction: column; gap: 10px; }
  .finding-card {
    padding: 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .finding-card header { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-bottom: 6px; }
  .finding-card header strong { color: var(--text-primary); }
  .finding-card header code { color: var(--text-muted); font-size: 11px; }
  .finding-card p { margin: 0 0 6px; color: var(--text-secondary); font-size: 13px; line-height: 1.45; }
  .finding-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .ai-hint, .auto-fix { font-size: 12px; color: var(--text-muted); }
  .ai-agree-badge, .ai-source-badge {
    display: inline-flex;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
    font-size: 10px;
    font-weight: 800;
  }
  .ai-human { margin: 0 0 12px; color: var(--text-primary); font-size: 14px; line-height: 1.5; }
  .ai-stats { display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 12px; }
  .ai-stat { padding: 6px 10px; border-radius: 8px; background: var(--bg-tertiary); font-size: 12px; color: var(--text-muted); }
  .ai-stat strong { color: var(--text-primary); margin-right: 4px; }
  .ai-list { margin-top: 12px; }
  .ai-list strong { display: block; margin-bottom: 6px; font-size: 12px; color: var(--text-muted); }
  .ai-list ul { margin: 0; padding-left: 18px; color: var(--text-secondary); font-size: 13px; }
  .ai-list li { margin-bottom: 6px; }
  .ai-feedback { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 14px; }
  .crash-tags { display: flex; flex-wrap: wrap; gap: 6px; }
  .crash-tags code { padding: 2px 6px; border-radius: 4px; background: var(--bg-secondary); font-size: 11px; }
  .diag-list { display: flex; flex-direction: column; gap: 8px; }
  .diag-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .diag-row p { margin: 4px 0 0; color: var(--text-secondary); font-size: 12px; }
  .diag-actions { display: flex; flex-wrap: wrap; gap: 6px; align-items: flex-start; }
  .dup-jar-list {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dup-jar-list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-secondary);
  }
  .dup-jar-list .pill {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    color: var(--text-muted);
  }
  .merged-item {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .merged-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .merged-body strong { color: var(--text-primary); font-size: 13px; }
  .merged-body span { color: var(--text-muted); font-size: 12px; }
  .src-tag {
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-secondary);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 800;
  }
  .merged-item.ai .src-tag { color: var(--accent-primary); background: rgba(27, 217, 106, 0.12); }
  .notice.warning { color: #fde68a; background: rgba(245, 158, 11, 0.08); border-color: rgba(245, 158, 11, 0.28); }
  .notice.tight { padding: 8px 10px; margin-bottom: 10px; font-size: 12px; }
  .muted-box { padding: 12px; border-radius: 10px; border: 1px dashed var(--border-color); }
  .loading, .empty { padding: 24px; text-align: center; color: var(--text-muted); }
  .log-pre {
    margin: 0;
    max-height: 320px;
    padding: 12px;
    border-radius: 12px;
    background: #09090b;
    color: #d4d4d8;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow: auto;
  }
  .plan-card, .author-form { margin-top: 12px; padding: 12px; border-top: 1px solid var(--border-color); }
  .author-form label { display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px; font-size: 12px; color: var(--text-muted); }
  .author-form textarea, .author-form input {
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }
  :global(.spin) { animation: dx-spin 0.9s linear infinite; }
  @keyframes dx-spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) {
    .dx-verdict { grid-template-columns: 1fr; }
    .merged-item { grid-template-columns: 1fr; }
  }
</style>
