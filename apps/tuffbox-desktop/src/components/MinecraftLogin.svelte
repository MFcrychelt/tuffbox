<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { Loader2, Copy, Check, LogIn, X, User, Monitor, Globe } from "lucide-svelte";
  import { api } from "../lib/api";
  import { authState, skinPath, type SkinSource } from "../lib/store";
  import { toasts } from "../lib/toast";

  const dispatch = createEventDispatcher();

  let mode: "select" | "microsoft-code" | "microsoft-polling" | "offline-form" = "select";
  let deviceCode: { userCode: string; verificationUri: string } | null = null;
  let polling = false;
  let errorMsg = "";
  let copied = false;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  // Offline form
  let offlineUsername = "";
  let skinSource: SkinSource = "mojang";
  let loggingIn = false;

  async function startMicrosoftLogin() {
    mode = "microsoft-code";
    errorMsg = "";
    try {
      const info = await api.mcAuth.startDeviceCode();
      deviceCode = info;
      mode = "microsoft-polling";
      startPolling();
      try { await open(info.verificationUri); } catch {}
    } catch (e) {
      errorMsg = String(e);
      mode = "select";
    }
  }

  function startPolling() {
    if (pollTimer) clearInterval(pollTimer);
    polling = true;
    pollTimer = setInterval(async () => {
      try {
        const result = await api.mcAuth.pollDeviceCode();
        polling = false;
        if (pollTimer) clearInterval(pollTimer);
        authState.set({
          loggedIn: true,
          profile: result.profile,
          expiresAt: Date.now() + 86400000,
          loginType: "microsoft",
          skinSource: "mojang",
          accounts: $authState.accounts,
          activeAccountUuid: result.profile.uuid,
        });
        if (result.profile.skinUrl) {
          try {
            const path = await api.mcAuth.getSkinPath(result.profile.uuid);
            skinPath.set(path);
          } catch {}
        }
        toasts.success(`Logged in as ${result.profile.name}`);
        mode = "select";
        setTimeout(() => dispatch("close"), 800);
      } catch (e) {
        const msg = String(e);
        if (msg.includes("timed out") || msg.includes("expired")) {
          polling = false;
          if (pollTimer) clearInterval(pollTimer);
          errorMsg = msg;
          mode = "select";
        }
      }
    }, 5000);
  }

  async function handleOfflineLogin() {
    if (!offlineUsername.trim()) {
      errorMsg = "Enter a username";
      return;
    }
    loggingIn = true;
    errorMsg = "";
    try {
      const result = await api.mcAuth.offlineLogin(offlineUsername.trim(), skinSource);
      authState.set({
        loggedIn: true,
        profile: result.profile,
        expiresAt: null,
        loginType: "offline",
        skinSource,
        accounts: $authState.accounts,
        activeAccountUuid: result.profile.uuid,
      });
      if (result.profile.skinUrl) {
        try {
          const path = await api.mcAuth.getSkinPath(result.profile.uuid);
          skinPath.set(path);
        } catch {}
      }
      toasts.success(`Playing as ${result.profile.name}`);
      mode = "select";
      setTimeout(() => dispatch("close"), 600);
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loggingIn = false;
    }
  }

  async function copyCode() {
    if (!deviceCode) return;
    await navigator.clipboard.writeText(deviceCode.userCode);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  function close() {
    if (pollTimer) clearInterval(pollTimer);
    polling = false;
    dispatch("close");
  }

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div class="overlay" on:click|self={close}>
  <div class="modal">
    <div class="modal-header">
      <div class="modal-title">
        <LogIn size={18} />
        <h3>{mode === "offline-form" ? "Offline Login" : mode === "microsoft-polling" ? "Microsoft Login" : "Sign In"}</h3>
      </div>
      <button class="close-btn" on:click={close} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if mode === "select"}
        <div class="login-options">
          <button class="login-option" on:click={startMicrosoftLogin}>
            <div class="option-icon ms">
              <Globe size={20} />
            </div>
            <div class="option-info">
              <span class="option-title">Microsoft Account</span>
              <span class="option-desc">Online play, skins, Realms</span>
            </div>
            <Check size={16} class="option-arrow" />
          </button>

          <button class="login-option" on:click={() => (mode = "offline-form")}>
            <div class="option-icon offline">
              <User size={20} />
            </div>
            <div class="option-info">
              <span class="option-title">Offline Mode</span>
              <span class="option-desc">Play with custom username</span>
            </div>
            <Check size={16} class="option-arrow" />
          </button>
        </div>

        <p class="hint">Offline mode fetches skins from Ely.by, TLauncher, or Mojang.</p>

      {:else if mode === "microsoft-polling" && deviceCode}
        <div class="code-content">
          <div class="code-display">
            <span class="code">{deviceCode.userCode}</span>
            <button class="copy-btn" on:click={copyCode} title="Copy code">
              {#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}
            </button>
          </div>
          <p class="instruction">
            Go to <a href={deviceCode?.verificationUri ?? "#"} on:click|preventDefault={() => deviceCode && open(deviceCode.verificationUri)}>{deviceCode?.verificationUri}</a>
            <br />and enter the code above.
          </p>
          <div class="polling-indicator">
            <Loader2 size={16} class="spin" />
            <span>Waiting for authentication...</span>
          </div>
        </div>

      {:else if mode === "offline-form"}
        <form class="offline-form" on:submit|preventDefault={handleOfflineLogin}>
          <label class="field">
            <span>Username</span>
            <input
              bind:value={offlineUsername}
              placeholder="Enter username"
              maxlength={16}
              autofocus
              disabled={loggingIn}
            />
          </label>

          <label class="field">
            <span>Skin Source</span>
            <div class="skin-source-grid">
              <button
                type="button"
                class="source-option"
                class:active={skinSource === "mojang"}
                on:click={() => (skinSource = "mojang")}
              >
                <Monitor size={14} />
                Mojang
              </button>
              <button
                type="button"
                class="source-option"
                class:active={skinSource === "elyby"}
                on:click={() => (skinSource = "elyby")}
              >
                <Globe size={14} />
                Ely.by
              </button>
              <button
                type="button"
                class="source-option"
                class:active={skinSource === "tlauncher"}
                on:click={() => (skinSource = "tlauncher")}
              >
                <Globe size={14} />
                TLauncher
              </button>
              <button
                type="button"
                class="source-option"
                class:active={skinSource === "offline"}
                on:click={() => (skinSource = "offline")}
              >
                <User size={14} />
                None
              </button>
            </div>
          </label>

          {#if errorMsg}
            <div class="error-msg">{errorMsg}</div>
          {/if}

          <button class="primary-btn" type="submit" disabled={loggingIn || !offlineUsername.trim()}>
            {#if loggingIn}
              <Loader2 size={16} class="spin" /> Loading...
            {:else}
              <LogIn size={16} /> Play
            {/if}
          </button>
        </form>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(4px);
    display: flex; align-items: center; justify-content: center;
    z-index: 200;
  }

  .modal {
    background: var(--bg-elevated); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl); width: 420px; max-width: 90vw;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5); overflow: hidden;
  }

  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 18px 22px; border-bottom: 1px solid var(--border-color);
  }

  .modal-title { display: flex; align-items: center; gap: 10px; color: var(--text-primary); }
  .modal-title h3 { font-size: 16px; font-weight: 700; }

  .close-btn {
    width: 32px; height: 32px; padding: 0; display: flex; align-items: center; justify-content: center;
    border-radius: 8px; background: transparent; color: var(--text-muted); border: none;
  }
  .close-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .modal-body { padding: 22px; }

  /* ─── Login options ──────────────────────── */
  .login-options { display: flex; flex-direction: column; gap: 10px; }

  .login-option {
    display: flex; align-items: center; gap: 14px; padding: 14px 16px;
    background: var(--bg-primary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); cursor: pointer; text-align: left;
    transition: all 0.15s ease; width: 100%;
  }
  .login-option:hover { border-color: var(--accent-primary); background: rgba(27, 217, 106, 0.04); }

  .option-icon {
    width: 40px; height: 40px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  }
  .option-icon.ms { background: linear-gradient(135deg, #0078d4, #00a4ef); color: #fff; }
  .option-icon.offline { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border-color); }

  .option-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .option-title { font-weight: 700; font-size: 14px; color: var(--text-primary); }
  .option-desc { font-size: 12px; color: var(--text-muted); }

  :global(.option-arrow) { color: var(--text-muted); }

  .hint { font-size: 11px; color: var(--text-muted); text-align: center; margin-top: 12px; }

  /* ─── Code display ───────────────────────── */
  .code-content { display: flex; flex-direction: column; align-items: center; gap: 16px; }

  .code-display {
    display: flex; align-items: center; gap: 12px;
    background: var(--bg-primary); border: 2px solid var(--accent-primary);
    border-radius: var(--border-radius-md); padding: 14px 20px;
  }
  .code {
    font-family: ui-monospace, monospace; font-size: 28px; font-weight: 900;
    letter-spacing: 4px; color: var(--accent-primary);
  }
  .copy-btn {
    width: 36px; height: 36px; padding: 0; display: flex; align-items: center; justify-content: center;
    border-radius: 8px; background: var(--bg-hover); color: var(--text-secondary); border: none;
  }
  .copy-btn:hover { background: var(--accent-primary); color: #000; }

  .instruction { color: var(--text-secondary); font-size: 13px; text-align: center; line-height: 1.6; }
  .instruction a { color: var(--accent-primary); text-decoration: none; font-weight: 600; }
  .instruction a:hover { text-decoration: underline; }

  .polling-indicator { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; }

  /* ─── Offline form ───────────────────────── */
  .offline-form { display: flex; flex-direction: column; gap: 16px; }

  .field { display: flex; flex-direction: column; gap: 6px; }
  .field span { font-size: 12px; font-weight: 600; color: var(--text-secondary); }

  .field input {
    width: 100%; padding: 10px 14px; background: var(--bg-primary);
    border: 1px solid var(--border-color); border-radius: var(--border-radius-md);
    color: var(--text-primary); font-size: 14px; outline: none;
  }
  .field input:focus { border-color: var(--accent-primary); }

  .skin-source-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; }

  .source-option {
    display: flex; align-items: center; justify-content: center; gap: 5px;
    padding: 8px 6px; border-radius: 8px; background: var(--bg-primary);
    border: 1px solid var(--border-color); color: var(--text-secondary);
    font-size: 11px; font-weight: 600; cursor: pointer; transition: all 0.15s;
  }
  .source-option:hover { border-color: var(--text-muted); color: var(--text-primary); }
  .source-option.active {
    border-color: var(--accent-primary); color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.06);
  }

  .error-msg {
    color: #f87171; font-size: 12px; background: rgba(248, 113, 113, 0.1);
    border: 1px solid rgba(248, 113, 113, 0.2); border-radius: 8px;
    padding: 8px 12px; text-align: center;
  }

  .primary-btn { width: 100%; padding: 12px 24px; font-size: 15px; }
</style>
