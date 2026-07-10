<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Rocket, RefreshCw, Save, Tag, AlertTriangle, CheckCircle2, Camera, Package, Server, FolderOpen } from "lucide-svelte";
  import { api } from "../lib/api";
  import { projectPath, projectInfo, recentProjects } from "../lib/store";

  type Issue = { severity: "error" | "warning"; code: string; message: string; target?: string | null };
  type Artifact = { id: string; kind: string; path: string; createdAt: string; fileCount: number; overrideCount: number };

  let version = $projectInfo?.version ?? "1.0.0";
  let changelog = "";
  let issues: Issue[] = [];
  let artifacts: Artifact[] = [];
  let checklist: Record<string, boolean> = {
    version: false,
    validation: false,
    artifacts: false,
    changelog: false,
    snapshot: false,
  };
  let publishTargets = [
    { id: "modrinth", label: "Modrinth", state: "not exported", artifactKind: "mrpack" },
    { id: "curseforge", label: "CurseForge", state: "not exported", artifactKind: "curseforge" },
    { id: "github", label: "GitHub Releases", state: "not prepared", artifactKind: null },
  ];

  let exportLoading: string | null = null;

  let githubRelease: any = null;
  let githubLoading = false;

  async function generateGithubRelease() {
    if (!$projectPath) return;
    githubLoading = true;
    try {
      const tag = version.trim() ? `v${version.trim()}` : null;
      githubRelease = await invoke("generate_github_release", { path: $projectPath, tag, target: null });
      publishTargets = publishTargets.map(t => t.id === "github" ? { ...t, state: "prepared" } : t);
      message = `GitHub release prepared: ${githubRelease.tagName}`;
    } catch(e) { error = String(e); }
    finally { githubLoading = false; }
  }

  async function copyReleaseBody() {
    if (!githubRelease) return;
    try { await navigator.clipboard.writeText(githubRelease.body); message = "Release body copied to clipboard."; }
    catch { message = "Failed to copy."; }
  }
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
      artifacts = await invoke("list_release_artifacts", { path: $projectPath });
      version = $projectInfo?.version ?? version;
      syncPublishTargets();
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function syncPublishTargets() {
    publishTargets = publishTargets.map((target) => {
      if (target.id === "github") {
        return { ...target, state: githubRelease ? "prepared" : "not prepared" };
      }
      const kind = target.artifactKind;
      const found = kind ? artifacts.find((a) => a.kind === kind) : null;
      return {
        ...target,
        state: found ? `exported · ${found.fileCount} files` : "not exported",
      };
    });
  }

  async function exportArtifact(kind: "mrpack" | "server" | "prism" | "curseforge") {
    if (!$projectPath) return;
    exportLoading = kind;
    error = "";
    message = "";
    try {
      let result: { path: string; fileCount: number };
      if (kind === "mrpack") result = await api.export.modrinthPack(null, $projectPath);
      else if (kind === "server") result = await api.export.serverPack(null, $projectPath);
      else if (kind === "prism") result = await api.export.prismInstance(null, $projectPath);
      else result = await api.export.curseforgePack(null, $projectPath);
      message = `Exported ${kind}: ${result.path}`;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      exportLoading = null;
    }
  }

  async function openProjectFolder() {
    if (!$projectPath) return;
    try {
      await invoke("open_project_folder", { path: $projectPath });
    } catch (e) {
      error = String(e);
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

  async function copyArtifactPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      message = "Artifact path copied.";
    } catch {
      message = path;
    }
  }

  function markPublishTarget(id: string) {
    publishTargets = publishTargets.map((target) =>
      target.id === id ? { ...target, state: "marked ready (manual upload)" } : target
    );
    message = `${publishTargets.find((target) => target.id === id)?.label} marked for manual upload.`;
  }

  async function createReleaseDraft() {
    if (!$projectPath) return;
    loading = true;
    error = "";
    message = "";
    try {
      const result: any = await invoke("create_release_draft", { path: $projectPath, changelog });
      message = `Release draft created: ${result.draftPath} (${result.artifactCount} artifacts).`;
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
      checklist.snapshot = true;
      message = `Release snapshot ${result.snapshot.id} created. Changelog: ${result.changelogPath}`;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $: errorCount = issues.filter((issue) => issue.severity === "error").length;
  $: warningCount = issues.filter((issue) => issue.severity === "warning").length;
  $: checklist.version = Boolean(version.trim());
  $: checklist.validation = errorCount === 0;
  $: checklist.artifacts = artifacts.length > 0;
  $: checklist.changelog = changelog.trim().length > 0;
  $: releaseReady = Object.values(checklist).every(Boolean);
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

        <div class="release-checklist">
          <h3>Release checklist</h3>
          {#each Object.entries(checklist) as [key, done]}
            <label class:done>
              <input type="checkbox" bind:checked={checklist[key]} />
              <span>{key}</span>
            </label>
          {/each}
          <div class="ready" class:ok={releaseReady}>{releaseReady ? "Ready to ship" : "Release not ready yet"}</div>
        </div>

        <div class="publish-targets">
          <h3>Export & publish</h3>
          {#each publishTargets as target}
            <div class="publish-target">
              <div><strong>{target.label}</strong><span>{target.state}</span></div>
              <div class="target-actions">
                {#if target.id === "modrinth"}
                  <button class="secondary mini" on:click={() => exportArtifact("mrpack")} disabled={!!exportLoading || errorCount > 0}>
                    {exportLoading === "mrpack" ? "…" : "Export .mrpack"}
                  </button>
                {:else if target.id === "curseforge"}
                  <button class="secondary mini" on:click={() => exportArtifact("curseforge")} disabled={!!exportLoading || errorCount > 0}>
                    {exportLoading === "curseforge" ? "…" : "Export zip"}
                  </button>
                {:else if target.id === "github"}
                  <button class="secondary mini" on:click={generateGithubRelease} disabled={githubLoading}>
                    {githubLoading ? "…" : "Generate"}
                  </button>
                {/if}
                {#if target.id !== "github"}
                  <button class="ghost mini" on:click={() => markPublishTarget(target.id)}>Mark ready</button>
                {/if}
              </div>
            </div>
          {/each}
        </div>

        <div class="quick-exports">
          <h3>Quick exports</h3>
          <div class="export-btns">
            <button class="secondary mini" on:click={() => exportArtifact("server")} disabled={!!exportLoading}>
              <Server size={12} /> {exportLoading === "server" ? "…" : "Server pack"}
            </button>
            <button class="secondary mini" on:click={() => exportArtifact("prism")} disabled={!!exportLoading}>
              <Package size={12} /> {exportLoading === "prism" ? "…" : "Prism zip"}
            </button>
            <button class="ghost mini" on:click={openProjectFolder}>
              <FolderOpen size={12} /> Open folder
            </button>
          </div>
        </div>

        <div class="artifact-list">
          <h3>Artifacts</h3>
          {#if artifacts.length === 0}
            <div class="muted-box">No exported artifacts recorded yet. Use Export stage first.</div>
          {:else}
            {#each artifacts.slice(0, 6) as artifact}
              <div class="artifact-row">
                <strong>{artifact.kind}</strong>
                <span>{artifact.path}</span>
                <small>{artifact.fileCount} files · {artifact.overrideCount} overrides</small>
                <button class="ghost mini" on:click={() => copyArtifactPath(artifact.path)}>Copy path</button>
              </div>
            {/each}
          {/if}
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

        {#if githubRelease}
          <div class="github-preview">
            <h4>GitHub Release: {githubRelease.tagName}</h4>
            <div class="github-actions">
              <button class="secondary mini" on:click={copyReleaseBody}>Copy body</button>
              <span class="gh-meta">{githubRelease.artifactCount} artifacts · release.json saved</span>
            </div>
            <pre class="gh-body-preview">{githubRelease.body?.slice(0, 2000)}{githubRelease.body?.length > 2000 ? "..." : ""}</pre>
          </div>
        {/if}

        <div class="release-actions">
          <button class="secondary" on:click={createReleaseDraft} disabled={loading || !changelog.trim()}>
            <Rocket size={16} /> Create release draft
          </button>
          <button on:click={createReleaseSnapshot} disabled={loading || errorCount > 0}>
            <Camera size={16} /> Create release snapshot
          </button>
        </div>
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
  .release-room { max-width: none; width: 100%; }
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
  .release-checklist, .artifact-list, .publish-targets { display: grid; gap: 8px; }
  .release-checklist h3, .artifact-list h3, .publish-targets h3 { margin: 0; color: var(--text-secondary); font-size: 14px; }
  .release-checklist label { display: flex; align-items: center; gap: 8px; padding: 9px 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 12px; text-transform: none; letter-spacing: 0; }
  .release-checklist label.done { border-color: rgba(27, 217, 106, .28); }
  .release-checklist input { width: auto; }
  .ready { padding: 9px 10px; border-radius: 12px; color: var(--text-muted); background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .ready.ok { color: var(--accent-primary); border-color: rgba(27, 217, 106, .35); }
  .publish-target { display: flex; justify-content: space-between; gap: 10px; align-items: center; padding: 10px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .target-actions, .export-btns { display: flex; gap: 6px; flex-wrap: wrap; }
  .quick-exports { display: grid; gap: 8px; }
  .quick-exports h3 { margin: 0; color: var(--text-secondary); font-size: 14px; }
  .publish-target div { display: grid; gap: 3px; }
  .publish-target strong { color: var(--text-primary); }
  .publish-target span { color: var(--text-muted); font-size: 12px; }
  .artifact-row, .muted-box { display: grid; gap: 4px; padding: 10px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .artifact-row strong { color: var(--text-primary); text-transform: uppercase; font-size: 12px; }
  .artifact-row span, .artifact-row small, .muted-box { color: var(--text-muted); font-size: 12px; word-break: break-all; }
  .mini { padding: 5px 8px; font-size: 11px; justify-self: start; }
  .release-actions { display: flex; gap: 10px; flex-wrap: wrap; }
  .issues { display: grid; gap: 8px; }
  .issue { display: grid; gap: 4px; padding: 12px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .issue.warning { border-color: rgba(245, 158, 11, 0.3); }
  .issue.error { border-color: rgba(239, 68, 68, 0.3); }
  .issue.ok { color: var(--accent-primary); display: flex; align-items: center; gap: 8px; }
  .github-preview { margin-top: 14px; padding: 14px; border: 1px solid rgba(139,92,246,.25); border-radius: var(--border-radius-lg); background: rgba(139,92,246,.03); }
  .github-preview h4 { color: var(--accent-secondary); margin: 0 0 8px; font-size: 14px; }
  .github-actions { display: flex; gap: 8px; align-items: center; margin-bottom: 10px; }
  .gh-meta { color: var(--text-muted); font-size: 11px; }
  .gh-body-preview { margin: 0; padding: 12px; border-radius: 8px; background: #0d0d10; color: #d4d4d8; font-size: 11px; line-height: 1.5; max-height: 300px; overflow: auto; white-space: pre-wrap; font-family: ui-monospace,monospace; }

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
