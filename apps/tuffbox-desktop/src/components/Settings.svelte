<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-shell";
  import { onMount } from "svelte";
  import { Palette, Info, Moon, Sun, Command, Plug, KeyRound, CheckCircle2, AlertTriangle, Loader2, Gamepad2, Bot } from "lucide-svelte";
  import { api } from "../lib/api";
  import type { PresenceSettings } from "../lib/store";
  import AiConnectionModal from "./AiConnectionModal.svelte";

  type AiSettings = {
    provider: string;
    endpoint: string;
    model: string;
    diagnoseMode?: string;
    crashKbEndpoint?: string;
  };
  type IntegrationSettings = { githubRepository: string; ai: AiSettings };
  type IntegrationStatus = {
    settings: IntegrationSettings;
    githubTokenSet: boolean;
    modrinthTokenSet: boolean;
    curseforgeTokenSet: boolean;
    aiApiKeySet: boolean;
    crashKbTokenSet?: boolean;
  };
  type UpdateCheck = {
    currentVersion: string;
    latestVersion: string;
    updateAvailable: boolean;
    releaseUrl?: string | null;
    checkedAt?: string;
  };

  let theme = localStorage.getItem("tuffbox-theme") || "dark";
  let shortcuts: any[] = [];
  let shortcutsOpen = false;
  let appVersion = "";
  let updateCheck: UpdateCheck | null = null;
  let updateError = "";
  let updateLoading = false;

  let integrationsLoading = false;
  let integrationsError = "";
  let integrationsMessage = "";
  let githubRepository = "";
  let aiProvider: "ollama" | "openai-compatible" = "ollama";
  let aiEndpoint = "";
  let aiModel = "";
  let diagnoseMode: "server" | "local" | "kb_only" = "server";
  let crashKbEndpoint = "";
  let githubTokenSet = false;
  let modrinthTokenSet = false;
  let curseforgeTokenSet = false;
  let aiApiKeySet = false;
  let crashKbTokenSet = false;

  // Masked password drafts — never prefilled with real secrets
  let githubTokenDraft = "";
  let modrinthTokenDraft = "";
  let curseforgeTokenDraft = "";
  let aiApiKeyDraft = "";
  let crashKbTokenDraft = "";

  let savingSettings = false;
  let savingSecret: string | null = null;
  let clearingSecret: string | null = null;
  let testingProvider: string | null = null;
  let testResults: Record<string, string> = {};

  let discordRpcEnabled = false;
  let discordClientId = "";
  let discordSaving = false;
  let discordMessage = "";
  let discordError = "";
  let aiModalOpen = false;

  async function loadPresence() {
    discordError = "";
    try {
      const s = await api.presence.get();
      discordRpcEnabled = !!s.discordRpcEnabled;
      discordClientId = s.discordClientId ?? "";
    } catch (e) {
      discordError = String(e);
    }
  }

  async function savePresence() {
    discordSaving = true;
    discordError = "";
    discordMessage = "";
    try {
      const settings: PresenceSettings = {
        discordRpcEnabled,
        discordClientId: discordClientId.trim(),
      };
      await api.presence.save(settings);
      discordMessage = "Discord presence settings saved.";
    } catch (e) {
      discordError = String(e);
    } finally {
      discordSaving = false;
    }
  }

  async function checkUpdate() {
    updateLoading = true;
    updateError = "";
    updateCheck = null;
    try {
      updateCheck = await invoke<UpdateCheck>("check_for_app_update");
    } catch (e) {
      updateError = String(e);
    } finally {
      updateLoading = false;
    }
  }

  async function loadAppVersion() {
    try {
      appVersion = await invoke<string>("get_app_version");
    } catch {
      appVersion = "";
    }
  }

  async function loadIntegrations() {
    integrationsLoading = true;
    integrationsError = "";
    try {
      const status = await invoke<IntegrationStatus>("get_integration_status");
      githubRepository = status.settings?.githubRepository ?? "";
      aiProvider = (status.settings?.ai?.provider === "openai-compatible" ? "openai-compatible" : "ollama");
      aiEndpoint = status.settings?.ai?.endpoint ?? "";
      aiModel = status.settings?.ai?.model ?? "";
      const dm = status.settings?.ai?.diagnoseMode ?? "server";
      diagnoseMode = dm === "local" || dm === "kb_only" ? dm : "server";
      crashKbEndpoint = status.settings?.ai?.crashKbEndpoint ?? "";
      githubTokenSet = !!status.githubTokenSet;
      modrinthTokenSet = !!status.modrinthTokenSet;
      curseforgeTokenSet = !!status.curseforgeTokenSet;
      aiApiKeySet = !!status.aiApiKeySet;
      crashKbTokenSet = !!status.crashKbTokenSet;
      githubTokenDraft = "";
      modrinthTokenDraft = "";
      curseforgeTokenDraft = "";
      aiApiKeyDraft = "";
      crashKbTokenDraft = "";
    } catch (e) {
      integrationsError = String(e);
    } finally {
      integrationsLoading = false;
    }
  }

  async function saveIntegrationSettings() {
    savingSettings = true;
    integrationsError = "";
    integrationsMessage = "";
    try {
      await invoke("save_integration_settings", {
        settings: {
          githubRepository: githubRepository.trim(),
          ai: {
            provider: aiProvider,
            endpoint: aiEndpoint.trim(),
            model: aiModel.trim(),
            diagnoseMode,
            crashKbEndpoint: crashKbEndpoint.trim(),
          },
        },
      });
      integrationsMessage = "Integration settings saved.";
      await loadIntegrations();
    } catch (e) {
      integrationsError = String(e);
    } finally {
      savingSettings = false;
    }
  }

  async function saveSecret(kind: string, value: string) {
    if (!value.trim()) {
      integrationsError = `Enter a ${kind} credential before saving.`;
      return;
    }
    savingSecret = kind;
    integrationsError = "";
    integrationsMessage = "";
    try {
      await invoke("set_integration_secret", { kind, value: value.trim() });
      integrationsMessage = `${kind} credential saved.`;
      if (kind === "github") githubTokenDraft = "";
      if (kind === "modrinth") modrinthTokenDraft = "";
      if (kind === "curseforge") curseforgeTokenDraft = "";
      if (kind === "ai") aiApiKeyDraft = "";
      if (kind === "crash_kb") crashKbTokenDraft = "";
      await loadIntegrations();
    } catch (e) {
      integrationsError = String(e);
    } finally {
      savingSecret = null;
    }
  }

  async function clearSecret(kind: string) {
    clearingSecret = kind;
    integrationsError = "";
    integrationsMessage = "";
    try {
      await invoke("clear_integration_secret", { kind });
      integrationsMessage = `${kind} credential cleared.`;
      await loadIntegrations();
    } catch (e) {
      integrationsError = String(e);
    } finally {
      clearingSecret = null;
    }
  }

  async function testProvider(provider: string) {
    testingProvider = provider;
    integrationsError = "";
    try {
      const result = await invoke<string>("test_integration", { provider });
      testResults = { ...testResults, [provider]: result };
      integrationsMessage = result;
    } catch (e) {
      testResults = { ...testResults, [provider]: "" };
      integrationsError = String(e);
    } finally {
      testingProvider = null;
    }
  }

  async function openReleaseUrl() {
    if (!updateCheck?.releaseUrl) return;
    try {
      await open(updateCheck.releaseUrl);
    } catch (e) {
      updateError = String(e);
    }
  }

  onMount(async () => {
    document.documentElement.setAttribute("data-theme", theme);
    try { shortcuts = await invoke("get_keyboard_shortcuts"); } catch {}
    await loadAppVersion();
    await loadIntegrations();
    await loadPresence();
  });

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("tuffbox-theme", theme);
    document.documentElement.setAttribute("data-theme", theme);
  }

  function statusLabel(set: boolean) {
    return set ? "Configured" : "Not set";
  }
</script>

<div class="settings">
  <div class="settings-grid">
    <section class="card">
      <div class="card-title">
        <Palette size={18} />
        <h3>Appearance</h3>
      </div>
      <div class="field">
        <label for="theme-select">
          <Moon size={14} />
          Theme
        </label>
        <button id="theme-select" class="theme-toggle" type="button" on:click={toggleTheme}>
          {#if theme === "dark"}
            <Moon size={16} /> Dark theme
          {:else}
            <Sun size={16} /> Light theme
          {/if}
        </button>
      </div>
    </section>

    <section class="card">
      <div class="card-title">
        <Command size={18} />
        <h3>Keyboard shortcuts</h3>
      </div>
      <button class="ghost" on:click={() => (shortcutsOpen = !shortcutsOpen)}>
        {shortcutsOpen ? "Hide" : "Show"} shortcuts ({shortcuts.length})
      </button>
      {#if shortcutsOpen}
        <div class="shortcut-list">
          {#each shortcuts as s (s.key + s.action + (s.context ?? ''))}
            <div class="shortcut-row">
              <kbd>{s.key}</kbd>
              <span>{s.action}</span>
              <small>{s.context}</small>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section class="card card-wide">
      <div class="card-title">
        <Plug size={18} />
        <h3>Integrations</h3>
      </div>

      {#if integrationsLoading}
        <div class="inline-status"><Loader2 size={14} class="spin" /> Loading integration status…</div>
      {/if}
      {#if integrationsError}<div class="notice error"><AlertTriangle size={14} /> {integrationsError}</div>{/if}
      {#if integrationsMessage}<div class="notice success"><CheckCircle2 size={14} /> {integrationsMessage}</div>{/if}

      <div class="integrations">
        <div class="provider-block">
          <div class="provider-head">
            <strong>GitHub</strong>
            <span class:ok={githubTokenSet}>{statusLabel(githubTokenSet)}</span>
          </div>
          <label>
            Default repository (owner/name)
            <input bind:value={githubRepository} placeholder="owner/repository" autocomplete="off" />
          </label>
          <label>
            <KeyRound size={12} /> Personal access token
            <input
              type="password"
              bind:value={githubTokenDraft}
              placeholder={githubTokenSet ? "•••••••• (enter new to replace)" : "ghp_…"}
              autocomplete="new-password"
            />
          </label>
          <div class="row-actions">
            <button class="secondary mini" on:click={() => saveSecret("github", githubTokenDraft)} disabled={!!savingSecret || !githubTokenDraft.trim()}>
              {savingSecret === "github" ? "Saving…" : "Save token"}
            </button>
            <button class="ghost mini" on:click={() => clearSecret("github")} disabled={!githubTokenSet || !!clearingSecret}>
              {clearingSecret === "github" ? "Clearing…" : "Clear"}
            </button>
            <button class="ghost mini" on:click={() => testProvider("github")} disabled={!githubTokenSet || !!testingProvider}>
              {testingProvider === "github" ? "Testing…" : "Test"}
            </button>
          </div>
          {#if testResults.github}<small class="test-ok">{testResults.github}</small>{/if}
        </div>

        <div class="provider-block">
          <div class="provider-head">
            <strong>Modrinth</strong>
            <span class:ok={modrinthTokenSet}>{statusLabel(modrinthTokenSet)}</span>
          </div>
          <label>
            <KeyRound size={12} /> API token
            <input
              type="password"
              bind:value={modrinthTokenDraft}
              placeholder={modrinthTokenSet ? "•••••••• (enter new to replace)" : "Token"}
              autocomplete="new-password"
            />
          </label>
          <div class="row-actions">
            <button class="secondary mini" on:click={() => saveSecret("modrinth", modrinthTokenDraft)} disabled={!!savingSecret || !modrinthTokenDraft.trim()}>
              {savingSecret === "modrinth" ? "Saving…" : "Save token"}
            </button>
            <button class="ghost mini" on:click={() => clearSecret("modrinth")} disabled={!modrinthTokenSet || !!clearingSecret}>
              {clearingSecret === "modrinth" ? "Clearing…" : "Clear"}
            </button>
            <button class="ghost mini" on:click={() => testProvider("modrinth")} disabled={!modrinthTokenSet || !!testingProvider}>
              {testingProvider === "modrinth" ? "Testing…" : "Test"}
            </button>
          </div>
          {#if testResults.modrinth}<small class="test-ok">{testResults.modrinth}</small>{/if}
        </div>

        <div class="provider-block">
          <div class="provider-head">
            <strong>CurseForge</strong>
            <span class:ok={curseforgeTokenSet}>{statusLabel(curseforgeTokenSet)}</span>
          </div>
          <label>
            <KeyRound size={12} /> API token
            <input
              type="password"
              bind:value={curseforgeTokenDraft}
              placeholder={curseforgeTokenSet ? "•••••••• (enter new to replace)" : "Token"}
              autocomplete="new-password"
            />
          </label>
          <div class="row-actions">
            <button class="secondary mini" on:click={() => saveSecret("curseforge", curseforgeTokenDraft)} disabled={!!savingSecret || !curseforgeTokenDraft.trim()}>
              {savingSecret === "curseforge" ? "Saving…" : "Save token"}
            </button>
            <button class="ghost mini" on:click={() => clearSecret("curseforge")} disabled={!curseforgeTokenSet || !!clearingSecret}>
              {clearingSecret === "curseforge" ? "Clearing…" : "Clear"}
            </button>
            <button class="ghost mini" on:click={() => testProvider("curseforge")} disabled={!curseforgeTokenSet || !!testingProvider}>
              {testingProvider === "curseforge" ? "Testing…" : "Test"}
            </button>
          </div>
          {#if testResults.curseforge}<small class="test-ok">{testResults.curseforge}</small>{/if}
        </div>

        <div class="provider-block">
          <div class="provider-head">
            <strong>AI</strong>
            <span class:ok={aiProvider === "ollama" || aiApiKeySet}>
              {aiProvider === "ollama" ? "Ollama" : aiApiKeySet ? "API key set" : "API (no key)"}
            </span>
          </div>
          <p class="hint">
            Provider: <code>{aiProvider}</code>
            · Endpoint: <code>{aiEndpoint || "—"}</code>
            · Model: <code>{aiModel || "—"}</code>
          </p>
          <div class="row-actions">
            <button type="button" class="secondary mini" on:click={() => (aiModalOpen = true)}>
              <Bot size={14} /> Configure AI connection…
            </button>
            <button class="ghost mini" on:click={() => testProvider("ai")} disabled={!!testingProvider || (aiProvider === "openai-compatible" && !aiApiKeySet && !aiEndpoint.includes("127.0.0.1") && !aiEndpoint.includes("localhost"))}>
              {testingProvider === "ai" ? "Testing…" : "Test AI"}
            </button>
          </div>
          {#if testResults.ai}<small class="test-ok">{testResults.ai}</small>{/if}
        </div>

        <div class="provider-block">
          <div class="provider-head">
            <strong>Crash KB</strong>
            <span class:ok={!!crashKbEndpoint}>{crashKbEndpoint ? diagnoseMode : "offline seed"}</span>
          </div>
          <p class="hint">Private crash knowledge base. Full corpus stays on your server; launcher only gets matched plans/hits.</p>
          <label>
            Diagnose mode
            <select bind:value={diagnoseMode}>
              <option value="server">server (default) — remote diagnose + LLM</option>
              <option value="local">local — remote lookup + your Ollama/API</option>
              <option value="kb_only">kb_only — matched case actions, no LLM</option>
            </select>
          </label>
          <label>
            Crash KB API base URL
            <input bind:value={crashKbEndpoint} placeholder="https://kb.example.com" />
          </label>
          <label>
            Crash KB token
            <input type="password" bind:value={crashKbTokenDraft} placeholder={crashKbTokenSet ? "•••••••• (set)" : "optional bearer token"} autocomplete="off" />
          </label>
          <div class="row-actions">
            <button class="mini" disabled={savingSecret === "crash_kb" || !crashKbTokenDraft.trim()} on:click={() => saveSecret("crash_kb", crashKbTokenDraft)}>
              {savingSecret === "crash_kb" ? "Saving…" : "Save token"}
            </button>
            <button class="ghost mini" disabled={clearingSecret === "crash_kb" || !crashKbTokenSet} on:click={() => clearSecret("crash_kb")}>
              Clear
            </button>
          </div>
        </div>
      </div>

      <div class="row-actions save-row">
        <button on:click={saveIntegrationSettings} disabled={savingSettings || integrationsLoading}>
          {savingSettings ? "Saving…" : "Save settings"}
        </button>
        <button class="ghost" on:click={loadIntegrations} disabled={integrationsLoading}>Reload status</button>
      </div>
    </section>

    <section class="card">
      <div class="card-title">
        <Gamepad2 size={18} />
        <h3>Discord Rich Presence</h3>
      </div>
      {#if discordError}<div class="notice error"><AlertTriangle size={14} /> {discordError}</div>{/if}
      {#if discordMessage}<div class="notice success"><CheckCircle2 size={14} /> {discordMessage}</div>{/if}
      <label class="check-row">
        <input type="checkbox" bind:checked={discordRpcEnabled} />
        Show playing status in Discord while Minecraft is running
      </label>
      <label>
        Application Client ID
        <input
          bind:value={discordClientId}
          placeholder="Create an app at discord.com/developers"
          autocomplete="off"
        />
      </label>
      <p class="hint">
        Create an application in the Discord Developer Portal and paste its Client ID.
        Optional asset key <code>tuffbox</code> can be uploaded for a large image.
      </p>
      <div class="row-actions">
        <button on:click={savePresence} disabled={discordSaving}>
          {discordSaving ? "Saving…" : "Save Discord settings"}
        </button>
      </div>
    </section>

    <section class="card">
      <div class="card-title">
        <Info size={18} />
        <h3>About</h3>
      </div>
      <button class="ghost" on:click={async () => { await loadAppVersion(); await checkUpdate(); }} disabled={updateLoading}>
        {updateLoading ? "Checking…" : "Check for updates"}
      </button>
      {#if updateError}
        <div class="update-info error"><AlertTriangle size={14} /> {updateError}</div>
      {/if}
      {#if updateCheck}
        <div class="update-info">
          {#if updateCheck.updateAvailable}
            <span class="update-avail">Update available: {updateCheck.latestVersion}</span>
            {#if updateCheck.releaseUrl}
              <button class="ghost mini" on:click={openReleaseUrl}>Open release</button>
            {/if}
          {:else}
            <span class="update-ok">Up to date ({updateCheck.currentVersion})</span>
          {/if}
        </div>
      {/if}
      <div class="about">
        <div class="logo-big">T</div>
        <div>
          <h4>TuffBox IDE</h4>
          <p>Developer harness for Minecraft modpacks.</p>
          <span class="version">Version {appVersion || "…"}</span>
        </div>
      </div>
    </section>
  </div>
</div>

<AiConnectionModal bind:open={aiModalOpen} on:saved={loadIntegrations} />

<style>
  .settings {
    max-width: 980px;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 20px;
  }

  .card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 24px;
  }

  .card-wide {
    grid-column: 1 / -1;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 20px;
    color: var(--text-secondary);
  }

  .card-title h3 {
    font-size: 16px;
    color: var(--text-primary);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  label :global(svg) {
    display: inline;
  }

  input, select {
    width: 100%;
  }

  .about {
    display: flex;
    align-items: center;
    gap: 18px;
    margin-top: 14px;
  }

  .logo-big {
    width: 64px;
    height: 64px;
    border-radius: var(--border-radius-lg);
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 32px;
    color: #000;
    box-shadow: 0 8px 24px rgba(27, 217, 106, 0.25);
  }

  .about h4 {
    font-size: 18px;
    margin-bottom: 4px;
  }

  .about p {
    color: var(--text-secondary);
    font-size: 13px;
    margin-bottom: 8px;
  }

  .theme-toggle { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); color: var(--text-secondary); cursor: pointer; font-size: 13px; font-weight: 600; }
  .theme-toggle:hover { border-color: var(--accent-primary); color: var(--text-primary); }

  .shortcut-list { display: grid; gap: 4px; margin-top: 8px; }
  .shortcut-row { display: flex; align-items: center; gap: 12px; padding: 6px 10px; border-radius: 6px; background: var(--bg-tertiary); }
  .shortcut-row kbd { font-family: ui-monospace,monospace; font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); border: 1px solid var(--border-color); color: var(--text-primary); min-width: 60px; text-align: center; }
  .shortcut-row span { flex: 1; color: var(--text-secondary); font-size: 12px; }
  .shortcut-row small { color: var(--text-muted); font-size: 10px; }

  .update-info { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 8px 10px; border-radius: 8px; background: var(--bg-tertiary); border: 1px solid var(--border-color); margin: 10px 0; font-size: 12px; }
  .update-info.error { color: #fecaca; border-color: rgba(239, 68, 68, 0.28); background: rgba(239, 68, 68, 0.08); }
  .update-avail { color: var(--accent-primary); font-weight: 700; }
  .update-ok { color: var(--text-muted); }

  .version {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 4px;
  }

  .integrations { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 14px; }
  .provider-block { display: grid; gap: 10px; padding: 14px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .provider-head { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
  .provider-head strong { color: var(--text-primary); }
  .provider-head span { font-size: 11px; color: var(--text-muted); font-weight: 700; }
  .provider-head span.ok { color: var(--accent-primary); }
  .row-actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .save-row { margin-top: 16px; }
  .mini { padding: 5px 8px; font-size: 11px; }
  .hint { margin: 0; color: var(--text-muted); font-size: 12px; line-height: 1.4; }
  .check-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-secondary);
    margin-bottom: 12px;
    cursor: pointer;
  }
  .check-row input { accent-color: var(--accent-primary); }
  .test-ok { color: var(--accent-primary); font-size: 11px; }
  .notice { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-radius: 10px; margin-bottom: 12px; border: 1px solid var(--border-color); font-size: 12px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .inline-status { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 12px; margin-bottom: 10px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
