<script lang="ts">
  import { Github, KeyRound, UploadCloud, Link2, Copy, Check } from "@lucide/svelte";
  import { onDestroy, onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../lib/api";
  import { projectPath } from "../lib/store";
  import { copyText } from "../lib/clipboard";
  import { open } from "@tauri-apps/plugin-shell";

  let { repository = $bindable("") }: { repository?: string } = $props();

  let authed = $state(false);
  let busy = $state(false);
  let error = $state("");
  let message = $state("");
  let userCode = $state("");
  let verificationUri = $state("");
  let preview = $state<{
    packVersion: string;
    fileCount: number;
    hasExternalAssets: boolean;
  } | null>(null);
  let shareUrl = $state("");
  type PublishPhase = "" | "staging" | "commit" | "assets" | "done";

  let phase = $state<PublishPhase>("");
  let conflict = $state("");
  let pollTimer: ReturnType<typeof setTimeout> | null = null;
  let pollGeneration = 0;
  let polling = $state(false);
  let loginStarting = $state(false);
  let oauthClientMissing = $state(false);
  let twoPhase = $state<boolean | null>(null);
  let stopProgress: (() => void) | null = null;
  let destroyed = false;
  let attemptId = 0;
  let activePublishAttempt = 0;
  let scopedPath: string | null = "";
  let scopedRepository = repository.trim();

  onMount(() => {
    void listen<{ phase?: string }>("github-pack-progress", (event) => {
      if (!activePublishAttempt) return;
      const nextPhase = event.payload?.phase;
      if (nextPhase === "staging" || nextPhase === "commit" || nextPhase === "assets" || nextPhase === "done") {
        phase = nextPhase;
      }
    }).then((unlisten) => {
      if (destroyed) {
        unlisten();
      } else {
        stopProgress = unlisten;
      }
    }).catch((cause) => {
      if (!destroyed) error = `GitHub publish progress is unavailable: ${String(cause)}`;
    });
  });

  onMount(() => projectPath.subscribe((path) => {
    if (path !== scopedPath) {
      scopedPath = path;
      clearScopedState();
    }
    void refreshAuth();
  }));

  $effect(() => {
    const nextRepository = repository.trim();
    if (nextRepository !== scopedRepository) {
      scopedRepository = nextRepository;
      clearScopedState();
    }
  });

  onDestroy(() => {
    destroyed = true;
    stopPolling();
    stopProgress?.();
    stopProgress = null;
    if (copyTimer) clearTimeout(copyTimer);
  });

  function clearScopedState() {
    activePublishAttempt = 0;
    preview = null;
    shareUrl = "";
    phase = "";
    twoPhase = null;
    conflict = "";
    message = "";
    error = "";
    // A device-login flow belongs to the previous project/repo scope: stop
    // polling and unblock the login button instead of leaving it disabled.
    if (polling || loginStarting) {
      stopPolling();
      loginStarting = false;
      userCode = "";
    }
  }

  function stopPolling() {
    pollGeneration += 1;
    polling = false;
    if (pollTimer) clearTimeout(pollTimer);
    pollTimer = null;
  }

  async function refreshAuth() {
    try {
      authed = await api.transport.github.authStatus();
    } catch {
      authed = false;
    }
  }

  async function startLogin() {
    if (loginStarting || polling) return;
    stopPolling();
    const generation = pollGeneration;
    loginStarting = true;
    error = "";
    message = "";
    oauthClientMissing = false;
    try {
      const info = await api.transport.github.startDeviceCode();
      if (generation !== pollGeneration) return;
      userCode = info.userCode;
      verificationUri = info.verificationUri;
      message = info.message;
      polling = true;
      const expiresAt = Date.now() + Math.max(0, info.expiresIn) * 1000;
      let intervalSeconds = Math.max(5, info.interval);

      const schedulePoll = () => {
        if (!polling || generation !== pollGeneration) return;
        const remainingMs = expiresAt - Date.now();
        if (remainingMs <= 0) {
          stopPolling();
          userCode = "";
          error = "GitHub device code expired. Start login again.";
          return;
        }
        pollTimer = setTimeout(poll, Math.min(intervalSeconds * 1000, remainingMs));
      };

      const poll = async () => {
        pollTimer = null;
        if (!polling || generation !== pollGeneration) return;
        if (Date.now() >= expiresAt) {
          stopPolling();
          userCode = "";
          error = "GitHub device code expired. Start login again.";
          return;
        }
        try {
          await api.transport.github.pollDeviceCode();
          if (generation !== pollGeneration) return;
          stopPolling();
          authed = true;
          userCode = "";
          message = "GitHub login saved on this device.";
        } catch (e) {
          if (generation !== pollGeneration) return;
          const text = String(e);
          const normalized = text.toLowerCase();
          if (normalized.includes("slow_down")) {
            intervalSeconds += 5;
            schedulePoll();
          } else if (normalized.includes("authorization_pending")) {
            // The current backend folds GitHub's slow_down into authorization_pending,
            // so conservatively back off while still honoring the initial interval.
            intervalSeconds += 5;
            schedulePoll();
          } else {
            stopPolling();
            userCode = "";
            if (normalized.includes("expired_token") || normalized.includes("expired")) {
              error = "GitHub device code expired. Start login again.";
            } else if (normalized.includes("access_denied") || normalized.includes("denied")) {
              error = "GitHub login was denied.";
            } else {
              error = text;
            }
          }
        }
      };

      schedulePoll();
    } catch (e) {
      if (generation !== pollGeneration) return;
      const text = String(e);
      oauthClientMissing = text.toLowerCase().includes("oauth client id") && text.toLowerCase().includes("not configured");
      error = text;
    } finally {
      if (generation === pollGeneration) loginStarting = false;
    }
  }

  async function runPreview() {
    if (!$projectPath) return;
    const targetPath = $projectPath;
    const attempt = ++attemptId;
    busy = true;
    activePublishAttempt = 0;
    preview = null;
    shareUrl = "";
    phase = "";
    twoPhase = null;
    conflict = "";
    message = "";
    error = "";
    try {
      const result = await api.transport.github.stagePreview(targetPath);
      if (attempt !== attemptId || targetPath !== $projectPath) return;
      preview = {
        packVersion: result.packVersion,
        fileCount: result.fileCount,
        hasExternalAssets: result.hasExternalAssets,
      };
      message = `Staged ${result.fileCount} files (v${result.packVersion}).`;
    } catch (e) {
      if (attempt !== attemptId || targetPath !== $projectPath) return;
      error = String(e);
    } finally {
      if (attempt === attemptId) busy = false;
    }
  }

  async function publishPack() {
    if (!$projectPath || !repository.trim()) return;
    const targetPath = $projectPath;
    const targetRepository = repository.trim();
    const attempt = ++attemptId;
    const hasExternalAssets = preview?.hasExternalAssets ?? null;
    busy = true;
    activePublishAttempt = attempt;
    preview = null;
    shareUrl = "";
    error = "";
    message = "";
    conflict = "";
    phase = "staging";
    twoPhase = hasExternalAssets;
    try {
      const result = await api.transport.github.publish(targetRepository, targetPath);
      if (attempt !== attemptId || targetPath !== $projectPath || targetRepository !== repository.trim()) return;
      if (result.conflict) {
        conflict = String(result.message ?? "Branch moved. Pull or resolve the GitHub repo, then publish again. TuffBox never force-pushes.");
        phase = "";
        return;
      }
      shareUrl = String(result.shareUrl ?? "");
      twoPhase = typeof result.twoPhase === "boolean" ? result.twoPhase : twoPhase;
      phase = "done";
      message = result.noop
        ? String(result.message ?? "Already up to date.")
        : `Published ${shareUrl || "to GitHub"}.`;
    } catch (e) {
      if (attempt !== attemptId || targetPath !== $projectPath || targetRepository !== repository.trim()) return;
      error = String(e);
      phase = "";
    } finally {
      if (activePublishAttempt === attempt) activePublishAttempt = 0;
      if (attempt === attemptId) busy = false;
    }
  }

  async function openShare() {
    if (!shareUrl) return;
    await open(shareUrl);
  }

  // Ready-to-paste message for messengers: browsable https link first (works
  // for everyone), one-click tuffbox:// deep link second (for installed apps).
  let shareMessage = $derived.by(() => {
    if (!shareUrl) return "";
    const repoPath = shareUrl.replace(/^https:\/\/github\.com\//, "").replace(/\/+$/, "");
    const meta = preview ? ` v${preview.packVersion}` : "";
    const lines = [
      `Minecraft pack${meta} — ${repoPath}`,
      shareUrl,
      `Install in TuffBox: tuffbox://install?repo=${encodeURIComponent(repoPath)}`,
    ];
    return lines.join("\n");
  });

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyShare() {
    if (!shareUrl) return;
    try {
      await copyText(shareMessage || shareUrl);
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => {
        copied = false;
      }, 1600);
    } catch {
      message = "Copy failed — select and copy the share URL manually.";
    }
  }

  function phaseLabel(value: string): string {
    if (value === "staging") return "Staging…";
    if (value === "commit") return "Publishing staged Git tree…";
    if (value === "assets") return "Uploading Release assets…";
    if (value === "done") return "Publish complete";
    return "Publishing…";
  }

  function phaseRank(value: PublishPhase): number {
    return ["", "staging", "commit", "assets", "done"].indexOf(value);
  }

  let publishSteps = $derived(
    twoPhase === false ? ["staging", "commit", "done"] : ["staging", "commit", "assets", "done"],
  );
</script>

<section class="pack-transport">
  <h3><Github size={16} /> GitHub Pack Transport</h3>
  <p class="hint">
    Publishes the canonical TuffBox manifest plus a packwiz tree as one Git commit.
    Friends install anonymously from the public repo URL — no Microsoft or Modrinth account.
  </p>
  <div class="auth-row">
    <span class:ok={authed}>{authed ? "Author signed in" : "Author login required"}</span>
    <button class="secondary mini" onclick={startLogin} disabled={busy || loginStarting || polling}>
      <KeyRound size={12} /> {loginStarting ? "Starting GitHub login…" : polling ? "Waiting for GitHub…" : authed ? "Re-login" : "GitHub device login"}
    </button>
  </div>
  {#if oauthClientMissing}
    <p class="config-error">
      Device OAuth is unavailable: this build has no GitHub OAuth client ID. Set
      <code>TUFFBOX_GITHUB_CLIENT_ID</code> when building, or add a GitHub PAT in Settings.
    </p>
  {/if}
  {#if userCode}
    <p class="code-line">
      Open <button class="linkish" onclick={() => open(verificationUri)}>{verificationUri}</button>
      and enter <code>{userCode}</code>
    </p>
  {/if}
  <label>
    Public repository
    <input bind:value={repository} placeholder="owner/repository" />
  </label>
  <div class="actions">
    <button class="secondary mini" onclick={runPreview} disabled={busy || !$projectPath}>Preview tree</button>
    <button class="mini" onclick={publishPack} disabled={busy || !authed || !repository.trim()}>
      <UploadCloud size={12} /> {busy ? phaseLabel(phase) : "Publish pack"}
    </button>
    {#if shareUrl}
      <button class="ghost mini" onclick={copyShare} title="Copy share message (link + one-click install)">
        {#if copied}
          <Check size={12} /> Copied!
        {:else}
          <Copy size={12} /> Copy share message
        {/if}
      </button>
      <button class="ghost mini" onclick={openShare}><Link2 size={12} /> Open share link</button>
    {/if}
  </div>
  {#if phase}
    <div class="publish-progress" aria-live="polite">
      <div class="phase-summary">
        <strong>{phaseLabel(phase)}</strong>
        {#if twoPhase === true}
          <span>Two-phase publish: Git commit first, then Release assets, then ready.</span>
        {:else if twoPhase === false}
          <span>Single-phase Git commit publish.</span>
        {/if}
      </div>
      <ol aria-label="Publish progress" class:two-phase={twoPhase === true} class:single-phase={twoPhase === false}>
        {#each publishSteps as step (step)}
          <li class:active={phase === step} class:complete={phaseRank(phase) > phaseRank(step as PublishPhase)}>
            {phaseLabel(step)}
          </li>
        {/each}
      </ol>
    </div>
  {/if}
  {#if preview}
    <small>{preview.fileCount} files · v{preview.packVersion}{preview.hasExternalAssets ? " · large jars via Release assets" : ""}</small>
  {/if}
  {#if message}<p class="ok-msg">{message}</p>{/if}
  {#if conflict}<p class="err-msg">Conflict: {conflict}</p>{/if}
  {#if error}<p class="err-msg">{error}</p>{/if}
</section>

<style>
  .pack-transport {
    display: grid;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
  }
  h3 { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 14px; color: var(--text-secondary); }
  .hint, .code-line { margin: 0; color: var(--text-muted); font-size: 12px; }
  .auth-row, .actions { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .auth-row span { font-size: 12px; color: var(--text-muted); }
  .auth-row span.ok { color: var(--accent-primary); }
  label { display: grid; gap: 6px; color: var(--text-secondary); font-weight: 700; font-size: 13px; }
  .ok-msg { margin: 0; color: var(--accent-primary); font-size: 12px; }
  .err-msg { margin: 0; color: #fecaca; font-size: 12px; }
  .config-error {
    margin: 0;
    padding: 8px;
    border: 1px solid color-mix(in srgb, #fecaca 35%, var(--border-color));
    border-radius: var(--border-radius-sm);
    color: #fecaca;
    font-size: 12px;
  }
  .publish-progress { display: grid; gap: 6px; }
  .phase-summary { display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; font-size: 12px; }
  .phase-summary span { color: var(--text-muted); }
  .publish-progress ol {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .publish-progress ol.single-phase {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
  .publish-progress li {
    padding: 4px 6px;
    border: 1px solid var(--border-color);
    border-radius: 4px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }
  .publish-progress li.active { border-color: var(--accent-primary); color: var(--text-primary); }
  .publish-progress li.complete { color: var(--accent-primary); }
  .linkish { background: none; border: 0; color: var(--accent-primary); cursor: pointer; padding: 0; }
</style>
