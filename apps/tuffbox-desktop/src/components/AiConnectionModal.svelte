<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    X,
    Bot,
    KeyRound,
    RefreshCw,
    CheckCircle2,
    AlertTriangle,
    Plug,
    FolderOpen,
    Download,
    FileUp,
    Search,
  } from "lucide-svelte";

  export let open = false;

  const dispatch = createEventDispatcher<{ saved: void; close: void }>();

  type AiProvider = "ollama" | "openai-compatible";
  type IntegrationStatus = {
    settings: {
      githubRepository: string;
      ai: {
        provider: string;
        endpoint: string;
        model: string;
        diagnoseMode?: string;
        crashKbEndpoint?: string;
        ollamaBinaryPath?: string;
        ollamaModelsPath?: string;
      };
    };
    aiApiKeySet: boolean;
  };

  type OllamaDetect = {
    installed: boolean;
    running: boolean;
    binaryPath: string;
    modelsPath?: string;
    modelsPathConfigured?: boolean;
    defaultModel?: string;
    endpoint: string;
    models: string[];
    needsModel: boolean;
    error?: string | null;
    suggestedModels: string[];
    suggestedModelNotes?: Record<string, string>;
  };

  type PresetId = "ollama" | "openai" | "openrouter" | "hermes" | "custom";

  const PRESETS: {
    id: PresetId;
    label: string;
    provider: AiProvider;
    endpoint: string;
    model: string;
    needsKey: boolean;
    hint: string;
  }[] = [
    {
      id: "ollama",
      label: "Ollama (local)",
      provider: "ollama",
      endpoint: "http://127.0.0.1:11434",
      model: "",
      needsKey: false,
      hint: "Local models via Ollama. TuffBox detects your install — you choose which model to download.",
    },
    {
      id: "openai",
      label: "OpenAI",
      provider: "openai-compatible",
      endpoint: "https://api.openai.com/v1",
      model: "gpt-4o-mini",
      needsKey: true,
      hint: "Official OpenAI Chat Completions API.",
    },
    {
      id: "openrouter",
      label: "OpenRouter",
      provider: "openai-compatible",
      endpoint: "https://openrouter.ai/api/v1",
      model: "openai/gpt-4o-mini",
      needsKey: true,
      hint: "OpenRouter OpenAI-compatible gateway.",
    },
    {
      id: "hermes",
      label: "Hermes / custom API",
      provider: "openai-compatible",
      endpoint: "http://127.0.0.1:8000/v1",
      model: "hermes",
      needsKey: false,
      hint: "Any OpenAI-compatible endpoint (Hermes CLI, vLLM, LM Studio). API key optional.",
    },
    {
      id: "custom",
      label: "Custom…",
      provider: "openai-compatible",
      endpoint: "",
      model: "",
      needsKey: false,
      hint: "Enter your own base URL, model, and optional API key.",
    },
  ];

  let preset: PresetId = "ollama";
  let provider: AiProvider = "ollama";
  let endpoint = "http://127.0.0.1:11434";
  let model = "";
  let ollamaBinaryPath = "";
  let ollamaModelsPath = "";
  let diagnoseMode = "server";
  let crashKbEndpoint = "";
  let apiKeyDraft = "";
  let apiKeySet = false;
  let loading = false;
  let saving = false;
  let testing = false;
  let listingModels = false;
  let detecting = false;
  let scanningDisk = false;
  let pulling = false;
  let importing = false;
  let error = "";
  let message = "";
  let testResult = "";
  let ollamaModels: string[] = [];
  let detect: OllamaDetect | null = null;
  let pullName = "qwen2.5:7b";
  let ggufName = "";

  $: if (open) {
    void load();
  }

  async function load() {
    loading = true;
    error = "";
    message = "";
    testResult = "";
    try {
      const status = await invoke<IntegrationStatus>("get_integration_status");
      provider = status.settings?.ai?.provider === "openai-compatible" ? "openai-compatible" : "ollama";
      endpoint = status.settings?.ai?.endpoint || (provider === "ollama" ? "http://127.0.0.1:11434" : "");
      model = status.settings?.ai?.model || "";
      ollamaBinaryPath = status.settings?.ai?.ollamaBinaryPath ?? "";
      ollamaModelsPath = status.settings?.ai?.ollamaModelsPath ?? "";
      if (!model.trim() && provider === "ollama") {
        model = "qwen2.5:7b";
      }
      diagnoseMode = status.settings?.ai?.diagnoseMode || "server";
      crashKbEndpoint = status.settings?.ai?.crashKbEndpoint ?? "";
      apiKeySet = !!status.aiApiKeySet;
      apiKeyDraft = "";
      preset = detectPreset(provider, endpoint);
      if (provider === "ollama") {
        await probeOllama();
      } else {
        detect = null;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function detectPreset(p: AiProvider, ep: string): PresetId {
    const e = ep.trim().replace(/\/$/, "");
    if (p === "ollama" || e.includes("11434")) return "ollama";
    if (e.includes("api.openai.com")) return "openai";
    if (e.includes("openrouter.ai")) return "openrouter";
    if (p === "openai-compatible") return e ? "hermes" : "custom";
    return "custom";
  }

  function applyPreset(id: PresetId) {
    preset = id;
    const p = PRESETS.find((x) => x.id === id);
    if (!p) return;
    provider = p.provider;
    if (p.endpoint) endpoint = p.endpoint;
    if (p.model) model = p.model;
    testResult = "";
    if (provider === "ollama") void probeOllama();
    else detect = null;
  }

  async function probeOllama() {
    detecting = true;
    listingModels = true;
    try {
      detect = await invoke<OllamaDetect>("detect_ollama", {
        endpoint: endpoint || null,
        binaryPath: ollamaBinaryPath || null,
      });
      ollamaModels = detect?.models ?? [];
      if (detect?.binaryPath && !ollamaBinaryPath.trim()) {
        ollamaBinaryPath = detect.binaryPath;
      }
      if (detect?.suggestedModels?.length && !pullName.trim()) {
        pullName = detect.suggestedModels[0];
      }
      if (detect?.defaultModel && !model.trim()) {
        model = detect.defaultModel;
        pullName = detect.defaultModel;
      }
      if (detect?.modelsPath && !ollamaModelsPath.trim()) {
        // Show default path as placeholder value only when user hasn't configured one —
        // keep field empty so "auto" remains clear; placeholder shows detect.modelsPath.
      }
      if (!model.trim() && ollamaModels.length > 0) {
        model = ollamaModels[0];
      }
      if (detect?.installed && detect.needsModel) {
        message = "Ollama is installed. Pick a model name below and click Install model.";
      } else if (detect?.installed && detect.running) {
        message = `Ollama is ready (${ollamaModels.length} model${ollamaModels.length === 1 ? "" : "s"}).`;
      }
    } catch (e) {
      detect = null;
      ollamaModels = [];
      error = String(e);
    } finally {
      detecting = false;
      listingModels = false;
    }
  }

  async function refreshOllamaModels() {
    await probeOllama();
  }

  async function installModel() {
    const name = (pullName || model).trim();
    if (!name) {
      error = "Enter a model name (e.g. qwen2.5:7b) or choose a suggestion.";
      return;
    }
    pulling = true;
    error = "";
    message = `Downloading ${name}… this can take several minutes.`;
    try {
      // Persist path/endpoint first so pull uses the same config.
      await persistAiSettings(name);
      const result = await invoke<{ ok: boolean; model: string; models: string[]; modelsPath?: string }>("pull_ollama_model", {
        model: name,
        endpoint: endpoint || null,
        binaryPath: ollamaBinaryPath || null,
        modelsPath: ollamaModelsPath.trim() || null,
      });
      model = result.model;
      pullName = result.model;
      ollamaModels = result.models ?? [];
      const where = result.modelsPath?.trim();
      message = where
        ? `Model “${result.model}” installed to ${where}`
        : `Model “${result.model}” installed and selected.`;
      await probeOllama();
      dispatch("saved");
    } catch (e) {
      error = String(e);
      message = "";
    } finally {
      pulling = false;
    }
  }

  async function pickGgufAndImport() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: "Select local model file (.gguf)",
        filters: [
          { name: "GGUF model", extensions: ["gguf"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof selected !== "string" || !selected) return;

      importing = true;
      error = "";
      const stem =
        ggufName.trim() ||
        selected
          .replace(/^.*[\\/]/, "")
          .replace(/\.gguf$/i, "")
          .toLowerCase()
          .replace(/[^a-z0-9._-]+/g, "-") ||
        "local-model";
      message = `Importing ${selected} as “${stem}”…`;
      await persistAiSettings(stem);
      const result = await invoke<{ ok: boolean; model: string; models: string[] }>("import_ollama_gguf", {
        filePath: selected,
        modelName: stem,
        binaryPath: ollamaBinaryPath || null,
      });
      model = result.model;
      ggufName = result.model;
      ollamaModels = result.models ?? [];
      message = `Imported “${result.model}” from local file.`;
      await probeOllama();
      dispatch("saved");
    } catch (e) {
      error = String(e);
      message = "";
    } finally {
      importing = false;
    }
  }

  async function scanDiskForOllama() {
    scanningDisk = true;
    error = "";
    message = "Scanning C:\\ for Ollama and model folders…";
    try {
      const scan = await invoke<{
        bestBinary?: string | null;
        bestModelsDir?: string | null;
        models?: string[];
        binaries?: string[];
        modelsDirs?: string[];
        visited?: number;
        truncated?: boolean;
      }>("scan_ollama_disk", { root: "C:\\" });

      if (scan.bestBinary) {
        ollamaBinaryPath = scan.bestBinary;
      }
      if (scan.bestModelsDir) {
        ollamaModelsPath = scan.bestModelsDir;
      }
      const diskModels = scan.models ?? [];
      if (diskModels.length > 0) {
        ollamaModels = diskModels;
        if (!model.trim() || !diskModels.includes(model)) {
          // Prefer a mid-size qwen if present.
          const prefer =
            diskModels.find((m) => /qwen3:8b/i.test(m)) ||
            diskModels.find((m) => /qwen3:4b/i.test(m)) ||
            diskModels.find((m) => /:8b\b/i.test(m)) ||
            diskModels[0];
          model = prefer;
          pullName = prefer;
        }
      }

      const bits: string[] = [];
      if (scan.bestBinary) bits.push(`binary: ${scan.bestBinary}`);
      if (scan.bestModelsDir) bits.push(`models: ${scan.bestModelsDir}`);
      bits.push(`${diskModels.length} model tag(s)`);
      if (scan.truncated) bits.push(`scan capped (~${scan.visited} dirs)`);
      message =
        diskModels.length || scan.bestBinary
          ? `Found on C:\\ — ${bits.join(" · ")}. Save to apply.`
          : "Nothing found on C:\\. Install Ollama or pick folders manually.";

      await persistAiSettings(model || pullName || diskModels[0] || "qwen3:8b");
      await probeOllama();
      dispatch("saved");
    } catch (e) {
      error = String(e);
      message = "";
    } finally {
      scanningDisk = false;
    }
  }

  async function pickOllamaPath() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: false,
        title: "Select ollama executable",
        filters: [
          { name: "Ollama", extensions: ["exe"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (typeof selected === "string" && selected) {
        ollamaBinaryPath = selected;
        await probeOllama();
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function pickOllamaFolder() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: true,
        title: "Select Ollama install folder",
      });
      if (typeof selected === "string" && selected) {
        ollamaBinaryPath = selected;
        await probeOllama();
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function pickOllamaModelsFolder() {
    try {
      const selected = await openDialog({
        multiple: false,
        directory: true,
        title: "Select Ollama models folder (OLLAMA_MODELS)",
      });
      if (typeof selected === "string" && selected) {
        ollamaModelsPath = selected;
        await persistAiSettings(model || pullName || "qwen2.5:7b");
        message =
          "Models folder saved. Click Install model — TuffBox will download into this folder (not C:\\Users\\…\\.ollama).";
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function persistAiSettings(activeModel: string) {
    const status = await invoke<IntegrationStatus>("get_integration_status");
    await invoke("save_integration_settings", {
      settings: {
        githubRepository: status.settings?.githubRepository ?? "",
        ai: {
          provider,
          endpoint: endpoint.trim(),
          model: activeModel.trim() || "qwen2.5:7b",
          diagnoseMode,
          crashKbEndpoint,
          ollamaBinaryPath: ollamaBinaryPath.trim(),
          ollamaModelsPath: ollamaModelsPath.trim(),
        },
      },
    });
  }

  async function save() {
    saving = true;
    error = "";
    message = "";
    try {
      await persistAiSettings(model.trim());
      if (apiKeyDraft.trim()) {
        await invoke("set_integration_secret", { kind: "ai", value: apiKeyDraft.trim() });
        apiKeyDraft = "";
      }
      message = "AI connection saved.";
      await load();
      dispatch("saved");
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function clearKey() {
    try {
      await invoke("clear_integration_secret", { kind: "ai" });
      apiKeySet = false;
      message = "API key cleared.";
    } catch (e) {
      error = String(e);
    }
  }

  async function test() {
    testing = true;
    error = "";
    testResult = "";
    try {
      await save();
      const result = await invoke<string>("test_integration", { provider: "ai" });
      testResult = result;
      message = result;
    } catch (e) {
      error = String(e);
    } finally {
      testing = false;
    }
  }

  function close() {
    open = false;
    dispatch("close");
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

{#if open}
  <div class="backdrop" role="presentation" on:click|self={close}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="AI connection">
      <header>
        <div class="title">
          <Bot size={18} />
          <div>
            <strong>AI connection</strong>
            <small>Ollama or OpenAI-compatible API (Hermes-style)</small>
          </div>
        </div>
        <button class="icon" on:click={close} aria-label="Close"><X size={16} /></button>
      </header>

      {#if loading}
        <p class="muted">Loading…</p>
      {:else}
        {#if error}<div class="notice error"><AlertTriangle size={14} /> {error}</div>{/if}
        {#if message}<div class="notice ok"><CheckCircle2 size={14} /> {message}</div>{/if}

        <div class="presets">
          {#each PRESETS as p}
            <button type="button" class="preset" class:on={preset === p.id} on:click={() => applyPreset(p.id)}>
              {p.label}
            </button>
          {/each}
        </div>
        <p class="hint">{PRESETS.find((p) => p.id === preset)?.hint}</p>

        <label>
          Mode
          <select
            bind:value={provider}
            on:change={() => {
              preset = detectPreset(provider, endpoint);
              if (provider === "ollama" && !endpoint) endpoint = "http://127.0.0.1:11434";
              if (provider === "ollama") void probeOllama();
              else detect = null;
            }}
          >
            <option value="ollama">Ollama</option>
            <option value="openai-compatible">API key / OpenAI-compatible</option>
          </select>
        </label>

        <label>
          Endpoint
          <input
            bind:value={endpoint}
            placeholder={provider === "ollama" ? "http://127.0.0.1:11434" : "https://api.openai.com/v1"}
            autocomplete="off"
          />
        </label>

        {#if provider === "ollama"}
          <label>
            Ollama install path
            <div class="model-row">
              <input
                bind:value={ollamaBinaryPath}
                placeholder="ollama.exe or install folder (empty = auto-detect)"
                autocomplete="off"
              />
              <button class="ghost mini" type="button" title="Pick ollama.exe" on:click={pickOllamaPath}>
                <FolderOpen size={14} />
              </button>
              <button class="ghost mini" type="button" title="Pick install folder" on:click={pickOllamaFolder}>
                Folder
              </button>
              <button class="ghost mini" type="button" title="Re-detect" on:click={probeOllama} disabled={detecting || scanningDisk}>
                <RefreshCw size={14} class={detecting ? "spin" : ""} />
              </button>
            </div>
          </label>

          <label>
            Models install folder
            <div class="model-row">
              <input
                bind:value={ollamaModelsPath}
                placeholder={detect?.modelsPath || "%USERPROFILE%\\.ollama\\models (empty = default)"}
                autocomplete="off"
              />
              <button class="ghost mini" type="button" title="Pick models folder" on:click={pickOllamaModelsFolder}>
                <FolderOpen size={14} />
              </button>
              <button
                class="ghost mini"
                type="button"
                title="Scan entire C: drive for ollama.exe and models folders"
                on:click={scanDiskForOllama}
                disabled={scanningDisk || detecting}
              >
                <Search size={14} class={scanningDisk ? "spin" : ""} />
                {scanningDisk ? "Scanning C:…" : "Scan C:"}
              </button>
            </div>
            <p class="hint">
              Where Ollama stores downloaded weights (<code>OLLAMA_MODELS</code>).
              Don’t know the path? Click <strong>Scan C:</strong> — TuffBox searches the disk for
              <code>ollama.exe</code> and folders with <code>blobs</code>/<code>manifests</code>,
              then fills paths and lists installed tags (e.g. <code>qwen3:8b</code>).
            </p>
          </label>

          {#if detect}
            <div class="status" class:ok={detect.installed && detect.running} class:warn={detect.installed && !detect.running} class:bad={!detect.installed}>
              {#if detect.installed && detect.running}
                Ollama found{detect.binaryPath ? ` at ${detect.binaryPath}` : ""} · running · {detect.models.length} model{detect.models.length === 1 ? "" : "s"}
              {:else if detect.installed}
                Ollama found{detect.binaryPath ? ` at ${detect.binaryPath}` : ""}, but the API is not responding. Open the Ollama app, then refresh.
              {:else}
                Ollama not found. Install from <a href="https://ollama.com" target="_blank" rel="noreferrer">ollama.com</a> or set the path above.
              {/if}
            </div>
          {/if}

          {#if detect?.installed}
            <div class="install-box">
              <strong>Install a model</strong>
              <p class="hint">Enter the Ollama tag you want (you choose), or import a local <code>.gguf</code> file.</p>
              <label>
                Model name / tag
                <div class="model-row">
                  <input bind:value={pullName} placeholder="e.g. qwen2.5:7b" autocomplete="off" />
                  <button type="button" on:click={installModel} disabled={pulling || importing || !pullName.trim()}>
                    <Download size={14} />
                    {pulling ? "Installing…" : "Install model"}
                  </button>
                </div>
              </label>
              {#if detect.suggestedModels?.length}
                <div class="suggestions">
                  {#each detect.suggestedModels as s}
                    <button
                      type="button"
                      class="chip"
                      class:on={pullName === s}
                      title={detect.suggestedModelNotes?.[s] ?? s}
                      on:click={() => (pullName = s)}
                    >
                      {s}
                      {#if detect.suggestedModelNotes?.[s]}
                        <small>{detect.suggestedModelNotes[s]}</small>
                      {/if}
                    </button>
                  {/each}
                </div>
                <p class="hint">Default is <code>qwen2.5:7b</code> (better crash plans). Use <code>llama3.2:3b</code> only if you need a smaller/faster model.</p>
              {/if}
              <label>
                Local model file (optional)
                <div class="model-row">
                  <input bind:value={ggufName} placeholder="Name after import (optional)" autocomplete="off" />
                  <button class="ghost" type="button" on:click={pickGgufAndImport} disabled={pulling || importing}>
                    <FileUp size={14} />
                    {importing ? "Importing…" : "Import .gguf"}
                  </button>
                </div>
              </label>
            </div>
          {/if}

          <label>
            Active model
            {#if ollamaModels.length > 0}
              <div class="model-row">
                <select bind:value={model}>
                  {#each ollamaModels as m}
                    <option value={m}>{m}</option>
                  {/each}
                </select>
                <button class="ghost mini" type="button" on:click={refreshOllamaModels} disabled={listingModels}>
                  <RefreshCw size={14} class={listingModels ? "spin" : ""} />
                </button>
              </div>
            {:else}
              <div class="model-row">
                <input bind:value={model} placeholder="Install a model above first" autocomplete="off" />
              </div>
            {/if}
          </label>
        {:else}
          <label>
            Model
            <input bind:value={model} placeholder="gpt-4o-mini" autocomplete="off" />
          </label>
        {/if}

        {#if provider === "openai-compatible"}
          <label>
            <span class="lab"><KeyRound size={12} /> API key {apiKeySet ? "(saved)" : "(optional for local)"}</span>
            <input
              type="password"
              bind:value={apiKeyDraft}
              placeholder={apiKeySet ? "•••••••• (enter new to replace)" : "sk-… or leave empty for local servers"}
              autocomplete="new-password"
            />
          </label>
          {#if apiKeySet}
            <button class="ghost mini" type="button" on:click={clearKey}>Clear saved key</button>
          {/if}
        {/if}

        <footer>
          <button class="ghost" type="button" on:click={test} disabled={testing || saving || pulling || importing || scanningDisk}>
            <Plug size={14} />
            {testing ? "Testing…" : "Test connection"}
          </button>
          <div class="spacer"></div>
          <button class="ghost" type="button" on:click={close}>Cancel</button>
          <button type="button" on:click={save} disabled={saving || scanningDisk || !endpoint.trim() || (provider === "openai-compatible" && !model.trim())}>
            {saving ? "Saving…" : "Save"}
          </button>
        </footer>
        {#if testResult}<small class="test-ok">{testResult}</small>{/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 80;
    background: rgba(0, 0, 0, 0.55);
    display: grid; place-items: center;
    padding: 16px;
  }
  .modal {
    width: min(560px, 100%);
    max-height: min(92vh, 820px);
    overflow: auto;
    background: var(--bg-secondary, #16161a);
    border: 1px solid var(--border-color, #2a2a32);
    border-radius: 14px;
    padding: 16px 18px 14px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.45);
    display: flex; flex-direction: column; gap: 10px;
  }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 10px; }
  .title { display: flex; gap: 10px; align-items: center; }
  .title strong { display: block; font-size: 15px; }
  .title small { color: var(--text-muted); font-size: 11px; }
  .icon {
    border: none; background: transparent; color: var(--text-muted); cursor: pointer; padding: 4px;
  }
  .presets { display: flex; flex-wrap: wrap; gap: 6px; }
  .preset {
    border: 1px solid var(--border-color); background: var(--bg-tertiary, #1e1e24);
    color: var(--text-secondary); border-radius: 999px; padding: 5px 10px; font-size: 11px; cursor: pointer;
  }
  .preset.on {
    background: linear-gradient(145deg, #fbbf24, #d97706); color: #1a1200; border-color: transparent; font-weight: 700;
  }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--text-muted); }
  .lab { display: inline-flex; align-items: center; gap: 4px; }
  input, select {
    background: var(--bg-primary, #0e0e12); border: 1px solid var(--border-color);
    border-radius: 8px; padding: 8px 10px; color: var(--text-primary); font-size: 13px;
  }
  .model-row { display: flex; gap: 6px; align-items: center; }
  .model-row input, .model-row select { flex: 1; min-width: 0; }
  .hint { margin: 0; font-size: 11px; color: var(--text-muted); line-height: 1.4; }
  .hint code { font-size: 10px; }
  .muted { color: var(--text-muted); }
  .status {
    font-size: 12px; line-height: 1.4; padding: 8px 10px; border-radius: 8px;
    border: 1px solid var(--border-color); background: var(--bg-primary);
    color: var(--text-secondary); word-break: break-word;
  }
  .status.ok { border-color: rgba(34, 197, 94, 0.35); color: #86efac; }
  .status.warn { border-color: rgba(251, 191, 36, 0.4); color: #fde68a; }
  .status.bad { border-color: rgba(239, 68, 68, 0.35); color: #fca5a5; }
  .status a { color: inherit; }
  .install-box {
    display: flex; flex-direction: column; gap: 8px;
    padding: 10px 12px; border-radius: 10px;
    border: 1px dashed var(--border-color); background: rgba(251, 191, 36, 0.06);
  }
  .install-box strong { font-size: 13px; color: var(--text-primary); }
  .suggestions { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip {
    border: 1px solid var(--border-color); background: var(--bg-tertiary);
    color: var(--text-secondary); border-radius: 999px; padding: 4px 9px; font-size: 11px;
    cursor: pointer; font-weight: 500;
    display: inline-flex; flex-direction: column; align-items: flex-start; gap: 2px;
    border-radius: 10px;
  }
  .chip small { font-size: 9px; font-weight: 500; opacity: 0.8; max-width: 140px; text-align: left; }
  .chip.on {
    background: linear-gradient(145deg, #fbbf24, #d97706); color: #1a1200; border-color: transparent; font-weight: 700;
  }
  footer { display: flex; align-items: center; gap: 8px; margin-top: 6px; flex-wrap: wrap; }
  .spacer { flex: 1; }
  button {
    display: inline-flex; align-items: center; gap: 6px;
    border-radius: 8px; padding: 8px 12px; font-size: 12px; cursor: pointer;
    border: 1px solid transparent; background: linear-gradient(145deg, #fbbf24, #d97706); color: #1a1200; font-weight: 700;
  }
  button:disabled { opacity: 0.55; cursor: not-allowed; }
  button.ghost {
    background: var(--bg-tertiary); border-color: var(--border-color); color: var(--text-secondary); font-weight: 500;
  }
  button.mini { padding: 6px 8px; }
  .notice {
    display: flex; gap: 8px; align-items: flex-start; font-size: 12px; line-height: 1.35;
    padding: 8px 10px; border-radius: 8px;
  }
  .notice.error { background: rgba(239, 68, 68, 0.12); color: #fca5a5; }
  .notice.ok { background: rgba(34, 197, 94, 0.12); color: #86efac; }
  .test-ok { color: #86efac; font-size: 11px; }
  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
