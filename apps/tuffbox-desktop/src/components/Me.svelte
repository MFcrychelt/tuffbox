<script lang="ts">
  import { onMount } from "svelte";
  import {
    User,
    LogIn,
    LogOut,
    Clock,
    Shield,
    Plus,
    ArrowLeftRight,
    Trash2,
    Globe,
    Monitor,
    ArrowLeft,
    Upload,
    Link2,
  } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import {
    authState,
    skinPath,
    projectPath,
    loginTypeLabel,
    formatPlaytime,
    type CapeProvider,
    type CapeCatalog,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import SkinPreview3D from "./SkinPreview3D.svelte";
  import AccountManager from "./AccountManager.svelte";
  import MinecraftLogin from "./MinecraftLogin.svelte";

  export let onBack: () => void = () => {};

  let showAccountManager = false;
  let showLogin = false;
  let playtimeSeconds = 0;
  let busy = false;
  let capeCatalog: CapeCatalog | null = null;
  let mojangCapeMenuOpen = false;

  let skinUrlInput = "";
  let skinVariant: "classic" | "slim" = "classic";
  let skinBusy = false;

  $: skinUrl = $authState.profile?.skinUrl ?? null;
  $: capeUrl = $authState.profile?.capeUrl ?? null;
  $: accountKey = $authState.activeAccountUuid ?? $authState.profile?.uuid ?? "";
  $: activeAuthority =
    $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority ?? null;
  $: mojangCapeOffers = (capeCatalog?.offers ?? []).filter((o) => o.provider === "mojang");
  $: otherCapeOffers = (capeCatalog?.offers ?? []).filter((o) => o.provider !== "mojang");
  $: canChangeMojangCape =
    $authState.loginType === "microsoft" && mojangCapeOffers.some((o) => o.canActivate);
  $: canChangeMojangSkin = $authState.loginType === "microsoft" && $authState.loggedIn;

  const capeProviders: { id: CapeProvider; label: string }[] = [
    { id: "mojang", label: "Mojang" },
    { id: "optifine", label: "OptiFine" },
    { id: "tlauncher", label: "TLauncher" },
    { id: "none", label: "None" },
  ];

  async function applyAuthState(state: Awaited<ReturnType<typeof api.mcAuth.getAuthStatus>>) {
    authState.set(state);
    if (state.profile?.uuid) {
      try {
        skinPath.set(await api.mcAuth.getSkinPath(state.profile.uuid));
      } catch {
        skinPath.set(null);
      }
    } else {
      skinPath.set(null);
    }
  }

  async function refreshAuth() {
    try {
      await applyAuthState(await api.mcAuth.getAuthStatus());
    } catch {}
  }

  async function refreshPlaytime() {
    const path = $projectPath;
    if (!path) {
      playtimeSeconds = 0;
      return;
    }
    try {
      const stats = await api.stats.get(path);
      playtimeSeconds = stats.totalPlaytimeSeconds ?? 0;
    } catch {
      playtimeSeconds = 0;
    }
  }

  async function refreshCapes() {
    if (!$authState.loggedIn) {
      capeCatalog = null;
      return;
    }
    try {
      capeCatalog = await api.mcAuth.listCapes();
    } catch {
      capeCatalog = null;
    }
  }

  async function switchAccount(uuid: string) {
    if (uuid === $authState.activeAccountUuid) return;
    busy = true;
    try {
      await applyAuthState(await api.mcAuth.switchAccount(uuid));
      await refreshCapes();
      toasts.success(`Switched to ${$authState.profile?.name ?? "account"}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function removeAccount(uuid: string) {
    busy = true;
    try {
      await applyAuthState(await api.mcAuth.removeAccount(uuid));
      await refreshCapes();
      toasts.info("Account removed");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function logout() {
    busy = true;
    try {
      await applyAuthState(await api.mcAuth.logout());
      await refreshCapes();
      toasts.info("Signed out");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
    }
  }

  async function setCapeProvider(provider: CapeProvider) {
    try {
      await applyAuthState(await api.mcAuth.setCapeProvider(provider));
      await refreshCapes();
      mojangCapeMenuOpen =
        provider === "mojang" &&
        $authState.loginType === "microsoft" &&
        (capeCatalog?.offers ?? []).some((o) => o.provider === "mojang" && o.canActivate);
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function applyCape(capeId: string) {
    try {
      await applyAuthState(await api.mcAuth.applyCape(capeId));
      mojangCapeMenuOpen = true;
      await refreshCapes();
      toasts.success("Cape equipped");
    } catch (e) {
      toasts.error(String(e));
    }
  }

  function openMojangCapeMenu() {
    mojangCapeMenuOpen = true;
    if (($authState.capeProvider ?? "mojang") !== "mojang") {
      void setCapeProvider("mojang");
    }
  }

  async function applySkinFromUrl() {
    const url = skinUrlInput.trim();
    if (!url) {
      toasts.error("Enter a skin PNG URL");
      return;
    }
    skinBusy = true;
    try {
      await applyAuthState(await api.mcAuth.applySkin(url, skinVariant));
      toasts.success("Skin updated");
      skinUrlInput = "";
    } catch (e) {
      toasts.error(String(e));
    } finally {
      skinBusy = false;
    }
  }

  async function uploadSkinFile() {
    skinBusy = true;
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Skin PNG", extensions: ["png"] }],
      });
      if (!selected || Array.isArray(selected)) {
        skinBusy = false;
        return;
      }
      await applyAuthState(await api.mcAuth.uploadSkinFile(selected, skinVariant));
      toasts.success("Skin uploaded");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      skinBusy = false;
    }
  }

  onMount(() => {
    void refreshAuth();
    void refreshPlaytime();
    void refreshCapes();
  });
</script>

<div class="me-page">
  <div class="me-top">
    <button class="back-btn" on:click={onBack} title="Back">
      <ArrowLeft size={18} />
      <span>Back</span>
    </button>
    <h1 class="me-title">Me</h1>
    <div class="me-top-actions">
      {#if $authState.loggedIn}
        <button class="ghost-btn danger" disabled={busy} on:click={logout} title="Sign out">
          <LogOut size={16} />
          Sign out
        </button>
      {/if}
      <button class="ghost-btn" on:click={() => (showLogin = true)}>
        <Plus size={16} />
        Add account
      </button>
    </div>
  </div>

  <div class="me-hero">
    <div class="skin-col">
      {#if $authState.loggedIn && $authState.profile}
        <SkinPreview3D
          {skinUrl}
          {capeUrl}
          {accountKey}
          playerName={$authState.profile.name}
          showName={false}
          width={280}
          height={380}
        />
        <div class="player-name mc-font">{$authState.profile.name}</div>
        <span
          class="type-badge"
          class:microsoft={$authState.loginType === "microsoft"}
          class:offline={$authState.loginType === "offline"}
          class:ygg={$authState.loginType === "yggdrasil"}
        >
          {loginTypeLabel($authState.loginType, activeAuthority)}
        </span>
      {:else}
        <div class="skin-empty">
          <User size={48} />
          <p>Not signed in</p>
          <button class="accent-btn" on:click={() => (showLogin = true)}>
            <LogIn size={16} /> Sign in
          </button>
        </div>
      {/if}
    </div>

    <div class="info-col">
      <section class="card">
        <div class="card-head">
          <Clock size={16} />
          <h3>Playtime</h3>
        </div>
        {#if $projectPath}
          <div class="playtime-value">{formatPlaytime(playtimeSeconds)}</div>
          <p class="hint">Total time in this instance (all sessions).</p>
        {:else}
          <p class="hint">Open an instance to track playtime.</p>
        {/if}
      </section>

      {#if canChangeMojangSkin}
        <section class="card">
          <div class="card-head">
            <Upload size={16} />
            <h3>Change skin</h3>
          </div>
          <div class="skin-form">
            <div class="variant-row">
              <button
                class="chip"
                class:active={skinVariant === "classic"}
                on:click={() => (skinVariant = "classic")}
              >Classic</button>
              <button
                class="chip"
                class:active={skinVariant === "slim"}
                on:click={() => (skinVariant = "slim")}
              >Slim</button>
            </div>
            <div class="url-row">
              <input
                class="skin-input"
                type="url"
                placeholder="https://…/skin.png"
                bind:value={skinUrlInput}
                disabled={skinBusy}
              />
              <button class="mini" disabled={skinBusy} on:click={applySkinFromUrl} title="Apply URL">
                <Link2 size={14} />
              </button>
            </div>
            <button class="accent-btn wide" disabled={skinBusy} on:click={uploadSkinFile}>
              <Upload size={16} />
              Upload PNG
            </button>
            <p class="hint">Microsoft accounts only. PNG must be a valid Minecraft skin.</p>
          </div>
        </section>
      {/if}

      <section class="card">
        <div class="card-head">
          <Shield size={16} />
          <h3>Cape source</h3>
        </div>
        <div class="provider-row">
          {#each capeProviders as opt (opt.id)}
            <button
              class="chip"
              class:active={($authState.capeProvider ?? "mojang") === opt.id}
              disabled={!$authState.loggedIn}
              on:click={() => setCapeProvider(opt.id)}
            >
              {opt.label}
            </button>
          {/each}
        </div>

        {#if canChangeMojangCape}
          <button
            class="mini"
            disabled={!$authState.loggedIn}
            on:click={() => (mojangCapeMenuOpen ? (mojangCapeMenuOpen = false) : openMojangCapeMenu())}
          >
            {mojangCapeMenuOpen ? "Hide cape menu" : "Show cape"}
          </button>
        {/if}

        {#if mojangCapeMenuOpen && canChangeMojangCape}
          <div class="cape-list">
            {#each mojangCapeOffers as offer (offer.id)}
              <div class="cape-row" class:active={offer.active}>
                <span>{offer.label}</span>
                <button class="mini" on:click={() => applyCape(offer.id)} disabled={offer.active}>
                  {offer.active ? "Active" : "Equip"}
                </button>
              </div>
            {/each}
          </div>
        {/if}

        {#if otherCapeOffers.length}
          <div class="cape-list">
            {#each otherCapeOffers as offer (offer.provider + offer.id)}
              <div class="cape-row" class:active={($authState.capeProvider ?? "mojang") === offer.provider}>
                <span>{offer.label} ({offer.provider})</span>
                {#if ($authState.capeProvider ?? "mojang") !== offer.provider}
                  <button class="mini" on:click={() => setCapeProvider(offer.provider)}>Show</button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>

      <section class="card">
        <div class="card-head">
          <User size={16} />
          <h3>Accounts</h3>
          <button class="ghost-icon" title="Add account" on:click={() => (showLogin = true)}>
            <Plus size={16} />
          </button>
          <button class="ghost-icon" title="Manage accounts" on:click={() => (showAccountManager = true)}>
            <ArrowLeftRight size={16} />
          </button>
        </div>

        {#if $authState.accounts.length === 0}
          <p class="hint">No saved accounts. Sign in with Microsoft, Offline, Ely.by, LittleSkin, or custom Yggdrasil.</p>
          <button class="accent-btn" on:click={() => (showLogin = true)}>
            <LogIn size={16} /> Add account
          </button>
        {:else}
          <div class="account-list">
            {#each $authState.accounts as account (account.uuid)}
              <div class="account-item" class:active={account.uuid === $authState.activeAccountUuid}>
                <button
                  class="account-main"
                  disabled={busy || account.uuid === $authState.activeAccountUuid}
                  on:click={() => switchAccount(account.uuid)}
                  title={account.uuid === $authState.activeAccountUuid ? "Active" : "Switch"}
                >
                  <div
                    class="account-ico"
                    class:ms={account.loginType === "microsoft"}
                    class:off={account.loginType === "offline"}
                    class:ygg={account.loginType === "yggdrasil"}
                  >
                    {#if account.loginType === "microsoft"}
                      <Globe size={14} />
                    {:else if account.loginType === "yggdrasil"}
                      <Monitor size={14} />
                    {:else}
                      <User size={14} />
                    {/if}
                  </div>
                  <div class="account-text">
                    <span class="mc-font name">{account.name}</span>
                    <span class="meta">{loginTypeLabel(account.loginType, account.authority)}</span>
                  </div>
                </button>
                <div class="actions">
                  {#if account.uuid !== $authState.activeAccountUuid}
                    <button
                      class="ghost-icon"
                      title="Switch"
                      disabled={busy}
                      on:click={() => switchAccount(account.uuid)}
                    >
                      <ArrowLeftRight size={14} />
                    </button>
                  {/if}
                  <button
                    class="ghost-icon danger"
                    title="Remove"
                    disabled={busy}
                    on:click={() => removeAccount(account.uuid)}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    </div>
  </div>
</div>

{#if showLogin}
  <MinecraftLogin
    on:close={() => {
      showLogin = false;
      void refreshAuth();
      void refreshCapes();
    }}
  />
{/if}
{#if showAccountManager}
  <AccountManager
    on:close={() => {
      showAccountManager = false;
      void refreshAuth();
      void refreshCapes();
    }}
  />
{/if}

<style>
  .me-page {
    max-width: 980px;
    margin: 0 auto;
    padding: 8px 4px 32px;
  }

  .me-top {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 18px;
  }

  .back-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .back-btn:hover {
    color: var(--text-primary);
    border-color: var(--accent-primary);
  }

  .me-title {
    flex: 1;
    margin: 0;
    font-size: 20px;
    font-weight: 800;
    color: var(--text-primary);
  }

  .me-top-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .ghost-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .ghost-btn:hover {
    color: var(--text-primary);
    background: var(--bg-hover);
  }

  .ghost-btn.danger:hover {
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.35);
    background: rgba(239, 68, 68, 0.08);
  }

  .me-hero {
    display: grid;
    grid-template-columns: 300px 1fr;
    gap: 28px;
    align-items: start;
  }

  @media (max-width: 820px) {
    .me-hero {
      grid-template-columns: 1fr;
    }
  }

  .skin-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .player-name {
    font-family: var(--font-minecraft);
    font-size: 12px;
    letter-spacing: 0.5px;
    color: var(--text-primary);
    text-shadow: 1px 1px 0 #3f3f3f;
  }

  .skin-empty {
    width: 280px;
    height: 380px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--bg-secondary);
    border: 1px dashed var(--border-color);
    border-radius: var(--border-radius-lg);
    color: var(--text-muted);
  }

  .info-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 16px 18px;
  }

  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    color: var(--text-secondary);
  }

  .card-head h3 {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
  }

  .playtime-value {
    font-size: 28px;
    font-weight: 800;
    color: var(--accent-primary);
    letter-spacing: -0.5px;
  }

  .hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 6px 0 0;
  }

  .skin-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .variant-row {
    display: flex;
    gap: 6px;
  }

  .url-row {
    display: flex;
    gap: 6px;
  }

  .skin-input {
    flex: 1;
    min-width: 0;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 13px;
  }

  .accent-btn.wide {
    justify-content: center;
    width: 100%;
  }

  .provider-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    padding: 6px 10px;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-primary);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .chip.active {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
  }

  .chip:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .cape-list {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cape-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
    font-size: 12px;
  }

  .cape-row.active {
    border-color: rgba(27, 217, 106, 0.4);
  }

  .mini {
    padding: 4px 8px;
    font-size: 11px;
    font-weight: 700;
    border-radius: 6px;
    border: none;
    background: var(--accent-primary);
    color: #000;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .mini:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .account-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .account-item {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px 4px 4px;
    border-radius: var(--border-radius-md);
    background: var(--bg-primary);
    border: 1px solid var(--border-color);
  }

  .account-item.active {
    border-color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.04);
  }

  .account-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .account-main:disabled {
    cursor: default;
  }

  .account-main:not(:disabled):hover {
    background: var(--bg-hover);
  }

  .account-ico {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .account-ico.ms {
    background: linear-gradient(135deg, #0078d4, #00a4ef);
    color: #fff;
  }

  .account-ico.off {
    border: 1px solid rgba(245, 158, 11, 0.35);
    color: #fde68a;
  }

  .account-ico.ygg {
    background: rgba(168, 85, 247, 0.18);
    color: #e9d5ff;
  }

  .account-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .account-text .name {
    font-family: var(--font-minecraft);
    font-size: 10px;
    color: var(--text-primary);
    text-shadow: 1px 1px 0 #3f3f3f;
  }

  .account-text .meta {
    font-size: 11px;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    gap: 2px;
  }

  .ghost-icon {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .ghost-icon:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .ghost-icon.danger:hover {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
  }

  .type-badge {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    padding: 2px 8px;
    border-radius: 4px;
    letter-spacing: 0.4px;
  }

  .type-badge.microsoft {
    color: #93c5fd;
    background: rgba(59, 130, 246, 0.15);
  }

  .type-badge.offline {
    color: #fde68a;
    background: rgba(245, 158, 11, 0.12);
  }

  .type-badge.ygg {
    color: #e9d5ff;
    background: rgba(168, 85, 247, 0.15);
  }

  .accent-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    border-radius: var(--border-radius-md);
    background: var(--accent-primary);
    color: #000;
    border: none;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }

  .accent-btn:hover {
    background: var(--accent-hover);
  }

  .accent-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .mc-font {
    font-family: var(--font-minecraft);
  }
</style>
