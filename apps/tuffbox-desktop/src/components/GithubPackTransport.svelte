<script lang="ts">
  import { Github, KeyRound, UploadCloud, Link2 } from "@lucide/svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api } from "../lib/api";
  import { projectPath } from "../lib/store";
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
  let phase = $state("");
  let conflict = $state("");
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let stopProgress: (() => void) | null = null;

  onMount(() => {
    void listen<{ phase?: string }>("github-pack-progress", (event) => {
      phase = event.payload?.phase ?? "";
    }).then((unlisten) => {
      stopProgress = unlisten;
    });
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    stopProgress?.();
  });

  async function refreshAuth() {
    try {
      authed = await api.transport.github.authStatus();
    } catch {
      authed = false;
    }
  }

  $effect(() => {
    void $projectPath;
    untrack(() => {
      void refreshAuth();
    });
  });

  async function startLogin() {
    error = "";
    message = "";
    try {
      const info = await api.transport.github.startDeviceCode();
      userCode = info.userCode;
      verificationUri = info.verificationUri;
      message = info.message;
      if (pollTimer) clearInterval(pollTimer);
      pollTimer = setInterval(async () => {
        try {
          await api.transport.github.pollDeviceCode();
          if (pollTimer) clearInterval(pollTimer);
          pollTimer = null;
          authed = true;
          userCode = "";
          message = "GitHub login saved on this device.";
        } catch (e) {
          const text = String(e);
          if (!text.includes("authorization_pending")) {
            if (pollTimer) clearInterval(pollTimer);
            pollTimer = null;
            error = text;
          }
        }
      }, Math.max(5, info.interval) * 1000);
    } catch (e) {
      error = String(e);
    }
  }

  async function runPreview() {
    if (!$projectPath) return;
    busy = true;
    error = "";
    try {
      const result = await api.transport.github.stagePreview($projectPath);
      preview = {
        packVersion: result.packVersion,
        fileCount: result.fileCount,
        hasExternalAssets: result.hasExternalAssets,
      };
      message = `Staged ${result.fileCount} files (v${result.packVersion}).`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function publishPack() {
    if (!$projectPath || !repository.trim()) return;
    busy = true;
    error = "";
    message = "";
    conflict = "";
    phase = "staging";
    try {
      const result = await api.transport.github.publish(repository.trim(), $projectPath);
      if (result.conflict) {
        conflict = String(result.message ?? "Branch moved. Pull or resolve the GitHub repo, then publish again. TuffBox never force-pushes.");
        return;
      }
      shareUrl = String(result.shareUrl ?? "");
      phase = "done";
      message = result.noop
        ? String(result.message ?? "Already up to date.")
        : `Published ${shareUrl || "to GitHub"}.`;
    } catch (e) {
      error = String(e);
      phase = "";
    } finally {
      busy = false;
    }
  }

  async function openShare() {
    if (!shareUrl) return;
    await open(shareUrl);
  }

  function phaseLabel(value: string): string {
    if (value === "staging") return "Staging…";
    if (value === "commit") return "Committing…";
    if (value === "assets") return "Uploading assets…";
    if (value === "done") return "Done";
    return "Publishing…";
  }
</script>

<section class="pack-transport">
  <h3><Github size={16} /> GitHub Pack Transport</h3>
  <p class="hint">
    Publishes the canonical TuffBox manifest plus a packwiz tree as one Git commit.
    Friends install anonymously from the public repo URL — no Microsoft or Modrinth account.
  </p>
  <div class="auth-row">
    <span class:ok={authed}>{authed ? "Author signed in" : "Author login required"}</span>
    <button class="secondary mini" onclick={startLogin} disabled={busy}>
      <KeyRound size={12} /> {authed ? "Re-login" : "GitHub device login"}
    </button>
  </div>
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
      <button class="ghost mini" onclick={openShare}><Link2 size={12} /> Open share link</button>
    {/if}
  </div>
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
  .linkish { background: none; border: 0; color: var(--accent-primary); cursor: pointer; padding: 0; }
</style>
