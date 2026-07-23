<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { launchWithFeedback } from "../lib/launch";
  import { onMount } from "svelte";
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
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import AiConnectionModal from "./AiConnectionModal.svelte";

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

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && diagnosis) return;
    loading = true;
    error = null;
    try {
      const reportId = preferLatestLog ? null : (selectedReportId || null);
      const data: CrashDiagnosis = await invoke("get_crash_diagnosis", {
        path: $projectPath,
        reportId,
      });
      diagnosis = data;
      // Keep selection only when backend actually analyzed that report.
      // Do NOT fall back to reports[0] — that re-locks Diagnose onto a stale crash.
      selectedReportId = data.selectedReport?.summary.id ?? "";
      preferLatestLog = !selectedReportId || data.analysisSource === "latest_log";
      plan = null;
      detectWrongLoaderMods();
      if (data.sessionHealthy && preferLatestLog) {
        // Successful relaunch — don't re-run crash-phrase detectors on leftover ERROR lines.
        crashFindings = [];
        crashMcreator = [];
        crashClassFinder = [];
        crashSupportMsg = null;
        // Record resolved crash in History when a prior fix was applied.
        void invoke("confirm_crash_resolution_from_diagnose", { path: $projectPath }).catch(() => {});
      } else {
        void runCrashAssistant();
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
    return preferLatestLog ? null : (selectedReportId || null);
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

  /// Per-diagnostic fix: handle duplicate mods by opening the graph view.
  async function fixDeduplicate(idx: number) {
    fixingIdx = idx;
    message = "Duplicate mods found. Open the Dependency Graph (Resolve stage) to review and remove duplicates manually.";
    fixingIdx = null;
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
  let crashSupportMsg: string | null = null;
  let crashShowSupport = false;

  async function runCrashAssistant() {
    if (!$projectPath) return;
    crashLoading = true;
    try {
      const result: any = await invoke("run_crash_assistant_full", {
        path: $projectPath,
        reportId: preferLatestLog ? null : (selectedReportId || null),
      });
      crashFindings = result.findings ?? [];
      crashMcreator = result.mcreatorMods ?? [];
      crashClassFinder = result.classFinderResults ?? [];
      crashSupportMsg = result.supportMessageDiscord || result.supportMessageGithub || null;
      crashShowSupport = false;
    } catch (e) {
      error = String(e);
    } finally {
      crashLoading = false;
    }
  }

  async function copySupportMsg() {
    const text = crashSupportMsg ?? "";
    try { await navigator.clipboard.writeText(text); message = "Support message copied."; }
    catch { message = "Failed to copy."; }
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

  async function runAiExplain() {
    if (!$projectPath) return;
    aiLoading = true;
    error = null;
    aiFeedbackMsg = null;
    try {
      try {
        const prep = await invoke<{ ok?: boolean; model?: string; skipped?: boolean }>(
          "ensure_ollama_model",
        );
        if (prep?.model) {
          message = `AI ready (${prep.model}). Analyzing crash…`;
        } else {
          message = "Preparing local AI…";
        }
      } catch (prepErr) {
        // Still try analyze — it also ensures Ollama; surface prep hint if that fails too.
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
      await loadPendingPlan();
      const similar = context.similarCaseCount ?? 0;
      const model = context.aiModel ?? aiAnalysis?.model ?? "AI";
      message = `AI analysis ready (${model}${similar ? `, ${similar} KB hit(s)` : ""}). Review before applying fixes.`;
    } catch (e) {
      const msg = String(e);
      if (/not installed|Install model|no model|Settings → AI/i.test(msg)) {
        error = `${msg} Open Settings → Integrations → Configure AI to install a model.`;
      } else if (/model.*(not found)|pull|download/i.test(msg)) {
        error = `Local AI model missing: ${msg}`;
      } else if (/ollama|connection refused|failed to fetch|tcp|unreachable/i.test(msg)) {
        error = `Ollama unavailable — install from https://ollama.com, set the path in Settings → AI, then install a model there. ${msg}`;
      } else {
        error = msg;
      }
      aiAnalysis = null;
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
      await loadCrashReports();
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

  $: selectedReport = diagnosis?.selectedReport ?? null;
  $: suspected = diagnosis?.suspectedMods ?? [];
  $: topSuspect = suspected[0] ?? null;
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
    for (const s of selectedReport?.signals ?? []) {
      if (s.lineNumber && s.lineNumber > 0) {
        const prev = m.get(s.lineNumber);
        // Keep the most severe kind already recorded (critical wins).
        m.set(s.lineNumber, prev ?? s.kind);
      }
    }
    return m;
  })();

  // --- Inline log search (Find-in-log, IDE style) ---
  let logQuery = "";
  let logMatches: { line: number }[] = [];
  let activeMatch = 0;
  let logPreEl: HTMLPreElement | null = null;

  // Splits log text into lines and highlights the current search query.
  function highlightLog(text: string, query: string): string {
    if (!query) return escapeHtml(text);
    const q = query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const re = new RegExp(`(${q})`, "gi");
    return escapeHtml(text).replace(re, "<mark>$1</mark>");
  }

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  // Counts query matches across the current crash-report / log text and
  // prepares line offsets for jumping between them.
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
      <span>Diagnose 2.0</span>
    </div>
    <div class="primary-actions">
      <button class="primary" on:click={runTest} disabled={!$projectPath || launching || loading}>
        <Play size={16} class={launching ? "spin" : ""} />
        {launching ? "Launching…" : "Test launch"}
      </button>
      <button class="secondary" on:click={createFixPlan} disabled={!$projectPath || planning}>
        <Wrench size={16} />
        {planning ? "Creating…" : "Create fix plan"}
      </button>
      <button class="secondary" on:click={applyFix} disabled={!$projectPath || applying || !diagnosis?.fixPlan || diagnosis.fixPlan.actions?.length === 0}>
        <CheckCircle size={16} />
        {applying ? "Applying…" : "Apply fix plan"}
      </button>
      <button class="ghost" on:click={openFolder} disabled={!$projectPath}>
        <FolderOpen size={16} />
        Folder
      </button>
      <button class="refresh" on:click={() => load(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        {loading ? "Refreshing…" : "Refresh"}
      </button>
    </div>
  </div>

  <details class="analysis-tools">
    <summary>
      <span><Wrench size={16} /> Analysis tools</span>
      <span class="tools-hint">Scanners, fix planning and export <ChevronDown size={15} /></span>
    </summary>
    <div class="actions">
      <button class="secondary" on:click={createFixPlan} disabled={!$projectPath || planning}>
        <Wrench size={16} />
        {planning ? "Creating..." : "Create fix plan"}
      </button>
      <button class="secondary" on:click={scanOreGen} disabled={!$projectPath || oreLoading}>
        <Database size={16} />
        {oreLoading ? "Scanning..." : "Ore gen scan"}
      </button>
      <button class="secondary" on:click={runCrashAssistant} disabled={!$projectPath || crashLoading}>
        <Zap size={16} />
        {crashLoading ? "Analyzing..." : "Crash Assistant"}
      </button>
      <button class="secondary" on:click={scanDuplicateItems} disabled={!$projectPath || duplicateLoading}>
        <GitMerge size={16} />
        {duplicateLoading ? "Scanning..." : "Find duplicates"}
      </button>
      <button class="secondary" on:click={generateUnify} disabled={!$projectPath || unifyLoading}>
        <Zap size={16} />
        {unifyLoading ? "Generating..." : "Unify config"}
      </button>
      <button class="secondary" on:click={runAiExplain} disabled={!$projectPath || aiLoading} title={!swarmEnabled ? "Local AI always available; network KB requires TuffSwarm in Settings" : "AI explain (uses network KB when swarm is on)"}>
        <MessageCircle size={16} />
        {aiLoading ? "Analyzing..." : "AI explain"}
      </button>
      <button
        class="secondary"
        on:click={applyPendingNetworkFix}
        disabled={!$projectPath || pendingBusy || !pendingPlan || !swarmEnabled}
        title={swarmEnabled ? "Apply pending_action_plan.json from network match" : "Enable TuffSwarm network in Settings"}
      >
        <Download size={16} />
        {pendingBusy ? "Applying…" : "Apply network fix"}
      </button>
      <button class="secondary" on:click={() => openAuthorForm({ fromAnalysis: !!aiAnalysis })} disabled={!$projectPath || authorBusy} title="Save crash + fix as a private KB case">
        <BookMarked size={16} />
        Save KB case
      </button>
      <button class="secondary" on:click={() => (aiModalOpen = true)} title="Configure Ollama or API key">
        <Bot size={16} /> AI settings
      </button>
    </div>
  </details>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}
  {#if pendingPlan && swarmEnabled}
    <div class="notice warning">
      Network pending plan ready ({(pendingPlan.actions ?? []).length} action(s), confidence {Math.round((pendingPlan.confidence ?? 0) * 100)}%).
      Review then use <strong>Apply network fix</strong> — nothing auto-applies.
    </div>
  {:else if !swarmEnabled}
    <div class="notice" style="opacity:0.85">
      TuffSwarm network is off — Creation Mode and network Fix Mode are disabled. Local Crash Assistant still works. Enable in Settings.
    </div>
  {/if}

  {#if loading && !diagnosis}
    <div class="loading">Loading crash diagnosis...</div>
  {:else if !$projectPath}
    <EmptyState icon={Stethoscope} title="No project selected" description="Open a project to analyze crash reports, latest.log and recent snapshots." />
  {:else if diagnosis}
    {#if diagnosis.sessionHealthy && preferLatestLog}
      <div class="muted-box stale-warn" style="border-color: rgba(27, 217, 106, 0.35); margin-bottom: 12px;">
        <CheckCircle size={16} style="vertical-align: -3px; color: var(--success, #1bd96a);" />
        Minecraft launched successfully — <code>latest.log</code> has no fresh crash markers.
        Crash-log fix suggestions are paused. Graph conflicts below still apply.
      </div>
    {/if}
    <section class="diagnosis-summary" class:neutral={!topSuspect || diagnosis.sessionHealthy}>
      {#if diagnosis.sessionHealthy && preferLatestLog && !topSuspect}
        <div class="summary-icon"><CheckCircle size={22} /></div>
        <div class="summary-body">
          <span class="eyebrow">Session status</span>
          <h1>Build is healthy</h1>
          <p class="summary-copy">
            The current instance reached a successful Minecraft session. Historical crash-reports stay listed for reference,
            but Diagnose will not ask you to apply crash-log fixes until a new failure appears.
          </p>
        </div>
      {:else if topSuspect}
        <div class="summary-icon"><AlertTriangle size={22} /></div>
        <div class="summary-body">
          <span class="eyebrow">Likely crash source</span>
          <div class="summary-heading">
            <h1>{topSuspect.name}</h1>
            <strong>{topSuspect.confidence}% confidence</strong>
          </div>
          <div class="summary-meta">
            <code>{topSuspect.id}{topSuspect.version ? ` · ${topSuspect.version}` : ""}</code>
            <span class:installed={topSuspect.knownInManifest}>
              {#if topSuspect.knownInManifest}<BadgeCheck size={14} />{:else}<CircleHelp size={14} />{/if}
              {topSuspect.knownInManifest ? "Installed mod" : "Inferred from logs"}
            </span>
          </div>
          {#if providedByEvidence && topSuspect.knownInManifest}
            <p class="mapping-note">
              The crash log’s <code>provided by</code> identifier was mapped to this installed mod.
            </p>
          {/if}
          {#if strongestEvidence}
            <div class="summary-evidence">
              <span>Strongest evidence · {strongestEvidence.source}:{strongestEvidence.lineNumber}</span>
              <p>{strongestEvidence.text}</p>
            </div>
          {:else}
            <p class="summary-copy">This mod has the highest combined diagnostic confidence, but no raw evidence line was returned.</p>
          {/if}
          {#if topSuspect.knownInManifest}
            <div class="summary-actions">
              <button
                class="secondary"
                on:click={() => fixDisableMod(topSuspect.id)}
                disabled={disablingModId !== null || fixingIdx !== null}
                title="Rename jar to .disabled so the loader skips it"
              >
                <Ban size={15} />
                {disablingModId === topSuspect.id ? "Disabling…" : "Disable mod"}
              </button>
              <button
                class="secondary"
                on:click={() => applyTopSuspectUpdate()}
                disabled={disablingModId !== null || fixingIdx !== null}
                title="Update to the latest compatible version"
              >
                <ArrowUpCircle size={15} /> Update mod
              </button>
              <button
                class="ghost danger"
                on:click={() => fixRemoveMod(topSuspect.id, -1)}
                disabled={disablingModId !== null || fixingIdx !== null}
                title="Remove from manifest and delete the jar"
              >
                <Trash2 size={15} /> Remove mod
              </button>
            </div>
          {/if}
        </div>
      {:else}
        <div class="summary-icon"><CircleHelp size={22} /></div>
        <div class="summary-body">
          <span class="eyebrow">Diagnosis summary</span>
          <h1>No likely mod identified yet</h1>
          <p class="summary-copy">Select a crash report or run Minecraft once, then refresh. Diagnose will compare crash-log identifiers with installed manifest mods.</p>
        </div>
      {/if}
    </section>

    {#if problems.length > 0}
      <section class="problems-panel panel">
        <div class="problems-head">
          <h2><ListChecks size={16} /> Problems <span class="count">{problems.length}</span></h2>
          <div class="problems-legend">
            {#each ["critical", "error", "warning", "info"] as sev}
              {@const n = problems.filter((p) => p.severity === sev).length}
              {#if n > 0}<span class="legend-pill {sev}">{n} {sev}</span>{/if}
            {/each}
          </div>
        </div>
        <div class="problems-list">
          {#each problems as problem (problem.id)}
            <div class="problem-row {problem.severity}">
              <span class="sev-dot"></span>
              <div class="problem-body">
                <div class="problem-title">
                  <strong>{problem.title}</strong>
                  <small class="problem-source">{problem.source}</small>
                </div>
                <p class="problem-detail">{problem.detail}</p>
                {#if problem.actions.length}
                  <div class="problem-actions">
                    {#each problem.actions.slice(0, 4) as action (action.kind + (action.modId ?? ""))}
                      <button
                        class="primary small"
                        on:click={() => applyHintFixAction({ id: problem.id, title: problem.title, severity: problem.severity, detail: problem.detail, steps: [], relatedMods: [], fix: null, fixes: [] }, action)}
                        disabled={applyingHintId !== null}
                        title={action.modId ? `Target: ${action.modId}` : "System-level fix"}
                      >
                        <Wrench size={13} /> {action.label}
                      </button>
                    {/each}
                    {#if problem.actions.length > 4}
                      <small class="more-fixes">+{problem.actions.length - 4} more</small>
                    {/if}
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <div class="stats">
      <div class="stat-card" class:danger={diagnosis.reports.length > 0}>
        <strong>{diagnosis.reports.length}</strong>
        <span>Crash reports</span>
      </div>
      <div class="stat-card" class:danger={suspected.length > 0}>
        <strong>{suspected.length}</strong>
        <span>Suspected mods</span>
      </div>
      <div class="stat-card" class:danger={errorCount > 0} class:warning={errorCount === 0 && warningCount > 0}>
        <strong>{errorCount + warningCount}</strong>
        <span>Graph issues</span>
      </div>
      <div class="stat-card" class:accent={diagnosis.latestLog.exists}>
        <strong>{diagnosis.latestLog.exists ? diagnosis.latestLog.signals.length : 0}</strong>
        <span>latest.log signals</span>
      </div>
    </div>

    {#if dedupedHints.length > 0}
      <section class="hints-panel panel">
        <h2><Lightbulb size={16} /> Recommended fixes</h2>
        <div class="hints-list">
          {#each dedupedHints as hint (hint.id)}
            <div class="hint-card {hint.severity.toLowerCase()}">
              <div class="hint-head">
                <strong>{hint.title}</strong>
                {#if applyingHintId === hint.id}
                  <span class="fixing-spinner">Applying…</span>
                {/if}
              </div>
              <p class="hint-detail">{hint.detail}</p>
              {#if hint.steps?.length}
                <ol class="hint-steps">
                  {#each hint.steps as step (step)}
                    <li>{step}</li>
                  {/each}
                </ol>
              {/if}
              {#if hint.relatedMods?.length}
                <div class="related-mods">
                  {#each hint.relatedMods as modId (modId)}
                    <span class="mod-pill">{modId}</span>
                  {/each}
                </div>
              {/if}
              {#if (hint.fixes && hint.fixes.length) || hint.fix}
                <div class="hint-actions">
                  {#each (hint.fixes && hint.fixes.length ? hint.fixes : hint.fix ? [hint.fix] : []) as action (action.kind + (action.modId ?? ""))}
                    <button
                      class="primary small"
                      on:click={() => applyHintFixAction(hint, action)}
                      disabled={applyingHintId !== null}
                      title={action.modId ? `Target: ${action.modId}` : "System-level fix"}
                    >
                      <Wrench size={14} />
                      {action.label}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <div class="diagnose-grid">
      <aside class="reports panel">
        <h2><Bug size={16} /> Crash reports</h2>
        {#if diagnosis.crashReportStale}
          <div class="muted-box stale-warn">
            Newest crash-report is older than <code>logs/latest.log</code> (game launched since). Analyzing the live log — click a report below only if you want that historical crash.
          </div>
        {/if}
        <button
          type="button"
          class="report-card"
          class:selected={preferLatestLog || !selectedReportId}
          on:click={() => chooseLatestLog()}
        >
          <strong>logs/latest.log</strong>
          <span>
            {#if diagnosis.latestLog.exists}
              Live session · {diagnosis.latestLog.signals.length} signals
            {:else}
              Missing
            {/if}
          </span>
        </button>
        {#if diagnosis.reports.length === 0}
          <div class="muted-box">No files in <code>crash-reports/*.txt</code>.</div>
        {:else}
          {#each diagnosis.reports as report (report.id)}
            <button class="report-card" class:selected={!preferLatestLog && selectedReportId === report.id} on:click={() => chooseReport(report.id)}>
              <strong>{report.name}</strong>
              <span>{formatBytes(report.size)} · {formatDate(report.modified)}</span>
            </button>
          {/each}
        {/if}

        <h2 class="log-title"><Terminal size={16} /> latest.log status</h2>
        {#if diagnosis.latestLog.exists}
          <div class="log-status ok">Found · {diagnosis.latestLog.signals.length} parser signals</div>
        {:else}
          <div class="log-status">Missing: run a Test profile to create logs/latest.log.</div>
        {/if}

        <h2 class="log-title"><Terminal size={16} /> launcher.log</h2>
        {#if diagnosis.launcherLog.exists}
          <div class="log-status ok">Found · {diagnosis.launcherLog.signals.length} parser signals</div>
        {:else}
          <div class="log-status">Placeholder will be created by Diagnose.</div>
        {/if}

        {#if graphDiagnostics.length > 0}
          <h2 class="log-title"><AlertTriangle size={16} /> Conflicts & Issues</h2>
          {#each graphDiagnostics as diag, idx (`${diag.code}-${idx}`)}
            <div class="conflict-card {diag.severity.toLowerCase()}">
              <div class="conflict-header">
                <strong>{diag.code}</strong>
                {#if fixingIdx === idx}
                  <span class="fixing-spinner">Fixing...</span>
                {/if}
              </div>
              <p style="font-size: 12px; margin: 4px 0; color: var(--text-muted);">{diag.message}</p>
              {#if diag.relatedNodes && diag.relatedNodes.length > 0}
                <div class="related-mods">
                  {#each diag.relatedNodes as node (node)}
                    <span class="mod-pill">{node}</span>
                  {/each}
                </div>
              {/if}
              <div class="conflict-actions">
                {#if diag.code === "MISSING_DEPENDENCY" && diag.relatedNodes?.length >= 2}
                  {@const missingModId = diag.relatedNodes[1].startsWith("mod:") ? diag.relatedNodes[1].slice(4) : diag.relatedNodes[1]}
                  <button class="ghost mini" on:click={() => fixMissingDependency(missingModId, idx)} disabled={fixingIdx !== null}>
                    <Download size={14} /> Install {missingModId}
                  </button>
                {:else if diag.code === "MOD_CONFLICT" && diag.relatedNodes?.length >= 2}
                  {@const modA = diag.relatedNodes[0].startsWith("mod:") ? diag.relatedNodes[0].slice(4) : diag.relatedNodes[0]}
                  {@const modB = diag.relatedNodes[1].startsWith("mod:") ? diag.relatedNodes[1].slice(4) : diag.relatedNodes[1]}
                  <button class="ghost mini" on:click={() => fixDisableMod(modA, idx)} disabled={fixingIdx !== null || disablingModId !== null} title="Rename to .jar.disabled">
                    <Ban size={14} /> Disable {modA}
                  </button>
                  <button class="ghost mini" on:click={() => fixDisableMod(modB, idx)} disabled={fixingIdx !== null || disablingModId !== null} title="Rename to .jar.disabled">
                    <Ban size={14} /> Disable {modB}
                  </button>
                  <button class="ghost mini danger" on:click={() => fixRemoveMod(modA, idx)} disabled={fixingIdx !== null || disablingModId !== null}>
                    <Trash2 size={14} /> Remove {modA}
                  </button>
                  <button class="ghost mini danger" on:click={() => fixRemoveMod(modB, idx)} disabled={fixingIdx !== null || disablingModId !== null}>
                    <Trash2 size={14} /> Remove {modB}
                  </button>
                {:else if diag.code === "DUPLICATE_MOD"}
                  <button class="ghost mini" on:click={() => fixDeduplicate(idx)} disabled={fixingIdx !== null}>
                    <GitMerge size={14} /> Review duplicates
                  </button>
                {:else}
                  <button class="ghost mini" on:click={() => createFixPlan()} disabled={fixingIdx !== null}>
                    <Wrench size={14} /> Create fix plan
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        {/if}
      </aside>

      <section class="reader panel">
        <div class="panel-header">
          <div>
            <h2><FileText size={16} /> {selectedReport?.summary.name ?? "logs/latest.log"}</h2>
            <p>
              {#if selectedReport}
                {selectedReport.signals.length} parser signals · {selectedReport.suspectedMods.length} report suspects
              {:else if diagnosis.latestLog.exists}
                Analyzing live <code>logs/latest.log</code> · {diagnosis.latestLog.signals.length} signals · {diagnosis.latestLog.suspectedMods.length} suspects
              {:else}
                Run Minecraft to create logs/latest.log, or select a crash-report.
              {/if}
            </p>
          </div>
        </div>

        {#if selectedReport}
          <div class="crash-preview">
            <div class="crash-preview-bar">
              <span>Crash log preview</span>
              <div class="preview-tools">
                <div class="find-box">
                  <Search size={13} />
                  <input
                    class="find-input"
                    placeholder="Find in log…"
                    bind:value={logQuery}
                    on:input={() => recomputeLogMatches(selectedReport?.content ?? "")}
                  />
                  {#if logQuery}
                    <span class="find-count">{logMatches.length ? `${activeMatch + 1}/${logMatches.length}` : "0/0"}</span>
                    <button class="find-nav" on:click={() => jumpToMatch(-1)} disabled={!logMatches.length} title="Previous">↑</button>
                    <button class="find-nav" on:click={() => jumpToMatch(1)} disabled={!logMatches.length} title="Next">↓</button>
                    <button class="find-nav" on:click={() => { logQuery = ""; logMatches = []; activeMatch = 0; }} title="Clear">✕</button>
                  {/if}
                </div>
                <small>{formatBytes(selectedReport.summary.size)}</small>
              </div>
            </div>
            {#if selectedReport.sections && selectedReport.sections.length}
              <div class="toc">
                {#each selectedReport.sections as section (section.title + section.startLine)}
                  <button class="toc-item" on:click={() => scrollLogToLine(section.startLine)} title={section.preview}>
                    {section.title}
                  </button>
                {/each}
              </div>
            {/if}
            <pre class="report-content" bind:this={logPreEl}>
              {#each selectedReport.content.split("\n") as line, i}
                <div
                  class="log-line"
                  class:active={logQuery && i === logMatches[activeMatch]?.line}
                  class:signal={signalLineMap.has(i + 1)}
                  data-sig={signalLineMap.get(i + 1) ?? ""}
                >
                  {#if signalLineMap.has(i + 1)}<span class="sig-marker" title={signalLineMap.get(i + 1)??""}>{signalLineMap.get(i + 1) ?? ""}</span>{/if}{@html highlightLog(line, logQuery)}
                </div>
              {/each}
            </pre>
          </div>
        {:else}
          <div class="empty inline">No crash report to open yet.</div>
        {/if}

        <details class="raw-section latest-log">
          <summary>
            <span><Terminal size={16} /> latest.log tail</span>
            <small>{diagnosis.latestLog.exists ? "last parser window" : "missing"}</small>
          </summary>
          <pre class="log-content">{diagnosis.latestLog.tail || "logs/latest.log will appear here after a Test run."}</pre>
        </details>

        <details class="raw-section latest-log">
          <summary>
            <span><Terminal size={16} /> launcher.log tail</span>
            <small>{diagnosis.launcherLog.exists ? "launcher parser window" : "missing"}</small>
          </summary>
          <pre class="log-content">{diagnosis.launcherLog.tail || "launcher.log will appear here after launcher events."}</pre>
        </details>
      </section>

      <aside class="inspector panel">
        {#if signalGroups.length > 0}
          <h2><Stethoscope size={16} /> Signal groups</h2>
          <div class="signal-groups">
            {#each signalGroups as group (group.title)}
              <div class="signal-group">
                <strong>{group.title}</strong>
                <span>{group.hint}</span>
                <small>{group.items.length} signal(s)</small>
                <p>{hypothesisForGroup(group.title)}</p>
                <code>{group.items[0].source}:{group.items[0].lineNumber}</code>
              </div>
            {/each}
          </div>
        {/if}

        {#if selectedReport?.modEntries?.length}
          <h2><FileText size={16} /> Crash Mods section</h2>
          <div class="mod-entry-list">
            {#each selectedReport.modEntries.slice(0, 18) as entry (`${entry.id}-${entry.version ?? ""}`)}
              <div class="mod-entry">
                <strong>{entry.id}</strong>
                <span>{entry.name ?? "unknown name"}</span>
                <code>{entry.version ?? "unknown version"}</code>
              </div>
            {/each}
          </div>
        {/if}

        <h2><AlertTriangle size={16} /> Suspected mods</h2>
        {#if suspected.length === 0}
          <div class="muted-box">No suspected mods extracted yet. Parser watches <code>Mod File</code>, <code>Caused by</code>, <code>Mixin</code>, <code>Exception</code>, <code>OpenGL</code>, performance and resource-warning lines.</div>
        {:else}
          <div class="suspects">
            {#each suspected as mod (mod.id)}
              <div class="suspect-card" class:unresolved={!mod.knownInManifest}>
                <div class="suspect-head">
                  <div>
                    <strong>{mod.name}</strong>
                    <span class="suspect-identity">{mod.id}{mod.version ? ` · ${mod.version}` : ""}</span>
                  </div>
                  <b>{mod.confidence}%</b>
                </div>
                <div class="badges">
                  <small class:known={mod.knownInManifest}>
                    {#if mod.knownInManifest}<BadgeCheck size={13} />{:else}<CircleHelp size={13} />{/if}
                    {mod.knownInManifest ? "Installed mod" : "Unresolved log identifier"}
                  </small>
                  {#if mod.fileName}<small>{mod.fileName}</small>{/if}
                </div>
                {#each mod.evidence.slice(0, 3) as item (`${item.source}-${item.lineNumber}-${item.kind}`)}
                  <div class="evidence">
                    <code>{item.kind} · {item.source}:{item.lineNumber}</code>
                    <p>{item.text}</p>
                  </div>
                {/each}
                {#if mod.knownInManifest}
                  <div class="suspect-actions">
                    <button
                      class="ghost mini"
                      on:click={() => fixDisableMod(mod.id)}
                      disabled={disablingModId !== null || fixingIdx !== null}
                      title="Rename jar to .disabled"
                    >
                      <Ban size={13} />
                      {disablingModId === mod.id ? "Disabling…" : "Disable"}
                    </button>
                    <button
                      class="ghost mini danger"
                      on:click={() => fixRemoveMod(mod.id, -1)}
                      disabled={disablingModId !== null || fixingIdx !== null}
                    >
                      <Trash2 size={13} /> Remove
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <h2 class="changes-title"><History size={16} /> Last changes</h2>
        {#if diagnosis.recentSnapshots.length === 0}
          <div class="muted-box">No snapshots yet. Risky changes should create snapshots before edits.</div>
        {:else}
          <div class="snapshot-list">
            {#each diagnosis.recentSnapshots as snapshot (snapshot.id)}
              <div class="snapshot-row">
                <strong>{snapshot.name}</strong>
                <span>{snapshot.createdAt} · {snapshot.reason}</span>
                {#if snapshot.changedFiles?.length}
                  <small>{snapshot.changedFiles.length} tracked files: {snapshot.changedFiles.slice(0, 3).join(", ")}</small>
                {:else}
                  <small>manifest/lockfile snapshot</small>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        {#if plan}
          <div class="plan-card">
            <h2><Wrench size={16} /> Fix plan</h2>
            <p>{plan.summary}</p>
            <div class="plan-meta">
              <span>Risk: {plan.risk}</span>
              <span>{plan.requiresSnapshot ? "Snapshot required" : "No snapshot required"}</span>
            </div>
            {#if plan.actions?.length}
              <ul>
                {#each plan.actions as action, actionIdx (actionIdx)}
                  <li>{actionLabel(action)}</li>
                {/each}
              </ul>
              <button on:click={applyFix} disabled={applying}>
                {applying ? "Applying..." : "Apply fix plan"}
              </button>
            {:else}
              <div class="muted-box compact">No deterministic file operation proposed. Use the notes as an investigation checklist.</div>
            {/if}
          </div>
        {/if}
      </aside>
    </div>

    {#if aiContext || aiAnalysis}
      <section class="ai-panel panel">
        <h2><MessageCircle size={16} /> AI Crash Explanation</h2>
        {#if aiAnalysis}
          <div class="ai-analysis">
            <p class="ai-human">{aiAnalysis.human_explanation ?? aiAnalysis.humanExplanation}</p>
            <div class="ai-stats">
              <div class="ai-stat"><strong>{Math.round(((aiAnalysis.confidence ?? 0) * 100))}%</strong> confidence</div>
              <div class="ai-stat"><strong>{(aiAnalysis.suspected_mods ?? aiAnalysis.suspectedMods ?? []).length}</strong> suspected mods</div>
              <div class="ai-stat"><strong>{aiPlanActions(aiAnalysis).length}</strong> actions</div>
              {#if aiAnalysis.diagnoseMode}
                <div class="ai-stat"><strong>{aiAnalysis.diagnoseMode}</strong> mode</div>
              {/if}
              {#if aiAnalysis.networkUsed}
                <div class="ai-stat"><strong>network</strong> used</div>
              {:else if aiAnalysis.swarmEnabled === false}
                <div class="ai-stat"><strong>local-only</strong> (swarm off)</div>
              {/if}
              {#if aiAnalysis.pendingPlanPath}
                <div class="ai-stat"><strong>pending</strong> plan saved</div>
              {/if}
            </div>
            {#if aiAnalysis.validation && aiAnalysis.validation.ok === false}
              <div class="notice error">Plan validation failed: {(aiAnalysis.validation.errors ?? []).join("; ")}</div>
            {:else if (aiAnalysis.needs_user_review ?? aiAnalysis.needsUserReview) !== false}
              <div class="notice warning">Review the plan, then apply. Nothing runs until you confirm.</div>
            {:else}
              <div class="notice warning">Low-risk plan — still requires explicit apply.</div>
            {/if}
            {#if (aiAnalysis.suspected_mods ?? aiAnalysis.suspectedMods)?.length}
              <div class="ai-list">
                <strong>Suspected mods</strong>
                <ul>
                  {#each (aiAnalysis.suspected_mods ?? aiAnalysis.suspectedMods) as modId (modId)}
                    <li><code>{modId}</code></li>
                  {/each}
                </ul>
              </div>
            {/if}
            {#if aiPlanActions(aiAnalysis).length}
              <div class="ai-list">
                <strong>Recommended actions</strong>
                <ul>
                  {#each aiPlanActions(aiAnalysis) as action, aIdx (aIdx)}
                    <li>
                      <strong>{action.op ?? action.action_type ?? action.actionType}</strong>
                      {#if action.modId ?? action.mod_id}<code>{action.modId ?? action.mod_id}</code>{/if}
                      {#if action.version}<code>@{action.version}</code>{/if}
                      {#if action.path}<code>{action.path}</code>{/if}
                      <span>{action.reason ?? action.description ?? ""}</span>
                      <small>risk: {action.risk}</small>
                    </li>
                  {/each}
                </ul>
              </div>
              <button
                class="secondary"
                disabled={aiApplyBusy || (aiAnalysis.validation && aiAnalysis.validation.ok === false)}
                on:click={applyAiPlan}
              >
                {aiApplyBusy ? "Applying…" : "Apply plan"}
              </button>
            {/if}
            <div class="ai-feedback">
              <span>Was this helpful?</span>
              <button class="secondary" disabled={aiFeedbackBusy} on:click={() => sendAiFeedback(true)}>Helped</button>
              <button class="secondary" disabled={aiFeedbackBusy} on:click={() => sendAiFeedback(false)}>Wrong</button>
              <button class="secondary" disabled={authorBusy} on:click={() => openAuthorForm({ fromAnalysis: true })}>
                <BookMarked size={14} /> Save as KB case
              </button>
              {#if aiFeedbackMsg}<small>{aiFeedbackMsg}</small>{/if}
            </div>
          </div>
        {:else}
          <p class="ai-desc">AI analysis failed or is incomplete. You can still use the raw prompt fallback below.</p>
        {/if}
        {#if aiContext}
          <div class="ai-stats">
            <div class="ai-stat"><strong>{aiContext.findingsCount}</strong> findings</div>
            <div class="ai-stat"><strong>{aiContext.similarCaseCount ?? 0}</strong> KB hits</div>
            <div class="ai-stat"><strong>{aiContext.aiModel ?? "—"}</strong> model</div>
            <div class="ai-stat"><strong>{aiContext.inventorySummary?.mods ?? aiContext.context?.installedModCount ?? 0}</strong> mods</div>
            <div class="ai-stat"><strong>{aiContext.inventorySummary?.configs ?? aiContext.context?.inventory?.configFiles?.length ?? 0}</strong> configs</div>
            <div class="ai-stat"><strong>{aiContext.inventorySummary?.packs ?? ((aiContext.context?.inventory?.resourcepacks?.length ?? 0) + (aiContext.context?.inventory?.datapacks?.length ?? 0))}</strong> packs</div>
            <div class="ai-stat"><strong>{aiContext.promptLength}</strong> chars prompt</div>
          </div>
          <button class="secondary" on:click={() => (aiShowPrompt = !aiShowPrompt)}>
            {aiShowPrompt ? "Hide" : "Show"} prompt
          </button>
          <button class="secondary" on:click={copyAiPrompt}>
            <Copy size={14} /> Copy prompt
          </button>
          {#if aiShowPrompt}
            <pre class="ai-prompt-text">{aiPrompt}</pre>
          {/if}
        {/if}
      </section>
    {/if}

    {#if authorOpen}
      <section class="ai-panel panel author-kb-panel">
        <h2><BookMarked size={16} /> Author KB case</h2>
        <p class="crash-intro">
          Pack-author tool: bind the current crash fingerprint to a solution + executable actions.
          Saved locally under <code>.tuffbox/crash_kb/</code>; export JSON has <strong>no notes</strong> (safe to upload to your private server).
        </p>
        {#if authorMsg}<div class="notice success">{authorMsg}</div>{/if}
        <div class="author-grid">
          <label>
            Case id
            <input bind:value={authorId} placeholder="authored-mixin-create" />
          </label>
          <label class="full">
            Solution (what fixed it)
            <textarea rows="3" bind:value={authorSolution} placeholder="Install Indium matching Sodium for Fabric 1.20.1"></textarea>
          </label>
          <label>
            Suspected mods (comma-separated)
            <input bind:value={authorSuspected} placeholder="sodium, indium" />
          </label>
          <label>
            Symptoms (one per line)
            <textarea rows="3" bind:value={authorSymptoms}></textarea>
          </label>
          <label class="full">
            Actions JSON (executable ActionPlan ops)
            <textarea class="mono" rows="8" bind:value={authorActionsJson} spellcheck="false"></textarea>
          </label>
          <label class="full">
            Notes (author-only, never exported)
            <textarea rows="2" bind:value={authorNotes} placeholder="Internal: saw this after updating Iris…"></textarea>
          </label>
          {#if authorFingerprint}
            <div class="full muted-box compact">
              <strong>Fingerprint</strong>
              <code>{authorFingerprint.key}</code>
              <div class="author-fp-meta">
                <span>{authorFingerprint.exception || "—"}</span>
                {#if authorFingerprint.modFile}<span>mod: {authorFingerprint.modFile}</span>{/if}
                {#if authorFingerprint.loader}<span>{authorFingerprint.loader} {authorFingerprint.mcMajor}</span>{/if}
              </div>
            </div>
          {/if}
        </div>
        <div class="row-actions" style="gap:8px;flex-wrap:wrap;margin-top:12px">
          <button disabled={authorBusy || !authorSolution.trim()} on:click={saveAuthorCase}>
            {authorBusy ? "Saving…" : "Save case"}
          </button>
          <button class="secondary" disabled={authorBusy} on:click={() => openAuthorForm({ fromAnalysis: !!aiAnalysis })}>
            Refresh from crash
          </button>
          <button class="secondary" disabled={!authorExportPreview} on:click={() => copyAuthorExport()}>
            <Copy size={14} /> Copy export JSON
          </button>
          <button class="secondary" on:click={openAuthorExportFolder}>
            <FolderOpen size={14} /> Open export folder
          </button>
          <button class="ghost" on:click={() => (authorOpen = false)}>Close</button>
        </div>
        {#if authorExportPreview}
          <pre class="ai-prompt-text">{authorExportPreview}</pre>
        {/if}
        {#if authorCases.length}
          <div class="ai-list" style="margin-top:16px">
            <strong>Authored cases in this project ({authorCases.length})</strong>
            <ul>
              {#each authorCases as c (c.id)}
                <li>
                  <code>{c.id}</code>
                  <span>{c.solution}</span>
                  <button class="ghost mini" on:click={() => copyAuthorExport(c.id)}>Copy JSON</button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </section>
    {/if}

    {#if crashFindings.length > 0 && !(diagnosis.sessionHealthy && preferLatestLog)}
      <section class="crash-assistant panel">
        <h2><Zap size={16} /> Crash Assistant ({crashFindings.length} finding{crashFindings.length > 1 ? "s" : ""})</h2>
        <p class="crash-intro">Analyzes the selected crash report, <code>logs/latest.log</code>, and currently installed mods. Apply fixes one at a time, then re-test.</p>
        <div class="crash-list">
          {#each crashFindings as f (f.code + f.title)}
            <div class="crash-card {f.severity}">
              <div class="crash-card-header">
                <span class="crash-sev {f.severity}">{f.severity}</span>
                <strong>{f.title}</strong>
                <code class="crash-code">{f.code}</code>
              </div>
              <p>{f.description}</p>
              {#if f.evidence}
                <pre class="crash-evidence">{f.evidence}</pre>
              {/if}
              {#if f.autoFix}
                <div class="crash-fix">
                  <strong>Suggested:</strong> {f.autoFix}
                </div>
              {/if}
              {#if f.fixes?.length}
                <div class="crash-fix-actions">
                  {#each f.fixes.slice(0, 6) as action, i (action.kind + (action.modId ?? "") + i)}
                    <button
                      class="secondary small"
                      disabled={!$projectPath || applyingHintId === `ca:${f.code}`}
                      on:click={() => applyCrashFindingFix(f, action)}
                    >
                      {applyingHintId === `ca:${f.code}` ? "Applying…" : action.label}
                    </button>
                  {/each}
                  {#if f.fixes.length > 6}
                    <small class="more-fixes">+{f.fixes.length - 6} more</small>
                  {/if}
                </div>
              {/if}
              {#if f.references?.length}
                <div class="crash-refs">
                  {#each f.references as ref (ref)}
                    <a href={ref} target="_blank" class="crash-link">{ref}</a>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
        {#if crashMcreator.length > 0}
          <div class="crash-mcreator">
            <strong>MCreator mods detected ({crashMcreator.length})</strong>
            <p>These mods were built with MCreator and may have compatibility issues:</p>
            <div class="crash-tags">{#each crashMcreator as m (m)}<code>{m}</code>{/each}</div>
          </div>
        {/if}

        {#if crashClassFinder.length > 0}
          <div class="crash-classfinder">
            <strong>Class finder results</strong>
            <p>Missing classes found in crash logs and their owning mods:</p>
            <div class="crash-tags">
              {#each crashClassFinder as cf (cf.className + cf.modId)}
                <div class="class-match"><code>{cf.className}</code> → <span>{cf.modId}</span></div>
              {/each}
            </div>
          </div>
        {/if}

        {#if crashSupportMsg}
          <div class="crash-support">
            <button on:click={() => (crashShowSupport = !crashShowSupport)}>
              {crashShowSupport ? "Hide" : "Show"} support message
            </button>
            <button on:click={copySupportMsg} class="secondary">Copy to clipboard</button>
            {#if crashShowSupport}
              <pre class="crash-support-msg">{crashSupportMsg}</pre>
            {/if}
          </div>
        {/if}
      </section>
    {/if}

    {#if oreFindings.length > 0}
      <section class="ore-gen panel">
        <h2><Database size={16} /> Ore generation scan ({oreFindings.length})</h2>
        <div class="ore-list">
          {#each oreFindings as ore (ore.configFile + ore.resource + (ore.enabledKey ?? ''))}
            <div class="ore-card">
              <div class="ore-header">
                <strong>{ore.resource}</strong>
                <span class="ore-conf">{ore.confidence}</span>
              </div>
              <div class="ore-key">{ore.enabledKey} = {ore.enabledValue}</div>
              <div class="ore-meta">
                {#if ore.veinSize}<span>vein: {ore.veinSize[1]}</span>{/if}
                {#if ore.minHeight}<span>y: {ore.minHeight[1]}–{ore.maxHeight?.[1] ?? "?"}</span>{/if}
                {#if ore.spawnsPerChunk}<span>/chunk: {ore.spawnsPerChunk[1]}</span>{/if}
              </div>
              <code class="ore-file">{ore.configFile}</code>
              {#if ore.knownMod}<small class="ore-known">known: {ore.knownMod}</small>{/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if duplicateFindings.length > 0}
      <section class="duplicates panel">
        <h2><GitMerge size={16} /> Duplicate items ({duplicateFindings.length})</h2>
        <div class="duplicate-list">
          {#each duplicateFindings as dup (dup.material + dup.itemType + (dup.dominantItem ?? ''))}
            <div class="duplicate-card">
              <strong>{dup.material}</strong>
              <span class="dup-type">{dup.itemType}</span>
              {#if dup.dominantItem}
                <code class="dup-dominant">{dup.dominantItem}</code>
              {/if}
              {#if dup.alternatives?.length}
                <div class="dup-alts">
                  {#each dup.alternatives as alt (alt.itemId)}
                    <span class="dup-alt">{alt.itemId}</span>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if unifyConfigResult}
      <section class="unify-result panel">
        <h2><Zap size={16} /> Unify config generated</h2>
        <div class="unify-meta">
          <p>Materials: {unifyConfigResult.materials?.length ?? 0}</p>
          <p>Item types: {unifyConfigResult.itemTypes?.length ?? 0}</p>
          <p>Scripts: {unifyConfigResult.scripts?.length ?? 0}</p>
        </div>
        <pre class="unify-preview">{JSON.stringify(unifyConfigResult, null, 2)}</pre>
      </section>
    {/if}

    {#if wrongLoaderJars.length > 0}
      <section class="wrong-loader panel">
        <h2><AlertTriangle size={16} /> Wrong-loader jars in mods/</h2>
        <p style="color: var(--text-muted); font-size: 12px; margin: 0 0 12px;">
          These .jar files were detected as built for a different mod loader than your project uses. They can cause crashes or silent failures.
        </p>
        <div class="wrong-loader-list">
          {#each wrongLoaderJars as jar (jar.fileName)}
            <div class="wrong-loader-card">
              <div class="wrong-loader-main">
                <strong>{jar.fileName}</strong>
                <span class="wrong-reason">{jar.reason}</span>
              </div>
              <div class="wrong-loader-actions">
                <button class="ghost mini" on:click={() => disableWrongJar(jar.fileName)} disabled={wrongLoaderFixing === jar.fileName}>
                  {wrongLoaderFixing === jar.fileName ? "..." : "Disable (.jar.disabled)"}
                </button>
                <button class="ghost mini danger" on:click={() => removeWrongJar(jar.fileName)} disabled={wrongLoaderFixing === jar.fileName}>
                  <Trash2 size={14} /> Remove
                </button>
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <section class="graph-health panel">
      <h2><Stethoscope size={16} /> Graph diagnostics</h2>
      {#if graphDiagnostics.length === 0}
        <div class="empty success">
          <AlertCircle size={28} color="#1bd96a" />
          <p>No dependency graph issues found.</p>
        </div>
      {:else}
        <div class="diagnostic-list">
          {#each graphDiagnostics as d, dIdx (`${d.code}-${dIdx}`)}
            <div class="diag-card {d.severity.toLowerCase()}">
              <div class="diag-icon">
                <svelte:component this={icon(d.severity)} size={20} />
              </div>
              <div>
                <div class="meta">
                  <span class="severity">{d.severity}</span>
                  <span class="code">{d.code}</span>
                </div>
                <p>{d.message}</p>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {:else}
    <div class="empty">Press refresh to load diagnosis.</div>
  {/if}
</div>

<AiConnectionModal bind:open={aiModalOpen} />

<style>
  .diagnostics { max-width: none; width: 100%; }
  .toolbar, .actions, .title, .primary-actions, .panel-header, .suspect-head, .meta, .plan-meta { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 10px; flex-wrap: wrap; }
  .title, h2 { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions .primary, .primary-actions .secondary, .primary-actions .ghost, .primary-actions .refresh { cursor: pointer; }
  .refresh { display: inline-flex; align-items: center; gap: 8px; }
  .analysis-tools { margin-bottom: 16px; border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); background: var(--bg-secondary); }
  .analysis-tools > summary { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 12px; color: var(--text-secondary); cursor: pointer; list-style: none; font-size: 12px; font-weight: 700; }
  .analysis-tools > summary::-webkit-details-marker { display: none; }
  .analysis-tools > summary span { display: flex; align-items: center; gap: 7px; }
  .analysis-tools .tools-hint { color: var(--text-muted); font-weight: 500; }
  .analysis-tools[open] .tools-hint :global(svg) { transform: rotate(180deg); }
  .analysis-tools .actions { padding: 0 12px 12px; border-top: 1px solid var(--border-color); padding-top: 12px; }
  h2 { display: flex; font-size: 14px; margin: 0 0 12px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .diagnosis-summary { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 14px; padding: 18px; margin-bottom: 16px; border: 1px solid rgba(245, 158, 11, 0.42); border-radius: var(--border-radius-lg); background: linear-gradient(135deg, rgba(245, 158, 11, 0.11), var(--bg-secondary) 65%); }
  .diagnosis-summary.neutral { border-color: var(--border-color); background: var(--bg-secondary); }
  .summary-icon { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 12px; color: var(--accent-warning); background: rgba(245, 158, 11, 0.13); }
  .neutral .summary-icon { color: var(--text-muted); background: var(--bg-tertiary); }
  .summary-body { min-width: 0; }
  .eyebrow { display: block; margin-bottom: 4px; color: var(--text-muted); font-size: 11px; font-weight: 800; letter-spacing: .08em; text-transform: uppercase; }
  .summary-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .summary-heading h1, .summary-body > h1 { margin: 0; color: var(--text-primary); font-size: 22px; line-height: 1.25; }
  .summary-heading strong { flex-shrink: 0; color: var(--accent-warning); font-size: 13px; }
  .summary-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-top: 7px; }
  .summary-meta code { color: var(--text-secondary); font-size: 12px; }
  .summary-meta span { display: inline-flex; align-items: center; gap: 5px; padding: 3px 8px; border-radius: 999px; color: var(--text-muted); background: var(--bg-tertiary); font-size: 11px; font-weight: 700; }
  .summary-meta span.installed { color: var(--accent-primary); }
  .mapping-note { margin: 12px 0 0; color: var(--text-secondary); font-size: 12px; }
  .mapping-note code { color: var(--accent-primary); }
  .summary-evidence { margin-top: 12px; padding: 10px 12px; border-left: 3px solid var(--accent-warning); border-radius: 0 10px 10px 0; background: var(--bg-tertiary); }
  .summary-evidence span { color: var(--text-muted); font-size: 11px; }
  .summary-evidence p, .summary-copy { margin: 5px 0 0; color: var(--text-secondary); font-size: 12px; line-height: 1.5; }
  .summary-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
  .summary-actions button { display: inline-flex; align-items: center; gap: 6px; }
  .suspect-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .ghost.danger { color: #fca5a5; }
  .ghost.danger:hover { color: #fecaca; }
  .stats { display: grid; grid-template-columns: repeat(4, minmax(120px, 1fr)); gap: 14px; margin-bottom: 16px; }
  .stat-card, .panel, .empty, .loading { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .stat-card { padding: 15px; display: flex; flex-direction: column; gap: 4px; }
  .stat-card strong { font-size: 26px; }
  .stat-card span, .muted-box, .panel-header p, .report-card span, .log-status, .snapshot-row span, .snapshot-row small, .suspect-head span { color: var(--text-muted); font-size: 12px; }
  .stat-card.danger { border-color: rgba(239, 68, 68, 0.35); background: rgba(239, 68, 68, 0.06); }
  .stat-card.warning { border-color: rgba(245, 158, 11, 0.35); background: rgba(245, 158, 11, 0.06); }
  .stat-card.accent { border-color: rgba(27, 217, 106, 0.3); background: rgba(27, 217, 106, 0.06); }
  .diagnose-grid { display: grid; grid-template-columns: 280px minmax(0, 1fr) 400px; gap: 16px; align-items: start; }
  .diagnose-grid > * { min-width: 0; }
  .reader { overflow: hidden; }
  .inspector { max-height: calc(100vh - 150px); overflow: auto; }
  .panel { padding: 16px; min-width: 0; }
  .panel-header { justify-content: space-between; gap: 12px; margin-bottom: 12px; }
  .panel-header h2 { margin: 0 0 4px; }
  .panel-header.small { margin: 18px 0 8px; }
  .panel-header.small span { color: var(--text-muted); font-size: 12px; }
  .report-card { width: 100%; background: var(--bg-tertiary); border: 1px solid var(--border-color); color: var(--text-secondary); padding: 11px; margin-bottom: 8px; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; text-align: left; transform: none; cursor: pointer; transition: border-color .12s ease, background .12s ease; }
  .report-card:hover, .report-card.selected { border-color: rgba(27, 217, 106, 0.35); background: rgba(27, 217, 106, 0.08); color: var(--text-primary); }
  .conflict-card { margin-bottom: 12px; padding: 12px; border-radius: 8px; border: 1px solid var(--border-color); background: var(--bg-tertiary); }
  .conflict-card.error { border-color: rgba(239, 68, 68, 0.5); }
  .conflict-card.warning { border-color: rgba(234, 179, 8, 0.5); }
  .conflict-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  .fixing-spinner { font-size: 11px; color: var(--accent-primary); animation: pulse 1.2s ease-in-out infinite; }
  .conflict-actions { display: flex; gap: 6px; margin-top: 8px; flex-wrap: wrap; }
  .conflict-actions :global(.mini.danger) { color: #f87171; }
  .conflict-actions :global(.mini.danger:hover) { background: rgba(239, 68, 68, 0.1); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
  .related-mods { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 8px; }
  .mod-pill { font-size: 11px; background: var(--bg-secondary); padding: 2px 6px; border-radius: 4px; border: 1px solid var(--border-color); }

  .log-title, .changes-title { margin-top: 20px; }
  .log-status { padding: 10px; border: 1px dashed var(--border-color); border-radius: 10px; }
  .log-status.ok { color: var(--accent-primary); border-color: rgba(27, 217, 106, 0.28); }
  pre { margin: 0; border-radius: 12px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow: auto; max-width: 100%; }
  .crash-preview {
    border: 1px solid var(--border-color);
    border-radius: 12px;
    background: #0d0d10;
    overflow: hidden;
  }
  .crash-preview-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 800;
    background: var(--bg-tertiary);
  }
  .crash-preview-bar small { color: var(--text-muted); font-weight: 500; }
  .crash-preview .report-content {
    margin: 0;
    max-height: 420px;
    overflow: auto;
    padding: 14px;
    color: #d4d4d8;
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-wrap;
    font-family: ui-monospace, monospace;
  }
  .raw-section { margin-top: 12px; border: 1px solid var(--border-color); border-radius: 12px; background: var(--bg-tertiary); overflow: hidden; }
  .raw-section > summary { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 11px 12px; cursor: pointer; color: var(--text-secondary); font-size: 12px; font-weight: 800; }
  .raw-section > summary span { display: flex; align-items: center; gap: 8px; }
  .raw-section > summary small { color: var(--text-muted); font-weight: 500; }
  .raw-section[open] > summary { border-bottom: 1px solid var(--border-color); }
  .report-content { max-height: 520px; padding: 16px; }
  .log-content { max-height: 250px; padding: 14px; color: #a1a1aa; }
  .suspects, .snapshot-list, .diagnostic-list, .signal-groups, .mod-entry-list { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .suspects { max-height: 520px; overflow: auto; padding-right: 4px; }
  .suspect-card, .snapshot-row, .diag-card, .plan-card, .signal-group, .mod-entry { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 12px; padding: 12px; }
  .mod-entry { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto; gap: 8px; align-items: center; }
  .mod-entry strong { color: var(--text-primary); }
  .mod-entry span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .signal-group { display: grid; gap: 5px; border-left: 4px solid rgba(27, 217, 106, .55); }
  .signal-group strong { color: var(--text-primary); }
  .signal-group span, .signal-group small, .signal-group p { color: var(--text-muted); font-size: 12px; }
  .signal-group p { margin: 2px 0; line-height: 1.45; }
  .suspect-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 10px; }
  .suspect-head strong { display: block; color: var(--text-primary); font-size: 15px; overflow-wrap: anywhere; }
  .suspect-card { border-left: 4px solid var(--accent-primary); min-width: 0; }
  .suspect-card.unresolved { border-left-color: var(--text-muted); }
  .suspect-identity { display: block; margin-top: 3px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; overflow-wrap: anywhere; word-break: break-all; }
  .suspect-head b { color: var(--accent-primary); }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; margin: 8px 0; }
  .badges small { display: inline-flex; align-items: center; gap: 5px; color: var(--text-muted); background: var(--bg-elevated); border-radius: 999px; padding: 3px 8px; }
  .badges small.known { color: var(--accent-primary); }
  .evidence { border-top: 1px solid var(--border-color); padding-top: 8px; margin-top: 8px; }
  .evidence code, .code { color: var(--text-muted); font-size: 11px; }
  .evidence p, .diag-card p, .plan-card p { margin: 5px 0 0; color: var(--text-secondary); line-height: 1.45; }
  .snapshot-row { display: flex; flex-direction: column; gap: 5px; }
  .snapshot-row strong { color: var(--text-primary); }
  .plan-card { margin-top: 16px; border-color: rgba(27, 217, 106, 0.32); background: rgba(27, 217, 106, 0.06); }
  .hints-panel { margin-top: 16px; border-color: rgba(96, 165, 250, 0.35); background: rgba(96, 165, 250, 0.04); }
  .hints-list { display: grid; gap: 10px; margin-top: 10px; }
  .hint-card { border: 1px solid var(--border-color); border-radius: 12px; padding: 12px; background: var(--bg-tertiary); }
  .hint-card.error { border-color: rgba(239, 68, 68, 0.4); background: rgba(239, 68, 68, 0.05); }
  .hint-card.warning { border-color: rgba(245, 158, 11, 0.4); background: rgba(245, 158, 11, 0.05); }
  .hint-card.info { border-color: rgba(96, 165, 250, 0.3); background: rgba(96, 165, 250, 0.05); }
  .hint-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .hint-head strong { color: var(--text-primary); }
  .hint-detail { margin: 6px 0 0; color: var(--text-secondary); line-height: 1.45; font-size: 13px; }
  .hint-steps { margin: 8px 0 0 18px; color: var(--text-muted); font-size: 12px; line-height: 1.5; }
  .hint-steps li { margin: 2px 0; }
  .hint-actions { margin-top: 10px; display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .primary.small { font-size: 12px; padding: 6px 10px; white-space: nowrap; flex: 0 0 auto; min-width: 0; max-width: 100%; }
  .fixing-spinner { color: var(--accent); font-size: 12px; }
  .plan-meta { justify-content: space-between; gap: 8px; color: var(--text-muted); font-size: 12px; margin: 10px 0; }
  .plan-card ul { margin: 8px 0 0 18px; color: var(--text-secondary); font-size: 12px; }
  .graph-health { margin-top: 16px; }
  .wrong-loader { margin-top: 16px; border-color: rgba(245, 158, 11, 0.35); background: rgba(245, 158, 11, 0.04); }
  .wrong-loader-list { display: grid; gap: 8px; }
  .wrong-loader-card { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; padding: 12px; border-radius: 12px; border: 1px solid rgba(245, 158, 11, 0.2); background: var(--bg-tertiary); }
  .wrong-loader-main { display: grid; gap: 5px; min-width: 0; }
  .wrong-loader-main strong { color: var(--text-primary); }
  .wrong-reason { color: var(--text-muted); font-size: 12px; line-height: 1.4; }
  .wrong-loader-actions { display: flex; gap: 6px; flex-shrink: 0; }
  .perf-audit { margin-top: 16px; border-color: rgba(96,165,250,.3); background: rgba(96,165,250,.03); }
  .perf-list { display: grid; gap: 8px; }
  .perf-card { padding: 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); display: grid; gap: 4px; }
  .perf-card.warning { border-color: rgba(245,158,11,.3); }
  .perf-card.info { border-color: rgba(96,165,250,.25); }
  .perf-card-header { display: flex; justify-content: space-between; align-items: center; }
  .perf-card strong { color: var(--text-primary); font-size: 13px; }
  .perf-severity { font-size: 10px; text-transform: uppercase; font-weight: 800; color: var(--text-muted); padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); }
  .perf-card p { color: var(--text-muted); font-size: 12px; margin: 0; line-height: 1.4; }
  .perf-file { font-size: 11px; color: var(--accent-primary); word-break: break-all; }
  .ore-gen { margin-top: 16px; border-color: rgba(245,158,11,.3); background: rgba(245,158,11,.03); }
  .ore-list { display: grid; gap: 6px; }
  .ore-card { padding: 10px 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); display: grid; gap: 4px; }
  .ore-header { display: flex; justify-content: space-between; align-items: center; }
  .ore-header strong { color: var(--text-primary); text-transform: capitalize; }
  .ore-conf { font-size: 10px; text-transform: uppercase; font-weight: 800; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); color: var(--accent-secondary); }
  .ore-key { font-family: ui-monospace, monospace; font-size: 12px; color: var(--accent-primary); }
  .ore-meta { display: flex; gap: 10px; flex-wrap: wrap; }
  .ore-meta span { font-size: 11px; color: var(--text-muted); }
  .ore-file { font-size: 10px; color: var(--text-muted); }
  .ore-known { font-size: 10px; color: var(--accent-primary); }
  .crash-assistant { margin-top: 16px; border-color: rgba(239,68,68,.3); background: rgba(239,68,68,.02); }
  .crash-intro { margin: 0 0 10px; color: var(--text-muted); font-size: 12px; }
  .crash-list { display: grid; gap: 8px; }
  .crash-card { padding: 14px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); display: grid; gap: 8px; }
  .crash-card.critical { border-color: rgba(239,68,68,.5); background: rgba(239,68,68,.06); }
  .crash-card.error { border-color: rgba(239,68,68,.35); }
  .crash-card.warning { border-color: rgba(245,158,11,.35); }
  .crash-card-header { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .crash-card strong { color: var(--text-primary); font-size: 14px; }
  .crash-sev { font-size: 10px; text-transform: uppercase; font-weight: 800; padding: 3px 7px; border-radius: 4px; }
  .crash-sev.critical, .crash-sev.error { background: rgba(239,68,68,.15); color: #fca5a5; }
  .crash-sev.warning { background: rgba(245,158,11,.15); color: #fde68a; }
  .crash-sev.info { background: rgba(96,165,250,.15); color: #93c5fd; }
  .crash-code { font-size: 11px; color: var(--text-muted); }
  .crash-card p { color: var(--text-muted); font-size: 12px; margin: 0; line-height: 1.45; }
  .crash-evidence {
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: #0d0d10;
    color: #d4d4d8;
    font-size: 11px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: ui-monospace, monospace;
    max-height: 120px;
    overflow: auto;
  }
  .crash-fix { padding: 8px 10px; border-radius: 8px; background: rgba(27,217,106,.08); border: 1px solid rgba(27,217,106,.2); font-size: 12px; color: var(--accent-primary); }
  .crash-fix strong { color: var(--accent-primary); }
  .crash-card > p,
  .crash-evidence { max-width: 100%; overflow-wrap: anywhere; word-break: break-word; }
  .crash-fix-actions { display: flex; flex-wrap: wrap; gap: 6px; width: 100%; }
  .crash-fix-actions button { flex: 0 0 auto; white-space: nowrap; max-width: 100%; }
  .crash-fix-actions .small { font-size: 11px; padding: 5px 10px; }
  .more-fixes { color: var(--text-muted); font-size: 11px; align-self: center; }
  .crash-refs { display: flex; gap: 8px; flex-wrap: wrap; }
  .crash-link { font-size: 11px; color: var(--accent-secondary); text-decoration: none; }
  .crash-support { margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border-color); display: flex; gap: 8px; align-items: center; }
  .crash-mcreator { margin-top: 8px; padding: 10px 12px; border-radius: 8px; background: rgba(245,158,11,.08); border: 1px solid rgba(245,158,11,.25); }
  .crash-mcreator strong { color: #fde68a; font-size: 12px; display: block; margin-bottom: 4px; }
  .crash-mcreator p { color: var(--text-muted); font-size: 11px; margin: 0 0 6px; }
  .crash-tags { display: flex; gap: 6px; flex-wrap: wrap; }
  .crash-tags code { font-size: 11px; background: var(--bg-elevated); padding: 3px 7px; border-radius: 4px; }
  .crash-classfinder { margin-top: 8px; padding: 10px 12px; border-radius: 8px; background: rgba(96,165,250,.06); border: 1px solid rgba(96,165,250,.2); }
  .crash-classfinder strong { color: #93c5fd; font-size: 12px; display: block; margin-bottom: 4px; }
  .crash-classfinder p { color: var(--text-muted); font-size: 11px; margin: 0 0 6px; }
  .class-match { display: flex; gap: 8px; align-items: center; font-size: 11px; }
  .class-match span { color: var(--accent-primary); font-weight: 700; }
  .ai-panel { margin-top: 16px; border-color: rgba(139,92,246,.3); background: rgba(139,92,246,.03); }
  .ai-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 10px; margin-bottom: 12px; }
  .ai-stat { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 10px; padding: 10px; text-align: center; }
  .ai-stat strong { font-size: 20px; color: var(--accent-secondary); }
  .ai-human { color: var(--text-primary); font-size: 14px; line-height: 1.5; margin: 0 0 12px; }
  .ai-list { margin: 10px 0; }
  .ai-list ul { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 6px; }
  .ai-list li { color: var(--text-secondary); font-size: 12px; }
  .ai-list small { color: var(--text-muted); margin-left: 6px; }
  .ai-feedback {
    display: flex; flex-wrap: wrap; align-items: center; gap: 8px;
    margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--border-color);
    font-size: 12px; color: var(--text-muted);
  }
  .notice.warning { color: #fde68a; background: rgba(245, 158, 11, 0.08); border: 1px solid rgba(245, 158, 11, 0.28); border-radius: 10px; padding: 10px 12px; margin-bottom: 10px; font-size: 12px; }
  .ai-desc { color: var(--text-muted); font-size: 12px; margin: 0 0 10px; line-height: 1.4; }
  .ai-prompt-text { margin: 10px 0 0; padding: 14px; border-radius: 10px; background: #0d0d10; color: #d4d4d8; font-size: 11px; line-height: 1.5; max-height: 400px; overflow: auto; white-space: pre-wrap; font-family: ui-monospace,monospace; }
  .author-kb-panel { border-color: rgba(52, 211, 153, 0.35); background: rgba(52, 211, 153, 0.04); }
  .author-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .author-grid label { display: grid; gap: 6px; font-size: 12px; color: var(--text-muted); font-weight: 600; }
  .author-grid label.full { grid-column: 1 / -1; }
  .author-grid input, .author-grid textarea {
    width: 100%; padding: 8px 10px; border-radius: 8px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-primary); font: inherit; font-weight: 400;
  }
  .author-grid textarea.mono { font-family: ui-monospace, monospace; font-size: 11px; }
  .author-fp-meta { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 6px; font-size: 11px; color: var(--text-muted); }
  @media (max-width: 720px) { .author-grid { grid-template-columns: 1fr; } }

  .crash-support-msg { margin: 10px 0 0; padding: 14px; border-radius: 10px; background: #0d0d10; color: #d4d4d8; font-size: 12px; line-height: 1.5; max-height: 360px; overflow: auto; white-space: pre-wrap; font-family: ui-monospace,monospace; }
  .diag-card { display: flex; gap: 12px; border-left: 4px solid var(--text-muted); }
  .diag-card.error { border-left-color: var(--accent-danger); }
  .diag-card.warning { border-left-color: var(--accent-warning); }
  .diag-card.info { border-left-color: #60a5fa; }
  .diag-icon { color: var(--text-muted); margin-top: 2px; }
  .diag-card.error .diag-icon { color: var(--accent-danger); }
  .diag-card.warning .diag-icon { color: var(--accent-warning); }
  .diag-card.info .diag-icon { color: #60a5fa; }
  .meta { gap: 8px; }
  .severity { font-size: 11px; font-weight: 800; text-transform: uppercase; color: var(--text-secondary); }
  .code { background: var(--bg-elevated); padding: 2px 7px; border-radius: 4px; font-family: ui-monospace, monospace; }
  .muted-box { padding: 12px; background: var(--bg-tertiary); border-radius: 12px; }
  .muted-box.compact { padding: 9px; }
  .muted-box.stale-warn {
    margin-bottom: 10px;
    border: 1px solid rgba(245, 166, 35, 0.35);
    background: rgba(245, 166, 35, 0.08);
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.4;
  }
  .empty, .loading { color: var(--text-muted); padding: 70px; text-align: center; }
  .empty.inline { padding: 40px; }
  .empty.success { display: flex; flex-direction: column; align-items: center; gap: 12px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1180px) {
    .diagnose-grid { grid-template-columns: minmax(220px, .7fr) minmax(0, 1.3fr); }
    .inspector { grid-column: 1 / -1; }
    .stats { grid-template-columns: repeat(2, 1fr); }
  }
  @media (max-width: 760px) {
    .diagnose-grid, .stats { grid-template-columns: 1fr; }
    .inspector { grid-column: auto; }
    .summary-heading { align-items: flex-start; flex-direction: column; gap: 5px; }
    .analysis-tools > summary { align-items: flex-start; }
    .analysis-tools .tools-hint { display: none; }
    .actions button { flex: 1 1 160px; justify-content: center; }
  }
  @media (max-width: 480px) {
    .diagnosis-summary { grid-template-columns: 1fr; padding: 14px; }
    .toolbar { align-items: flex-start; }
    .title span { font-size: 14px; }
  .mod-entry { grid-template-columns: 1fr; }
  .wrong-loader-card, .wrong-loader-actions { flex-direction: column; }
  .wrong-loader-actions { width: 100%; }
  }

  /* --- Unified Problems panel (IDE "Problems" tool window) --- */
  .problems-panel { margin-top: 16px; border-color: rgba(239, 68, 68, 0.25); background: rgba(239, 68, 68, 0.04); }
  .problems-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 10px; flex-wrap: wrap; }
  .problems-head h2 { margin: 0; }
  .problems-head .count { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 20px; padding: 1px 9px; font-size: 11px; color: var(--text-secondary); }
  .problems-legend { display: flex; gap: 6px; flex-wrap: wrap; }
  .legend-pill { font-size: 11px; padding: 2px 8px; border-radius: 12px; border: 1px solid var(--border-color); }
  .legend-pill.critical { color: #fecaca; border-color: rgba(239, 68, 68, 0.5); }
  .legend-pill.error { color: #fecaca; border-color: rgba(239, 68, 68, 0.4); }
  .legend-pill.warning { color: #fde68a; border-color: rgba(234, 179, 8, 0.4); }
  .legend-pill.info { color: #bfdbfe; border-color: rgba(59, 130, 246, 0.4); }
  .problems-list { display: grid; gap: 8px; }
  .problem-row { display: flex; align-items: flex-start; gap: 10px; padding: 10px 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); }
  .problem-row.critical { border-color: rgba(239, 68, 68, 0.5); }
  .problem-row.error { border-color: rgba(239, 68, 68, 0.3); }
  .problem-row.warning { border-color: rgba(234, 179, 8, 0.35); }
  .problem-row.info { border-color: rgba(59, 130, 246, 0.3); }
  .sev-dot { width: 9px; height: 9px; border-radius: 50%; margin-top: 5px; flex-shrink: 0; background: var(--text-muted); }
  .problem-row.critical .sev-dot, .problem-row.error .sev-dot { background: #ef4444; }
  .problem-row.warning .sev-dot { background: #eab308; }
  .problem-row.info .sev-dot { background: #3b82f6; }
  .problem-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .problem-title { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .problem-title strong { color: var(--text-primary); }
  .problem-source { font-size: 10px; text-transform: uppercase; letter-spacing: .04em; color: var(--text-muted); border: 1px solid var(--border-color); border-radius: 10px; padding: 0 7px; }
  .problem-detail {
    margin: 0;
    color: var(--text-secondary);
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    word-break: break-word;
    max-width: 100%;
  }
  .problem-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    width: 100%;
    align-items: center;
  }
  .problem-actions button {
    flex: 0 0 auto;
    white-space: nowrap;
    max-width: 100%;
  }

  /* --- Find-in-log + section TOC --- */
  .preview-tools { display: flex; align-items: center; gap: 12px; }
  .find-box { display: flex; align-items: center; gap: 6px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 8px; padding: 3px 8px; }
  .find-box :global(svg) { color: var(--text-muted); }
  .find-input { background: transparent; border: none; outline: none; color: var(--text-primary); font-size: 12px; width: 150px; }
  .find-count { font-size: 11px; color: var(--text-muted); min-width: 30px; text-align: center; }
  .find-nav { background: transparent; border: none; color: var(--text-secondary); cursor: pointer; font-size: 13px; padding: 0 3px; }
  .find-nav:hover:not(:disabled) { color: var(--text-primary); }
  .find-nav:disabled { opacity: .4; cursor: default; }
  .toc { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 0 4px; border-bottom: 1px solid var(--border-color); margin-bottom: 8px; }
  .toc-item { font-size: 11px; padding: 3px 9px; border-radius: 12px; border: 1px solid var(--border-color); background: var(--bg-tertiary); color: var(--text-secondary); cursor: pointer; transition: border-color .12s ease, color .12s ease; }
  .toc-item:hover { border-color: rgba(27, 217, 106, 0.4); color: var(--text-primary); }
  .report-content .log-line { padding: 0 2px; border-radius: 3px; }
  .report-content .log-line.active { background: rgba(234, 179, 8, 0.22); outline: 1px solid rgba(234, 179, 8, 0.5); }
  .report-content .log-line.signal { background: rgba(239, 68, 68, 0.12); box-shadow: inset 3px 0 0 rgba(239, 68, 68, 0.85); }
  .report-content .log-line.signal[data-sig="Mixin"],
  .report-content .log-line.signal[data-sig="Exception"],
  .report-content .log-line.signal[data-sig="CausedBy"] { background: rgba(249, 115, 22, 0.12); box-shadow: inset 3px 0 0 rgba(249, 115, 22, 0.85); }
  .report-content .log-line.signal[data-sig="OutOfMemory"],
  .report-content .log-line.signal[data-sig="Watchdog"],
  .report-content .log-line.signal[data-sig="EulaNotAccepted"],
  .report-content .log-line.signal[data-sig="PortConflict"],
  .report-content .log-line.signal[data-sig="CorruptJar"],
  .report-content .log-line.signal[data-sig="MissingDependency"],
  .report-content .log-line.signal[data-sig="SuspectedMods"] { background: rgba(239, 68, 68, 0.16); box-shadow: inset 3px 0 0 rgba(239, 68, 68, 0.95); }
  .sig-marker { display: inline-block; font-size: 9px; font-weight: 700; letter-spacing: .03em; text-transform: uppercase; color: var(--bg-primary); background: rgba(239, 68, 68, 0.85); border-radius: 3px; padding: 0 4px; margin-right: 6px; vertical-align: middle; }
  .report-content :global(mark) { background: rgba(234, 179, 8, 0.45); color: inherit; border-radius: 2px; padding: 0 1px; }

  /* --- Diagnostics button-group + card spacing hardening ---
     Actions sit below detail (stacked), so text is never squeezed into
     single-letter columns by side-by-side flex siblings. */
  .problems-list, .hints-list, .suspects, .diagnostic-list, .signal-groups, .mod-entry-list {
    display: grid;
    gap: 12px;
  }
  .hint-actions, .suspect-actions, .conflict-actions, .plan-card ul {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .hint-actions button, .suspect-actions button, .conflict-actions button {
    white-space: nowrap;
    flex: 0 0 auto;
    min-width: 0;
    max-width: 100%;
  }
  .hint-card, .problem-row, .suspect-card, .conflict-card, .diag-card, .signal-group, .mod-entry, .plan-card {
    overflow-wrap: break-word;
    word-break: normal;
  }
  .problem-body { overflow-wrap: break-word; word-break: normal; }
  .suspects { max-height: none; overflow: visible; }
</style>
