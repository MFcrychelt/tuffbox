<script lang="ts">
  import { X, Folder, Loader2, Download, Search, Package } from "lucide-svelte";
  import { createEventDispatcher, onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { projectPath } from "../lib/store";
  import { trapFocus } from "../lib/focusTrap";
  import LoadingButton from "./LoadingButton.svelte";

  const dispatch = createEventDispatcher<{ close: void; created: string }>();

  // Each "input type" on the home screen is an isolated, self-validating
  // page (PrismLauncher-style: page-per-type) instead of one shared
  // form whose fields leak across modes.
  type CreateMode = "blank" | "import" | "curseforge";
  let mode: CreateMode = "blank";

  // --- Blank instance (explicit, typed inputs) ---
  let name = "New Instance";
  let minecraftVersion = "1.20.1";
  let loader: "vanilla" | "fabric" | "forge" | "neoforge" | "quilt" = "fabric";
  let loaderVersion = "";
  let mcVersions: { id: string; popular: boolean }[] = [];
  let loaderVersions: { id: string; stable: boolean }[] = [];
  let loadingMc = true;
  let loadingLoader = false;

  // --- Templates (blank helper) ---
  let templates: any[] = [];
  let templatesLoaded = false;
  let useTemplate = false;

  // --- Import pack (.mrpack / zip) ---
  let importName = "New Instance";
  let importPath = "";

  // --- CurseForge browse ---
  let cfName = "New Instance";
  let cfQuery = "";
  let cfHits: any[] = [];
  let cfLoading = false;
  let cfSelected: any = null;
  let cfFiles: any[] = [];
  let cfFilesLoading = false;
  let cfFileId: number | null = null;

  // --- Shared ---
  let location = "";
  let loading = false;
  let error = "";
  let installMessage = "";
  let packPhase = "";

  let unlistenPack: UnlistenFn | null = null;

  const loaders = [
    { id: "vanilla", label: "Vanilla" },
    { id: "fabric", label: "Fabric" },
    { id: "forge", label: "Forge" },
    { id: "neoforge", label: "NeoForge" },
    { id: "quilt", label: "Quilt" },
  ];

  async function loadTemplates() {
    if (!$projectPath) {
      templates = [];
      templatesLoaded = true;
      return;
    }
    try {
      templates = (await invoke("list_templates", { path: $projectPath }).catch(() => [])) as any[];
    } catch {
      templates = [];
    }
    templatesLoaded = true;
  }

  onMount(async () => {
    loadingMc = true;
    try {
      const versions = await invoke("get_minecraft_versions");
      mcVersions = versions as { id: string; popular: boolean }[];
      if (!mcVersions.some((v) => v.id === minecraftVersion)) {
        minecraftVersion = mcVersions[0]?.id ?? "";
      }
      await loadDefaultHome();
      location = guessLocation();
      await loadLoaderVersions();
    } catch (e) {
      error = `Failed to load Minecraft versions: ${e}`;
    } finally {
      loadingMc = false;
    }
    unlistenPack = await listen<{ phase?: string; message?: string }>("modpack-install-progress", (event) => {
      packPhase = event.payload.message || event.payload.phase || "";
    });
  });

  onDestroy(() => {
    unlistenPack?.();
  });

  function guessLocation(): string {
    const home = (defaultHome ?? "").replace(/\/$/, "");
    return `${home}/TuffBox/instances/${slugify(activeName())}`;
  }

  let defaultHome = "";
  async function loadDefaultHome() {
    defaultHome = (await invoke("get_home_dir").catch(() => "")) as string;
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
    } catch {
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

  async function pickImportFile() {
    const selected = await open({
      multiple: false,
      filters: [
        { name: "Modpacks", extensions: ["mrpack", "zip"] },
        { name: "All", extensions: ["*"] },
      ],
      title: "Import modpack (.mrpack / CurseForge zip / Prism zip)",
    });
    if (selected && typeof selected === "string") {
      importPath = selected;
      const base = selected.replace(/\\/g, "/").split("/").pop() ?? "Imported pack";
      importName = base.replace(/\.(mrpack|zip)$/i, "");
      location = guessLocation();
    }
  }

  function slugify(value: string) {
    return value
      .toLowerCase()
      .replace(/[^a-z0-9-_]/g, "-")
      .replace(/--+/g, "-")
      .replace(/^-+|-+$/g, "");
  }

  function applyTemplateLoader(kindRaw: string) {
    const kind = kindRaw.toLowerCase();
    const allowed = ["fabric", "forge", "neoforge", "quilt", "vanilla"];
    if (allowed.includes(kind)) {
      loader = kind as "vanilla" | "fabric" | "forge" | "neoforge" | "quilt";
    }
  }

  // Name for the current mode drives the default location slug.
  function activeName(): string {
    if (mode === "import") return importName || name;
    if (mode === "curseforge") return cfName || name;
    return name;
  }

  // Page-per-type validation: each mode validates only its own inputs.
  $: blankValid = !!minecraftVersion && (loader === "vanilla" || !!loaderVersion);
  $: importValid = !!importPath;
  $: cfValid = !!cfSelected && cfFileId !== null;
  $: canCreate = mode === "blank" ? blankValid : mode === "import" ? importValid : cfValid;

  async function create() {
    if (!blankValid) {
      error = "Pick a Minecraft version and a loader version (or Vanilla).";
      return;
    }
    loading = true;
    error = "";
    installMessage = "";
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

  async function installFromFile() {
    if (!importValid) {
      error = "Pick a modpack file first.";
      return;
    }
    loading = true;
    error = "";
    installMessage = "Installing pack…";
    try {
      const parent = location.replace(/\\/g, "/").replace(/\/[^\/]+$/, "") || location;
      const result: any = await invoke("install_modpack", {
        source: importPath,
        targetDir: parent,
        instanceName: importName,
      });
      const failed = result?.download?.failed?.length ?? 0;
      if (failed > 0) {
        error = `Installed with ${failed} download failure(s) — open Content and Retry.`;
      }
      dispatch("created", result.path as string);
      dispatch("close");
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
      installMessage = "";
    }
  }

  async function searchCurseForge() {
    cfLoading = true;
    error = "";
    cfSelected = null;
    cfFiles = [];
    cfFileId = null;
    try {
      cfHits = await invoke("search_curseforge_modpacks", {
        query: cfQuery,
        gameVersion: null,
        offset: 0,
      });
      if (cfHits.length === 0) {
        installMessage = "No modpacks found.";
      }
    } catch (e) {
      error = `${e}`;
      cfHits = [];
    } finally {
      cfLoading = false;
    }
  }

  async function selectCfPack(hit: any) {
    cfSelected = hit;
    cfName = hit.name || cfName;
    cfFilesLoading = true;
    cfFiles = [];
    cfFileId = null;
    try {
      cfFiles = await invoke("get_curseforge_modpack_files", {
        modId: hit.id,
        gameVersion: null,
      });
      cfFileId = cfFiles[0]?.id ?? null;
    } catch (e) {
      error = `${e}`;
    } finally {
      cfFilesLoading = false;
    }
  }

  function onCfFileChange(e: Event) {
    const v = (e.currentTarget as HTMLSelectElement).value;
    cfFileId = v ? Number(v) : null;
  }

  async function installFromCurseForge() {
    if (!cfValid) {
      error = "Select a modpack file version.";
      return;
    }
    loading = true;
    error = "";
    installMessage = "Downloading CurseForge modpack…";
    try {
      const parent = location.replace(/\\/g, "/").replace(/\/[^\/]+$/, "") || location;
      const result: any = await invoke("install_modpack", {
        source: `cf:${cfSelected.id}:${cfFileId}`,
        targetDir: parent,
        instanceName: cfName,
      });
      const failed = result?.download?.failed?.length ?? 0;
      if (failed > 0) {
        error = `Installed with ${failed} download failure(s) — open Content and Retry.`;
      }
      dispatch("created", result.path as string);
      dispatch("close");
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
      installMessage = "";
      packPhase = "";
    }
  }

  $: if (mode === "blank" && (minecraftVersion || loader)) {
    if (!loadingMc) loadLoaderVersions();
  }
</script>

<div class="modal-backdrop" on:click={(e) => e.target === e.currentTarget && dispatch("close")} role="button" tabindex="-1" aria-label="Close" on:keydown={(e) => e.key === 'Enter' && dispatch('close')}>
  <div class="modal wide" role="dialog" aria-modal="true" aria-labelledby="add-instance-title" use:trapFocus={{ onEscape: () => dispatch("close") }}>
    <div class="modal-header">
      <h2 id="add-instance-title">Add Instance</h2>
      <button class="icon-btn" on:click={() => dispatch("close")} aria-label="Close add instance dialog">
        <X size={18} />
      </button>
    </div>

    <div class="tabs">
      <button class:active={mode === "blank"} on:click={() => (mode = "blank")}>Blank</button>
      <button class:active={mode === "import"} on:click={() => (mode = "import")}>Import pack</button>
       <button class:active={mode === "curseforge"} on:click={() => (mode = "curseforge")}>CurseForge</button>
     </div>

     <div class="modal-body">
       {#if error}
         <div class="error">{error}</div>
       {/if}
       {#if installMessage || packPhase}
         <div class="notice">{packPhase || installMessage}</div>
       {/if}

       {#if mode === "blank"}
         <!-- Page 1: typed, self-validating blank instance fields -->
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
                   applyTemplateLoader(String(tpl.manifest.loader.kind));
                 }
                 if (tpl.manifest?.loader?.version) loaderVersion = tpl.manifest.loader.version;
                 useTemplate = false;
                 loadLoaderVersions();
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
           <input id="inst-name" bind:value={name} on:input={() => (location = guessLocation())} />
         </div>

         <div class="field">
           <label for="inst-mc">Minecraft version</label>
           {#if loadingMc}
             <div class="field-loader"><Loader2 size={16} class="spin" /> Loading versions...</div>
           {:else}
             <select id="inst-mc" bind:value={minecraftVersion}>
               {#each mcVersions as v}
                 <option value={v.id}>{v.id}{#if v.popular} ★{/if}</option>
               {/each}
             </select>
           {/if}
         </div>

         <div class="field-row">
           <div class="field">
             <label for="inst-loader">Loader</label>
             <select id="inst-loader" bind:value={loader} on:change={() => loadLoaderVersions()}>
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
               <input id="inst-loader-version" value="No loader (Vanilla)" disabled />
             {:else}
               <select id="inst-loader-version" bind:value={loaderVersion}>
                 {#each loaderVersions as v}
                   <option value={v.id}>{v.id}{#if v.stable} (stable){/if}</option>
                 {/each}
               </select>
             {/if}
           </div>
         </div>
       {:else if mode === "import"}
         <!-- Page 2: import — name derived from file, isolated -->
         <p class="muted">Import a Modrinth <code>.mrpack</code>, CurseForge zip, or Prism instance zip — mods download automatically (Prism-style).</p>
         <div class="field">
           <label for="inst-pack-file">Pack file</label>
           <div class="input-row">
             <input id="inst-pack-file" bind:value={importPath} placeholder="path/to/pack.mrpack or .zip" />
             <button class="secondary" on:click={pickImportFile} aria-label="Choose modpack file"><Folder size={16} /></button>
           </div>
         </div>
         <div class="field">
           <label for="inst-name-imp">Instance name</label>
           <input id="inst-name-imp" bind:value={importName} on:input={() => (location = guessLocation())} />
         </div>
       {:else}
         <!-- Page 3: CurseForge browse — name + file selection isolated -->
         <p class="muted">Browse CurseForge modpacks (same API as PrismLauncher Flame).</p>
         <div class="search-row">
           <div class="search">
             <Search size={16} />
             <input aria-label="Search CurseForge modpacks" bind:value={cfQuery} placeholder="Search modpacks…" on:keydown={(e) => e.key === "Enter" && searchCurseForge()} />
           </div>
           <button class="secondary" on:click={searchCurseForge} disabled={cfLoading}>
             {#if cfLoading}<Loader2 size={16} class="spin" />{:else}<Search size={16} />{/if}
             Search
           </button>
         </div>
         <div class="cf-layout">
           <div class="cf-list">
             {#each cfHits as hit}
               <button class="cf-row" class:active={cfSelected?.id === hit.id} on:click={() => selectCfPack(hit)}>
                 {#if hit.iconUrl}
                   <img src={hit.iconUrl} alt="" />
                 {:else}
                   <span class="cf-icon"><Package size={18} /></span>
                 {/if}
                 <div>
                   <strong>{hit.name}</strong>
                   <span>{hit.summary?.slice(0, 100) ?? ""}</span>
                 </div>
               </button>
             {:else}
               <div class="muted compact">{cfLoading ? "Searching…" : "Search for a modpack to begin."}</div>
             {/each}
           </div>
           <div class="cf-detail">
             {#if cfSelected}
               <h3>{cfSelected.name}</h3>
               {#if cfFilesLoading}
                 <div class="field-loader"><Loader2 size={16} class="spin" /> Loading versions…</div>
               {:else}
                 <label for="cf-file">Pack version</label>
                 <select id="cf-file" value={cfFileId ?? ""} on:change={onCfFileChange}>
                   {#each cfFiles as f}
                     <option value={f.id}>{f.displayName} · {(f.gameVersions || []).slice(0, 3).join(", ")}</option>
                   {/each}
                 </select>
                 <div class="field" style="margin-top:12px">
                   <label for="inst-name-cf">Instance name</label>
                   <input id="inst-name-cf" bind:value={cfName} on:input={() => (location = guessLocation())} />
                 </div>
               {/if}
             {:else}
               <div class="muted compact">Select a pack to choose its file version.</div>
             {/if}
           </div>
         </div>
       {/if}

       <div class="field">
         <label for="inst-location">Location</label>
         <div class="input-row">
           <input id="inst-location" bind:value={location} />
           <button class="secondary" on:click={selectLocation} aria-label="Choose instance location"><Folder size={16} /></button>
         </div>
       </div>
     </div>

      <div class="modal-footer">
        <button class="ghost" on:click={() => dispatch("close")} disabled={loading}>Cancel</button>
        {#if mode === "blank"}
          <LoadingButton {loading} disabled={!blankValid} on:click={create}>
            Create instance
          </LoadingButton>
        {:else if mode === "import"}
          <LoadingButton {loading} disabled={!importValid} on:click={installFromFile}>
            <Download size={16} /> Install pack
          </LoadingButton>
        {:else}
          <LoadingButton {loading} disabled={!cfValid} on:click={installFromCurseForge}>
            <Download size={16} /> Install from CurseForge
          </LoadingButton>
        {/if}
      </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed; inset: 0; z-index: 80;
    background: rgba(0,0,0,0.55);
    display: flex; align-items: center; justify-content: center;
    padding: 24px;
  }
  .modal {
    width: min(560px, 100%);
    max-height: min(90vh, 820px);
    overflow: auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 18px;
    display: flex; flex-direction: column;
  }
  .modal.wide { width: min(880px, 100%); }
  .modal-header {
    display: flex; justify-content: space-between; align-items: center;
    padding: 18px 20px 8px;
  }
  .modal-header h2 { margin: 0; font-size: 20px; }
  .tabs {
    display: flex; gap: 6px; padding: 0 20px 8px;
  }
  .tabs button {
    background: transparent; border: 1px solid transparent; color: var(--text-muted);
    padding: 8px 12px; border-radius: 999px; font-weight: 600;
  }
  .tabs button.active {
    border-color: rgba(27,217,106,.35);
    background: rgba(27,217,106,.1);
    color: var(--accent-primary);
  }
   .modal-body { padding: 8px 20px 16px; display: flex; flex-direction: column; gap: 14px; }
  .modal-footer {
    display: flex; justify-content: flex-end; gap: 10px;
    padding: 12px 20px 18px; border-top: 1px solid var(--border-color);
  }
   .field { display: grid; gap: 6px; }
   .field label { font-size: 12px; color: var(--text-muted); font-weight: 600; }
   .field input, .field select, .cf-detail select {
     box-sizing: border-box; width: 100%; height: 42px; padding: 0 12px; border-radius: 10px;
     border: 1px solid var(--border-color); background: var(--bg-tertiary); color: var(--text-primary);
     font-size: 14px; line-height: 1;
   }
   .field input:disabled {
     color: var(--text-muted); background: var(--bg-elevated); cursor: not-allowed;
     opacity: 0.85;
   }
  .field-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .input-row { display: flex; gap: 8px; }
  .input-row input { flex: 1; }
  .error {
    padding: 10px 12px; border-radius: 10px;
    background: rgba(239,68,68,.12); border: 1px solid rgba(239,68,68,.35); color: #fca5a5;
  }
  .notice {
    padding: 10px 12px; border-radius: 10px;
    background: rgba(27,217,106,.08); border: 1px solid rgba(27,217,106,.25); color: var(--accent-primary);
  }
  .muted { color: var(--text-muted); font-size: 13px; }
  .muted.compact { padding: 16px; text-align: center; }
  .field-loader { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; }
   .template-btn { align-self: flex-start; }
  .template-list { display: grid; gap: 6px; }
  .template-row {
    display: grid; text-align: left; gap: 2px; padding: 10px 12px;
    border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); color: var(--text-primary);
  }
  .template-row span { color: var(--text-muted); font-size: 12px; }
  .search-row { display: flex; gap: 8px; }
  .search {
    flex: 1; display: flex; align-items: center; gap: 8px;
    padding: 0 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary);
  }
  .search input { border: 0; background: transparent; color: var(--text-primary); width: 100%; padding: 10px 0; }
   .cf-layout {
     display: grid; grid-template-columns: 1.2fr 0.9fr; gap: 12px;
     min-height: 300px; max-height: 380px;
   }
  .cf-list { overflow: auto; display: grid; gap: 6px; align-content: start; }
  .cf-row {
    display: grid; grid-template-columns: 40px 1fr; gap: 10px; text-align: left;
    padding: 10px; border-radius: 12px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-secondary);
  }
  .cf-row.active, .cf-row:hover { border-color: rgba(27,217,106,.4); background: rgba(27,217,106,.06); }
  .cf-row img, .cf-icon {
    width: 40px; height: 40px; border-radius: 10px; object-fit: cover;
    background: var(--bg-elevated); display: flex; align-items: center; justify-content: center;
  }
  .cf-row strong { display: block; color: var(--text-primary); font-size: 13px; }
  .cf-row span { font-size: 11px; color: var(--text-muted); }
  .cf-detail {
    border: 1px solid var(--border-color); border-radius: 14px; padding: 12px;
    background: var(--bg-tertiary); overflow: auto;
  }
  .cf-detail h3 { margin: 0 0 12px; font-size: 16px; }
  .icon-btn {
    background: transparent; border: 0; color: var(--text-muted); cursor: pointer;
  }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) {
    .cf-layout, .field-row { grid-template-columns: 1fr; }
  }
</style>
