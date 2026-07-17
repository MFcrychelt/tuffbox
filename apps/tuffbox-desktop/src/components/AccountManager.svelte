<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    X,
    LogIn,
    LogOut,
    Plus,
    User,
    Globe,
    Monitor,
    Check,
    Trash2,
    ArrowLeftRight,
    Shield,
    Palette,
  } from "lucide-svelte";
  import { api } from "../lib/api";
  import {
    authState,
    skinPath,
    type AccountEntry,
    type SkinSource,
    type LoginType,
  } from "../lib/store";
  import { toasts } from "../lib/toast";

  const dispatch = createEventDispatcher();

  let mode: "list" | "add-select" | "add-offline" = "list";
  let accounts: AccountEntry[] = [];
  let activeUuid: string | null = null;
  let offlineUsername = "";
  let offlineSkinSource: SkinSource = "mojang";
  let busy = false;
  let errorMsg = "";

  $: accounts = $authState.accounts;
  $: activeUuid = $authState.activeAccountUuid;

  async function loadAccounts() {
    try {
      const state = await api.mcAuth.getAuthStatus();
      authState.set(state);
      accounts = state.accounts;
      activeUuid = state.activeAccountUuid;
    } catch {}
  }

  async function switchAccount(uuid: string) {
    if (uuid === activeUuid) return;
    busy = true;
    try {
      const state = await api.mcAuth.switchAccount(uuid);
      authState.set(state);
      if (state.profile) {
        try {
          const path = await api.mcAuth.getSkinPath(state.profile.uuid);
          skinPath.set(path);
        } catch {
          skinPath.set(null);
        }
      }
      toasts.success(`Switched to ${state.profile?.name ?? "account"}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function removeAccount(uuid: string) {
    busy = true;
    try {
      await api.mcAuth.removeAccount(uuid);
      await loadAccounts();
      toasts.info("Account removed");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function addOffline() {
    if (!offlineUsername.trim()) {
      errorMsg = "Enter a username";
      return;
    }
    busy = true;
    errorMsg = "";
    try {
      const result = await api.mcAuth.offlineLogin(
        offlineUsername.trim(),
        offlineSkinSource
      );
      authState.set({
        loggedIn: true,
        profile: result.profile,
        expiresAt: null,
        loginType: "offline",
        skinSource: offlineSkinSource,
        accounts: [...accounts, {
          uuid: result.profile.uuid,
          name: result.profile.name,
          loginType: "offline",
          skinSource: offlineSkinSource,
          addedAt: Date.now(),
        }],
        activeAccountUuid: result.profile.uuid,
      });
      if (result.profile.skinUrl) {
        try {
          const path = await api.mcAuth.getSkinPath(result.profile.uuid);
          skinPath.set(path);
        } catch {}
      }
      toasts.success(`Added ${result.profile.name}`);
      mode = "list";
      offlineUsername = "";
    } catch (e) {
      errorMsg = String(e);
    } finally {
      busy = false;
    }
  }

  async function startMicrosoftAdd() {
    busy = true;
    errorMsg = "";
    try {
      const info = await api.mcAuth.startDeviceCode();
      // Open browser
      try {
        const { open } = await import("@tauri-apps/plugin-shell");
        await open(info.verificationUri);
      } catch {}

      // Poll until success
      const result = await api.mcAuth.pollDeviceCode();
      await loadAccounts();
      toasts.success(`Added ${result.profile.name}`);
      mode = "list";
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("timed out") && !msg.includes("expired")) {
        errorMsg = msg;
      }
    } finally {
      busy = false;
    }
  }

  function close() {
    dispatch("close");
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div class="overlay" on:click|self={close}>
  <div class="modal">
    <div class="modal-header">
      {#if mode !== "list"}
        <button class="back-btn" on:click={() => (mode = "list")} aria-label="Back">
          <ArrowLeftRight size={16} />
        </button>
      {/if}
      <div class="modal-title">
        <User size={18} />
        <h3>
          {#if mode === "list"}Accounts{:else if mode === "add-select"}Add Account{:else}Add Offline Account{/if}
        </h3>
      </div>
      <button class="close-btn" on:click={close} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if mode === "list"}
        {#if accounts.length === 0}
          <div class="empty-accounts">
            <User size={32} />
            <p>No accounts added yet</p>
            <button class="accent-btn" on:click={() => (mode = "add-select")}>
              <Plus size={16} /> Add Account
            </button>
          </div>
        {:else}
          <div class="account-list">
            {#each accounts as account}
              <div
                class="account-item"
                class:active={account.uuid === activeUuid}
              >
                <div class="account-avatar">
                  {#if account.loginType === "microsoft"}
                    <Globe size={16} />
                  {:else}
                    <User size={16} />
                  {/if}
                </div>
                <div class="account-info">
                  <span class="account-name">{account.name}</span>
                  <span class="account-meta">
                    {account.loginType === "microsoft" ? "Microsoft" : "Offline"}
                    {#if account.uuid === activeUuid}
                      <span class="active-badge">Active</span>
                    {/if}
                  </span>
                </div>
                <div class="account-actions">
                  {#if account.uuid !== activeUuid}
                    <button
                      class="icon-btn small"
                      on:click={() => switchAccount(account.uuid)}
                      disabled={busy}
                      title="Switch to this account"
                    >
                      <ArrowLeftRight size={14} />
                    </button>
                  {/if}
                  <button
                    class="icon-btn small danger"
                    on:click={() => removeAccount(account.uuid)}
                    disabled={busy}
                    title="Remove account"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            {/each}
          </div>

          <button class="accent-btn full-width" on:click={() => (mode = "add-select")}>
            <Plus size={16} /> Add Account
          </button>
        {/if}

      {:else if mode === "add-select"}
        <div class="add-options">
          <button class="add-option" on:click={startMicrosoftAdd} disabled={busy}>
            <div class="option-icon ms"><Globe size={20} /></div>
            <div class="option-info">
              <span class="option-title">Microsoft Account</span>
              <span class="option-desc">Online play, skins, Realms</span>
            </div>
          </button>

          <button class="add-option" on:click={() => (mode = "add-offline")} disabled={busy}>
            <div class="option-icon offline"><User size={20} /></div>
            <div class="option-info">
              <span class="option-title">Offline Account</span>
              <span class="option-desc">Play with custom username</span>
            </div>
          </button>
        </div>

      {:else if mode === "add-offline"}
        <form class="offline-form" on:submit|preventDefault={addOffline}>
          <label class="field">
            <span>Username</span>
            <input
              bind:value={offlineUsername}
              placeholder="Enter username"
              maxlength={16}
              autofocus
              disabled={busy}
            />
          </label>

          <label class="field">
            <span>Skin Source</span>
            <div class="skin-source-grid">
              <button type="button" class="source-option" class:active={offlineSkinSource === "mojang"} on:click={() => (offlineSkinSource = "mojang")}>
                <Monitor size={14} /> Mojang
              </button>
              <button type="button" class="source-option" class:active={offlineSkinSource === "elyby"} on:click={() => (offlineSkinSource = "elyby")}>
                <Globe size={14} /> Ely.by
              </button>
              <button type="button" class="source-option" class:active={offlineSkinSource === "tlauncher"} on:click={() => (offlineSkinSource = "tlauncher")}>
                <Globe size={14} /> TLauncher
              </button>
              <button type="button" class="source-option" class:active={offlineSkinSource === "offline"} on:click={() => (offlineSkinSource = "offline")}>
                <User size={14} /> None
              </button>
            </div>
          </label>

          {#if errorMsg}
            <div class="error-msg">{errorMsg}</div>
          {/if}

          <button class="accent-btn full-width" type="submit" disabled={busy || !offlineUsername.trim()}>
            {busy ? "Adding..." : "Add Account"}
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
    border-radius: var(--border-radius-xl); width: 440px; max-width: 90vw;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5); overflow: hidden;
    max-height: 80vh; display: flex; flex-direction: column;
  }

  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 18px 22px; border-bottom: 1px solid var(--border-color);
  }

  .modal-title { display: flex; align-items: center; gap: 10px; color: var(--text-primary); flex: 1; }
  .modal-title h3 { font-size: 16px; font-weight: 700; }

  .back-btn, .close-btn {
    width: 32px; height: 32px; padding: 0; display: flex; align-items: center; justify-content: center;
    border-radius: 8px; background: transparent; color: var(--text-muted); border: none;
  }
  .back-btn:hover, .close-btn:hover { background: var(--bg-hover); color: var(--text-primary); }

  .modal-body { padding: 22px; overflow-y: auto; }

  /* ─── Account List ──────────────────────── */
  .empty-accounts {
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    padding: 32px; text-align: center; color: var(--text-muted);
  }
  .empty-accounts p { font-size: 13px; }

  .account-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 16px; }

  .account-item {
    display: flex; align-items: center; gap: 12px; padding: 10px 12px;
    background: var(--bg-primary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md); transition: all 0.15s;
  }
  .account-item.active { border-color: var(--accent-primary); background: rgba(27, 217, 106, 0.04); }

  .account-avatar {
    width: 36px; height: 36px; border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
    background: var(--bg-elevated); color: var(--text-muted);
    flex-shrink: 0;
  }

  .account-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .account-name { font-weight: 700; font-size: 13px; color: var(--text-primary); }
  .account-meta { font-size: 11px; color: var(--text-muted); display: flex; align-items: center; gap: 6px; }

  .active-badge {
    font-size: 10px; font-weight: 700; color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.12); padding: 1px 5px; border-radius: 4px;
  }

  .account-actions { display: flex; gap: 4px; }

  .icon-btn.small {
    width: 28px; height: 28px; padding: 0;
    display: flex; align-items: center; justify-content: center;
    border-radius: 6px; background: transparent; border: none;
    color: var(--text-muted); cursor: pointer;
  }
  .icon-btn.small:hover { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn.small.danger:hover { background: rgba(239, 68, 68, 0.12); color: #ef4444; }

  /* ─── Add Options ───────────────────────── */
  .add-options { display: flex; flex-direction: column; gap: 10px; }

  .add-option {
    display: flex; align-items: center; gap: 14px; padding: 14px 16px;
    background: var(--bg-primary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); cursor: pointer; text-align: left;
    transition: all 0.15s ease; width: 100%; color: var(--text-primary);
  }
  .add-option:hover { border-color: var(--accent-primary); background: rgba(27, 217, 106, 0.04); }
  .add-option:disabled { opacity: 0.5; cursor: not-allowed; }

  .option-icon {
    width: 40px; height: 40px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  }
  .option-icon.ms { background: linear-gradient(135deg, #0078d4, #00a4ef); color: #fff; }
  .option-icon.offline { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border-color); }

  .option-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .option-title { font-weight: 700; font-size: 14px; }
  .option-desc { font-size: 12px; color: var(--text-muted); }

  /* ─── Offline Form ──────────────────────── */
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

  /* ─── Buttons ───────────────────────────── */
  .accent-btn {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    padding: 10px 16px; border-radius: var(--border-radius-md);
    background: var(--accent-primary); color: #000; border: none;
    font-size: 13px; font-weight: 700; cursor: pointer;
    transition: all 0.15s;
  }
  .accent-btn:hover { background: var(--accent-hover); }
  .accent-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .accent-btn.full-width { width: 100%; }
</style>
