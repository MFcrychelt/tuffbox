<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import {
    Palette, Info, Command, Plug, KeyRound, CheckCircle2, AlertTriangle, Loader2,
    Gamepad2, Bot, Network, Coffee, Terminal, HardDrive, Settings2, Download,
  } from "lucide-svelte";
  import { api } from "../lib/api";
  import type { PresenceSettings, LauncherSettings } from "../lib/store";
  import {
    readStoredTheme, commitTheme, type ThemeId,
  } from "../lib/themes";
  import AiConnectionModal from "./AiConnectionModal.svelte";
  import ThemePicker from "./ThemePicker.svelte";
  import JavaPickerModal from "./JavaPickerModal.svelte";

  type SettingsTab = "general" | "appearance" | "java" | "commands" | "runtime" | "integrations" | "about";
  let tab: SettingsTab = "appearance";

  type AiSettings = {
    provider: string;
    endpoint: string;
    model: string;
    diagnoseMode?: string;
    crashKbEndpoint?: string;
    ollamaBinaryPath?: string;
  };
  type SwarmSettings = {
    enabled?: boolean;
    onboardingDone?: boolean;
    sharePromptsEnabled?: boolean;
    supabaseUrl?: string;
    hubUrl?: string;
    p2pEnabled?: boolean;
    p2pControlUrl?: string;
  };
  type IntegrationSettings = { githubRepository: string; ai: AiSettings; swarm?: SwarmSettings };
  type IntegrationStatus = {
    settings: IntegrationSettings;
    githubTokenSet: boolean;
    modrinthTokenSet: boolean;
    curseforgeTokenSet: boolean;
    aiApiKeySet: boolean;
    crashKbTokenSet?: boolean;
    swarmSupabaseAnonSet?: boolean;
    swarmSupabaseUsingBuiltin?: boolean;
    swarmSupabaseConfigured?: boolean;
  };
  type UpdateCheck = {
    currentVersion: string;
    latestVersion: string;
    updateAvailable: boolean;
    releaseUrl?: string | null;
    checkedAt?: string;
  };

  let theme: ThemeId = readStoredTheme();
  let reducedMotion = localStorage.getItem("tuffbox-reduced-motion") === "1";
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
  let ollamaBinaryPath = "";
  let diagnoseMode: "server" | "local" | "kb_only" = "server";
  let crashKbEndpoint = "";
  let githubTokenSet = false;
  let modrinthTokenSet = false;
  let curseforgeTokenSet = false;
  let aiApiKeySet = false;
  let crashKbTokenSet = false;

  let githubTokenDraft = "";
  let modrinthTokenDraft = "";
  let curseforgeTokenDraft = "";
  let aiApiKeyDraft = "";
  let crashKbTokenDraft = "";
  let swarmEnabled = false;
  let swarmSharePrompts = true;
  let swarmSupabaseUrl = "";
  let swarmSupabaseAnonDraft = "";
  let swarmSupabaseAnonSet = false;
  let swarmSupabaseUsingBuiltin = true;
  let swarmSupabaseConfigured = false;
  let swarmSupabaseAdvanced = false;
  let swarmHubUrl = "";
  let swarmP2pEnabled = false;
  let swarmP2pControlUrl = "http://127.0.0.1:8790";
  let swarmP2pStatus = "";
  let swarmSaving = false;

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

  // Launcher settings
  let launcher: LauncherSettings = {
    theme: "tuffbox",
    potatoPc: false,
    concurrentDownloads: 8,
    gameResolution: null,
    preLaunchHook: null,
    postExitHook: null,
    wrapperCommand: null,
    runtimePath: null,
    instancesPath: null,
    defaultJavaPath: null,
    javaCustomArgs: null,
    defaultMemoryMb: 4096,
  };
  let launcherSaving = false;
  let launcherMsg = "";
  let launcherErr = "";
  let defaultRuntimePath = "";
  let runtimeDraft = "";
  let defaultInstancesPath = "";
  let instancesDraft = "";
  let showJavaPicker = false;
  let resMode: "default" | "1080p" | "720p" | "custom" = "default";
  let customW = 1280;
  let customH = 720;

  const tabs: { id: SettingsTab; label: string; icon: typeof Palette }[] = [
    { id: "appearance", label: "Appearance", icon: Palette },
    { id: "general", label: "General", icon: Settings2 },
    { id: "java", label: "Java", icon: Coffee },
    { id: "commands", label: "Commands", icon: Terminal },
    { id: "runtime", label: "Runtime", icon: HardDrive },
    { id: "integrations", label: "Integrations", icon: Plug },
    { id: "about", label: "About", icon: Info },
  ];

  function syncResModeFromLauncher() {
    const r = launcher.gameResolution;
    if (!r) {
      resMode = "default";
      return;
    }
    if (r.width === 1920 && r.height === 1080) resMode = "1080p";
    else if (r.width === 1280 && r.height === 720) resMode = "720p";
    else {
      resMode = "custom";
      customW = r.width;
      customH = r.height;
    }
  }

  async function loadLauncher() {
    launcherErr = "";
    try {
      launcher = await api.launcher.get();
      theme = (THEMES_SAFE(launcher.theme) as ThemeId) || readStoredTheme();
      reducedMotion = !!launcher.potatoPc;
      applyPotatoPc(reducedMotion);
      localStorage.setItem("tuffbox-reduced-motion", reducedMotion ? "1" : "0");
      commitTheme(theme);
      syncResModeFromLauncher();
      const info = await api.launcher.runtimePathInfo();
      defaultRuntimePath = info.default;
      runtimeDraft = launcher.runtimePath?.trim() || info.current;
      const inst = await api.launcher.instancesPathInfo();
      defaultInstancesPath = inst.default;
      instancesDraft = launcher.instancesPath?.trim() || inst.current;
    } catch (e) {
      launcherErr = String(e);
    }
  }

  function THEMES_SAFE(id: string): string {
    const ok = ["tuffbox", "tuffbox-light", "carbon", "inferno", "aether", "frost", "pixelato", "win95"];
    if (id === "dark") return "tuffbox";
    if (id === "light") return "tuffbox-light";
    return ok.includes(id) ? id : "tuffbox";
  }

  async function persistLauncher(partial?: Partial<LauncherSettings>) {
    launcherSaving = true;
    launcherErr = "";
    launcherMsg = "";
    try {
      const next: LauncherSettings = { ...launcher, ...partial };
      if (partial?.theme) {
        theme = THEMES_SAFE(partial.theme) as ThemeId;
        commitTheme(theme);
        next.theme = theme;
      }
      launcher = await api.launcher.save(next);
      launcherMsg = "Saved.";
      setTimeout(() => (launcherMsg = ""), 1600);
    } catch (e) {
      launcherErr = String(e);
    } finally {
      launcherSaving = false;
    }
  }

  function onThemeChange(id: ThemeId) {
    theme = id;
    void persistLauncher({ theme: id });
  }

  function applyResolution(mode: typeof resMode) {
    resMode = mode;
    if (mode === "default") void persistLauncher({ gameResolution: null });
    else if (mode === "1080p") void persistLauncher({ gameResolution: { width: 1920, height: 1080 } });
    else if (mode === "720p") void persistLauncher({ gameResolution: { width: 1280, height: 720 } });
    else void persistLauncher({ gameResolution: { width: customW, height: customH } });
  }

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
      ollamaBinaryPath = status.settings?.ai?.ollamaBinaryPath ?? "";
      const dm = status.settings?.ai?.diagnoseMode ?? "server";
      diagnoseMode = dm === "local" || dm === "kb_only" ? dm : "server";
      crashKbEndpoint = status.settings?.ai?.crashKbEndpoint ?? "";
      githubTokenSet = !!status.githubTokenSet;
      modrinthTokenSet = !!status.modrinthTokenSet;
      curseforgeTokenSet = !!status.curseforgeTokenSet;
      aiApiKeySet = !!status.aiApiKeySet;
      crashKbTokenSet = !!status.crashKbTokenSet;
      swarmEnabled = !!status.settings?.swarm?.enabled;
      swarmSharePrompts = status.settings?.swarm?.sharePromptsEnabled !== false;
      swarmSupabaseUrl = status.settings?.swarm?.supabaseUrl ?? "";
      swarmSupabaseAnonSet = !!status.swarmSupabaseAnonSet;
      swarmSupabaseUsingBuiltin = status.swarmSupabaseUsingBuiltin !== false;
      swarmSupabaseConfigured = !!status.swarmSupabaseConfigured;
      swarmHubUrl = status.settings?.swarm?.hubUrl ?? "";
      swarmP2pEnabled = !!status.settings?.swarm?.p2pEnabled;
      swarmP2pControlUrl =
        status.settings?.swarm?.p2pControlUrl?.trim() || "http://127.0.0.1:8790";
      githubTokenDraft = "";
      modrinthTokenDraft = "";
      curseforgeTokenDraft = "";
      aiApiKeyDraft = "";
      crashKbTokenDraft = "";
      swarmSupabaseAnonDraft = "";
      if (swarmEnabled && swarmP2pEnabled) {
        void refreshP2pStatus();
      } else {
        swarmP2pStatus = "";
      }
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
            ollamaBinaryPath: ollamaBinaryPath.trim(),
          },
          swarm: {
            enabled: swarmEnabled,
            onboardingDone: true,
            sharePromptsEnabled: swarmSharePrompts,
            supabaseUrl: swarmSupabaseUrl.trim(),
            hubUrl: swarmHubUrl.trim(),
            p2pEnabled: swarmP2pEnabled,
            p2pControlUrl: swarmP2pControlUrl.trim() || "http://127.0.0.1:8790",
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
      if (kind === "swarm_supabase") swarmSupabaseAnonDraft = "";
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

  async function toggleSwarmEnabled() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const next = !swarmEnabled;
      const s = await invoke<SwarmSettings>("set_swarm_enabled", { enabled: next });
      swarmEnabled = !!s.enabled;
      integrationsMessage = swarmEnabled
        ? "TuffSwarm network enabled — Fix Mode (network) and Creation Mode available."
        : "Network disabled — Creation Mode and network Fix Mode are blocked.";
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function toggleSharePrompts() {
    swarmSaving = true;
    try {
      const next = !swarmSharePrompts;
      const s = await invoke<SwarmSettings>("set_swarm_share_prompts", { enabled: next });
      swarmSharePrompts = s.sharePromptsEnabled !== false;
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function saveSupabaseUrl() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const s = await invoke<SwarmSettings>("set_swarm_supabase_url", {
        supabaseUrl: swarmSupabaseUrl.trim(),
      });
      swarmSupabaseUrl = s.supabaseUrl ?? "";
      integrationsMessage = swarmSupabaseUrl
        ? "Supabase URL saved."
        : "Supabase URL cleared.";
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function toggleP2pEnabled() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const next = !swarmP2pEnabled;
      const s = await invoke<SwarmSettings>("set_swarm_p2p", {
        enabled: next,
        controlUrl: swarmP2pControlUrl.trim() || null,
      });
      swarmP2pEnabled = !!s.p2pEnabled;
      swarmP2pControlUrl = s.p2pControlUrl?.trim() || "http://127.0.0.1:8790";
      if (swarmP2pEnabled) {
        await ensureP2pNode();
      } else {
        swarmP2pStatus = "P2P off — hub HTTP fallback only";
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function refreshP2pStatus() {
    try {
      const st = await invoke<{ enabled?: boolean; healthy?: boolean; controlUrl?: string; node?: { peers?: number; capsuleCount?: number } }>(
        "get_p2p_node_status",
      );
      if (!st.enabled) {
        swarmP2pStatus = "P2P disabled";
        return;
      }
      if (st.healthy) {
        const peers = st.node?.peers ?? 0;
        const caps = st.node?.capsuleCount ?? 0;
        swarmP2pStatus = `Node healthy · ${peers} peer(s) · ${caps} capsule(s)`;
      } else {
        swarmP2pStatus = "Node not reachable — will try hub fallback";
      }
    } catch (e) {
      swarmP2pStatus = String(e);
    }
  }

  async function ensureP2pNode() {
    swarmSaving = true;
    integrationsError = "";
    try {
      await invoke("ensure_p2p_node");
      await refreshP2pStatus();
      integrationsMessage = "tuffswarm-node attached (P2P preferred; hub remains fallback).";
    } catch (e) {
      integrationsError = String(e);
      swarmP2pStatus = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function openReleaseUrl() {
    if (!updateCheck?.releaseUrl) return;
    try {
      await openShell(updateCheck.releaseUrl);
    } catch (e) {
      updateError = String(e);
    }
  }

  onMount(async () => {
    applyPotatoPc(reducedMotion);
    try { shortcuts = await invoke("get_keyboard_shortcuts"); } catch {}
    await loadAppVersion();
    await loadIntegrations();
    await loadPresence();
    await loadLauncher();
  });

  function applyPotatoPc(on: boolean) {
    document.documentElement.classList.toggle("potato-pc", on);
  }

  function toggleReducedMotion() {
    reducedMotion = !reducedMotion;
    localStorage.setItem("tuffbox-reduced-motion", reducedMotion ? "1" : "0");
    applyPotatoPc(reducedMotion);
    void persistLauncher({ potatoPc: reducedMotion });
  }

  function statusLabel(set: boolean) {
    return set ? "Configured" : "Not set";
  }

  async function browseRuntime() {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected === "string") {
      runtimeDraft = selected;
    }
  }

  async function browseInstances() {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Select modpack / instances folder",
    });
    if (typeof selected === "string") {
      instancesDraft = selected;
    }
  }

  async function applyRuntimePath() {
    try {
      await api.launcher.validateRuntimePath(runtimeDraft);
      await persistLauncher({ runtimePath: runtimeDraft.trim() || null });
    } catch (e) {
      launcherErr = String(e);
    }
  }

  async function applyInstancesPath() {
    try {
      await api.launcher.validateInstancesPath(instancesDraft);
      await persistLauncher({ instancesPath: instancesDraft.trim() || null });
      instancesDraft = launcher.instancesPath?.trim() || defaultInstancesPath;
      launcherMsg = "Modpack download folder saved.";
    } catch (e) {
      launcherErr = String(e);
    }
  }
</script>

<div class="settings fade-slide-in">
  <nav class="tabs" aria-label="Settings sections">
    {#each tabs as t (t.id)}
      <button
        type="button"
        class="tab press-effect"
        class:active={tab === t.id}
        on:click={() => (tab = t.id)}
      >
        <svelte:component this={t.icon} size={16} />
        {t.label}
      </button>
    {/each}
  </nav>

  {#if launcherErr}<div class="notice error"><AlertTriangle size={14} /> {launcherErr}</div>{/if}
  {#if launcherMsg}<div class="notice success"><CheckCircle2 size={14} /> {launcherMsg}</div>{/if}

  <div class="settings-grid">
    {#if tab === "appearance"}
      <section class="card card-wide">
        <div class="card-title">
          <Palette size={18} />
          <h3>Appearance</h3>
        </div>
        <div class="field">
          <span class="field-label">Theme</span>
          <ThemePicker value={theme} onChange={onThemeChange} />
          <p class="hint">Hover a swatch to preview — click to save.</p>
        </div>
        <label class="check-row">
          <input type="checkbox" checked={reducedMotion} on:change={toggleReducedMotion} />
          Potato PC mode (reduce motion / animations)
        </label>
        <p class="hint">Disables CSS animations and transitions for weaker machines.</p>
      </section>
    {/if}

    {#if tab === "general"}
      <section class="card">
        <div class="card-title">
          <Download size={18} />
          <h3>Downloads</h3>
        </div>
        <label>
          Concurrent downloads (1–32)
          <input
            type="number"
            min="1"
            max="32"
            bind:value={launcher.concurrentDownloads}
            on:change={() =>
              persistLauncher({
                concurrentDownloads: Math.min(32, Math.max(1, Number(launcher.concurrentDownloads) || 8)),
              })}
          />
        </label>
      </section>

      <section class="card">
        <div class="card-title">
          <Gamepad2 size={18} />
          <h3>Game resolution</h3>
        </div>
        <div class="chip-row">
          <button type="button" class="chip press-effect" class:active={resMode === "default"} on:click={() => applyResolution("default")}>Default</button>
          <button type="button" class="chip press-effect" class:active={resMode === "1080p"} on:click={() => applyResolution("1080p")}>1080p</button>
          <button type="button" class="chip press-effect" class:active={resMode === "720p"} on:click={() => applyResolution("720p")}>720p</button>
          <button type="button" class="chip press-effect" class:active={resMode === "custom"} on:click={() => (resMode = "custom")}>Custom</button>
        </div>
        {#if resMode === "custom"}
          <div class="res-custom">
            <label>
              Width
              <input type="number" min="640" bind:value={customW} />
            </label>
            <label>
              Height
              <input type="number" min="480" bind:value={customH} />
            </label>
            <button type="button" class="secondary" on:click={() => applyResolution("custom")} disabled={launcherSaving}>
              Apply
            </button>
          </div>
        {/if}
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
          <Command size={18} />
          <h3>Keyboard shortcuts</h3>
        </div>
        <button class="ghost" on:click={() => (shortcutsOpen = !shortcutsOpen)}>
          {shortcutsOpen ? "Hide" : "Show"} shortcuts ({shortcuts.length})
        </button>
        {#if shortcutsOpen}
          <div class="shortcut-list">
            {#each shortcuts as s (s.key + s.action + (s.context ?? ""))}
              <div class="shortcut-row">
                <kbd>{s.key}</kbd>
                <span>{s.action}</span>
                <small>{s.context}</small>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    {#if tab === "java"}
      <section class="card card-wide">
        <div class="card-title">
          <Coffee size={18} />
          <h3>Java</h3>
        </div>
        <label>
          Default Java path
          <div class="path-row">
            <input
              readonly
              value={launcher.defaultJavaPath ?? "Auto-detect"}
              title={launcher.defaultJavaPath ?? "Auto-detect"}
            />
            <button type="button" class="secondary" on:click={() => (showJavaPicker = true)}>Browse…</button>
          </div>
        </label>
        <label>
          Custom JVM arguments
          <textarea
            rows="3"
            bind:value={launcher.javaCustomArgs}
            placeholder="-XX:+UseG1GC …"
            on:blur={() => persistLauncher({ javaCustomArgs: launcher.javaCustomArgs?.trim() || null })}
          ></textarea>
        </label>
        <label>
          Default memory (MB)
          <input
            type="number"
            min="512"
            step="256"
            bind:value={launcher.defaultMemoryMb}
            on:change={() =>
              persistLauncher({
                defaultMemoryMb: Math.max(512, Number(launcher.defaultMemoryMb) || 4096),
              })}
          />
        </label>
        <div class="row-actions save-row">
          <button
            type="button"
            disabled={launcherSaving}
            on:click={() =>
              persistLauncher({
                javaCustomArgs: launcher.javaCustomArgs?.trim() || null,
                defaultMemoryMb: Math.max(512, Number(launcher.defaultMemoryMb) || 4096),
              })}
          >
            {launcherSaving ? "Saving…" : "Save Java settings"}
          </button>
        </div>
      </section>
    {/if}

    {#if tab === "commands"}
      <section class="card card-wide">
        <div class="card-title">
          <Terminal size={18} />
          <h3>Launch commands</h3>
        </div>
        <label>
          Pre-launch hook
          <input
            bind:value={launcher.preLaunchHook}
            placeholder="Command before game start"
            on:blur={() => persistLauncher({ preLaunchHook: launcher.preLaunchHook?.trim() || null })}
          />
        </label>
        <label>
          Post-exit hook
          <input
            bind:value={launcher.postExitHook}
            placeholder="Command after game exits"
            on:blur={() => persistLauncher({ postExitHook: launcher.postExitHook?.trim() || null })}
          />
        </label>
        <label>
          Wrapper command
          <input
            bind:value={launcher.wrapperCommand}
            placeholder="e.g. gamemoderun"
            on:blur={() => persistLauncher({ wrapperCommand: launcher.wrapperCommand?.trim() || null })}
          />
        </label>
        <div class="row-actions save-row">
          <button
            type="button"
            disabled={launcherSaving}
            on:click={() =>
              persistLauncher({
                preLaunchHook: launcher.preLaunchHook?.trim() || null,
                postExitHook: launcher.postExitHook?.trim() || null,
                wrapperCommand: launcher.wrapperCommand?.trim() || null,
              })}
          >
            {launcherSaving ? "Saving…" : "Save"}
          </button>
        </div>
      </section>
    {/if}

    {#if tab === "runtime"}
      <section class="card card-wide">
        <div class="card-title">
          <HardDrive size={18} />
          <h3>Runtime path</h3>
        </div>
        <p class="hint">
          Move the shared runtime (libraries, assets, Java) to another disk to free space on the system drive.
          Default: <code>{defaultRuntimePath || "…"}</code>
        </p>
        <label>
          Runtime directory
          <div class="path-row">
            <input bind:value={runtimeDraft} placeholder={defaultRuntimePath || "Runtime path"} />
            <button type="button" class="secondary" on:click={browseRuntime}>Browse…</button>
          </div>
        </label>
        <div class="row-actions">
          <button type="button" on:click={applyRuntimePath} disabled={launcherSaving}>
            {launcherSaving ? "Saving…" : "Apply path"}
          </button>
          <button
            type="button"
            class="ghost"
            disabled={!defaultRuntimePath}
            on:click={() => {
              runtimeDraft = defaultRuntimePath;
              void applyRuntimePath();
            }}
          >
            Reset to default
          </button>
        </div>
      </section>

      <section class="card card-wide">
        <div class="card-title">
          <HardDrive size={18} />
          <h3>Modpacks / instances folder</h3>
        </div>
        <p class="hint">
          Where Discover and Add Instance download modpacks by default.
          Default: <code>{defaultInstancesPath || "…"}</code>
        </p>
        <label>
          Download directory
          <div class="path-row">
            <input bind:value={instancesDraft} placeholder={defaultInstancesPath || "Instances path"} />
            <button type="button" class="secondary" on:click={browseInstances}>Browse…</button>
          </div>
        </label>
        <div class="row-actions">
          <button type="button" on:click={applyInstancesPath} disabled={launcherSaving}>
            {launcherSaving ? "Saving…" : "Apply path"}
          </button>
          <button
            type="button"
            class="ghost"
            disabled={!defaultInstancesPath}
            on:click={() => {
              instancesDraft = defaultInstancesPath;
              void applyInstancesPath();
            }}
          >
            Reset to default
          </button>
        </div>
      </section>
    {/if}

    {#if tab === "integrations"}
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
              {#if aiProvider === "ollama"}
                · Path: <code>{ollamaBinaryPath || "auto"}</code>
              {/if}
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
              <strong><Network size={14} /> TuffSwarm</strong>
              <span class:ok={swarmEnabled}>{swarmEnabled ? "enabled" : "off"}</span>
            </div>
            <p class="hint">
              Shares crash→fix capsules (fingerprint + solution + actions — not raw logs).
              Community backend (Supabase) is built in — users only need to enable the network.
              Optional: custom hub / P2P for self-hosting.
            </p>
            <label class="check-row">
              <input
                type="checkbox"
                checked={swarmEnabled}
                disabled={swarmSaving}
                on:change={toggleSwarmEnabled}
              />
              Use TuffSwarm network
            </label>
            {#if swarmSupabaseConfigured}
              <small class="test-ok">
                {swarmSupabaseUsingBuiltin
                  ? "Community Supabase: connected (built-in)"
                  : "Supabase: using custom URL / key override"}
              </small>
            {:else}
              <small class="test-ok" style="opacity:0.8">Supabase backend not configured</small>
            {/if}
            <button
              type="button"
              class="ghost mini"
              disabled={!swarmEnabled}
              on:click={() => (swarmSupabaseAdvanced = !swarmSupabaseAdvanced)}
            >
              {swarmSupabaseAdvanced ? "Hide advanced backend" : "Advanced backend override…"}
            </button>
            {#if swarmSupabaseAdvanced}
            <label>
              Supabase URL override (empty = built-in)
              <input
                bind:value={swarmSupabaseUrl}
                placeholder="https://xxxx.supabase.co"
                disabled={!swarmEnabled}
                autocomplete="off"
              />
            </label>
            <div class="row-actions">
              <button
                type="button"
                class="mini"
                disabled={swarmSaving || !swarmEnabled}
                on:click={saveSupabaseUrl}
              >
                Save Supabase URL
              </button>
            </div>
            <label>
              Supabase anon key override
              <input
                type="password"
                bind:value={swarmSupabaseAnonDraft}
                placeholder={swarmSupabaseAnonSet ? "•••••••• (custom set)" : "leave empty for built-in"}
                disabled={!swarmEnabled}
                autocomplete="off"
              />
            </label>
            <div class="row-actions">
              <button
                class="mini"
                disabled={
                  swarmSaving ||
                  savingSecret === "swarm_supabase" ||
                  !swarmSupabaseAnonDraft.trim()
                }
                on:click={() => saveSecret("swarm_supabase", swarmSupabaseAnonDraft)}
              >
                {savingSecret === "swarm_supabase" ? "Saving…" : "Save anon key"}
              </button>
              <button
                class="ghost mini"
                disabled={
                  swarmSaving ||
                  clearingSecret === "swarm_supabase" ||
                  !swarmSupabaseAnonSet
                }
                on:click={() => clearSecret("swarm_supabase")}
              >
                Clear override
              </button>
            </div>
            {/if}
            <label>
              Swarm hub URL (optional fallback)
              <input
                bind:value={swarmHubUrl}
                placeholder="http://192.168.1.10:8787"
                disabled={!swarmEnabled}
                autocomplete="off"
              />
            </label>
            <label class="check-row">
              <input
                type="checkbox"
                checked={swarmP2pEnabled}
                disabled={swarmSaving || !swarmEnabled}
                on:change={toggleP2pEnabled}
              />
              Prefer local P2P node (Phase C)
            </label>
            <label>
              P2P control URL
              <input
                bind:value={swarmP2pControlUrl}
                placeholder="http://127.0.0.1:8790"
                disabled={!swarmEnabled || !swarmP2pEnabled}
                autocomplete="off"
              />
            </label>
            <div class="row-actions">
              <button
                type="button"
                class="secondary mini"
                disabled={!swarmEnabled || !swarmP2pEnabled || swarmSaving}
                on:click={ensureP2pNode}
              >
                Start / attach node
              </button>
              <button
                type="button"
                class="ghost mini"
                disabled={!swarmEnabled || !swarmP2pEnabled || swarmSaving}
                on:click={refreshP2pStatus}
              >
                Refresh status
              </button>
            </div>
            {#if swarmP2pStatus}
              <small class="test-ok">{swarmP2pStatus}</small>
            {/if}
            <label class="check-row">
              <input
                type="checkbox"
                checked={swarmSharePrompts}
                disabled={swarmSaving || !swarmEnabled}
                on:change={toggleSharePrompts}
              />
              Ask to share capsule after a successful relaunch
            </label>
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
    {/if}

    {#if tab === "about"}
      <section class="card card-wide">
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
    {/if}
  </div>
</div>

<AiConnectionModal bind:open={aiModalOpen} on:saved={loadIntegrations} />

{#if showJavaPicker}
  <JavaPickerModal
    current={launcher.defaultJavaPath ?? "Auto-detect"}
    on:close={() => (showJavaPicker = false)}
    on:selected={(e) => { showJavaPicker = false; void persistLauncher({ defaultJavaPath: e.detail }); }}
  />
{/if}

<style>
  .settings {
    max-width: 980px;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 18px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border-color);
  }

  .tab {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text-primary);
    background: var(--bg-tertiary);
  }

  .tab.active {
    color: var(--text-primary);
    background: var(--bg-elevated);
    border-color: var(--border-color);
    box-shadow: 0 0 0 1px var(--accent-primary);
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
    margin-bottom: 16px;
  }

  .field-label {
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 600;
    margin-bottom: 12px;
  }

  label :global(svg) {
    display: inline;
  }

  input, select, textarea {
    width: 100%;
  }

  textarea {
    resize: vertical;
    min-height: 72px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }

  .path-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .path-row input {
    flex: 1;
    min-width: 0;
  }

  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 12px;
  }

  .chip {
    padding: 7px 12px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .chip:hover {
    color: var(--text-primary);
    border-color: var(--accent-primary);
  }

  .chip.active {
    color: var(--text-primary);
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 1px var(--accent-primary);
  }

  .res-custom {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 10px;
    align-items: end;
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
  .hint { margin: 0 0 12px; color: var(--text-muted); font-size: 12px; line-height: 1.4; }
  .check-row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-secondary);
    margin-bottom: 12px;
    cursor: pointer;
    font-weight: 500;
  }
  .check-row input { accent-color: var(--accent-primary); width: auto; }
  .test-ok { color: var(--accent-primary); font-size: 11px; }
  .notice { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-radius: 10px; margin-bottom: 12px; border: 1px solid var(--border-color); font-size: 12px; }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .inline-status { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 12px; margin-bottom: 10px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 640px) {
    .res-custom {
      grid-template-columns: 1fr;
    }
  }
</style>
