<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { Loader2, Copy, Check, LogIn, X, User, Monitor, Globe, Shield } from "lucide-svelte";
  import { api } from "../lib/api";
  import { authState, skinPath, loginTypeLabel, type SkinSource, type YggdrasilPreset } from "../lib/store";
  import { toasts } from "../lib/toast";

  const dispatch = createEventDispatcher();

  let mode: "select" | "microsoft-code" | "microsoft-polling" | "offline-form" | "yggdrasil-form" = "select";
  let deviceCode: { userCode: string; verificationUri: string } | null = null;
  let polling = false;
  let errorMsg = "";
  let copied = false;
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  // Offline form
  let offlineUsername = "";
  let skinSource: SkinSource = "mojang";
  let loggingIn = false;

  // Yggdrasil form
  let yggPresets: YggdrasilPreset[] = [];
  let yggPresetId = "elyby";
  let yggAuthority = "";
  let yggUsername = "";
  let yggPassword = "";

  $: existingAccounts = $authState.accounts ?? [];

  async function switchExisting(uuid: string) {
    loggingIn = true;
    errorMsg = "";
    try {
      const state = await api.mcAuth.switchAccount(uuid);
      authState.set(state);
      if (state.profile) {
        try {
          skinPath.set(await api.mcAuth.getSkinPath(state.profile.uuid));
        } catch {
          skinPath.set(null);
        }
      }
      toasts.success(`Switched to ${state.profile?.name ?? "account"}`);
      setTimeout(() => dispatch("close"), 400);
    } catch (e) {
      errorMsg = String(e);
    } finally {
      loggingIn = false;
    }
  }

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
          capeProvider: $authState.capeProvider ?? "mojang",
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
        capeProvider: $authState.capeProvider ?? "mojang",
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

  async function openYggdrasilForm() {
    errorMsg = "";
    try {
      yggPresets = await api.mcAuth.listYggdrasilPresets();
    } catch {
      yggPresets = [
        { id: "elyby", label: "Ely.by", authority: "https://authserver.ely.by/api/authlib-injector" },
        { id: "littleskin", label: "LittleSkin", authority: "https://littleskin.cn/api/yggdrasil" },
        { id: "custom", label: "Custom", authority: "" },
      ];
    }
    const preset = yggPresets.find((p) => p.id === yggPresetId) ?? yggPresets[0];
    yggPresetId = preset?.id ?? "elyby";
    yggAuthority = preset?.authority ?? "";
    mode = "yggdrasil-form";
  }

  function selectYggPreset(id: string) {
    yggPresetId = id;
    const preset = yggPresets.find((p) => p.id === id);
    if (preset && id !== "custom") {
      yggAuthority = preset.authority;
    }
  }

  async function handleYggdrasilLogin() {
    if (!yggUsername.trim() || !yggPassword) {
      errorMsg = "Enter username and password";
      return;
    }
    if (!yggAuthority.trim()) {
      errorMsg = "Enter authority URL";
      return;
    }
    loggingIn = true;
    errorMsg = "";
    try {
      const result = await api.mcAuth.yggdrasilLogin(
        yggUsername.trim(),
        yggPassword,
        yggAuthority.trim()
      );
      const state = await api.mcAuth.getAuthStatus();
      authState.set(state);
      if (result.profile.skinUrl) {
        try {
          skinPath.set(await api.mcAuth.getSkinPath(result.profile.uuid));
        } catch {}
      }
      toasts.success(`Logged in as ${result.profile.name}`);
      mode = "select";
      yggPassword = "";
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
        <h3>
          {#if mode === "offline-form"}Offline Login
          {:else if mode === "yggdrasil-form"}Yggdrasil Login
          {:else if mode === "microsoft-polling"}Microsoft Login
          {:else}Sign In{/if}
        </h3>
      </div>
      <button class="close-btn" on:click={close} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if mode === "select"}
        {#if existingAccounts.length > 0}
          <div class="existing-accounts">
            <div class="existing-label">Saved accounts</div>
            {#each existingAccounts as account (account.uuid)}
              <button
                class="account-row"
                class:active={account.uuid === $authState.activeAccountUuid}
                disabled={loggingIn}
                on:click={() => switchExisting(account.uuid)}
              >
                <div class="account-ico" class:ms={account.loginType === "microsoft"} class:ygg={account.loginType === "yggdrasil"}>
                  {#if account.loginType === "microsoft"}
                    <Globe size={16} />
                  {:else if account.loginType === "yggdrasil"}
                    <Shield size={16} />
                  {:else}
                    <User size={16} />
                  {/if}
                </div>
                <div class="account-text">
                  <span class="mc-nick">{account.name}</span>
                  <span
                    class="account-type"
                    class:mojang={account.loginType === "microsoft"}
                    class:offline={account.loginType === "offline"}
                    class:ygg={account.loginType === "yggdrasil"}
                  >
                    {loginTypeLabel(account.loginType, account.authority)}
                  </span>
                </div>
                {#if account.uuid === $authState.activeAccountUuid}
                  <Check size={14} class="check" />
                {/if}
              </button>
            {/each}
          </div>
          <div class="divider"><span>or add new</span></div>
        {/if}

        <div class="login-options">
          <button class="login-option" on:click={startMicrosoftLogin}>
            <div class="option-icon ms">
              <Globe size={20} />
            </div>
            <div class="option-info">
              <span class="option-title">Microsoft / Mojang</span>
              <span class="option-desc">Online play, skins, Realms, cape switch</span>
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

          <button class="login-option" on:click={openYggdrasilForm}>
            <div class="option-icon ygg">
              <Shield size={20} />
            </div>
            <div class="option-info">
              <span class="option-title">Ely.by / LittleSkin / Custom</span>
              <span class="option-desc">authlib-injector Yggdrasil providers</span>
            </div>
            <Check size={16} class="option-arrow" />
          </button>
        </div>

        <p class="hint">Offline mode fetches skins from Ely.by, TLauncher, or Mojang. Capes can be shown from OptiFine / TLauncher / Mojang.</p>

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

      {:else if mode === "yggdrasil-form"}
        <form class="offline-form" on:submit|preventDefault={handleYggdrasilLogin}>
          <div class="skin-source-grid ygg-presets">
            {#each yggPresets as preset}
              <button
                type="button"
                class="source-option"
                class:active={yggPresetId === preset.id}
                on:click={() => selectYggPreset(preset.id)}
                disabled={loggingIn}
              >
                {preset.label}
              </button>
            {/each}
          </div>

          <label class="field">
            <span>Authority URL</span>
            <input
              bind:value={yggAuthority}
              placeholder="https://…/api/yggdrasil"
              disabled={loggingIn || yggPresetId !== "custom"}
            />
          </label>

          <label class="field">
            <span>Email / Username</span>
            <input bind:value={yggUsername} placeholder="account@example.com" autofocus disabled={loggingIn} />
          </label>

          <label class="field">
            <span>Password</span>
            <input type="password" bind:value={yggPassword} placeholder="••••••••" disabled={loggingIn} />
          </label>

          {#if errorMsg}
            <div class="error-msg">{errorMsg}</div>
          {/if}

          <button class="primary-btn" type="submit" disabled={loggingIn || !yggUsername.trim() || !yggPassword}>
            {#if loggingIn}
              <Loader2 size={16} class="spin" /> Signing in...
            {:else}
              <LogIn size={16} /> Sign in
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
  .existing-accounts { display: flex; flex-direction: column; gap: 6px; margin-bottom: 4px; }
  .existing-label { font-size: 11px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .account-row {
    display: flex; align-items: center; gap: 12px; width: 100%;
    padding: 10px 12px; border-radius: 10px; text-align: left;
    background: var(--bg-primary); border: 1px solid var(--border-color);
    color: var(--text-primary); cursor: pointer;
  }
  .account-row:hover { border-color: var(--accent-primary); }
  .account-row.active { border-color: var(--accent-primary); background: rgba(27, 217, 106, 0.05); }
  .account-ico {
    width: 32px; height: 32px; border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
    background: var(--bg-elevated); color: var(--text-muted);
  }
  .account-ico.ms { background: linear-gradient(135deg, #0078d4, #00a4ef); color: #fff; }
  .account-ico.ygg { background: rgba(168, 85, 247, 0.18); color: #e9d5ff; }
  .account-text { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .mc-nick {
    font-family: var(--font-minecraft);
    font-size: 10px;
    letter-spacing: 0.4px;
    text-shadow: 1px 1px 0 #3f3f3f;
  }
  .account-type { font-size: 10px; font-weight: 800; text-transform: uppercase; }
  .account-type.mojang { color: #93c5fd; }
  .account-type.offline { color: #fde68a; }
  .account-type.ygg { color: #e9d5ff; }
  :global(.check) { color: var(--accent-primary); }
  .divider {
    display: flex; align-items: center; gap: 10px; margin: 14px 0;
    color: var(--text-muted); font-size: 11px;
  }
  .divider::before, .divider::after {
    content: ""; flex: 1; height: 1px; background: var(--border-color);
  }

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
  .option-icon.ygg { background: rgba(168, 85, 247, 0.2); color: #e9d5ff; }

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
  .ygg-presets { grid-template-columns: repeat(3, 1fr); }

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
