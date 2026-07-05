<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
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
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type Diagnostic = {
    severity: string;
    code: string;
    message: string;
    relatedNodes?: any[];
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
  let error: string | null = null;
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
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
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
  async function applyFix(diag: Diagnostic) {
    if (!$projectPath) return;
    loading = true;
    try {
      message = `Resolved conflict for ${diag.code}`;
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
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
  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
</script>

<div class="diagnostics">
  <div class="toolbar">
    <div class="title">
      <Stethoscope size={18} />
      <span>Diagnose 2.0</span>
    </div>
    <div class="actions">
      <button class="secondary" on:click={createFixPlan} disabled={!$projectPath || planning}>
        <Wrench size={16} />
        {planning ? "Creating..." : "Create fix plan"}
      </button>
      <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}

  {#if loading && !diagnosis}
    <div class="loading">Loading crash diagnosis...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to analyze crash reports, latest.log and recent snapshots.</div>
  {:else if diagnosis}
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
          {#each diagnosis.reports as report}
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
          {#each graphDiagnostics as diag}
            <div class="conflict-card {diag.severity.toLowerCase()}">
              <div class="conflict-header">
                <strong>{diag.code}</strong>
                <button class="secondary" style="padding: 4px 8px; font-size: 11px;" on:click={() => applyFix(diag)}>Fix Issue</button>
              </div>
              <p style="font-size: 12px; margin: 4px 0; color: var(--text-muted);">{diag.message}</p>
              {#if diag.relatedNodes && diag.relatedNodes.length > 0}
                <div class="related-mods">
                  {#each diag.relatedNodes as node}
                    <span class="mod-pill">{node}</span>
                  {/each}
                </div>
              {/if}
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
          {#if selectedReport.sections?.length}
            <div class="sections-strip">
              {#each selectedReport.sections as section}
                <div class="section-pill">
                  <strong>{section.title}</strong>
                  <span>lines {section.startLine}-{section.endLine}</span>
                </div>
              {/each}
            </div>
            <div class="section-previews">
              {#each selectedReport.sections.slice(0, 4) as section}
                <details>
                  <summary>{section.title}</summary>
                  <pre>{section.preview || "No preview."}</pre>
                </details>
              {/each}
            </div>
          {/if}
          <pre class="report-content">{selectedReport.content}</pre>
        {:else}
          <div class="empty inline">No crash report to open yet.</div>
        {/if}

        <div class="latest-log">
          <div class="panel-header small">
            <h2><Terminal size={16} /> latest.log tail</h2>
            <span>{diagnosis.latestLog.exists ? "last parser window" : "missing"}</span>
          </div>
          <pre class="log-content">{diagnosis.latestLog.tail || "logs/latest.log will appear here after a Test run."}</pre>
        </div>

        <div class="latest-log">
          <div class="panel-header small">
            <h2><Terminal size={16} /> launcher.log tail</h2>
            <span>{diagnosis.launcherLog.exists ? "launcher parser window" : "missing"}</span>
          </div>
          <pre class="log-content">{diagnosis.launcherLog.tail || "launcher.log will appear here after launcher events."}</pre>
        </div>
      </section>

      <aside class="inspector panel">
        {#if signalGroups.length > 0}
          <h2><Stethoscope size={16} /> Signal groups</h2>
          <div class="signal-groups">
            {#each signalGroups as group}
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
            {#each selectedReport.modEntries.slice(0, 18) as entry}
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
            {#each suspected as mod}
              <div class="suspect-card">
                <div class="suspect-head">
                  <div>
                    <strong>{mod.name}</strong>
                    <span>{mod.id}{mod.version ? ` · ${mod.version}` : ""}</span>
                  </div>
                  <b>{mod.confidence}%</b>
                </div>
                <div class="badges">
                  <small class:known={mod.knownInManifest}>{mod.knownInManifest ? "manifest" : "inferred"}</small>
                  {#if mod.fileName}<small>{mod.fileName}</small>{/if}
                </div>
                {#each mod.evidence.slice(0, 3) as item}
                  <div class="evidence">
                    <code>{item.kind} · {item.source}:{item.lineNumber}</code>
                    <p>{item.text}</p>
                  </div>
                {/each}
              </div>
            {/each}
          </div>
        {/if}

        <h2 class="changes-title"><History size={16} /> Last changes</h2>
        {#if diagnosis.recentSnapshots.length === 0}
          <div class="muted-box">No snapshots yet. Risky changes should create snapshots before edits.</div>
        {:else}
          <div class="snapshot-list">
            {#each diagnosis.recentSnapshots as snapshot}
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
                {#each plan.actions as action}
                  <li>{actionLabel(action)}</li>
                {/each}
              </ul>
            {:else}
              <div class="muted-box compact">No deterministic file operation proposed. Use the notes as an investigation checklist.</div>
            {/if}
          </div>
        {/if}
      </aside>
    </div>

    <section class="graph-health panel">
      <h2><Stethoscope size={16} /> Graph diagnostics</h2>
      {#if graphDiagnostics.length === 0}
        <div class="empty success">
          <AlertCircle size={28} color="#1bd96a" />
          <p>No dependency graph issues found.</p>
        </div>
      {:else}
        <div class="diagnostic-list">
          {#each graphDiagnostics as d}
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
  .toolbar, .actions, .title, .panel-header, .suspect-head, .meta, .plan-meta { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .title, h2 { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .actions { gap: 10px; }
  h2 { display: flex; font-size: 14px; margin: 0 0 12px; }
  .notice { padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
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
  .related-mods { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 8px; }
  .mod-pill { font-size: 11px; background: var(--bg-secondary); padding: 2px 6px; border-radius: 4px; border: 1px solid var(--border-color); }

  .log-title, .changes-title { margin-top: 20px; }
  .log-status { padding: 10px; border: 1px dashed var(--border-color); border-radius: 10px; }
  .log-status.ok { color: var(--accent-primary); border-color: rgba(27, 217, 106, 0.28); }
  pre { margin: 0; border-radius: 12px; background: #09090b; color: #d4d4d8; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; font-size: 12px; line-height: 1.55; white-space: pre-wrap; overflow: auto; }
  .sections-strip { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
  .section-pill { display: grid; gap: 2px; padding: 8px 10px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .section-pill strong { color: var(--text-secondary); font-size: 12px; }
  .section-pill span { color: var(--text-muted); font-size: 11px; }
  .section-previews { display: grid; gap: 8px; margin-bottom: 12px; }
  .section-previews details { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 12px; padding: 10px; }
  .section-previews summary { cursor: pointer; color: var(--text-secondary); font-weight: 800; }
  .section-previews pre { margin-top: 8px; max-height: 180px; padding: 10px; }
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
  .suspect-head strong { display: block; color: var(--text-primary); }
  .suspect-head b { color: var(--accent-primary); }
  .badges { display: flex; gap: 6px; flex-wrap: wrap; margin: 8px 0; }
  .badges small { color: var(--text-muted); background: var(--bg-elevated); border-radius: 999px; padding: 3px 8px; }
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
  @media (max-width: 1180px) { .diagnose-grid { grid-template-columns: 1fr; } .stats { grid-template-columns: repeat(2, 1fr); } }
  @media (max-width: 700px) { .stats { grid-template-columns: 1fr; } }
</style>
