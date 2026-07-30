<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { launchWithFeedback } from "../lib/launch";
  import { onMount } from "svelte";
  import {
    Stethoscope,
    Play,
    FolderOpen,
    ArrowUpCircle,
    RefreshCw,
    AlertCircle,
    AlertTriangle,
    Info,
    ListChecks,
    FileText,
    History,
    Wrench,
    Bug,
    Download,
    Trash2,
    Database,
    Copy,
    ChevronDown,
    BadgeCheck,
    Ban,
    Bot,
    BookMarked,
    Share2,
    ArrowDownToLine,
    MoreHorizontal,
  } from "lucide-svelte";
  import { diagnoseFocusPaths, historyFocusEventId, ideStageRequest, projectPath } from "../lib/store";
  import { shareCrashLogWithFeedback } from "../lib/mclogs";
  import EmptyState from "./EmptyState.svelte";
  import AiConnectionModal from "./AiConnectionModal.svelte";
  import DiagnoseTriagePanels from "./diagnostics/DiagnoseTriagePanels.svelte";
  import DiagnosePlanReviewModal from "./diagnostics/DiagnosePlanReviewModal.svelte";
  import DiagnoseLogViewer from "./diagnostics/DiagnoseLogViewer.svelte";
  import DiagnoseConflictsJars from "./diagnostics/DiagnoseConflictsJars.svelte";
  import DiagnoseAnalysisTabs from "./diagnostics/DiagnoseAnalysisTabs.svelte";
  import DiagnoseVerdictHero from "./diagnostics/DiagnoseVerdictHero.svelte";
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
    hsErrLogs?: {
      id: string;
      name: string;
      kind: string;
      problematicFrame?: string | null;
      preview: string;
    }[];
    worldCoords?: { x: number; y: number; z: number; label: string } | null;
    memoryHint?: string | null;
  };

  let diagnosis: CrashDiagnosis | null = null;
  let selectedReportId = "";
  let preferLatestLog = true;
  let preferLauncherLog = false;
  /// Sentinel: force latest.log analysis (never auto-pick a crash file).
  const LATEST_LOG_SOURCE = "__latest_log__";
  const LAUNCHER_LOG_SOURCE = "__launcher_log__";
  let analysisBusy = false;
  /** Detail panel under the verdict: rules findings vs AI explanation. */
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
    else if (el.value === LAUNCHER_LOG_SOURCE) chooseLauncherLog();
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
      const reportId = preferLatestLog
        ? LATEST_LOG_SOURCE
        : preferLauncherLog
          ? LAUNCHER_LOG_SOURCE
          : selectedReportId || null;
      const data: CrashDiagnosis = await invoke("get_crash_diagnosis", {
        path: $projectPath,
        reportId,
      });
      diagnosis = data;
      if (requestedLatest || data.analysisSource === "latest_log") {
        preferLatestLog = true;
        preferLauncherLog = false;
        selectedReportId = "";
      } else if (data.analysisSource === "launcher_log") {
        preferLatestLog = false;
        preferLauncherLog = true;
        selectedReportId = "";
      } else {
        preferLatestLog = false;
        preferLauncherLog = false;
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
    preferLauncherLog = false;
    selectedReportId = reportId;
    await load(true);
  }

  async function chooseLatestLog() {
    preferLatestLog = true;
    preferLauncherLog = false;
    selectedReportId = "";
    await load(true);
  }

  async function chooseLauncherLog() {
    preferLatestLog = false;
    preferLauncherLog = true;
    selectedReportId = "";
    await load(true);
  }

  function activeReportId(): string | null {
    if (preferLatestLog) return LATEST_LOG_SOURCE;
    if (preferLauncherLog) return LAUNCHER_LOG_SOURCE;
    return selectedReportId || null;
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
  let analysisToolsOpen = false;
  let classQuery = "";
  let classBusy = false;
  let classResults: { className: string; modId: string; modName: string }[] = [];
  let dependentResults: { className: string; modId: string; modName: string }[] = [];
  let bisectMods: string[] = [];
  let supportBusy = false;
  let importBusy = false;
  let importUrl = "";

  async function runClassFinder(q: string) {
    if (!$projectPath || !q.trim()) return;
    classBusy = true;
    classResults = [];
    try {
      classResults = await invoke("find_class_in_mods", {
        path: $projectPath,
        className: q.trim(),
      });
      message = classResults.length
        ? `Found ${classResults.length} mod(s) containing «${q.trim()}»`
        : `No mod jar contains «${q.trim()}»`;
    } catch (e) {
      error = String(e);
    } finally {
      classBusy = false;
    }
  }

  async function runFindDependents(q: string) {
    if (!$projectPath || !q.trim()) return;
    classBusy = true;
    dependentResults = [];
    try {
      dependentResults = await invoke("find_dependents_on_class", {
        path: $projectPath,
        className: q.trim(),
      });
      message = dependentResults.length
        ? `${dependentResults.length} mod(s) depend on «${q.trim()}»`
        : `No dependents for «${q.trim()}»`;
    } catch (e) {
      error = String(e);
    } finally {
      classBusy = false;
    }
  }

  function toggleBisect(modId: string) {
    if (bisectMods.includes(modId)) bisectMods = bisectMods.filter((id) => id !== modId);
    else bisectMods = [...bisectMods, modId];
  }

  async function applyBisectDisableHalf() {
    if (!$projectPath || bisectMods.length < 2) {
      message = "Select at least 2 suspected mods for bisect.";
      return;
    }
    const half = bisectMods.slice(0, Math.ceil(bisectMods.length / 2));
    for (const id of half) {
      await fixDisableMod(id);
    }
    message = `Bisect: disabled ${half.join(", ")}. Retest, then toggle the other half if still crashing.`;
    try {
      await invoke("scan_project_changes", { path: $projectPath });
    } catch {
      /* ignore */
    }
  }

  async function exportSupportPack() {
    if (!$projectPath || supportBusy) return;
    supportBusy = true;
    try {
      const findings = [
        ...(topFinding ? [`${topFinding.title}: ${topFinding.description}`] : []),
        ...crashFindings.slice(0, 8).map((f: any) => `${f.code}: ${f.title}`),
      ].join("\n");
      const events = recentPackEvents
        .slice(0, 8)
        .map((e) => `${e.ts} [${e.actor}] ${e.summary}`)
        .join("\n");
      const planJson = aiAnalysis
        ? JSON.stringify(
            {
              explanation: aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation,
              actions: aiPlanActions(aiAnalysis),
            },
            null,
            2,
          )
        : null;
      const result: { path: string; fileCount: number } = await invoke(
        "export_diagnose_support_pack",
        {
          path: $projectPath,
          reportId: activeReportId(),
          findingsSummary: findings,
          recentEventsSummary: events,
          actionPlanJson: planJson,
        },
      );
      message = `Support pack ready (${result.fileCount} files): ${result.path}`;
      try {
        await navigator.clipboard.writeText(result.path);
        message += " — path copied.";
      } catch {
        /* ignore */
      }
    } catch (e) {
      error = String(e);
    } finally {
      supportBusy = false;
    }
  }

  async function importExternalCrashFile(file: File) {
    if (!$projectPath) return;
    importBusy = true;
    try {
      const content = await file.text();
      const id: string = await invoke("import_external_crash", {
        path: $projectPath,
        fileName: file.name,
        content,
      });
      message = `Imported player crash → ${id}`;
      preferLatestLog = false;
      preferLauncherLog = false;
      selectedReportId = id;
      await load(true);
      await runUnifiedAnalysis();
    } catch (e) {
      error = String(e);
    } finally {
      importBusy = false;
    }
  }

  async function importFromMclogsUrl() {
    if (!$projectPath || !importUrl.trim()) return;
    importBusy = true;
    try {
      const url = importUrl.trim();
      if (!/^https:\/\/(api\.)?mclo\.gs\//i.test(url) && !/^https:\/\/mclo\.gs\//i.test(url)) {
        throw new Error("Only https://mclo.gs/… URLs are allowed");
      }
      const rawId = url.replace(/\/+$/, "").split("/").pop() || "";
      const fetchUrl = `https://api.mclo.gs/1/raw/${rawId}`;
      const res = await fetch(fetchUrl);
      if (!res.ok) throw new Error(`mclo.gs fetch failed (${res.status})`);
      const content = await res.text();
      const id: string = await invoke("import_external_crash", {
        path: $projectPath,
        fileName: `mclogs-${rawId}.txt`,
        content,
      });
      message = `Imported from mclo.gs → ${id}`;
      preferLatestLog = false;
      preferLauncherLog = false;
      selectedReportId = id;
      importUrl = "";
      await load(true);
      await runUnifiedAnalysis();
    } catch (e) {
      error = String(e);
    } finally {
      importBusy = false;
    }
  }

  function onDropCrash(e: DragEvent) {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (file) void importExternalCrashFile(file);
  }

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

  /** ActionPlan review modal (replaces window.confirm). */
  let planReviewOpen = false;
  let planReviewSource: "ai" | "network" = "ai";
  let planReviewRows: {
    key: string;
    selected: boolean;
    op: string;
    modId: string | null;
    path: string | null;
    patchPreview: string | null;
    reason: string;
    risk: string;
    diffKind?: "add" | "remove" | "change" | "other";
    destructive?: boolean;
    raw: any;
  }[] = [];
  let planReviewAcknowledged = false;
  let planReviewNeedsAck = false;
  let planReviewExplanation = "";
  let showApplyTrail = false;
  $: planReviewSelectedCount = planReviewRows.filter((r) => r.selected).length;
  $: planReviewCanApply =
    planReviewSelectedCount > 0 && (!planReviewNeedsAck || planReviewAcknowledged);

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

  let recentPackEvents: {
    id: string;
    ts: string;
    actor: string;
    op: string;
    summary: string;
    paths?: string[];
  }[] = [];

  async function loadRecentPackEvents() {
    if (!$projectPath) return;
    try {
      recentPackEvents = await invoke("list_recent_pack_events", {
        path: $projectPath,
        limit: 12,
      });
    } catch {
      recentPackEvents = [];
    }
  }

  function openHistoryEvent(eventId: string) {
    historyFocusEventId.set(eventId);
    ideStageRequest.set("history");
  }

  let lastRecentPackPath: string | null = null;
  $: if ($projectPath && $projectPath !== lastRecentPackPath) {
    lastRecentPackPath = $projectPath;
    void loadRecentPackEvents();
  }
  $: if ($diagnoseFocusPaths?.length) {
    const paths = $diagnoseFocusPaths;
    diagnoseFocusPaths.set(null);
    void applyHistoryFocus(paths);
  }

  async function applyHistoryFocus(paths: string[]) {
    message = `Focus from History: ${paths.join(", ")}`;
    if (!diagnosis && $projectPath) await load(true);
    const d = diagnosis;
    if (!d) return;
    const joined = paths.join(" ").toLowerCase().replace(/\\/g, "/");
    const matchReport = d.reports.find((r) => {
      const id = r.id.toLowerCase();
      const name = r.name.toLowerCase();
      const path = (r.path || "").toLowerCase().replace(/\\/g, "/");
      return paths.some((p) => {
        const n = p.toLowerCase().replace(/\\/g, "/");
        return id.includes(n) || name.includes(n) || path.includes(n) || n.includes(name);
      });
    });
    if (matchReport) {
      await chooseReport(matchReport.id);
    } else if (joined.includes("launcher")) {
      await chooseLauncherLog();
    } else if (joined.includes("latest.log") || joined.includes("/logs/")) {
      await chooseLatestLog();
    }
    setTimeout(() => jumpToFirstError(), 250);
  }

  async function runAiExplain(opts: { quiet?: boolean } = {}) {
    if (!$projectPath) return;
    aiLoading = true;
    if (!opts.quiet) error = null;
    aiSoftError = null;
    aiFeedbackMsg = null;
    try {
      // Catch external disk edits before building AI context.
      try {
        await invoke("scan_project_changes", { path: $projectPath });
        await loadRecentPackEvents();
      } catch {
        // non-fatal
      }
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

  function buildPlanRows(actions: any[]) {
    const destructiveOps = new Set([
      "disable_mod",
      "remove_mod",
      "remove_file",
      "uninstall_mod",
    ]);
    return actions.map((a: any, idx: number) => {
      const op = String(a.op ?? a.action_type ?? a.actionType ?? "action");
      const modId = a.modId ?? a.mod_id ?? null;
      const path = a.path ?? null;
      let patchPreview: string | null = null;
      if (op === "edit_config" || path) {
        const patch = a.patch ?? null;
        patchPreview = patch
          ? `${path ?? "?"}\n${typeof patch === "string" ? patch : JSON.stringify(patch, null, 2)}`.slice(0, 800)
          : path
            ? String(path)
            : null;
      }
      const diffKind = destructiveOps.has(op)
        ? "remove"
        : op === "edit_config" || op === "update_config" || op === "update_mod" || op === "change_mod_version"
          ? "change"
          : op.includes("install") || op.includes("download") || op === "reinstall_mod"
            ? "add"
            : "other";
      return {
        key: `${op}:${modId ?? path ?? idx}`,
        selected: true,
        op,
        modId,
        path,
        patchPreview,
        reason: String(a.reason ?? a.description ?? ""),
        risk: String(a.risk ?? "medium"),
        diffKind,
        destructive: destructiveOps.has(op) || (op === "edit_config" && String(a.risk ?? "") === "high"),
        raw: a,
      };
    });
  }

  function parseNetworkTrust(plan: any): {
    trustPercent: number | null;
    keeps: number | null;
    discards: number | null;
    mc: string | null;
    loader: string | null;
  } {
    const ctx = String(plan?.additionalContext ?? plan?.additional_context ?? "");
    const m = ctx.match(
      /trust:(\d+)\|keeps:(\d+)\|discards:(\d+)\|mc:([^|]*)\|loader:([^|]*)/,
    );
    if (!m) {
      return { trustPercent: null, keeps: null, discards: null, mc: null, loader: null };
    }
    return {
      trustPercent: Number(m[1]),
      keeps: Number(m[2]),
      discards: Number(m[3]),
      mc: m[4] && m[4] !== "-" ? m[4] : null,
      loader: m[5] && m[5] !== "-" ? m[5] : null,
    };
  }

  $: networkTrust = pendingPlan ? parseNetworkTrust(pendingPlan) : null;
  $: planReviewHasDestructive = planReviewRows.some((r: any) => r.destructive);

  function openAiPlanReview() {
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
    planReviewSource = "ai";
    planReviewRows = buildPlanRows(actions);
    planReviewNeedsAck = !!(aiAnalysis.needsUserReview ?? aiAnalysis.needs_user_review ?? true);
    planReviewAcknowledged = !planReviewNeedsAck;
    planReviewExplanation =
      aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation ?? "AI / KB ActionPlan";
    planReviewOpen = true;
  }

  function openNetworkPlanReview() {
    if (!$projectPath || !pendingPlan) return;
    const actions = pendingPlan.actions ?? [];
    if (!actions.length) {
      error = "Pending network plan has no actions.";
      return;
    }
    planReviewSource = "network";
    planReviewRows = buildPlanRows(actions);
    planReviewNeedsAck = !!(pendingPlan.needsUserReview ?? pendingPlan.needs_user_review ?? true);
    planReviewAcknowledged = !planReviewNeedsAck;
    planReviewExplanation =
      pendingPlan.humanExplanation ?? pendingPlan.human_explanation ?? "Network ActionPlan";
    planReviewOpen = true;
  }

  async function applyAiPlan() {
    openAiPlanReview();
  }

  async function confirmPlanReviewApply() {
    if (!$projectPath || !planReviewCanApply) return;
    const selected = planReviewRows.filter((r) => r.selected);
    if (!selected.length) return;
    planReviewOpen = false;
    if (planReviewSource === "network") {
      await executeNetworkPlanApply(selected.map((r) => r.raw));
    } else {
      await executeAiPlanApply(selected.map((r) => r.raw));
    }
  }

  async function executeAiPlanApply(actionsRaw: any[]) {
    if (!$projectPath || !aiAnalysis) return;
    aiApplyBusy = true;
    error = null;
    showApplyTrail = false;
    try {
      const plan = {
        schemaVersion: aiAnalysis.schemaVersion ?? 1,
        humanExplanation: aiAnalysis.humanExplanation ?? aiAnalysis.human_explanation ?? "",
        confidence: aiAnalysis.confidence ?? 0.5,
        suspectedMods: aiAnalysis.suspectedMods ?? aiAnalysis.suspected_mods ?? [],
        needsUserReview: false,
        source: aiAnalysis.source ?? null,
        matchedCaseIds: aiAnalysis.matchedCaseIds ?? [],
        actions: actionsRaw.map((a: any) => ({
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
      message = `Applied ${applied.length} action(s).${errs.length ? ` Errors: ${errs.join("; ")}` : ""} Snapshot first — next: Test launch to verify.`;
      showApplyTrail = applied.length > 0;
      if (errs.length) error = errs.join("; ");
      await load(true);
      if (applied.length && !errs.length) {
        window.dispatchEvent(new CustomEvent("tuffbox:crash-fix-applied"));
        const launchNow = confirm(
          "Fix applied. Run a Test launch now to verify?\n\nAfter the game starts cleanly, TuffBox can soft-verify and offer to share the fix.",
        );
        if (launchNow) {
          await runTest();
        } else {
          await openAuthorForm({ fromAnalysis: true });
        }
      }
    } catch (e) {
      error = String(e);
    } finally {
      aiApplyBusy = false;
    }
  }

  async function executeNetworkPlanApply(actionsRaw: any[]) {
    if (!$projectPath || !pendingPlan) return;
    if (!swarmEnabled) {
      error = "Enable TuffSwarm network in Settings to apply network fixes.";
      return;
    }
    pendingBusy = true;
    error = null;
    showApplyTrail = false;
    try {
      const plan = { ...pendingPlan, needsUserReview: false, actions: actionsRaw };
      const result: any = await invoke("apply_action_plan", {
        path: $projectPath,
        plan,
        fingerprintKey: pendingPlan.matchedCaseIds?.[0] ?? null,
      });
      const applied = result?.applied ?? [];
      const errs = result?.errors ?? [];
      message = `Network fix applied (${applied.length}).${errs.length ? ` Errors: ${errs.join("; ")}` : ""}`;
      showApplyTrail = applied.length > 0;
      if (errs.length) error = errs.join("; ");
      await invoke("clear_pending_network_plan", { path: $projectPath });
      pendingPlan = null;
      window.dispatchEvent(new CustomEvent("tuffbox:crash-fix-applied"));
    } catch (e) {
      error = String(e);
    } finally {
      pendingBusy = false;
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
    openNetworkPlanReview();
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
  /// then soft-verifies a pending crash-fix if the session looks healthy.
  async function runTest() {
    if (!$projectPath || launching) return;
    launching = true;
    error = null;
    message = "Launching Test profile — reproduce the crash, then come back.";
    const path = $projectPath;
    const result = await launchWithFeedback(
      { path, profile: "client" },
      {
        onStarted: () => {
          message = "Test launch started. If it stays healthy, soft-verify will confirm the fix.";
        },
      },
    );
    if (result) {
      message = "Test launch started. Waiting for a healthy session to soft-verify…";
      try {
        const rec = await invoke<{ id?: string; humanExplanation?: string } | null>(
          "confirm_crash_resolution_after_launch",
          { path },
        );
        if (rec) {
          message = `Fix verified${rec.humanExplanation ? `: ${rec.humanExplanation}` : ""}. You can share it if prompted.`;
          await load(true);
        } else {
          message =
            "Test launch started. Soft-verify will confirm after latest.log shows a healthy post-fix session (or re-run Diagnose).";
        }
      } catch {
        message = "Test launch started. Re-run Diagnose after it crashes/closes.";
      }
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

  // --- Log source text (viewer itself lives in DiagnoseLogViewer.svelte) ---
  let logViewerRef: { scrollToLine: (line: number) => void } | null = null;

  $: currentLogText = preferLatestLog
    ? (diagnosis?.latestLog?.tail ?? "")
    : preferLauncherLog
      ? (diagnosis?.launcherLog?.tail ?? "")
      : (selectedReport?.content ?? "");
  $: logDisplayText =
    currentLogText.length > 160_000 ? currentLogText.slice(currentLogText.length - 160_000) : currentLogText;
  $: logSourceKey = preferLatestLog ? LATEST_LOG_SOURCE : preferLauncherLog ? LAUNCHER_LOG_SOURCE : selectedReportId;
  $: logSourceLabel = preferLatestLog ? "latest.log" : (selectedReport?.summary?.name ?? "log");

  /** Delegates to the log viewer child, which owns the scroll container and
   *  its own truncation window (see DiagnoseLogViewer.scrollToLine). Called
   *  from the verdict hero, tools strip, and the triage panel's jumpLine event. */
  function scrollLogToLine(line: number) {
    logViewerRef?.scrollToLine(line);
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

  /** Jump to first / next ERROR line in the log (alias kept for older markup). */
  function jumpToFirstError() {
    jumpToNextError();
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
    { title: "Loader mismatch", hint: "Wrong loader/API/version bridge", items: allSignals.filter((s) => s.kind === "LoaderMismatch" || s.kind === "WrongLoader") },
    { title: "Mixin", hint: "Mixin apply / inject conflicts", items: allSignals.filter((s) => s.kind === "Mixin") },
    { title: "OutOfMemory", hint: "Java heap / native OOM", items: allSignals.filter((s) => s.kind === "OutOfMemory") },
    { title: "Render/OpenGL", hint: "Renderer, shader or GPU pipeline", items: allSignals.filter((s) => s.kind === "OpenGl") },
    { title: "Ticking / world", hint: "Ticking entity or world corruption signals", items: allSignals.filter((s) => s.kind === "TickingEntity") },
    { title: "Performance", hint: "Tick stalls and overload", items: allSignals.filter((s) => s.kind === "Performance") },
  ].filter((group) => group.items.length > 0);

  $: cascadingFinding = crashFindings.find(
    (f: any) => String(f.code ?? "").toUpperCase() === "CASCADING_CONFIG_ERROR",
  );
  $: mixinFinding = crashFindings.find(
    (f: any) => /mixin/i.test(String(f.code ?? "") + String(f.title ?? "")),
  );
  $: sideMismatchFinding = crashFindings.find(
    (f: any) => /client.?only|side.?mismatch|SERVER/i.test(String(f.code ?? "") + String(f.title ?? "")),
  );
  $: isHsErr =
    diagnosis?.analysisSource === "hs_err" ||
    (selectedReportId?.startsWith("hs_err/") ?? false);
  $: hsErrKind =
    diagnosis?.hsErrLogs?.find((h) => h.id === selectedReportId)?.kind ??
    (isHsErr ? "native" : null);

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
  <div class="dx-top">
    <div class="toolbar">
      <div class="title">
        <Stethoscope size={18} />
        <span>Diagnose</span>
        {#if analysisBusy || crashLoading || aiLoading}
          <span class="analyzing-pill">Analyzing…</span>
        {/if}
      </div>
    </div>

    {#if error}<div class="notice error">{error}</div>{/if}
    {#if message}
      <div class="notice success trail-notice">
        <span>{message}</span>
        {#if showApplyTrail}
          <div class="trail-links">
            <button class="ghost mini" type="button" on:click={() => ideStageRequest.set("history")}><History size={12} /> History</button>
            <button class="ghost mini" type="button" on:click={() => ideStageRequest.set("test")}><Play size={12} /> Test</button>
            <button class="ghost mini" type="button" on:click={() => ideStageRequest.set("snapshots")}><Database size={12} /> Snapshots</button>
          </div>
        {/if}
      </div>
    {/if}
    {#if aiSoftError}
      <div class="notice warning">
        AI unavailable — rules still work.
        <button class="ghost mini" type="button" on:click={() => (aiModalOpen = true)}>AI settings</button>
      </div>
    {/if}
    {#if pendingPlan && swarmEnabled}
      <div class="notice warning network-pending">
        <div class="network-pending-head">
          <span>Network ActionPlan ready ({(pendingPlan.actions ?? []).length} action(s)).</span>
          <button class="secondary small" on:click={applyPendingNetworkFix} disabled={pendingBusy}>
            {pendingBusy ? "Applying…" : "Review & apply"}
          </button>
        </div>
        {#if networkTrust && networkTrust.trustPercent != null}
          <div class="trust-card-line" title="Community soft-verify trust">
            <strong>{networkTrust.trustPercent}% trust</strong>
            {#if networkTrust.keeps != null}
              <span>· {networkTrust.keeps} keep / {networkTrust.discards ?? 0} discard</span>
            {/if}
            {#if networkTrust.mc || networkTrust.loader}
              <span>· {[networkTrust.mc, networkTrust.loader].filter(Boolean).join(" · ")}</span>
            {/if}
          </div>
        {/if}
        <div class="action-diff-row">
          {#each (pendingPlan.actions ?? []).slice(0, 6) as a, i (i + ":" + (a.op ?? ""))}
            {@const op = String(a.op ?? "action")}
            {@const kind =
              op.includes("disable") || op.includes("remove")
                ? "remove"
                : op.includes("edit") || op.includes("update") || op.includes("change")
                  ? "change"
                  : "add"}
            <span class="diff-chip {kind}">
              {kind === "add" ? "+" : kind === "remove" ? "−" : "~"}
              {op}{a.modId || a.mod_id ? ` ${a.modId ?? a.mod_id}` : ""}
            </span>
          {/each}
        </div>
      </div>
    {/if}
  </div>

  <div class="dx-scroll">
  {#if recentPackEvents.length > 0}
    <section class="panel recent-pack-panel">
      <div class="recent-head">
        <strong><History size={14} /> Recent pack changes</strong>
        <button class="ghost mini" on:click={() => ideStageRequest.set("history")}>Open History</button>
      </div>
      <div class="recent-list">
        {#each recentPackEvents as ev (ev.id)}
          <button type="button" class="recent-row" on:click={() => openHistoryEvent(ev.id)}>
            <span class="recent-actor">{ev.actor}</span>
            <span class="recent-sum">{ev.summary}</span>
            <small>{ev.ts?.slice(0, 19) ?? ""}</small>
          </button>
        {/each}
      </div>
    </section>
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
        value={preferLatestLog ? LATEST_LOG_SOURCE : preferLauncherLog ? LAUNCHER_LOG_SOURCE : selectedReportId}
        on:change={onSourceChange}
      >
        <option value={LATEST_LOG_SOURCE}>
          latest.log{diagnosis.latestLog.exists ? ` · ${diagnosis.latestLog.signals.length} signals` : " · missing"}
        </option>
        <option value={LAUNCHER_LOG_SOURCE}>
          launcher.log{diagnosis.launcherLog?.exists ? ` · ${diagnosis.launcherLog.signals.length} signals` : " · missing"}
        </option>
        {#each diagnosis.reports as report (report.id)}
          <option value={report.id}>{report.name} · {formatBytes(report.size)}</option>
        {/each}
      </select>
      <p class="muted-inline">
        Prefer a crash-report when present. If crash-reports is empty, use latest.log, launcher.log, or an hs_err_pid*.log.
      </p>
      {#if diagnosis.crashReportStale}
        <p class="muted-inline">Crash report is older than latest.log — live log is preferred.</p>
      {/if}
      {#if !diagnosis.reports.some((r) => r.id.startsWith("crash-reports/"))}
        <div class="dx-empty-sources">
          <span>No crash-reports yet.</span>
          <button type="button" class="ghost mini" on:click={chooseLatestLog}>latest.log</button>
          <button type="button" class="ghost mini" on:click={chooseLauncherLog}>launcher.log</button>
          {#if diagnosis.hsErrLogs?.length}
            {@const hsErr = diagnosis.hsErrLogs[0]}
            <button type="button" class="ghost mini" on:click={() => chooseReport(hsErr.id)}>
              {hsErr.name}
            </button>
          {/if}
          <button type="button" class="ghost mini" on:click={runTest} disabled={launching}>Test launch</button>
        </div>
      {/if}
      <div
        class="dx-drop"
        role="region"
        aria-label="Import player crash"
        on:dragover|preventDefault
        on:drop={onDropCrash}
      >
        <span>Drop a player crash-*.txt here, or paste mclo.gs URL:</span>
        <div class="dx-drop-row">
          <input type="url" placeholder="https://mclo.gs/…" bind:value={importUrl} />
          <button type="button" class="ghost mini" disabled={importBusy || !importUrl.trim()} on:click={importFromMclogsUrl}>
            {importBusy ? "…" : "Import"}
          </button>
          <button type="button" class="secondary small" disabled={supportBusy} on:click={exportSupportPack}>
            {supportBusy ? "…" : "Support pack"}
          </button>
        </div>
      </div>
    </section>

    <!-- Verdict first (answer before the scary log) -->
    <DiagnoseVerdictHero
      sessionOk={sessionOk}
      topSuspect={topSuspect}
      topFinding={topFinding}
      heroCulpritLabel={heroCulpritLabel}
      strongestEvidence={strongestEvidence}
      analysisBusy={analysisBusy}
      primaryRec={primaryRec}
      mergedRecommendations={mergedRecommendations}
      aiApplyBusy={aiApplyBusy}
      applyingHintId={applyingHintId}
      disablingModId={disablingModId}
      fixingIdx={fixingIdx}
      aiAnalysis={aiAnalysis}
      logDisplayText={logDisplayText}
      isHsErr={isHsErr}
      hsErrKind={hsErrKind}
      memoryHint={diagnosis.memoryHint}
      worldCoords={diagnosis.worldCoords}
      cascadingFinding={cascadingFinding}
      mixinFinding={mixinFinding}
      sideMismatchFinding={sideMismatchFinding}
      suspected={suspected}
      on:fixDisableMod={(e) => fixDisableMod(e.detail)}
      on:applyTopSuspectUpdate={() => applyTopSuspectUpdate()}
      on:applyAiPlan={applyAiPlan}
      on:jumpToFirstError={jumpToFirstError}
      on:applyBisectDisableHalf={applyBisectDisableHalf}
    />

    <!-- Secondary tools (collapsed — Analyze is primary in verdict) -->
    <details class="tools-strip panel collapsible-block">
      <summary>
        <span><MoreHorizontal size={16} /> More tools</span>
        <span class="tools-hint">
          Test launch · log · folders · scanners
          <ChevronDown size={14} />
        </span>
      </summary>
      <div class="tools-strip-body">
        <div class="tools-primary-row">
          <button
            class="primary"
            on:click={() => runUnifiedAnalysis()}
            disabled={!$projectPath || analysisBusy || loading || sessionOk}
            title="Re-run Crash Assistant + AI"
          >
            <RefreshCw size={15} class={analysisBusy ? "spin" : ""} />
            {analysisBusy ? "Analyzing…" : "Re-analyze"}
          </button>
          <button class="secondary" on:click={runTest} disabled={!$projectPath || launching || loading}>
            <Play size={15} class={launching ? "spin" : ""} />
            {launching ? "Launching…" : "Test launch"}
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
          <button class="ghost" on:click={exportSupportPack} disabled={!$projectPath || supportBusy} title="Zip crash + findings for Discord/GitHub">
            <Download size={15} /> {supportBusy ? "…" : "Support pack"}
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
      </div>
    </details>

    <DiagnoseTriagePanels
      signalGroups={signalGroups}
      sections={selectedReport?.sections ?? []}
      suspected={suspected}
      recentSnapshots={diagnosis.recentSnapshots ?? []}
      mcreatorMods={crashMcreator}
      classFinderResults={crashClassFinder}
      bind:classQuery
      classBusy={classBusy}
      classResults={classResults}
      dependentResults={dependentResults}
      bind:toolsOpen={analysisToolsOpen}
      disablingModId={disablingModId}
      bisectMods={bisectMods}
      worldCoords={null}
      memoryHint={null}
      cascadingBanner={cascadingFinding ? cascadingFinding.description : null}
      sourceHint=""
      on:jumpLine={(e) => {
        const ln = Number(e.detail) || 0;
        if (ln <= 0) jumpToFirstError();
        else scrollLogToLine(Math.max(0, ln - 1));
      }}
      on:disableMod={(e) => fixDisableMod(e.detail)}
      on:updateMod={async (e) => {
        const id = e.detail;
        if (!id) return;
        fixingIdx = -1;
        try {
          await invoke("apply_fix_action", {
            path: $projectPath,
            action: { kind: "updateMod", label: `Update ${id}`, modId: id },
          });
          message = `Update requested for ${id}`;
          await load(true);
        } catch (err) {
          error = String(err);
        } finally {
          fixingIdx = null;
        }
      }}
      on:toggleBisect={(e) => toggleBisect(e.detail)}
      on:findClass={(e) => runClassFinder(e.detail)}
      on:findDependents={(e) => runFindDependents(e.detail)}
      on:openSnapshots={() => ideStageRequest.set("snapshots")}
    />

    {#if bisectMods.length >= 2}
      <div class="notice warning">
        Bisect checklist: {bisectMods.join(", ")}
        <button type="button" class="secondary small" on:click={applyBisectDisableHalf}>Disable first half & retest</button>
      </div>
    {/if}

    <!-- Log viewer (after verdict, plan, and evidence) -->
    <DiagnoseLogViewer
      bind:this={logViewerRef}
      logDisplayText={logDisplayText}
      currentLogTextLength={currentLogText.length}
      sourceLabel={logSourceLabel}
      signalLineMap={signalLineMap}
      errorHits={errorHits}
      activeErrorHit={activeErrorHit}
      sharingLog={sharingLog}
      hasLogText={!!currentLogText}
      sourceKey={logSourceKey}
      on:jumpNextError={jumpToNextError}
      on:copy={copyCurrentLog}
      on:share={shareCurrentLog}
    />

    <!-- 3. Analysis as tabs (not side-by-side) -->
    <DiagnoseAnalysisTabs
      crashFindings={crashFindings}
      crashLoading={crashLoading}
      aiAnalysis={aiAnalysis}
      aiLoading={aiLoading}
      aiSoftError={aiSoftError}
      aiApplyBusy={aiApplyBusy}
      aiFeedbackBusy={aiFeedbackBusy}
      aiFeedbackMsg={aiFeedbackMsg}
      applyingHintId={applyingHintId}
      on:applyFindingFix={(e) => applyCrashFindingFix(e.detail.finding, e.detail.action)}
      on:retryAi={() => runAiExplain()}
      on:applyAiPlan={applyAiPlan}
      on:feedback={(e) => sendAiFeedback(e.detail)}
    />

    <!-- 4. Evidence (secondary) -->
    <DiagnoseConflictsJars
      graphDiagnostics={graphDiagnostics}
      duplicateJarGroups={duplicateJarGroups}
      wrongLoaderJars={wrongLoaderJars}
      fixingIdx={fixingIdx}
      duplicateJarFixing={duplicateJarFixing}
      wrongLoaderFixing={wrongLoaderFixing}
      on:fixMissingDependency={(e) => fixMissingDependency(e.detail.modId, e.detail.idx)}
      on:fixDeduplicate={(e) => fixDeduplicate(e.detail)}
      on:keepOneDuplicateJar={(e) => keepOneDuplicateJar(e.detail.modId, e.detail.fileName)}
      on:disableWrongJar={(e) => disableWrongJar(e.detail)}
      on:removeWrongJar={(e) => removeWrongJar(e.detail)}
    />

    <!-- Scanner results / KB authoring (tools live in the top strip) -->
    {#if plan || oreFindings?.length || duplicateFindings?.length || unifyConfigResult || authorOpen || aiShowPrompt}
      <section class="panel tools-results">
        <h2><Wrench size={16} /> Tool results</h2>
        {#if aiShowPrompt && aiPrompt}
          <pre class="log-pre">{aiPrompt.slice(0, 20000)}</pre>
        {/if}
        {#if plan}
          <div class="plan-card">
            <h3>Heuristic Fix plan (Crash Assistant)</h3>
            <p class="muted-inline">Rule-based — separate from AI ActionPlan above.</p>
            <p>{plan.summary}</p>
            <button class="primary" on:click={applyFix} disabled={applying}>{applying ? "Applying…" : "Apply heuristic fix plan"}</button>
          </div>
        {/if}
        {#if authorOpen}
          <div class="author-form">
            <h3>Save KB case</h3>
            <label>Case id<input bind:value={authorId} placeholder="authored-outofmemory" /></label>
            <label>Solution<textarea bind:value={authorSolution} rows="3"></textarea></label>
            <label>Symptoms (one per line)<textarea bind:value={authorSymptoms} rows="3"></textarea></label>
            <label>Suspected (comma)<input bind:value={authorSuspected} /></label>
            <label>Actions JSON<textarea bind:value={authorActionsJson} rows="6" class="mono"></textarea></label>
            <label>Notes (local only)<textarea bind:value={authorNotes} rows="2"></textarea></label>
            <div class="actions">
              <button class="primary" on:click={saveAuthorCase} disabled={authorBusy || !authorSolution.trim()}>Save</button>
              <button class="ghost" on:click={() => copyAuthorExport()} disabled={!authorExportPreview}>Copy export</button>
              <button class="ghost" on:click={openAuthorExportFolder}>Open folder</button>
              <button class="ghost" on:click={() => (authorOpen = false)}>Close</button>
            </div>
            {#if authorCases.length}
              <div class="author-cases">
                <strong>Saved cases</strong>
                {#each authorCases.slice(0, 6) as c (c.id)}
                  <button type="button" class="ghost mini" on:click={() => copyAuthorExport(c.id)}>{c.id}</button>
                {/each}
              </div>
            {/if}
            {#if authorMsg}<p class="muted-inline">{authorMsg}</p>{/if}
            {#if authorExportPreview}
              <pre class="log-pre">{authorExportPreview.slice(0, 4000)}</pre>
            {/if}
          </div>
        {/if}
        {#if oreFindings?.length || duplicateFindings?.length || unifyConfigResult || wrongLoaderJars.length || duplicateJarGroups.length}
          <div class="scanner-cards">
            {#if oreFindings?.length}
              <div class="scanner-card">
                <strong>Ore gen</strong>
                <p>{oreFindings.length} finding(s)</p>
                <button type="button" class="ghost mini" on:click={() => ideStageRequest.set("world-map")}>World map</button>
              </div>
            {/if}
            {#if duplicateFindings?.length}
              <div class="scanner-card">
                <strong>Duplicate items</strong>
                <p>{duplicateFindings.length} finding(s)</p>
                <button type="button" class="ghost mini" on:click={generateUnify} disabled={unifyLoading}>Generate unify</button>
                <button type="button" class="ghost mini" on:click={() => ideStageRequest.set("resolve")}>Resolve</button>
              </div>
            {/if}
            {#if unifyConfigResult}
              <div class="scanner-card">
                <strong>Unify config</strong>
                <p>Generated — review before applying.</p>
                <pre>{JSON.stringify(unifyConfigResult, null, 2).slice(0, 1200)}</pre>
              </div>
            {/if}
            {#if wrongLoaderJars.length}
              <div class="scanner-card">
                <strong>Wrong-loader jars</strong>
                <p>{wrongLoaderJars.length} jar(s)</p>
                <button type="button" class="ghost mini" on:click={() => detectWrongLoaderMods()}>Refresh</button>
              </div>
            {/if}
            {#if duplicateJarGroups.length}
              <div class="scanner-card">
                <strong>Duplicate mod jars</strong>
                <p>{duplicateJarGroups.length} group(s)</p>
                <button type="button" class="ghost mini" on:click={() => detectDuplicateModJars()}>Refresh</button>
              </div>
            {/if}
          </div>
        {/if}
      </section>
    {/if}
  {:else}
    <div class="empty">Press Refresh to load diagnosis.</div>
  {/if}
  </div>
</div>

<DiagnosePlanReviewModal
  bind:open={planReviewOpen}
  source={planReviewSource}
  explanation={planReviewExplanation}
  hasDestructive={planReviewHasDestructive}
  bind:rows={planReviewRows}
  needsAck={planReviewNeedsAck}
  bind:acknowledged={planReviewAcknowledged}
  canApply={planReviewCanApply}
  selectedCount={planReviewSelectedCount}
  busy={aiApplyBusy || pendingBusy}
  on:cancel={() => (planReviewOpen = false)}
  on:confirm={confirmPlanReviewApply}
/>

<AiConnectionModal bind:open={aiModalOpen} />

<style>
  .diagnostics {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    max-width: min(1280px, 100%);
    width: 100%;
    margin: 0 auto;
  }
  .dx-top {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .dx-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .toolbar, .actions, .title, .primary-actions, .panel-header, .suspect-head, .meta, .plan-meta { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 10px; flex-wrap: wrap; }
  .title, h2 { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions { gap: 8px; flex-wrap: wrap; }
  .primary-actions .primary, .primary-actions .secondary, .primary-actions .ghost { cursor: pointer; }
  .ghost.icon-only { padding: 8px; min-width: 36px; justify-content: center; }

  .tools-strip {
    padding: 0;
    margin-bottom: 14px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .tools-strip > summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    cursor: pointer;
    list-style: none;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .tools-strip > summary::-webkit-details-marker { display: none; }
  .tools-strip > summary span:first-child {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .tools-strip[open] .tools-hint :global(svg) { transform: rotate(180deg); }
  .tools-strip-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 14px 12px;
    border-top: 1px solid var(--border-color);
  }
  .tools-primary-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding-top: 10px;
  }
  .recent-pack-panel {
    margin-bottom: 14px;
  }
  .recent-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 8px;
  }
  .recent-head strong {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .recent-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .recent-row {
    display: grid;
    grid-template-columns: 64px 1fr auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    text-align: left;
    padding: 6px 8px;
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: var(--bg-primary);
    color: inherit;
    cursor: pointer;
    font: inherit;
  }
  .recent-row:hover {
    border-color: var(--border-color);
  }
  .recent-actor {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .recent-sum {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-row small {
    color: var(--text-muted);
    font-size: 10px;
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
  .notice.warning { color: #fde68a; background: rgba(245, 158, 11, 0.08); border-color: rgba(245, 158, 11, 0.28); }
  .network-pending { display: flex; flex-direction: column; gap: 8px; }
  .network-pending-head { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; justify-content: space-between; }
  .trust-card-line { font-size: 12px; color: var(--text-secondary); display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .action-diff-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .diff-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }
  .diff-chip.add { color: #86efac; border-color: rgba(34, 197, 94, 0.35); background: rgba(34, 197, 94, 0.1); }
  .diff-chip.remove { color: #fca5a5; border-color: rgba(239, 68, 68, 0.35); background: rgba(239, 68, 68, 0.1); }
  .diff-chip.change { color: #fde68a; border-color: rgba(245, 158, 11, 0.35); background: rgba(245, 158, 11, 0.1); }
  .muted-box { padding: 12px; border-radius: 10px; border: 1px dashed var(--border-color); }
  .loading, .empty { padding: 24px; text-align: center; color: var(--text-muted); }
  .log-pre {
    margin: 0;
    max-height: 320px;
    padding: 12px;
    border-radius: var(--border-radius-md);
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
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
  }
  :global(.spin) { animation: dx-spin 0.9s linear infinite; }
  @keyframes dx-spin { to { transform: rotate(360deg); } }
  .trail-notice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .trail-links { display: inline-flex; flex-wrap: wrap; gap: 4px; }
  .dx-empty-sources {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    color: var(--text-muted);
  }
  .dx-drop {
    margin-top: 4px;
    padding: 10px;
    border: 1px dashed var(--border-color);
    border-radius: var(--border-radius-sm);
    font-size: 12px;
    color: var(--text-muted);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dx-drop-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .dx-drop-row input {
    flex: 1;
    min-width: 160px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: inherit;
  }
  .scanner-cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 10px;
    margin-top: 12px;
  }
  .scanner-card {
    padding: 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    font-size: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .scanner-card pre {
    margin: 0;
    max-height: 120px;
    overflow: auto;
    font-size: 10px;
    color: var(--text-muted);
  }
  .author-form textarea.mono { font-family: ui-monospace, monospace; font-size: 11px; }
  .author-cases { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin-top: 8px; }
</style>
