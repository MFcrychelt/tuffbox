<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { X, Bot, KeyRound, RefreshCw, CheckCircle2, AlertTriangle, Plug } from "lucide-svelte";

  export let open = false;

  const dispatch = createEventDispatcher<{ saved: void; close: void }>();

  type AiProvider = "ollama" | "openai-compatible";
  type IntegrationStatus = {
    settings: { githubRepository: string; ai: { provider: string; endpoint: string; model: string } };
    aiApiKeySet: boolean;
  };

  type PresetId = "ollama" | "openai" | "openrouter" | "hermes" | "custom";

  const PRESETS: { id: PresetId; label: string; provider: AiProvider; endpoint: string; model: string; needsKey: boolean; hint: string }[] = [
    {
      id: "ollama",
      label: "Ollama (local)",
      provider: "ollama",
      endpoint: "http://127.0.0.1:11434",
      model: "qwen2.5-coder:7b",
      needsKey: false,
      hint: "Local models via Ollama. No API key.",
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
  let model = "qwen2.5-coder:7b";
  let apiKeyDraft = "";
  let apiKeySet = false;
  let loading = false;
  let saving = false;
  let testing = false;
  let listingModels = false;
  let error = "";
  let message = "";
  let testResult = "";
  let ollamaModels: string[] = [];

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
      apiKeySet = !!status.aiApiKeySet;
      apiKeyDraft = "";
      preset = detectPreset(provider, endpoint);
      if (provider === "ollama") {
        await refreshOllamaModels();
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
    if (provider === "ollama") void refreshOllamaModels();
  }

  async function refreshOllamaModels() {
    listingModels = true;
    try {
      ollamaModels = await invoke<string[]>("list_ollama_models", { endpoint: endpoint || null });
    } catch {
      ollamaModels = [];
    } finally {
      listingModels = false;
    }
  }

  async function save() {
    saving = true;
    error = "";
    message = "";
    try {
      const status = await invoke<IntegrationStatus>("get_integration_status");
      await invoke("save_integration_settings", {
        settings: {
          githubRepository: status.settings?.githubRepository ?? "",
          ai: {
            provider,
            endpoint: endpoint.trim(),
            model: model.trim(),
          },
        },
      });
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
      // Persist current fields first so Test hits the right endpoint.
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

        <label>
          Model
          {#if provider === "ollama" && ollamaModels.length > 0}
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
              <input bind:value={model} placeholder={provider === "ollama" ? "qwen2.5-coder:7b" : "gpt-4o-mini"} autocomplete="off" />
              {#if provider === "ollama"}
                <button class="ghost mini" type="button" on:click={refreshOllamaModels} disabled={listingModels} title="List Ollama models">
                  <RefreshCw size={14} class={listingModels ? "spin" : ""} />
                </button>
              {/if}
            </div>
          {/if}
        </label>

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
        {:else}
          <p class="hint">Ollama does not need an API key. Use <code>ollama pull &lt;model&gt;</code> then refresh the model list.</p>
        {/if}

        <footer>
          <button class="ghost" type="button" on:click={test} disabled={testing || saving}>
            <Plug size={14} />
            {testing ? "Testing…" : "Test connection"}
          </button>
          <div class="spacer"></div>
          <button class="ghost" type="button" on:click={close}>Cancel</button>
          <button type="button" on:click={save} disabled={saving || !endpoint.trim() || !model.trim()}>
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
    width: min(520px, 100%);
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
  .model-row input, .model-row select { flex: 1; }
  .hint { margin: 0; font-size: 11px; color: var(--text-muted); line-height: 1.4; }
  .hint code { font-size: 10px; }
  .muted { color: var(--text-muted); }
  footer { display: flex; align-items: center; gap: 8px; margin-top: 6px; flex-wrap: wrap; }
  .spacer { flex: 1; }
  button {
    display: inline-flex; align-items: center; gap: 6px;
    border-radius: 8px; padding: 8px 12px; font-size: 12px; cursor: pointer;
    border: 1px solid transparent; background: linear-gradient(145deg, #fbbf24, #d97706); color: #1a1200; font-weight: 700;
  }
  button.ghost {
    background: var(--bg-tertiary); border-color: var(--border-color); color: var(--text-secondary); font-weight: 500;
  }
  button.mini { padding: 6px 8px; }
  button:disabled { opacity: 0.55; cursor: not-allowed; }
  .notice {
    display: flex; align-items: center; gap: 6px; font-size: 12px; border-radius: 8px; padding: 8px 10px;
  }
  .notice.error { background: rgba(239,68,68,.12); color: #fecaca; border: 1px solid rgba(239,68,68,.3); }
  .notice.ok { background: rgba(34,197,94,.1); color: #bbf7d0; border: 1px solid rgba(34,197,94,.28); }
  .test-ok { color: #86efac; font-size: 11px; }
  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
