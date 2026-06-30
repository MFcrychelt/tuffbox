<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Rocket, RefreshCw, Save, Tag, AlertTriangle, CheckCircle2, Camera } from "lucide-svelte";
  import { projectPath, projectInfo, recentProjects } from "../lib/store";

  type Issue = { severity: "error" | "warning"; code: string; message: string; target?: string | null };

  let version = $projectInfo?.version ?? "1.0.0";
  let changelog = "";
  let issues: Issue[] = [];
  let loading = false;
  let error = "";
  let message = "";
  let lastLoadedPath: string | null = null;

  async function refresh() {
    if (!$projectPath) return;
    loading = true;
    error = "";
    message = "";
    try {
      issues = await invoke("validate_modrinth_export", { path: $projectPath });
      changelog = await invoke("generate_release_changelog", { path: $projectPath });
      version = $projectInfo?.version ?? version;
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function saveVersion() {
    if (!$projectPath || !version.trim()) return;
    loading = true;
    error = "";
    message = "";
    try {
      const info: any = await invoke("update_project_version", { path: $projectPath, version: version.trim() });
      projectInfo.set(info);
      recentProjects.updateInfo($projectPath, info);
      message = `Version updated to ${version}. Auto snapshot created.`;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function createReleaseSnapshot() {
    if (!$projectPath) return;
    loading = true;
    error = "";
    message = "";
    try {
      const result: any = await invoke("create_release_snapshot", { path: $projectPath, changelog });
      message = `Release snapshot ${result.snapshot.id} created. Changelog: ${result.changelogPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $: errorCount = issues.filter((issue) => issue.severity === "error").length;
  $: warningCount = issues.filter((issue) => issue.severity === "warning").length;
  $: if ($projectPath && lastLoadedPath !== $projectPath) refresh();
</script>

<div class="release-room">
  <div class="toolbar">
    <div class="title"><Rocket size={18} /> Release room</div>
    <button class="ghost" on:click={refresh} disabled={!$projectPath || loading}>
      <RefreshCw size={16} class={loading ? "spin" : ""} /> Refresh
    </button>
  </div>

  {#if error}<div class="notice error"><AlertTriangle size={16} /> {error}</div>{/if}
  {#if message}<div class="notice success"><CheckCircle2 size={16} /> {message}</div>{/if}

  {#if !$projectPath}
    <div class="empty">Open a project to prepare a release.</div>
  {:else}
    <div class="layout">
      <section class="panel release-panel">
        <h2>Version & checklist</h2>
        <label>
          Release version
          <div class="version-row">
            <input bind:value={version} placeholder="1.0.0" />
            <button class="secondary" on:click={saveVersion} disabled={loading || !version.trim()}>
              <Tag size={16} /> Save version
            </button>
          </div>
        </label>

        <div class="scorecards">
          <div class:error-card={errorCount > 0}><strong>{errorCount}</strong><span>blocking errors</span></div>
          <div class:warning-card={warningCount > 0}><strong>{warningCount}</strong><span>warnings</span></div>
          <div><strong>{changelog.split("\n").filter(Boolean).length}</strong><span>changelog lines</span></div>
        </div>

        <div class="issues">
          {#if issues.length === 0}
            <div class="issue ok"><CheckCircle2 size={16} /> Export validation passed.</div>
          {:else}
            {#each issues as issue}
              <div class="issue {issue.severity}">
                <strong>{issue.code}</strong>
                <span>{issue.message}</span>
                {#if issue.target}<code>{issue.target}</code>{/if}
              </div>
            {/each}
          {/if}
        </div>

        <button on:click={createReleaseSnapshot} disabled={loading || errorCount > 0}>
          <Camera size={16} /> Create release snapshot
        </button>
      </section>

      <section class="panel changelog-panel">
        <div class="changelog-header">
          <div>
            <h2>Changelog</h2>
            <p>Generated from manifest, brief, diagnostics, mods and recent snapshots. Edit before creating release snapshot.</p>
          </div>
          <button class="secondary" on:click={refresh} disabled={loading}>Regenerate</button>
        </div>
        <textarea bind:value={changelog} spellcheck="false" />
      </section>
    </div>
  {/if}
</div>

<style>
  .release-room { max-width: 1400px; }
  .toolbar, .title, .notice, .version-row, .changelog-header { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .notice { gap: 10px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .layout { display: grid; grid-template-columns: 380px minmax(0, 1fr); gap: 16px; }
  .panel, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .panel { padding: 18px; }
  .release-panel { display: grid; gap: 18px; align-content: start; }
  label { display: grid; gap: 8px; color: var(--text-secondary); font-weight: 700; }
  .version-row { gap: 10px; }
  .version-row input { flex: 1; }
  .scorecards { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .scorecards div { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 14px; padding: 12px; display: grid; gap: 3px; }
  .scorecards strong { font-size: 24px; }
  .scorecards span, .changelog-header p { color: var(--text-muted); font-size: 12px; }
  .error-card { border-color: rgba(239, 68, 68, 0.35) !important; color: #fecaca; }
  .warning-card { border-color: rgba(245, 158, 11, 0.35) !important; color: #fde68a; }
  .issues { display: grid; gap: 8px; }
  .issue { display: grid; gap: 4px; padding: 12px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .issue.warning { border-color: rgba(245, 158, 11, 0.3); }
  .issue.error { border-color: rgba(239, 68, 68, 0.3); }
  .issue.ok { color: var(--accent-primary); display: flex; align-items: center; gap: 8px; }
  .issue span { color: var(--text-muted); }
  code { color: var(--text-secondary); font-family: ui-monospace, monospace; }
  .changelog-panel { overflow: hidden; display: flex; flex-direction: column; min-height: 680px; }
  .changelog-header { justify-content: space-between; gap: 16px; padding-bottom: 14px; border-bottom: 1px solid var(--border-color); margin-bottom: 0; }
  textarea { flex: 1; resize: none; min-height: 600px; border: 0; outline: none; background: #09090b; color: #e5e7eb; padding: 18px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; line-height: 1.6; }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1100px) { .layout { grid-template-columns: 1fr; } }
</style>
