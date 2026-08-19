<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import {
    Palette, Info, Command, Plug, KeyRound, CheckCircle2, AlertTriangle, Loader2,
    Bot, Network, Coffee, Terminal, HardDrive, Settings2,
    MessageCircle, ExternalLink,
  } from "@lucide/svelte";
  import { api } from "../lib/api";
  import type { PresenceSettings, LauncherSettings, SidebarMode, UiScaleMode } from "../lib/store";
  import {
    autoHideWorkflowRail,
    sidebarMode,
    normalizeSidebarMode,
    applyUiScaleFromSettings,
    applyRoundedCorners,
    normalizeUiScalePercent,
    resolveUiScaleMode,
    suggestUiScalePercent,
    UI_SCALE_STEPS,
    notifyLauncherSettingsChanged,
    brandIcon,
    BRAND_ICON_CREEPER_SRC,
    BRAND_ICON_CREEPER_SRC_SM,
    settingsNavRequest,
    type BrandIconId,
  } from "../lib/store";
  import ConfettiBurst from "./ConfettiBurst.svelte";
  import {
    readStoredTheme, commitTheme, type ThemeId,
  } from "../lib/themes";
  import AiSettingsPanel from "./AiSettingsPanel.svelte";
  import ThemePicker from "./ThemePicker.svelte";
  import JavaPickerModal from "./JavaPickerModal.svelte";
  import { copyText } from "../lib/clipboard";

  type SettingsTab = "appearance" | "launcher" | "ai" | "integrations" | "about";
  let tab = $state<SettingsTab>("appearance");
  let launcherSub = $state<"general" | "java" | "commands" | "runtime">("general");

  $effect(() => {
    const req = $settingsNavRequest;
    if (!req) return;
    tab = req.tab;
    if (req.tab === "launcher" && req.launcherSub) {
      launcherSub = req.launcherSub;
    }
    settingsNavRequest.set(null);
  });

  type AiSettings = {
    provider: string;
    endpoint: string;
    model: string;
    diagnoseMode?: string;
    crashKbEndpoint?: string;
    ollamaBinaryPath?: string;
    ollamaModelsPath?: string;
    speculativeDecoding?: boolean;
    draftModel?: string;
  };
  type SwarmSettings = {
    enabled?: boolean;
    onboardingDone?: boolean;
    sharePromptsEnabled?: boolean;
    supabaseUrl?: string;
    hubUrl?: string;
    p2pEnabled?: boolean;
    p2pControlUrl?: string;
    p2pBootstrap?: string;
    p2pRelayServer?: boolean;
    volunteerDiagnose?: boolean;
    creationWorker?: boolean;
    advertisedVramMb?: number;
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

  async function ipc<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    if (!isTauri()) {
      throw new Error("Desktop IPC unavailable. Run the Tauri app, not the browser preview.");
    }
    return invoke<T>(cmd, args);
  }

  let theme = $state<ThemeId>(readStoredTheme());
  let reducedMotion = $state(localStorage.getItem("tuffbox-reduced-motion") === "1");
  let shortcuts = $state<any[]>([]);
  let shortcutsOpen = $state(false);
  let appVersion = $state("");
  let updateCheck = $state<UpdateCheck | null>(null);
  let updateError = $state("");
  let updateLoading = $state(false);

  let integrationsLoading = $state(false);
  let integrationsError = $state("");
  let integrationsMessage = $state("");
  let githubRepository = $state("");
  let aiProvider = $state<"ollama" | "openai-compatible">("ollama");
  let aiEndpoint = $state("");
  let aiModel = $state("");
  let ollamaBinaryPath = $state("");
  let ollamaModelsPath = $state("");
  let diagnoseMode = $state<"server" | "local" | "kb_only">("server");
  let crashKbEndpoint = $state("");
  let githubTokenSet = $state(false);
  let modrinthTokenSet = $state(false);
  let curseforgeTokenSet = $state(false);
  let aiApiKeySet = $state(false);
  let crashKbTokenSet = $state(false);

  let githubTokenDraft = $state("");
  let modrinthTokenDraft = $state("");
  let curseforgeTokenDraft = $state("");
  let aiApiKeyDraft = $state("");
  let crashKbTokenDraft = $state("");
  let swarmEnabled = $state(false);
  let swarmSharePrompts = $state(true);
  let swarmSupabaseUrl = $state("");
  let swarmSupabaseAnonDraft = $state("");
  let swarmSupabaseAnonSet = $state(false);
  let swarmSupabaseUsingBuiltin = $state(true);
  let swarmSupabaseConfigured = $state(false);
  /** Collapsed power-user swarm fields (hub, control URL, relay, VRAM, Supabase override). */
  let swarmAdvanced = $state(false);
  let swarmHubUrl = $state("");
  let swarmP2pEnabled = $state(false);
  let swarmP2pControlUrl = $state("http://127.0.0.1:8790");
  let swarmP2pBootstrap = $state("");
  let swarmP2pListenAddrs = $state<string[]>([]);
  let swarmP2pCopyMsg = $state("");
  let swarmP2pStatus = $state("");
  let swarmP2pHint = $state("");
  let swarmP2pRelayStatus = $state("");
  let swarmP2pGossipStatus = $state("");
  let swarmP2pWorkerStubStatus = $state("");
  let swarmP2pRelayServer = $state(false);
  let swarmVolunteerDiagnose = $state(false);
  let swarmCreationWorker = $state(false);
  let swarmAdvertisedVramMb = $state(0);
  let swarmSaving = $state(false);

  let savingSettings = $state(false);
  let savingSecret = $state<string | null>(null);
  let clearingSecret = $state<string | null>(null);
  let testingProvider = $state<string | null>(null);
  let testResults = $state<Record<string, string>>({});

  let discordRpcEnabled = $state(false);
  let discordClientId = $state("");
  let discordSaving = $state(false);
  let discordMessage = $state("");
  let discordError = $state("");

  // Launcher settings
  let launcher = $state<LauncherSettings>({
    theme: "tuffbox",
    potatoPc: false,
    perfAutoDetected: false,
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
    youtubeInlinePlayer: true,
    showYoutubeOnHome: false,
    ingameOverlay: true,
    autoHideWorkflowRail: false,
    sidebarMode: "full",
    uiScalePercent: 100,
    uiScaleMode: "auto",
    roundedCorners: true,
    hideInstanceHome: false,
  });
  let launcherSaving = $state(false);
  let launcherMsg = $state("");
  let launcherErr = $state("");
  let defaultRuntimePath = $state("");
  let runtimeDraft = $state("");
  let defaultInstancesPath = $state("");
  let instancesDraft = $state("");
  let showJavaPicker = $state(false);
  let resMode = $state<"default" | "854x480" | "1280x720" | "1920x1080" | "custom">("default");
  let customW = $state(1280);
  let customH = $state(720);
  let discordDirty = $state(false);
  let brandConfetti = $state(false);

  const concurrentOptions = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32];
  const concurrentSelectOptions = $derived(
    concurrentOptions.includes(launcher.concurrentDownloads)
      ? concurrentOptions
      : [...concurrentOptions, launcher.concurrentDownloads].sort((a, b) => a - b),
  );

  const tabs: { id: SettingsTab; label: string; icon: typeof Palette }[] = [
    { id: "appearance", label: "Appearance", icon: Palette },
    { id: "launcher", label: "Launcher", icon: Settings2 },
    { id: "ai", label: "AI", icon: Bot },
    { id: "integrations", label: "Integrations", icon: Plug },
    { id: "about", label: "About", icon: Info },
  ];

  const launcherSubs: { id: typeof launcherSub; label: string }[] = [
    { id: "general", label: "General" },
    { id: "java", label: "Java" },
    { id: "commands", label: "Commands" },
    { id: "runtime", label: "Paths" },
  ];

  function syncResModeFromLauncher() {
    const r = launcher.gameResolution;
    if (!r) {
      resMode = "default";
      return;
    }
    if (r.width === 1920 && r.height === 1080) resMode = "1920x1080";
    else if (r.width === 1280 && r.height === 720) resMode = "1280x720";
    else if (r.width === 854 && r.height === 480) resMode = "854x480";
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
      launcher = {
        ...launcher,
        uiScaleMode: resolveUiScaleMode(launcher),
      };
      theme = (THEMES_SAFE(launcher.theme) as ThemeId) || readStoredTheme();
      reducedMotion = !!launcher.potatoPc;
      applyPotatoPc(reducedMotion);
      localStorage.setItem("tuffbox-reduced-motion", reducedMotion ? "1" : "0");
      commitTheme(theme);
      autoHideWorkflowRail.set(!!launcher.autoHideWorkflowRail);
      sidebarMode.set(normalizeSidebarMode(launcher.sidebarMode));
      const applied = applyUiScaleFromSettings(launcher);
      launcher = { ...launcher, uiScalePercent: applied };
      applyRoundedCorners(launcher.roundedCorners !== false);
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
    const ok = [
      "tuffbox",
      "tuffbox-light",
      "carbon",
      "inferno",
      "aether",
      "frost",
      "pixelato",
      "win95",
      "solar",
      "fern",
      "blaze",
      "dusk",
      "glacier",
      "minecraft",
    ];
    if (id === "dark") return "tuffbox";
    if (id === "light") return "tuffbox-light";
    return ok.includes(id) ? id : "tuffbox";
  }

  async function persistLauncher(partial?: Partial<LauncherSettings>) {
    launcherSaving = true;
    launcherErr = "";
    launcherMsg = "";
    const prev = launcher;
    try {
      const next: LauncherSettings = { ...launcher, ...partial };
      if (partial?.theme) {
        theme = THEMES_SAFE(partial.theme) as ThemeId;
        commitTheme(theme);
        next.theme = theme;
      }
      if (partial && "uiScalePercent" in partial) {
        next.uiScalePercent = normalizeUiScalePercent(partial.uiScalePercent);
        if (!("uiScaleMode" in (partial ?? {}))) {
          next.uiScaleMode = "manual";
        }
      }
      if (partial && "uiScaleMode" in partial) {
        next.uiScaleMode = resolveUiScaleMode({
          uiScaleMode: partial.uiScaleMode,
          uiScalePercent: next.uiScalePercent,
        });
        if (next.uiScaleMode === "auto") {
          next.uiScalePercent = suggestUiScalePercent();
        }
      }
      // Optimistic: chips + zoom update before disk round-trip.
      launcher = next;
      if (partial && ("uiScalePercent" in partial || "uiScaleMode" in partial)) {
        const applied = applyUiScaleFromSettings(next);
        launcher = { ...launcher, uiScalePercent: applied };
        next.uiScalePercent = applied;
      }
      if (partial && "autoHideWorkflowRail" in partial) {
        autoHideWorkflowRail.set(!!next.autoHideWorkflowRail);
      }
      if (partial && "sidebarMode" in partial) {
        sidebarMode.set(normalizeSidebarMode(next.sidebarMode));
      }
      if (partial && "roundedCorners" in partial) {
        applyRoundedCorners(next.roundedCorners !== false);
      }

      const saved = await api.launcher.save(next);
      launcher = {
        ...saved,
        uiScaleMode: resolveUiScaleMode(saved),
        uiScalePercent: normalizeUiScalePercent(saved.uiScalePercent),
      };
      if (partial && ("uiScalePercent" in partial || "uiScaleMode" in partial)) {
        const applied = applyUiScaleFromSettings(launcher);
        launcher = { ...launcher, uiScalePercent: applied };
      }
      if (partial && "autoHideWorkflowRail" in partial) {
        autoHideWorkflowRail.set(!!launcher.autoHideWorkflowRail);
      }
      if (partial && "sidebarMode" in partial) {
        sidebarMode.set(normalizeSidebarMode(launcher.sidebarMode));
      }
      if (partial && "roundedCorners" in partial) {
        applyRoundedCorners(launcher.roundedCorners !== false);
      }
      notifyLauncherSettingsChanged(launcher);
      launcherMsg = "Saved.";
      setTimeout(() => (launcherMsg = ""), 1600);
    } catch (e) {
      launcher = prev;
      if (partial && ("uiScalePercent" in partial || "uiScaleMode" in partial)) {
        applyUiScaleFromSettings(prev);
      }
      launcherErr = String(e);
    } finally {
      launcherSaving = false;
    }
  }

  function onThemeChange(id: ThemeId) {
    theme = id;
    void persistLauncher({ theme: id });
  }

  async function loadPresence() {
    discordError = "";
    try {
      const s = await api.presence.get();
      discordRpcEnabled = !!s.discordRpcEnabled;
      discordClientId = s.discordClientId ?? "";
      discordDirty = false;
    } catch (e) {
      discordError = String(e);
    }
  }

  function applyResolution(mode: typeof resMode) {
    resMode = mode;
    if (mode === "default") void persistLauncher({ gameResolution: null });
    else if (mode === "1920x1080") void persistLauncher({ gameResolution: { width: 1920, height: 1080 } });
    else if (mode === "1280x720") void persistLauncher({ gameResolution: { width: 1280, height: 720 } });
    else if (mode === "854x480") void persistLauncher({ gameResolution: { width: 854, height: 480 } });
    else {
      const width = Math.min(7680, Math.max(640, Math.floor(Number(customW) || 1280)));
      const height = Math.min(4320, Math.max(480, Math.floor(Number(customH) || 720)));
      customW = width;
      customH = height;
      void persistLauncher({ gameResolution: { width, height } });
    }
  }

  function onConcurrentChange(ev: Event) {
    const el = ev.currentTarget as HTMLSelectElement;
    const n = Math.min(32, Math.max(1, Number(el.value) || 8));
    launcher.concurrentDownloads = n;
    void persistLauncher({ concurrentDownloads: n });
  }

  async function savePresence(): Promise<boolean> {
    discordSaving = true;
    discordError = "";
    discordMessage = "";
    try {
      const settings: PresenceSettings = {
        discordRpcEnabled,
        discordClientId: discordClientId.trim(),
      };
      if (settings.discordRpcEnabled && !settings.discordClientId) {
        throw new Error("Paste a Discord Application Client ID before enabling Rich Presence.");
      }
      await api.presence.save(settings);
      discordDirty = false;
      discordMessage = settings.discordRpcEnabled
        ? "Discord Rich Presence enabled."
        : "Discord Rich Presence saved.";
      setTimeout(() => {
        if (discordMessage.startsWith("Discord Rich Presence")) discordMessage = "";
      }, 2200);
      return true;
    } catch (e) {
      discordError = String(e);
      return false;
    } finally {
      discordSaving = false;
    }
  }

  async function onDiscordToggle() {
    const next = !discordRpcEnabled;
    if (next && !discordClientId.trim()) {
      discordError = "Paste a Discord Application Client ID before enabling Rich Presence.";
      discordMessage = "";
      return;
    }
    const prev = discordRpcEnabled;
    discordRpcEnabled = next;
    const ok = await savePresence();
    if (!ok) discordRpcEnabled = prev;
  }

  async function openDiscordPortal() {
    try {
      await openShell("https://discord.com/developers/applications");
    } catch (e) {
      discordError = String(e);
    }
  }

  async function checkUpdate() {
    updateLoading = true;
    updateError = "";
    updateCheck = null;
    try {
      updateCheck = await ipc<UpdateCheck>("check_for_app_update");
    } catch (e) {
      updateError = String(e);
    } finally {
      updateLoading = false;
    }
  }

  async function loadAppVersion() {
    try {
      appVersion = await ipc<string>("get_app_version");
    } catch {
      appVersion = "";
    }
  }

  async function loadIntegrations() {
    integrationsLoading = true;
    integrationsError = "";
    try {
      const status = await ipc<IntegrationStatus>("get_integration_status");
      githubRepository = status.settings?.githubRepository ?? "";
      aiProvider = (status.settings?.ai?.provider === "openai-compatible" ? "openai-compatible" : "ollama");
      aiEndpoint = status.settings?.ai?.endpoint ?? "";
      aiModel = status.settings?.ai?.model ?? "";
      ollamaBinaryPath = status.settings?.ai?.ollamaBinaryPath ?? "";
      ollamaModelsPath = status.settings?.ai?.ollamaModelsPath ?? "";
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
      swarmVolunteerDiagnose = !!status.settings?.swarm?.volunteerDiagnose;
      swarmCreationWorker = !!status.settings?.swarm?.creationWorker;
      swarmAdvertisedVramMb = Number(status.settings?.swarm?.advertisedVramMb ?? 0) || 0;
      swarmP2pRelayServer = !!status.settings?.swarm?.p2pRelayServer;
      swarmP2pControlUrl =
        status.settings?.swarm?.p2pControlUrl?.trim() || "http://127.0.0.1:8790";
      swarmP2pBootstrap = status.settings?.swarm?.p2pBootstrap?.trim() || "";
      githubTokenDraft = "";
      modrinthTokenDraft = "";
      curseforgeTokenDraft = "";
      aiApiKeyDraft = "";
      crashKbTokenDraft = "";
      swarmSupabaseAnonDraft = "";
      if (swarmEnabled && swarmP2pEnabled) {
        void refreshP2pStatus();
      } else if (swarmEnabled && !swarmP2pEnabled) {
        swarmP2pStatus = "";
        swarmP2pHint = "";
        swarmP2pRelayStatus = "";
        swarmP2pGossipStatus = "";
        swarmP2pWorkerStubStatus = "";
        swarmP2pListenAddrs = [];
        swarmP2pCopyMsg = "";
      } else {
        swarmP2pStatus = "";
        swarmP2pHint = "";
        swarmP2pRelayStatus = "";
        swarmP2pGossipStatus = "";
        swarmP2pWorkerStubStatus = "";
        swarmP2pListenAddrs = [];
        swarmP2pCopyMsg = "";
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
      // Preserve AI settings (edited on the AI tab) while saving integrations/swarm.
      const status = await ipc<IntegrationStatus>("get_integration_status");
      await ipc("save_integration_settings", {
        settings: {
          githubRepository: githubRepository.trim(),
          ai: status.settings?.ai ?? {
            provider: aiProvider,
            endpoint: aiEndpoint.trim(),
            model: aiModel.trim(),
            diagnoseMode,
            crashKbEndpoint: crashKbEndpoint.trim(),
            ollamaBinaryPath: ollamaBinaryPath.trim(),
            ollamaModelsPath: ollamaModelsPath.trim(),
          },
          swarm: {
            enabled: swarmEnabled,
            onboardingDone: true,
            sharePromptsEnabled: swarmSharePrompts,
            supabaseUrl: swarmSupabaseUrl.trim(),
            hubUrl: swarmHubUrl.trim(),
            p2pEnabled: swarmP2pEnabled,
            p2pControlUrl: swarmP2pControlUrl.trim() || "http://127.0.0.1:8790",
            p2pBootstrap: swarmP2pBootstrap.trim(),
            p2pRelayServer: swarmP2pRelayServer,
            volunteerDiagnose: swarmVolunteerDiagnose,
            creationWorker: swarmCreationWorker,
            advertisedVramMb: swarmAdvertisedVramMb,
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
      await ipc("set_integration_secret", { kind, value: value.trim() });
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
      await ipc("clear_integration_secret", { kind });
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
      const result = await ipc<string>("test_integration", { provider });
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
      const s = await ipc<SwarmSettings>("set_swarm_enabled", { enabled: next });
      swarmEnabled = !!s.enabled;
      integrationsMessage = swarmEnabled ? "TuffSwarm on" : "TuffSwarm off";
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
      const s = await ipc<SwarmSettings>("set_swarm_share_prompts", { enabled: next });
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
      const s = await ipc<SwarmSettings>("set_swarm_supabase_url", {
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
      const s = await ipc<SwarmSettings>("set_swarm_p2p", {
        enabled: next,
        controlUrl: swarmP2pControlUrl.trim() || null,
        bootstrap: swarmP2pBootstrap.trim() || null,
      });
      swarmP2pEnabled = !!s.p2pEnabled;
      swarmP2pControlUrl = s.p2pControlUrl?.trim() || "http://127.0.0.1:8790";
      swarmP2pBootstrap = s.p2pBootstrap?.trim() || "";
      if (swarmP2pEnabled) {
        await ensureP2pNode();
      } else {
        const vol = await ipc<SwarmSettings>("set_swarm_volunteer_diagnose", {
          enabled: false,
        });
        swarmVolunteerDiagnose = !!vol.volunteerDiagnose;
        const cre = await ipc<SwarmSettings>("set_swarm_creation_worker", {
          enabled: false,
        });
        swarmCreationWorker = !!cre.creationWorker;
        const rel = await ipc<SwarmSettings>("set_swarm_p2p_relay_server", {
          enabled: false,
        });
        swarmP2pRelayServer = !!rel.p2pRelayServer;
        swarmP2pStatus = "";
        swarmP2pHint = "";
        swarmP2pRelayStatus = "";
        swarmP2pGossipStatus = "";
        swarmP2pWorkerStubStatus = "";
        swarmP2pListenAddrs = [];
        swarmP2pCopyMsg = "";
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function toggleVolunteerDiagnose() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const next = !swarmVolunteerDiagnose;
      const s = await ipc<SwarmSettings>("set_swarm_volunteer_diagnose", {
        enabled: next,
      });
      swarmVolunteerDiagnose = !!s.volunteerDiagnose;
      if (swarmP2pEnabled) {
        await ipc("restart_p2p_node");
        await refreshP2pStatus();
        integrationsMessage = swarmVolunteerDiagnose ? "Fog on · node restarted" : "Fog off · node restarted";
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function toggleCreationWorker() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const next = !swarmCreationWorker;
      const s = await ipc<SwarmSettings>("set_swarm_creation_worker", {
        enabled: next,
      });
      swarmCreationWorker = !!s.creationWorker;
      if (swarmP2pEnabled) {
        await ipc("restart_p2p_node");
        await refreshP2pStatus();
        integrationsMessage = swarmCreationWorker
          ? "Creation on · node restarted"
          : "Creation off · node restarted";
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function applyAdvertisedVramMb(raw: string) {
    const parsed = Math.max(0, Math.floor(Number(raw) || 0));
    if (parsed === swarmAdvertisedVramMb) return;
    swarmSaving = true;
    integrationsError = "";
    try {
      const s = await ipc<SwarmSettings>("set_swarm_advertised_vram_mb", {
        vramMb: parsed,
      });
      swarmAdvertisedVramMb = Number(s.advertisedVramMb ?? 0) || 0;
      if (swarmP2pEnabled) {
        await ipc("restart_p2p_node");
        await refreshP2pStatus();
        integrationsMessage = `VRAM ${swarmAdvertisedVramMb} MB · node restarted`;
      } else {
        integrationsMessage = `VRAM ${swarmAdvertisedVramMb} MB`;
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function toggleP2pRelayServer() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const next = !swarmP2pRelayServer;
      const s = await ipc<SwarmSettings>("set_swarm_p2p_relay_server", {
        enabled: next,
      });
      swarmP2pRelayServer = !!s.p2pRelayServer;
      if (swarmP2pEnabled) {
        await ipc("restart_p2p_node");
        await refreshP2pStatus();
        integrationsMessage = swarmP2pRelayServer ? "Relay on · node restarted" : "Relay off · node restarted";
      }
    } catch (e) {
      integrationsError = String(e);
    } finally {
      swarmSaving = false;
    }
  }

  async function refreshP2pStatus() {
    try {
      if (!swarmEnabled) {
        swarmP2pStatus = "";
        swarmP2pHint = "";
        swarmP2pRelayStatus = "";
        swarmP2pGossipStatus = "";
        swarmP2pWorkerStubStatus = "";
        swarmP2pListenAddrs = [];
        swarmP2pCopyMsg = "";
        return;
      }
      const st = await ipc<{
        enabled?: boolean;
        healthy?: boolean;
        authorized?: boolean;
        controlUrl?: string;
        node?: {
          peers?: number;
          capsuleCount?: number;
          creationPeers?: string[];
          volunteerPeers?: string[];
          listenAddrs?: string[];
          relayServer?: boolean;
          circuitListenAddrs?: string[];
          gossipPublished?: number;
          gossipReceived?: number;
          gossipLastError?: string;
          vramMb?: number;
          maxJobs?: number;
        };
      }>("get_p2p_node_status");
      if (!st.enabled) {
        swarmP2pStatus = "P2P off";
        swarmP2pHint = "";
        swarmP2pRelayStatus = "";
        swarmP2pGossipStatus = "";
        swarmP2pWorkerStubStatus = "";
        swarmP2pListenAddrs = [];
        swarmP2pCopyMsg = "";
        return;
      }

      // Prefer live control URL from the sidecar when attached.
      const liveControl = st.controlUrl?.trim();
      if (liveControl) {
        swarmP2pControlUrl = liveControl;
      }

      const peers = st.node?.peers ?? 0;
      const caps = st.node?.capsuleCount ?? 0;
      const creationCount = (st.node?.creationPeers ?? []).length;
      const volunteerCount = (st.node?.volunteerPeers ?? []).length;
      const circuitAddrs = st.node?.circuitListenAddrs ?? [];
      const relayOn = !!st.node?.relayServer;
      const gossipPub = st.node?.gossipPublished ?? 0;
      const gossipRecv = st.node?.gossipReceived ?? 0;
      const gossipErr = (st.node?.gossipLastError ?? "").trim();
      const nodeVramMb = Number(st.node?.vramMb ?? swarmAdvertisedVramMb) || 0;

      const raw = st.node?.listenAddrs ?? [];
      const preferred = raw.filter(
        (a) =>
          !a.includes("/ip4/127.0.0.1/") &&
          !a.includes("/ip6/::1/") &&
          !a.includes("/ip4/0.0.0.0/") &&
          !a.includes("p2p-circuit"),
      );
      swarmP2pListenAddrs = (preferred.length ? preferred : raw.filter((a) => !a.includes("p2p-circuit"))).slice(0, 2);

      if (st.authorized === false) {
        swarmP2pStatus = "Unauthorized — Start / attach";
      } else if (st.healthy) {
        swarmP2pStatus = `Online · ${peers} peers · ${caps} capsules`;
      } else {
        swarmP2pStatus = "Node offline";
      }

      // Compact extras — only when Advanced is open or something noteworthy.
      if (relayOn) {
        swarmP2pRelayStatus = "Relay on";
      } else if (circuitAddrs.length > 0) {
        swarmP2pRelayStatus = `Circuit · ${circuitAddrs.length}`;
      } else {
        swarmP2pRelayStatus = "";
      }

      if (st.healthy && st.authorized !== false && (gossipPub || gossipRecv || gossipErr)) {
        swarmP2pGossipStatus = gossipErr
          ? `Gossip ${gossipPub}/${gossipRecv} · ${gossipErr}`
          : `Gossip ${gossipPub}/${gossipRecv}`;
      } else {
        swarmP2pGossipStatus = "";
      }

      swarmP2pWorkerStubStatus =
        swarmCreationWorker && st.healthy && st.authorized !== false && nodeVramMb > 0
          ? `VRAM ${nodeVramMb} MB`
          : "";

      const hints: string[] = [];
      if (swarmCreationWorker && creationCount === 0 && st.healthy) {
        hints.push("No Creation peers yet");
      }
      if (swarmVolunteerDiagnose && volunteerCount === 0 && st.healthy) {
        hints.push("No Fog peers yet");
      }
      swarmP2pHint = hints.join(" · ");
    } catch (e) {
      swarmP2pStatus = String(e);
      swarmP2pHint = "";
      swarmP2pRelayStatus = "";
      swarmP2pGossipStatus = "";
      swarmP2pWorkerStubStatus = "";
      swarmP2pListenAddrs = [];
      swarmP2pCopyMsg = "";
    }
  }

  async function copyP2pListenAddr(addr: string) {
    try {
      await copyText(addr);
      swarmP2pCopyMsg = "Copied";
    } catch {
      swarmP2pCopyMsg = "Copy failed";
    }
  }

  async function ensureP2pNode() {
    swarmSaving = true;
    integrationsError = "";
    try {
      const s = await ipc<SwarmSettings>("set_swarm_p2p", {
        enabled: true,
        controlUrl: swarmP2pControlUrl.trim() || null,
        bootstrap: swarmP2pBootstrap.trim(),
      });
      swarmP2pEnabled = !!s.p2pEnabled;
      swarmP2pControlUrl = s.p2pControlUrl?.trim() || "http://127.0.0.1:8790";
      swarmP2pBootstrap = s.p2pBootstrap?.trim() || "";
      await ipc("ensure_p2p_node");
      await refreshP2pStatus();
      integrationsMessage = "P2P node attached";
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
    try { shortcuts = await ipc("get_keyboard_shortcuts"); } catch {}
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

  function selectBrandIcon(id: BrandIconId) {
    const prev = $brandIcon;
    brandIcon.set(id);
    if (id === "creeper" && prev !== "creeper" && !reducedMotion) {
      brandConfetti = true;
    }
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
      {@const Icon = t.icon}
      <button
        type="button"
        class="tab press-effect"
        class:active={tab === t.id}
        onclick={() => (tab = t.id)}
      >
        <Icon size={16} />
        {t.label}
      </button>
    {/each}
  </nav>

  {#if tab === "launcher"}
    <nav class="launcher-subnav" aria-label="Launcher settings">
      {#each launcherSubs as s (s.id)}
        <button
          type="button"
          class="launcher-sub press-effect"
          class:active={launcherSub === s.id}
          onclick={() => (launcherSub = s.id)}
        >
          {s.label}
        </button>
      {/each}
    </nav>
  {/if}

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
          <input type="checkbox" checked={reducedMotion} onchange={toggleReducedMotion} />
          Potato PC mode (reduce motion / animations)
        </label>
        <p class="hint">Disables CSS animations and transitions for weaker machines.</p>

        <label class="check-row" style="margin-top: 14px;">
          <input
            type="checkbox"
            checked={launcher.roundedCorners !== false}
            disabled={launcherSaving}
            onchange={(e) => void persistLauncher({ roundedCorners: e.currentTarget.checked })}
          />
          Rounded corners
        </label>
        <p class="hint">Round edges on panels, cards, modals, and chrome — works with every theme.</p>

        <div class="settings-row" style="margin-top: 18px;">
          <div class="settings-row-text">
            <strong>Interface scale</strong>
            <p>
              Zoom the whole UI — buttons, sidebar, Content mod cards, dialogs.
              <strong>Auto</strong> picks a size from your screen and window; pick a percent to lock it.
            </p>
            <p class="hint" style="margin-top: 6px;">
              Suggested for this screen: {suggestUiScalePercent()}%
              {#if resolveUiScaleMode(launcher) === "auto"}
                · following window size
              {/if}
            </p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row scale-chips">
              <button
                type="button"
                class="chip press-effect"
                class:active={resolveUiScaleMode(launcher) === "auto"}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ uiScaleMode: "auto" as UiScaleMode })}
              >
                {#if resolveUiScaleMode(launcher) === "auto"}
                  Auto · {normalizeUiScalePercent(launcher.uiScalePercent)}%
                {:else}
                  Auto
                {/if}
              </button>
              {#each UI_SCALE_STEPS as pct (pct)}
                <button
                  type="button"
                  class="chip press-effect"
                  class:active={resolveUiScaleMode(launcher) === "manual" && normalizeUiScalePercent(launcher.uiScalePercent) === pct}
                  disabled={launcherSaving}
                  onclick={() => void persistLauncher({ uiScaleMode: "manual" as UiScaleMode, uiScalePercent: pct })}
                >
                  {pct}%
                </button>
              {/each}
            </div>
          </div>
        </div>
      </section>
    {/if}

    {#if tab === "launcher" && launcherSub === "general"}
      <section class="card card-wide">
        <div class="card-title">
          <Settings2 size={18} />
          <h3>General</h3>
        </div>

        <div class="settings-row">
          <div class="settings-row-text">
            <strong>YouTube feed on home</strong>
            <p>
              Minecraft YouTube strip on the home screen. Hidden by default — turn it on here.
            </p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row tight">
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.showYoutubeOnHome === true}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ showYoutubeOnHome: true })}
              >
                Shown
              </button>
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.showYoutubeOnHome !== true}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ showYoutubeOnHome: false })}
              >
                Hidden
              </button>
            </div>
          </div>
        </div>

        {#if launcher.showYoutubeOnHome}
        <div class="settings-row">
          <div class="settings-row-text">
            <strong>YouTube player</strong>
            <p>
              Litube-style in-app player loads a privacy embed only after you click a thumbnail.
              Preview-only keeps static images and opens videos in the system browser.
            </p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row tight">
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.youtubeInlinePlayer !== false}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ youtubeInlinePlayer: true })}
              >
                In-app player
              </button>
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.youtubeInlinePlayer === false}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ youtubeInlinePlayer: false })}
              >
                Preview only
              </button>
            </div>
          </div>
        </div>
        {/if}

        <div class="settings-row">
          <div class="settings-row-text">
            <strong>In-game overlay</strong>
            <p>
              F8 fullscreen overlay (OpenGL hook) — any MC version / loader. Friends, chat,
              YouTube feed via launcher IPC. Place <code>mpv-2.dll</code> next to the hook for video.
            </p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row tight">
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.ingameOverlay !== false}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ ingameOverlay: true })}
              >
                Enabled
              </button>
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.ingameOverlay === false}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ ingameOverlay: false })}
              >
                Disabled
              </button>
            </div>
          </div>
        </div>

        <div class="settings-row">
          <div class="settings-row-text">
            <strong>Dynamic bottom panel</strong>
            <p>
              Hide the IDE workflow rail (Content, Setup, …). Move the cursor to the bottom edge of
              the window to slide it out quickly; it hides again when you leave.
            </p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row tight">
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.autoHideWorkflowRail === true}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ autoHideWorkflowRail: true })}
              >
                Auto-hide
              </button>
              <button
                type="button"
                class="chip press-effect"
                class:active={launcher.autoHideWorkflowRail !== true}
                disabled={launcherSaving}
                onclick={() => void persistLauncher({ autoHideWorkflowRail: false })}
              >
                Always visible
              </button>
            </div>
          </div>
        </div>

        <div class="settings-row">
          <div class="settings-row-text">
            <strong>Concurrent downloads</strong>
            <p>How many files to fetch in parallel when installing mods or updating the instance.</p>
          </div>
          <div class="settings-row-control">
            <select
              class="control-select"
              value={String(launcher.concurrentDownloads)}
              onchange={onConcurrentChange}
              disabled={launcherSaving}
              aria-label="Concurrent downloads"
            >
              {#each concurrentSelectOptions as n (n)}
                <option value={String(n)}>{n}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="settings-row" class:settings-row-stack={resMode === "custom"}>
          <div class="settings-row-text">
            <strong>Game resolution</strong>
            <p>Window size passed to Minecraft on launch. Leave Default to use the game’s own setting.</p>
          </div>
          <div class="settings-row-control">
            <div class="chip-row tight">
              <button type="button" class="chip press-effect" class:active={resMode === "default"} disabled={launcherSaving} onclick={() => applyResolution("default")}>Default</button>
              <button type="button" class="chip press-effect" class:active={resMode === "854x480"} disabled={launcherSaving} onclick={() => applyResolution("854x480")}>854×480</button>
              <button type="button" class="chip press-effect" class:active={resMode === "1280x720"} disabled={launcherSaving} onclick={() => applyResolution("1280x720")}>720p</button>
              <button type="button" class="chip press-effect" class:active={resMode === "1920x1080"} disabled={launcherSaving} onclick={() => applyResolution("1920x1080")}>1080p</button>
              <button type="button" class="chip press-effect" class:active={resMode === "custom"} disabled={launcherSaving} onclick={() => (resMode = "custom")}>Custom</button>
            </div>
            {#if resMode === "custom"}
              <div class="res-custom">
                <label class="field-inline">
                  Width
                  <input type="number" min="640" max="7680" step="1" bind:value={customW} />
                </label>
                <label class="field-inline">
                  Height
                  <input type="number" min="480" max="4320" step="1" bind:value={customH} />
                </label>
                <button type="button" class="secondary" onclick={() => applyResolution("custom")} disabled={launcherSaving}>
                  Apply
                </button>
              </div>
            {/if}
          </div>
        </div>

        <div class="settings-row settings-row-stack">
          <div class="settings-row-text">
            <strong>Discord Rich Presence</strong>
            <p>Show what you’re playing in Discord while Minecraft is running. Needs an Application Client ID from the Discord Developer Portal.</p>
          </div>
          <div class="settings-row-control discord-block">
            {#if discordError}<div class="notice error compact"><AlertTriangle size={14} /> {discordError}</div>{/if}
            {#if discordMessage}<div class="notice success compact"><CheckCircle2 size={14} /> {discordMessage}</div>{/if}
            <label class="check-row">
              <input
                type="checkbox"
                checked={discordRpcEnabled}
                disabled={discordSaving}
                onchange={onDiscordToggle}
              />
              Enable Rich Presence
            </label>
            <label class="field-inline">
              Application Client ID
              <div class="path-row">
                <input
                  bind:value={discordClientId}
                  placeholder="Application ID from Discord Developer Portal"
                  autocomplete="off"
                  disabled={discordSaving}
                  oninput={() => (discordDirty = true)}
                />
                <button type="button" class="ghost mini" onclick={openDiscordPortal} title="Open Discord Developer Portal">
                  <ExternalLink size={14} /> Portal
                </button>
              </div>
            </label>
            <div class="row-actions">
              <button type="button" onclick={savePresence} disabled={discordSaving || !discordDirty}>
                {#if discordSaving}
                  <Loader2 size={14} class="spin" /> Saving…
                {:else}
                  <MessageCircle size={14} /> Save presence
                {/if}
              </button>
            </div>
            <p class="hint flat">Optional: upload a large image asset named <code>tuffbox</code> in the Discord app for a richer status card.</p>
          </div>
        </div>

        <div class="settings-row settings-row-stack">
          <div class="settings-row-text">
            <strong>Keyboard shortcuts</strong>
            <p>Built-in hotkeys for navigating TuffBox.</p>
          </div>
          <div class="settings-row-control">
            <button type="button" class="ghost" onclick={() => (shortcutsOpen = !shortcutsOpen)}>
              <Command size={14} />
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
          </div>
        </div>
      </section>
    {/if}

    {#if tab === "launcher" && launcherSub === "java"}
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
            <button type="button" class="secondary" onclick={() => (showJavaPicker = true)}>Browse…</button>
          </div>
        </label>
        <label>
          Custom JVM arguments
          <textarea
            rows="3"
            bind:value={launcher.javaCustomArgs}
            placeholder="-XX:+UseG1GC …"
            onblur={() => persistLauncher({ javaCustomArgs: launcher.javaCustomArgs?.trim() || null })}
          ></textarea>
        </label>
        <label>
          Default memory (MB)
          <input
            type="number"
            min="512"
            step="256"
            bind:value={launcher.defaultMemoryMb}
            onchange={() =>
              persistLauncher({
                defaultMemoryMb: Math.max(512, Number(launcher.defaultMemoryMb) || 4096),
              })}
          />
        </label>
        <div class="row-actions save-row">
          <button
            type="button"
            disabled={launcherSaving}
            onclick={() =>
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

    {#if tab === "launcher" && launcherSub === "commands"}
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
            onblur={() => persistLauncher({ preLaunchHook: launcher.preLaunchHook?.trim() || null })}
          />
        </label>
        <label>
          Post-exit hook
          <input
            bind:value={launcher.postExitHook}
            placeholder="Command after game exits"
            onblur={() => persistLauncher({ postExitHook: launcher.postExitHook?.trim() || null })}
          />
        </label>
        <label>
          Wrapper command
          <input
            bind:value={launcher.wrapperCommand}
            placeholder="e.g. gamemoderun"
            onblur={() => persistLauncher({ wrapperCommand: launcher.wrapperCommand?.trim() || null })}
          />
        </label>
        <div class="row-actions save-row">
          <button
            type="button"
            disabled={launcherSaving}
            onclick={() =>
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

    {#if tab === "launcher" && launcherSub === "runtime"}
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
            <button type="button" class="secondary" onclick={browseRuntime}>Browse…</button>
          </div>
        </label>
        <div class="row-actions">
          <button type="button" onclick={applyRuntimePath} disabled={launcherSaving}>
            {launcherSaving ? "Saving…" : "Apply path"}
          </button>
          <button
            type="button"
            class="ghost"
            disabled={!defaultRuntimePath}
            onclick={() => {
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
            <button type="button" class="secondary" onclick={browseInstances}>Browse…</button>
          </div>
        </label>
        <div class="row-actions">
          <button type="button" onclick={applyInstancesPath} disabled={launcherSaving}>
            {launcherSaving ? "Saving…" : "Apply path"}
          </button>
          <button
            type="button"
            class="ghost"
            disabled={!defaultInstancesPath}
            onclick={() => {
              instancesDraft = defaultInstancesPath;
              void applyInstancesPath();
            }}
          >
            Reset to default
          </button>
        </div>
      </section>
    {/if}

    {#if tab === "ai"}
      <section class="card card-wide">
        <div class="card-title">
          <Bot size={18} />
          <h3>AI</h3>
        </div>
        <p class="hint">Local Ollama models or a cloud API. Diagnose / Crash KB live under Advanced.</p>
        <AiSettingsPanel onsaved={loadIntegrations} />
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
              <button class="secondary mini" onclick={() => saveSecret("github", githubTokenDraft)} disabled={!!savingSecret || !githubTokenDraft.trim()}>
                {savingSecret === "github" ? "Saving…" : "Save token"}
              </button>
              <button class="ghost mini" onclick={() => clearSecret("github")} disabled={!githubTokenSet || !!clearingSecret}>
                {clearingSecret === "github" ? "Clearing…" : "Clear"}
              </button>
              <button class="ghost mini" onclick={() => testProvider("github")} disabled={!githubTokenSet || !!testingProvider}>
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
              <button class="secondary mini" onclick={() => saveSecret("modrinth", modrinthTokenDraft)} disabled={!!savingSecret || !modrinthTokenDraft.trim()}>
                {savingSecret === "modrinth" ? "Saving…" : "Save token"}
              </button>
              <button class="ghost mini" onclick={() => clearSecret("modrinth")} disabled={!modrinthTokenSet || !!clearingSecret}>
                {clearingSecret === "modrinth" ? "Clearing…" : "Clear"}
              </button>
              <button class="ghost mini" onclick={() => testProvider("modrinth")} disabled={!modrinthTokenSet || !!testingProvider}>
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
              <button class="secondary mini" onclick={() => saveSecret("curseforge", curseforgeTokenDraft)} disabled={!!savingSecret || !curseforgeTokenDraft.trim()}>
                {savingSecret === "curseforge" ? "Saving…" : "Save token"}
              </button>
              <button class="ghost mini" onclick={() => clearSecret("curseforge")} disabled={!curseforgeTokenSet || !!clearingSecret}>
                {clearingSecret === "curseforge" ? "Clearing…" : "Clear"}
              </button>
              <button class="ghost mini" onclick={() => testProvider("curseforge")} disabled={!curseforgeTokenSet || !!testingProvider}>
                {testingProvider === "curseforge" ? "Testing…" : "Test"}
              </button>
            </div>
            {#if testResults.curseforge}<small class="test-ok">{testResults.curseforge}</small>{/if}
          </div>

          <div class="provider-block">
            <div class="provider-head">
              <strong>AI</strong>
              <span class:ok={aiProvider === "ollama" || aiApiKeySet}>
                {aiProvider === "ollama" ? "Ollama" : aiApiKeySet ? "Cloud · key set" : "Cloud · no key"}
              </span>
            </div>
            <p class="hint">
              <code>{aiProvider}</code> · <code>{aiModel || "—"}</code>
              {#if aiProvider === "ollama"}
                · models <code>{ollamaModelsPath || "default"}</code>
              {/if}
              · diagnose <code>{diagnoseMode}</code>
            </p>
            <div class="row-actions">
              <button type="button" class="secondary mini" onclick={() => (tab = "ai")}>
                <Bot size={14} /> Open AI settings
              </button>
              <button
                class="ghost mini"
                onclick={() => testProvider("ai")}
                disabled={!!testingProvider || (aiProvider === "openai-compatible" && !aiApiKeySet && !aiEndpoint.includes("127.0.0.1") && !aiEndpoint.includes("localhost"))}
              >
                {testingProvider === "ai" ? "Testing…" : "Test AI"}
              </button>
            </div>
            {#if testResults.ai}<small class="test-ok">{testResults.ai}</small>{/if}
          </div>

          <div class="provider-block">
            <div class="provider-head">
              <strong><Network size={14} /> TuffSwarm</strong>
              <span class:ok={swarmEnabled}>{swarmEnabled ? "on" : "off"}</span>
            </div>
            <label class="check-row">
              <input
                type="checkbox"
                checked={swarmEnabled}
                disabled={swarmSaving}
                onchange={toggleSwarmEnabled}
              />
              Network
            </label>
            {#if swarmEnabled}
              <small class="test-ok">
                {#if swarmSupabaseConfigured}
                  {swarmSupabaseUsingBuiltin ? "Community backend · ready" : "Custom backend · ready"}
                {:else}
                  Backend not configured
                {/if}
              </small>
              <label class="check-row">
                <input
                  type="checkbox"
                  checked={swarmSharePrompts}
                  disabled={swarmSaving}
                  onchange={toggleSharePrompts}
                />
                Ask to share after fix
              </label>

              <label class="check-row">
                <input
                  type="checkbox"
                  checked={swarmP2pEnabled}
                  disabled={swarmSaving}
                  onchange={toggleP2pEnabled}
                />
                Local P2P
              </label>
              {#if swarmP2pEnabled}
                <div class="row-actions">
                  <button
                    type="button"
                    class="secondary mini"
                    disabled={swarmSaving}
                    onclick={ensureP2pNode}
                  >
                    Start / attach
                  </button>
                  <button
                    type="button"
                    class="ghost mini"
                    disabled={swarmSaving}
                    onclick={refreshP2pStatus}
                  >
                    Refresh
                  </button>
                </div>
                {#if swarmP2pStatus}
                  <small class="test-ok">{swarmP2pStatus}</small>
                {/if}
                {#if swarmP2pHint}
                  <small class="hint">{swarmP2pHint}</small>
                {/if}
                {#if swarmP2pListenAddrs.length}
                  <div class="p2p-listen-addrs">
                    {#each swarmP2pListenAddrs as addr (addr)}
                      <div class="row-actions p2p-addr-row">
                        <code class="p2p-addr">{addr}</code>
                        <button type="button" class="ghost mini" onclick={() => copyP2pListenAddr(addr)}>Copy</button>
                      </div>
                    {/each}
                    {#if swarmP2pCopyMsg}
                      <small class="test-ok">{swarmP2pCopyMsg}</small>
                    {/if}
                  </div>
                {/if}
                <label class="check-row">
                  <input
                    type="checkbox"
                    checked={swarmVolunteerDiagnose}
                    disabled={swarmSaving}
                    onchange={toggleVolunteerDiagnose}
                  />
                  Fog volunteer
                </label>
                <label class="check-row">
                  <input
                    type="checkbox"
                    checked={swarmCreationWorker}
                    disabled={swarmSaving}
                    onchange={toggleCreationWorker}
                  />
                  Creation worker
                </label>
              {/if}

              <button
                type="button"
                class="ghost mini"
                onclick={() => (swarmAdvanced = !swarmAdvanced)}
              >
                {swarmAdvanced ? "Hide advanced" : "Advanced…"}
              </button>
              {#if swarmAdvanced}
                <label>
                  Hub URL
                  <input
                    bind:value={swarmHubUrl}
                    placeholder="http://192.168.1.10:8787"
                    autocomplete="off"
                  />
                </label>
                <label>
                  P2P control URL
                  <input
                    bind:value={swarmP2pControlUrl}
                    placeholder="http://127.0.0.1:8790"
                    disabled={!swarmP2pEnabled}
                    autocomplete="off"
                  />
                </label>
                <label>
                  Bootstrap multiaddr
                  <input
                    bind:value={swarmP2pBootstrap}
                    placeholder="/ip4/…/tcp/…/p2p/…"
                    disabled={!swarmP2pEnabled}
                    autocomplete="off"
                  />
                </label>
                <label class="check-row">
                  <input
                    type="checkbox"
                    checked={swarmP2pRelayServer}
                    disabled={swarmSaving || !swarmP2pEnabled}
                    onchange={toggleP2pRelayServer}
                  />
                  Circuit Relay (VPS)
                </label>
                {#if swarmCreationWorker}
                  <label>
                    Advertised VRAM (MB)
                    <input
                      type="number"
                      min="0"
                      step="256"
                      value={swarmAdvertisedVramMb}
                      disabled={swarmSaving || !swarmP2pEnabled}
                      onchange={(e) => applyAdvertisedVramMb((e.currentTarget as HTMLInputElement).value)}
                    />
                  </label>
                {/if}
                {#if swarmP2pRelayStatus || swarmP2pGossipStatus || swarmP2pWorkerStubStatus}
                  <small class="hint">
                    {[swarmP2pRelayStatus, swarmP2pGossipStatus, swarmP2pWorkerStubStatus]
                      .filter(Boolean)
                      .join(" · ")}
                  </small>
                {/if}
                <label>
                  Supabase URL override
                  <input
                    bind:value={swarmSupabaseUrl}
                    placeholder="https://xxxx.supabase.co"
                    autocomplete="off"
                  />
                </label>
                <div class="row-actions">
                  <button
                    type="button"
                    class="mini"
                    disabled={swarmSaving}
                    onclick={saveSupabaseUrl}
                  >
                    Save URL
                  </button>
                </div>
                <label>
                  Supabase anon key override
                  <input
                    type="password"
                    bind:value={swarmSupabaseAnonDraft}
                    placeholder={swarmSupabaseAnonSet ? "••••••••" : "built-in"}
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
                    onclick={() => saveSecret("swarm_supabase", swarmSupabaseAnonDraft)}
                  >
                    {savingSecret === "swarm_supabase" ? "Saving…" : "Save key"}
                  </button>
                  <button
                    class="ghost mini"
                    disabled={
                      swarmSaving ||
                      clearingSecret === "swarm_supabase" ||
                      !swarmSupabaseAnonSet
                    }
                    onclick={() => clearSecret("swarm_supabase")}
                  >
                    Clear
                  </button>
                </div>
              {/if}
            {/if}
          </div>
        </div>

        <div class="row-actions save-row">
          <button onclick={saveIntegrationSettings} disabled={savingSettings || integrationsLoading}>
            {savingSettings ? "Saving…" : "Save settings"}
          </button>
          <button class="ghost" onclick={loadIntegrations} disabled={integrationsLoading}>Reload status</button>
        </div>
      </section>
    {/if}

    {#if tab === "about"}
      <section class="card card-wide">
        <div class="card-title">
          <Info size={18} />
          <h3>About</h3>
        </div>
        <button class="ghost" onclick={async () => { await loadAppVersion(); await checkUpdate(); }} disabled={updateLoading}>
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
                <button class="ghost mini" onclick={openReleaseUrl}>Open release</button>
              {/if}
            {:else}
              <span class="update-ok">Up to date ({updateCheck.currentVersion})</span>
            {/if}
          </div>
        {/if}
        <div class="about">
          {#if $brandIcon === "creeper"}
            <img class="logo-big logo-big-img" src={BRAND_ICON_CREEPER_SRC} alt="" draggable="false" />
          {:else}
            <div class="logo-big">T</div>
          {/if}
          <div>
            <h4>TuffBox IDE</h4>
            <p>Developer harness for Minecraft modpacks.</p>
            <span class="version">Version {appVersion || "…"}</span>
          </div>
        </div>
      </section>

      <section class="card card-wide">
        <div class="card-title">
          <Palette size={18} />
          <h3>App icon</h3>
        </div>
        <p class="hint">Shown on the left rail and on this About page.</p>
        <div class="brand-icon-picker" role="radiogroup" aria-label="App icon">
          <button
            type="button"
            class="brand-icon-option"
            class:selected={$brandIcon === "classic"}
            role="radio"
            aria-checked={$brandIcon === "classic"}
            onclick={() => selectBrandIcon("classic")}
          >
            <span class="brand-icon-preview brand-icon-classic" aria-hidden="true">T</span>
            <span class="brand-icon-label">Classic</span>
          </button>
          <button
            type="button"
            class="brand-icon-option"
            class:selected={$brandIcon === "creeper"}
            role="radio"
            aria-checked={$brandIcon === "creeper"}
            onclick={() => selectBrandIcon("creeper")}
          >
            <img
              class="brand-icon-preview brand-icon-creeper"
              src={BRAND_ICON_CREEPER_SRC_SM}
              alt=""
              draggable="false"
            />
            <span class="brand-icon-label">Creeper box</span>
          </button>
        </div>
      </section>
    {/if}
  </div>
</div>

{#if showJavaPicker}
  <JavaPickerModal
    current={launcher.defaultJavaPath ?? "Auto-detect"}
    onclose={() => (showJavaPicker = false)}
    onselected={(path) => { showJavaPicker = false; void persistLauncher({ defaultJavaPath: path }); }}
  />
{/if}

<ConfettiBurst active={brandConfetti} ondone={() => (brandConfetti = false)} />

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

  .launcher-subnav {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: -6px 0 16px;
  }

  .launcher-sub {
    display: inline-flex;
    align-items: center;
    padding: 6px 11px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .launcher-sub:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .launcher-sub.active {
    color: var(--text-primary);
    background: var(--bg-elevated);
    border-color: var(--accent-primary);
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
    line-height: 1.45;
    background: var(--bg-elevated);
    color: var(--text-primary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    padding: 10px 14px;
  }

  textarea::placeholder {
    color: var(--text-muted);
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

  .chip-row.tight {
    margin-bottom: 0;
    justify-content: flex-end;
  }

  .settings-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 24px;
    padding: 18px 0;
    border-bottom: 1px solid var(--border-color);
  }

  .settings-row:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }

  .card-title + .settings-row {
    padding-top: 0;
  }

  .settings-row-stack {
    align-items: flex-start;
    flex-direction: column;
  }

  .settings-row-text {
    flex: 1;
    min-width: 0;
  }

  .settings-row-text strong {
    display: block;
    color: var(--text-primary);
    font-size: 14px;
    font-weight: 700;
    margin-bottom: 4px;
  }

  .settings-row-text p {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.45;
    max-width: 42rem;
  }

  .settings-row-control {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 10px;
    min-width: min(320px, 100%);
  }

  .settings-row-stack .settings-row-control {
    align-items: stretch;
    width: 100%;
    min-width: 0;
  }

  .control-select {
    width: auto;
    min-width: 88px;
  }

  .field-inline {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 0;
    width: 100%;
    font-size: 12px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  .discord-block {
    width: 100%;
  }

  .hint.flat {
    margin: 0;
  }

  .notice.compact {
    margin-bottom: 0;
    padding: 8px 10px;
  }

  .row-actions button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
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
    color: var(--on-accent, #000);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent-primary) 25%, transparent);
  }

  .logo-big-img {
    display: block;
    object-fit: cover;
    background: transparent;
    box-shadow: 0 8px 24px color-mix(in srgb, var(--accent-primary) 25%, transparent);
    color: transparent;
    font-size: 0;
  }

  .brand-icon-picker {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .brand-icon-option {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    min-width: 120px;
    padding: 14px 16px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    cursor: pointer;
    transition: border-color 0.15s ease, box-shadow 0.15s ease, color 0.15s ease;
  }

  .brand-icon-option:hover {
    color: var(--text-primary);
    border-color: var(--text-muted);
  }

  .brand-icon-option.selected {
    color: var(--text-primary);
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 1px var(--accent-primary);
  }

  .brand-icon-preview {
    width: 48px;
    height: 48px;
    border-radius: var(--border-radius-md);
  }

  .brand-icon-classic {
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, #ffc500, #ff9500);
    color: #241703;
    font-weight: 900;
    font-size: 22px;
    box-shadow: 0 4px 14px rgba(255, 197, 0, 0.28);
  }

  .brand-icon-creeper {
    display: block;
    object-fit: cover;
    background: transparent;
  }

  .brand-icon-label {
    font-size: 12px;
    font-weight: 600;
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

  .ghost {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  .shortcut-list { display: grid; gap: 4px; margin-top: 4px; width: 100%; }
  .shortcut-row { display: flex; align-items: center; gap: 12px; padding: 6px 10px; border-radius: 6px; background: var(--bg-tertiary); }
  .shortcut-row kbd { font-family: ui-monospace,monospace; font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); border: 1px solid var(--border-color); color: var(--text-primary); min-width: 60px; text-align: center; }
  .shortcut-row span { flex: 1; color: var(--text-secondary); font-size: 12px; }
  .shortcut-row small { color: var(--text-muted); font-size: 10px; }

  .update-info { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding: 8px 10px; border-radius: var(--border-radius-sm); background: var(--bg-tertiary); border: 1px solid var(--border-color); margin: 10px 0; font-size: 12px; }
  .update-info.error { color: var(--accent-danger); border-color: color-mix(in srgb, var(--accent-danger) 28%, transparent); background: color-mix(in srgb, var(--accent-danger) 8%, transparent); }
  .update-avail { color: var(--accent-primary); font-weight: 700; }
  .update-ok { color: var(--text-muted); }

  .version {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 4px;
  }

  .integrations {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 320px), 1fr));
    gap: 14px;
  }
  .provider-block {
    display: grid;
    gap: 10px;
    padding: 14px;
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    min-width: 0;
  }
  .provider-block label {
    min-width: 0;
  }
  .provider-block input {
    min-width: 0;
  }
  .provider-block .hint,
  .provider-block .hint code,
  .provider-block code {
    overflow-wrap: anywhere;
    word-break: break-word;
  }
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
    flex-direction: row;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-secondary);
    margin-bottom: 12px;
    cursor: pointer;
    font-weight: 500;
  }
  .integrations .check-row,
  .card .check-row {
    flex-direction: row;
    align-items: center;
  }
  .check-row input {
    accent-color: var(--accent-primary);
    width: auto;
    flex-shrink: 0;
    margin: 0;
  }
  .chip-row.scale-chips {
    margin-bottom: 0;
    justify-content: flex-end;
    gap: 6px;
  }
  .chip-row.scale-chips .chip {
    padding: 6px 11px;
    font-size: 11px;
  }
  .test-ok { color: var(--accent-primary); font-size: 11px; }
  .p2p-listen-addrs { display: flex; flex-direction: column; gap: 4px; margin-top: 4px; }
  .p2p-addr-row { align-items: flex-start; }
  .p2p-addr {
    font-size: 10px;
    word-break: break-all;
    flex: 1;
    min-width: 0;
    color: var(--text-secondary, inherit);
  }
  .notice { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-radius: 10px; margin-bottom: 12px; border: 1px solid var(--border-color); font-size: 12px; }
  .notice.error { color: var(--accent-danger); background: color-mix(in srgb, var(--accent-danger) 8%, transparent); border-color: color-mix(in srgb, var(--accent-danger) 28%, transparent); }
  .notice.success { color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 8%, transparent); border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent); }
  .inline-status { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 12px; margin-bottom: 10px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 720px) {
    .settings-row {
      flex-direction: column;
      align-items: stretch;
    }

    .settings-row-control {
      align-items: stretch;
      min-width: 0;
    }

    .chip-row.tight {
      justify-content: flex-start;
    }

    .res-custom {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 640px) {
    .res-custom {
      grid-template-columns: 1fr;
    }
  }
</style>
