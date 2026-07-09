<script lang="ts">
  import { X, Folder, Loader2 } from "lucide-svelte";
  import { createEventDispatcher, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";

  const dispatch = createEventDispatcher<{ close: void; created: string }>();

  let name = "New Instance";
  let minecraftVersion = "1.20.1";
  let loader = "fabric";
  let loaderVersion = "";
  let location = "";
  let loading = false;
  let error = "";

  let mcVersions: { id: string; popular: boolean }[] = [];
  let loaderVersions: { id: string; stable: boolean }[] = [];
  let loadingMc = true;
  let loadingLoader = false;

  // Templates
  let templates: any[] = [];
  let templatesLoaded = false;
  let useTemplate = false;

  async function loadTemplates() {
    try {
      // Load templates from home dir TuffBox folder
      const home = await invoke("get_home_dir");
      const tuffboxDir = `${home}/TuffBox`;
      templates = (await invoke("list_templates", { path: `${tuffboxDir}/.templates` }).catch(() => [])) as any[];
    } catch { templates = []; }
    templatesLoaded = true;
  }

  const loaders = [
    { id: "vanilla", label: "Vanilla" },
    { id: "fabric", label: "Fabric" },
    { id: "forge", label: "Forge" },
    { id: "neoforge", label: "NeoForge" },
    { id: "quilt", label: "Quilt" },
  ];

  onMount(async () => {
    loadingMc = true;
    try {
      const [versions] = await Promise.all([
        invoke("get_minecraft_versions"),
        pickDefaultLocation(),
        loadLoaderVersions(),
      ]);
      mcVersions = versions as { id: string; popular: boolean }[];
      if (!mcVersions.some((v) => v.id === minecraftVersion)) {
        minecraftVersion = mcVersions[0]?.id ?? "";
      }
    } catch (e) {
      error = `Failed to load Minecraft versions: ${e}`;
    } finally {
      loadingMc = false;
    }
  });

  async function pickDefaultLocation() {
    const home = await invoke("get_home_dir").catch(() => "");
    location = `${home}/TuffBox/instances/${slugify(name)}`;
  }

  async function loadLoaderVersions() {
    if (loadingLoader) return;
    if (loader === "vanilla") {
      loaderVersions = [];
      loaderVersion = "";
      return;
    }
    loadingLoader = true;
    try {
      loaderVersions = await invoke("get_loader_versions", {
        loader,
        minecraftVersion,
      });
      loaderVersion = loaderVersions.find((v) => v.stable)?.id ?? loaderVersions[0]?.id ?? "";
    } catch (e) {
      loaderVersions = [];
      loaderVersion = "";
    } finally {
      loadingLoader = false;
    }
  }

  async function selectLocation() {
    const selected = await open({
      multiple: false,
      directory: true,
      title: "Select instance folder",
    });
    if (selected && typeof selected === "string") {
      location = selected;
    }
  }

  function slugify(value: string) {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9-_]/g, "-")
      .replace(/--+/g, "-")
      .replace(/^-+|-+$/g, "");
  }

  async function create() {
    loading = true;
    error = "";
    try {
      const path = await invoke("create_instance", {
        name,
        minecraftVersion,
        loader,
        loaderVersion,
        location,
      });
      dispatch("created", path as string);
      dispatch("close");
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  $: if (minecraftVersion || loader) {
    // reload loader versions when selection changes
    if (!loadingMc) loadLoaderVersions();
  }
</script>

<div class="modal-backdrop" on:click={(e) => e.target === e.currentTarget && dispatch("close")} role="button" tabindex="-1" aria-label="Close" on:keydown={(e) => e.key === 'Enter' && dispatch('close')}
>
  <div class="modal" role="dialog" aria-modal="true">
    <div class="modal-header">
      <h2>Add Instance</h2>
      <button class="icon-btn" on:click={() => dispatch("close")} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if error}
        <div class="error">{error}</div>
      {/if}

      <button class="ghost template-btn" on:click={() => { useTemplate = !useTemplate; if (useTemplate && !templatesLoaded) loadTemplates(); }}>
        {useTemplate ? "Create from scratch" : "Use template"}
      </button>

      {#if useTemplate && templates.length > 0}
        <div class="template-list">
          {#each templates.slice(0, 5) as tpl}
            <button class="template-row" on:click={() => {
              name = tpl.name || "New Instance";
              if (tpl.manifest?.minecraft?.version) minecraftVersion = tpl.manifest.minecraft.version;
              if (tpl.manifest?.loader?.kind) {
                const kind = String(tpl.manifest.loader.kind).toLowerCase();
                if (["fabric","forge","neoforge","quilt","vanilla"].includes(kind)) loader = kind;
              }
              if (tpl.manifest?.loader?.version) loaderVersion = tpl.manifest.loader.version;
              useTemplate = false;
            }}>
              <strong>{tpl.name}</strong>
              <span>{tpl.modCount || 0} mods · {tpl.manifest?.minecraft?.version || "?"}</span>
            </button>
          {/each}
        </div>
      {:else if useTemplate}
        <div class="muted">No templates found. Save a project as template first.</div>
      {/if}

      <div class="field">
        <label for="inst-name">Name</label>
        <input id="inst-name" bind:value={name} />
      </div>

      <div class="field">
        <label for="inst-mc">Minecraft version</label>
        {#if loadingMc}
          <div class="field-loader"><Loader2 size={16} class="spin" /> Loading versions...</div>
        {:else}
          <select id="inst-mc" bind:value={minecraftVersion}>
            {#each mcVersions as v}
              <option value={v.id}>
                {v.id}{#if v.popular} ★{/if}
              </option>
            {/each}
          </select>
        {/if}
      </div>

      <div class="field-row">
        <div class="field">
          <label for="inst-loader">Loader</label>
          <select id="inst-loader" bind:value={loader}>
            {#each loaders as l}
              <option value={l.id}>{l.label}</option>
            {/each}
          </select>
        </div>

        <div class="field">
          <label for="inst-loader-version">Loader version</label>
          {#if loadingLoader}
            <div class="field-loader"><Loader2 size={16} class="spin" /> Loading...</div>
          {:else if loader === "vanilla"}
            <input id="inst-loader-version" value="-" disabled />
          {:else}
            <select id="inst-loader-version" bind:value={loaderVersion}>
              {#each loaderVersions as v}
                <option value={v.id}>{v.id}{#if v.stable} (stable){/if}</option>
              {/each}
            </select>
          {/if}
        </div>
      </div>

      <div class="field">
        <label for="inst-location">Location</label>
        <div class="input-row">
          <input id="inst-location" bind:value={location} />
          <button class="secondary" on:click={selectLocation}>
            <Folder size={16} />
          </button>
        </div>
      </div>
    </div>

    <div class="modal-footer">
      <button class="ghost" on:click={() => dispatch("close")} disabled={loading}>Cancel</button>
      <button on:click={create} disabled={loading || !minecraftVersion}>
        {#if loading}
          <Loader2 size={16} class="spin" /> Creating...
        {:else}
          Create instance
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 24px;
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    width: 100%;
    max-width: 460px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 24px;
    border-bottom: 1px solid var(--border-color);
  }

  .modal-header h2 {
    font-size: 18px;
    font-weight: 800;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--border-radius-md);
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .modal-body {
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 16px 24px;
    border-top: 1px solid var(--border-color);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field label {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .field input,
  .field select {
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 14px;
  }

  .field input:focus,
  .field select:focus {
    outline: none;
    border-color: var(--accent-primary);
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .input-row {
    display: flex;
    gap: 8px;
  }

  .input-row input {
    flex: 1;
  }

  .field-loader {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    color: var(--text-muted);
    font-size: 14px;
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .template-btn { width: 100%; margin-bottom: 4px; }
  .template-list { display: grid; gap: 4px; margin-bottom: 10px; }
  .template-row { width: 100%; display: grid; gap: 2px; text-align: left; padding: 8px 10px; border-radius: 8px; background: var(--bg-tertiary); border: 1px solid var(--border-color); color: var(--text-secondary); transform: none; }
  .template-row:hover { border-color: rgba(27,217,106,.3); }
  .template-row strong { color: var(--text-primary); font-size: 13px; }
  .template-row span { color: var(--text-muted); font-size: 11px; }
  .muted { color: var(--text-muted); font-size: 12px; padding: 8px; }

  .error {
    background: rgba(239, 68, 68, 0.12);
    color: #ef4444;
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    font-size: 13px;
  }
</style>
