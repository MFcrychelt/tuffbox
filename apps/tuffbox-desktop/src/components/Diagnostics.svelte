<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    MessageCircle,
    Stethoscope,
    RefreshCw,
    AlertCircle,
    AlertTriangle,
    Info,
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
    Gauge,
    Copy,
    ChevronDown,
    BadgeCheck,
    CircleHelp,
    Ban,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

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
  };

  type CrashDiagnosis = {
    reports: CrashReportSummary[];
    selectedReport?: CrashReportAnalysis | null;
    latestLog: LatestLogAnalysis;
    launcherLog: LatestLogAnalysis;
    suspectedMods: SuspectedMod[];
    recentSnapshots: Snapshot[];
    graphDiagnostics: Diagnostic[];
    fixPlan: any;
  };

  let diagnosis: CrashDiagnosis | null = null;
  let selectedReportId = "";
  let loading = false;
  let planning = false;
  let applying = false;
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
      const data: CrashDiagnosis = await invoke("get_crash_diagnosis", {
        path: $projectPath,
        reportId: selectedReportId || null,
      });
      diagnosis = data;
      selectedReportId = data.selectedReport?.summary.id ?? data.reports[0]?.id ?? "";
      plan = null;
      detectWrongLoaderMods();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function onProjectPathChange(path: string | null) {
    if (!path || path === lastLoadedPath) return;
    lastLoadedPath = path;
    void load(true);
  }

  async function chooseReport(reportId: string) {
    selectedReportId = reportId;
    await load(true);
  }

  async function createFixPlan() {
    if (!$projectPath) return;
    planning = true;
    error = null;
    try {
      plan = await invoke("create_crash_fix_plan", {
        path: $projectPath,
        reportId: selectedReportId || null,
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
      await invoke("add_modrinth_mod_with_dependencies", {
        path: $projectPath,
        modId,
        side: "auto",
      });
      message = `Installed ${modId} with dependencies. Reloading...`;
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
      const result: any = await invoke("disable_project_mod", {
        path: $projectPath,
        modId,
      });
      message = result?.alreadyDisabled
        ? `${result.name ?? modId} was already disabled.`
        : `Disabled ${result?.name ?? modId} (${result?.fileName ?? "★.disabled"}). Rerun the Test profile to verify.`;
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

  /// Per-diagnostic fix: remove a conflicting mod from the project.
  async function fixRemoveMod(modId: string, idx: number) {
    if (!$projectPath) return;
    fixingIdx = idx;
    error = null;
    message = null;
    try {
      await invoke("remove_project_mod", { path: $projectPath, modId });
      message = `Removed ${modId}. Reloading...`;
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

  // Performance audit state
  let perfFindings: any[] = [];
  let perfLoading = false;

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
      const result: any = await invoke("run_crash_assistant_full", { path: $projectPath });
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

  async function runAiExplain() {
    if (!$projectPath) return;
    aiLoading = true;
    error = null;
    try {
      const context: any = await invoke("build_ai_crash_context", { path: $projectPath });
      aiContext = context;
      aiPrompt = context.prompt ?? "";
      aiShowPrompt = false;
      aiAnalysis = await invoke("analyze_crash_with_ai", { path: $projectPath });
      message = "AI analysis ready. Review the suggestions before applying any fix plan.";
    } catch (e) {
      error = String(e);
      aiAnalysis = null;
    } finally {
      aiLoading = false;
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

  async function runPerfAudit() {
    if (!$projectPath) return;
    perfLoading = true;
    try {
      perfFindings = await invoke("audit_performance", { path: $projectPath });
    } catch (e) {
      error = String(e);
    } finally {
      perfLoading = false;
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
        reportId: selectedReportId || null,
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

  $: graphDiagnostics = diagnosis?.graphDiagnostics ?? [];
  $: allSignals = [
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
</script>

<div class="diagnostics">
  <div class="toolbar">
    <div class="title">
      <Stethoscope size={18} />
      <span>Diagnose 2.0</span>
    </div>
    <div class="primary-actions">
      <button class="refresh" on:click={() => load(true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        {loading ? "Refreshing..." : "Refresh"}
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
      <button class="secondary" on:click={runPerfAudit} disabled={!$projectPath || perfLoading}>
        <Gauge size={16} />
        {perfLoading ? "Auditing..." : "Perf audit"}
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
        {duplicateLoading ? "Scanning..." : "Find dupes"}
      </button>
      <button class="secondary" on:click={generateUnify} disabled={!$projectPath || unifyLoading}>
        <Zap size={16} />
        {unifyLoading ? "Generating..." : "Unify config"}
      </button>
      <button class="secondary" on:click={runAiExplain} disabled={!$projectPath || aiLoading}>
        <MessageCircle size={16} />
        {aiLoading ? "Analyzing..." : "AI explain"}
      </button>
    </div>
  </details>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}

  {#if loading && !diagnosis}
    <div class="loading">Loading crash diagnosis...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to analyze crash reports, latest.log and recent snapshots.</div>
  {:else if diagnosis}
    <section class="diagnosis-summary" class:neutral={!topSuspect}>
      {#if topSuspect}
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

    <div class="diagnose-grid">
      <aside class="reports panel">
        <h2><Bug size={16} /> Crash reports</h2>
        {#if diagnosis.reports.length === 0}
          <div class="muted-box">No files in <code>crash-reports/*.txt</code>.</div>
        {:else}
          {#each diagnosis.reports as report (report.id)}
            <button class="report-card" class:selected={selectedReportId === report.id} on:click={() => chooseReport(report.id)}>
              <strong>{report.name}</strong>
              <span>{formatBytes(report.size)} · {formatDate(report.modified)}</span>
            </button>
          {/each}
        {/if}

        <h2 class="log-title"><Terminal size={16} /> latest.log</h2>
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
            <h2><FileText size={16} /> {selectedReport?.summary.name ?? "No crash report selected"}</h2>
            <p>{selectedReport ? `${selectedReport.signals.length} parser signals · ${selectedReport.suspectedMods.length} report suspects` : "Run Minecraft until it crashes or place a report in crash-reports/."}</p>
          </div>
        </div>

        {#if selectedReport}
          <div class="crash-preview">
            <div class="crash-preview-bar">
              <span>Crash log preview</span>
              <small>{formatBytes(selectedReport.summary.size)}</small>
            </div>
            <pre class="report-content">{selectedReport.content}</pre>
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
              <div class="ai-stat"><strong>{(aiAnalysis.recommended_actions ?? aiAnalysis.recommendedActions ?? []).length}</strong> actions</div>
            </div>
            {#if (aiAnalysis.needs_user_review ?? aiAnalysis.needsUserReview) !== false}
              <div class="notice warning">AI suggestions require manual review. Nothing was applied automatically.</div>
            {:else}
              <div class="notice warning">These are suggestions only — nothing was applied automatically.</div>
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
            {#if (aiAnalysis.recommended_actions ?? aiAnalysis.recommendedActions)?.length}
              <div class="ai-list">
                <strong>Recommended actions</strong>
                <ul>
                  {#each (aiAnalysis.recommended_actions ?? aiAnalysis.recommendedActions) as action, aIdx (aIdx)}
                    <li>
                      <strong>{action.action_type ?? action.actionType}</strong>
                      {#if action.mod_id ?? action.modId}<code>{action.mod_id ?? action.modId}</code>{/if}
                      <span>{action.description}</span>
                      <small>risk: {action.risk}</small>
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
          </div>
        {:else}
          <p class="ai-desc">AI analysis failed or is incomplete. You can still use the raw prompt fallback below.</p>
        {/if}
        {#if aiContext}
          <div class="ai-stats">
            <div class="ai-stat"><strong>{aiContext.findingsCount}</strong> findings</div>
            <div class="ai-stat"><strong>{aiContext.promptLength}</strong> chars prompt</div>
            <div class="ai-stat"><strong>{aiContext.context?.installedMods?.length ?? 0}</strong> mods</div>
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

    {#if crashFindings.length > 0}
      <section class="crash-assistant panel">
        <h2><Zap size={16} /> Crash Assistant ({crashFindings.length} finding{crashFindings.length > 1 ? "s" : ""})</h2>
        <div class="crash-list">
          {#each crashFindings as f (f.code + f.title)}
            <div class="crash-card {f.severity}">
              <div class="crash-card-header">
                <span class="crash-sev {f.severity}">{f.severity}</span>
                <strong>{f.title}</strong>
                <code class="crash-code">{f.code}</code>
              </div>
              <p>{f.description}</p>
              {#if f.autoFix}
                <div class="crash-fix">
                  <strong>Auto-fix:</strong> {f.autoFix}
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

    {#if perfFindings.length > 0}
      <section class="perf-audit panel">
        <h2><Gauge size={16} /> Performance audit ({perfFindings.length})</h2>
        <div class="perf-list">
          {#each perfFindings as finding, pIdx (finding.code + (finding.file ?? '') + pIdx)}
            <div class="perf-card {finding.severity}">
              <div class="perf-card-header">
                <strong>{finding.code}</strong>
                <span class="perf-severity">{finding.severity}</span>
              </div>
              <p>{finding.message}</p>
              {#if finding.file}<code class="perf-file">{finding.file}</code>{/if}
            </div>
          {/each}
        </div>
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

<style>
  .diagnostics { max-width: none; width: 100%; }
  .toolbar, .actions, .title, .primary-actions, .panel-header, .suspect-head, .meta, .plan-meta { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 10px; }
  .title, h2 { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 8px; flex-wrap: wrap; }
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
  .diagnose-grid { display: grid; grid-template-columns: 280px minmax(0, 1fr) 380px; gap: 16px; align-items: start; }
  .panel { padding: 16px; min-width: 0; }
  .panel-header { justify-content: space-between; gap: 12px; margin-bottom: 12px; }
  .panel-header h2 { margin: 0 0 4px; }
  .panel-header.small { margin: 18px 0 8px; }
  .panel-header.small span { color: var(--text-muted); font-size: 12px; }
  .report-card { width: 100%; background: var(--bg-tertiary); border: 1px solid var(--border-color); color: var(--text-secondary); padding: 11px; margin-bottom: 8px; display: flex; flex-direction: column; align-items: flex-start; gap: 4px; text-align: left; transform: none; }
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
  pre { margin: 0; border-radius: 12px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow: auto; }
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
  .suspects, .snapshot-list, .diagnostic-list, .signal-groups, .mod-entry-list { display: flex; flex-direction: column; gap: 10px; }
  .suspect-card, .snapshot-row, .diag-card, .plan-card, .signal-group, .mod-entry { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 12px; padding: 12px; }
  .mod-entry { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto; gap: 8px; align-items: center; }
  .mod-entry strong { color: var(--text-primary); }
  .mod-entry span { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .signal-group { display: grid; gap: 5px; border-left: 4px solid rgba(27, 217, 106, .55); }
  .signal-group strong { color: var(--text-primary); }
  .signal-group span, .signal-group small, .signal-group p { color: var(--text-muted); font-size: 12px; }
  .signal-group p { margin: 2px 0; line-height: 1.45; }
  .suspect-head { justify-content: space-between; gap: 10px; }
  .suspect-head strong { display: block; color: var(--text-primary); font-size: 15px; }
  .suspect-card { border-left: 4px solid var(--accent-primary); }
  .suspect-card.unresolved { border-left-color: var(--text-muted); }
  .suspect-identity { display: block; margin-top: 3px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }
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
  .crash-fix { padding: 8px 10px; border-radius: 8px; background: rgba(27,217,106,.08); border: 1px solid rgba(27,217,106,.2); font-size: 12px; color: var(--accent-primary); }
  .crash-fix strong { color: var(--accent-primary); }
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
  .ai-stats { display: grid; grid-template-columns: repeat(3,1fr); gap: 10px; margin-bottom: 12px; }
  .ai-stat { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 10px; padding: 10px; text-align: center; }
  .ai-stat strong { font-size: 20px; color: var(--accent-secondary); }
  .ai-human { color: var(--text-primary); font-size: 14px; line-height: 1.5; margin: 0 0 12px; }
  .ai-list { margin: 10px 0; }
  .ai-list ul { margin: 6px 0 0; padding-left: 18px; display: grid; gap: 6px; }
  .ai-list li { color: var(--text-secondary); font-size: 12px; }
  .ai-list small { color: var(--text-muted); margin-left: 6px; }
  .notice.warning { color: #fde68a; background: rgba(245, 158, 11, 0.08); border: 1px solid rgba(245, 158, 11, 0.28); border-radius: 10px; padding: 10px 12px; margin-bottom: 10px; font-size: 12px; }
  .ai-desc { color: var(--text-muted); font-size: 12px; margin: 0 0 10px; line-height: 1.4; }
  .ai-prompt-text { margin: 10px 0 0; padding: 14px; border-radius: 10px; background: #0d0d10; color: #d4d4d8; font-size: 11px; line-height: 1.5; max-height: 400px; overflow: auto; white-space: pre-wrap; font-family: ui-monospace,monospace; }

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
</style>
