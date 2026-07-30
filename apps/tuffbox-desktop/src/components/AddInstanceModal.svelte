<script lang="ts">
  import { X, Folder, Loader2, Download, Search, Package } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { projectPath } from "../lib/store";
  import { trapFocus } from "../lib/focusTrap";
  import LoadingButton from "./LoadingButton.svelte";

  let {
    onclose,
    oncreated,
  }: {
    onclose?: () => void;
    oncreated?: (path: string) => void;
  } = $props();

  // Each "input type" on the home screen is an isolated, self-validating
  // page (PrismLauncher-style: page-per-type) instead of one shared
  // form whose fields leak across modes.
  type CreateMode = "blank" | "import" | "curseforge";
  let mode: CreateMode = $state("blank");

  // --- Blank instance (explicit, typed inputs) ---
  let name = $state("New Instance");
  let minecraftVersion = $state("1.20.1");
  let loader: "vanilla" | "fabric" | "forge" | "neoforge" | "quilt" = $state("fabric");
  let loaderVersion = $state("");
  let mcVersions = $state<{ id: string; popular: boolean }[]>([]);
  let loaderVersions = $state<{ id: string; stable: boolean }[]>([]);
  let loadingMc = $state(true);
  let loadingLoader = $state(false);
  let memoryMode: "auto" | "manual" = $state("auto");
  let recommendedMemoryMb = $state(8192);
  let memoryMb = $state(8192);
  let memoryMaxMb = $state(16384);
  let jvmArgs = $state("-XX:+UseG1GC");

  // --- Templates (blank helper) ---
  let templates = $state<any[]>([]);
  let templatesLoaded = $state(false);
  let useTemplate = $state(false);

  // --- Import pack (.mrpack / zip) ---
  let importName = $state("New Instance");
  let importPath = $state("");

  // --- CurseForge browse ---
  let cfName = $state("New Instance");
  let cfQuery = $state("");
  let cfHits = $state<any[]>([]);
  let cfLoading = $state(false);
  let cfSelected = $state<any>(null);
  let cfFiles = $state<any[]>([]);
  let cfFilesLoading = $state(false);
  let cfFileId = $state<number | null>(null);

  // --- Shared ---
  let location = $state("");
  let loading = $state(false);
  let error = $state("");
  let installMessage = $state("");
  let packPhase = $state("");

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
      try {
        const settings = await invoke<{ defaultMemoryMb?: number }>("get_launcher_settings");
        if (settings?.defaultMemoryMb && settings.defaultMemoryMb >= 1024) {
          recommendedMemoryMb = settings.defaultMemoryMb;
          memoryMb = settings.defaultMemoryMb;
        }
      } catch {
        /* keep defaults */
      }
      try {
        // Rough upper bound for the slider from JS (navigator), fallback 16G.
        const deviceMemGb = (navigator as any).deviceMemory as number | undefined;
        if (deviceMemGb && deviceMemGb > 0) {
          memoryMaxMb = Math.max(8192, Math.min(65536, Math.floor(deviceMemGb * 1024 * 0.75)));
        }
      } catch {
        /* keep default max */
      }
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
    const home = (defaultHome ?? "").replace(/[\\/]+$/, "");
    if (mode === "import" || mode === "curseforge") {
      // Pack install uses this as the parent folder; instance subfolder is created inside.
      return home;
    }
    return `${home}/${slugify(activeName())}`;
  }

  let defaultHome = $state("");
  async function loadDefaultHome() {
    try {
      const info = await invoke<{ current: string; default: string }>("get_instances_path_info");
      defaultHome = info.current || info.default || "";
    } catch {
      defaultHome = ((await invoke("get_home_dir").catch(() => "")) as string);
      if (defaultHome) defaultHome = `${defaultHome.replace(/[\\/]+$/, "")}/TuffBox/instances`;
    }
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
      title: "Select folder for this instance",
      defaultPath: defaultHome || undefined,
    });
    if (selected && typeof selected === "string") {
      // For blank create, location is the instance folder itself.
      // For pack install we strip the leaf name — keep full path here and let install* use parent.
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
  const blankValid = $derived(!!minecraftVersion && (loader === "vanilla" || !!loaderVersion));
  const importValid = $derived(!!importPath);
  const cfValid = $derived(!!cfSelected && cfFileId !== null);
  const canCreate = $derived(mode === "blank" ? blankValid : mode === "import" ? importValid : cfValid);

  async function create() {
    if (!blankValid) {
      error = "Pick a Minecraft version and a loader version (or Vanilla).";
      return;
    }
    loading = true;
    error = "";
    installMessage = "";
    try {
      const mem = memoryMode === "auto" ? recommendedMemoryMb : memoryMb;
      const args = jvmArgs
        .split(/\s+/)
        .map((s) => s.trim())
        .filter(Boolean);
      const path = await invoke("create_instance", {
        name,
        minecraftVersion,
        loader,
        loaderVersion,
        location,
        memoryMb: mem,
        jvmArgs: args.length ? args : ["-XX:+UseG1GC"],
      });
      oncreated?.(path as string);
      onclose?.();
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  function pickLoader(id: string) {
    loader = id as typeof loader;
    loadLoaderVersions();
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
      const targetDir = location.replace(/[\\/]+$/, "") || defaultHome;
      const result: any = await invoke("install_modpack", {
        source: importPath,
        targetDir,
        instanceName: importName,
      });
      const failed = result?.download?.failed?.length ?? 0;
      if (failed > 0) {
        error = `Installed with ${failed} download failure(s) — open Content and Retry.`;
      }
      oncreated?.(result.path as string);
      onclose?.();
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
      const targetDir = location.replace(/[\\/]+$/, "") || defaultHome;
      const result: any = await invoke("install_modpack", {
        source: `cf:${cfSelected.id}:${cfFileId}`,
        targetDir,
        instanceName: cfName,
      });
      const failed = result?.download?.failed?.length ?? 0;
      if (failed > 0) {
        error = `Installed with ${failed} download failure(s) — open Content and Retry.`;
      }
      oncreated?.(result.path as string);
      onclose?.();
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
      installMessage = "";
      packPhase = "";
    }
  }

  $effect(() => {
    if (mode === "blank" && (minecraftVersion || loader)) {
      if (!loadingMc) loadLoaderVersions();
    }
  });
</script>

<div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && onclose?.()} role="button" tabindex="-1" aria-label="Close" onkeydown={(e) => e.key === 'Enter' && onclose?.()}>
  <div class="modal" class:wide={mode !== "blank"} role="dialog" aria-modal="true" aria-labelledby="add-instance-title" use:trapFocus={{ onEscape: () => onclose?.() }}>
    <div class="modal-hero">
      <div class="hero-copy">
        <h2 id="add-instance-title">Create modpack</h2>
        <p>Name it, pick loader + Minecraft, set memory — then build in IDE.</p>
      </div>
      <button class="icon-btn hero-close" onclick={() => onclose?.()} aria-label="Close add instance dialog">
        <X size={18} />
      </button>
    </div>

    <div class="tabs">
      <button class:active={mode === "blank"} onclick={() => { mode = "blank"; location = guessLocation(); }}>Blank</button>
      <button class:active={mode === "import"} onclick={() => { mode = "import"; location = guessLocation(); }}>Import pack</button>
      <button class:active={mode === "curseforge"} onclick={() => { mode = "curseforge"; location = guessLocation(); }}>CurseForge</button>
    </div>

    <div class="modal-body">
      {#if error}
        <div class="error">{error}</div>
      {/if}
      {#if installMessage || packPhase}
        <div class="notice">{packPhase || installMessage}</div>
      {/if}

      {#if mode === "blank"}
        <button class="ghost template-btn" onclick={() => { useTemplate = !useTemplate; if (useTemplate && !templatesLoaded) loadTemplates(); }}>
          {useTemplate ? "Create from scratch" : "Use template"}
        </button>

        {#if useTemplate && templates.length > 0}
          <div class="template-list">
            {#each templates.slice(0, 5) as tpl, i (tpl.name || i)}
              <button class="template-row" onclick={() => {
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

        <div class="name-row">
          <div class="pack-icon" aria-hidden="true">{(name.trim()[0] || "?").toUpperCase()}</div>
          <div class="field grow">
            <label for="inst-name">Profile name</label>
            <input id="inst-name" bind:value={name} placeholder="Enter pack name" oninput={() => (location = guessLocation())} />
          </div>
        </div>

        <div class="field">
          <span class="field-label">Loader</span>
          <div class="loader-chips" role="radiogroup" aria-label="Loader">
            {#each loaders as l (l.id)}
              <button
                type="button"
                class="loader-chip"
                class:active={loader === l.id}
                role="radio"
                aria-checked={loader === l.id}
                onclick={() => pickLoader(l.id)}
              >{l.label}</button>
            {/each}
          </div>
        </div>

        <div class="field-row">
          <div class="field">
            <label for="inst-mc">Minecraft version</label>
            {#if loadingMc}
              <div class="field-loader"><Loader2 size={16} class="spin" /> Loading versions...</div>
            {:else}
              <select id="inst-mc" bind:value={minecraftVersion}>
                {#each mcVersions as v (v.id)}
                  <option value={v.id}>{v.id}{#if v.popular} ★{/if}</option>
                {/each}
              </select>
            {/if}
          </div>
          <div class="field">
            <label for="inst-loader-version">Loader version</label>
            {#if loadingLoader}
              <div class="field-loader"><Loader2 size={16} class="spin" /> Loading...</div>
            {:else if loader === "vanilla"}
              <input id="inst-loader-version" value="No loader (Vanilla)" disabled />
            {:else}
              <select id="inst-loader-version" bind:value={loaderVersion}>
                {#each loaderVersions as v (v.id)}
                  <option value={v.id}>{v.id}{#if v.stable} (stable){/if}</option>
                {/each}
              </select>
            {/if}
          </div>
        </div>

        <div class="field">
          <span class="field-label">Memory</span>
          <div class="loader-chips" role="radiogroup" aria-label="Memory">
            <button
              type="button"
              class="loader-chip"
              class:active={memoryMode === "auto"}
              role="radio"
              aria-checked={memoryMode === "auto"}
              onclick={() => {
                memoryMode = "auto";
                memoryMb = recommendedMemoryMb;
              }}
            >Recommended · {recommendedMemoryMb} MB</button>
            <button
              type="button"
              class="loader-chip"
              class:active={memoryMode === "manual"}
              role="radio"
              aria-checked={memoryMode === "manual"}
              onclick={() => (memoryMode = "manual")}
            >Custom</button>
          </div>
          {#if memoryMode === "manual"}
            <div class="mem-slider">
              <div class="mem-value">{memoryMb} MB</div>
              <input
                class="mem-range"
                type="range"
                min="1024"
                max={memoryMaxMb}
                step="256"
                bind:value={memoryMb}
              />
              <div class="mem-scale">
                <span>1 GB</span>
                <span>{Math.round(memoryMaxMb / 1024)} GB</span>
              </div>
            </div>
          {/if}
        </div>

        <div class="field">
          <label for="inst-jvm">Launch arguments</label>
          <input id="inst-jvm" bind:value={jvmArgs} placeholder="Extra Java arguments" />
        </div>
      {:else if mode === "import"}
         <!-- Page 2: import — name derived from file, isolated -->
         <p class="muted">Import a Modrinth <code>.mrpack</code>, CurseForge zip, or Prism instance zip — mods download automatically (Prism-style).</p>
         <div class="field">
           <label for="inst-pack-file">Pack file</label>
           <div class="input-row">
             <input id="inst-pack-file" bind:value={importPath} placeholder="path/to/pack.mrpack or .zip" />
             <button class="secondary" onclick={pickImportFile} aria-label="Choose modpack file"><Folder size={16} /></button>
           </div>
         </div>
         <div class="field">
           <label for="inst-name-imp">Instance name</label>
           <input id="inst-name-imp" bind:value={importName} oninput={() => (location = guessLocation())} />
         </div>
       {:else}
         <!-- Page 3: CurseForge browse — name + file selection isolated -->
         <p class="muted">Browse CurseForge modpacks (same API as PrismLauncher Flame).</p>
         <div class="search-row">
           <div class="search">
             <Search size={16} />
             <input aria-label="Search CurseForge modpacks" bind:value={cfQuery} placeholder="Search modpacks…" onkeydown={(e) => e.key === "Enter" && searchCurseForge()} />
           </div>
           <button class="secondary" onclick={searchCurseForge} disabled={cfLoading}>
             {#if cfLoading}<Loader2 size={16} class="spin" />{:else}<Search size={16} />{/if}
             Search
           </button>
         </div>
         <div class="cf-layout">
           <div class="cf-list">
             {#each cfHits as hit (hit.id)}
               <button class="cf-row" class:active={cfSelected?.id === hit.id} onclick={() => selectCfPack(hit)}>
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
                 <select id="cf-file" value={cfFileId ?? ""} onchange={onCfFileChange}>
                   {#each cfFiles as f (f.id)}
                     <option value={f.id}>{f.displayName} · {(f.gameVersions || []).slice(0, 3).join(", ")}</option>
                   {/each}
                 </select>
                 <div class="field" style="margin-top:12px">
                   <label for="inst-name-cf">Instance name</label>
                   <input id="inst-name-cf" bind:value={cfName} oninput={() => (location = guessLocation())} />
                 </div>
               {/if}
             {:else}
               <div class="muted compact">Select a pack to choose its file version.</div>
             {/if}
           </div>
         </div>
       {/if}

       <div class="field">
         <label for="inst-location">{mode === "blank" ? "Instance folder" : "Download folder"}</label>
         <div class="input-row">
           <input id="inst-location" bind:value={location} placeholder={mode === "blank" ? "Folder for this instance" : "Parent folder for the modpack"} />
           <button class="secondary" onclick={selectLocation} aria-label="Choose location"><Folder size={16} /></button>
         </div>
         {#if mode !== "blank"}
           <span class="path-hint">The modpack will be installed as a new folder inside this path.</span>
         {/if}
       </div>
     </div>

      <div class="modal-footer">
        <button class="ghost" onclick={() => onclose?.()} disabled={loading}>Cancel</button>
        {#if mode === "blank"}
          <LoadingButton {loading} disabled={!blankValid} onclick={create}>
            Create
          </LoadingButton>
        {:else if mode === "import"}
          <LoadingButton {loading} disabled={!importValid} onclick={installFromFile}>
            <Download size={16} /> Install pack
          </LoadingButton>
        {:else}
          <LoadingButton {loading} disabled={!cfValid} onclick={installFromCurseForge}>
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
  .modal-hero {
    position: relative;
    padding: 22px 20px 16px;
    background:
      radial-gradient(ellipse at 20% 0%, rgba(27, 217, 106, 0.28), transparent 55%),
      radial-gradient(ellipse at 90% 40%, rgba(59, 130, 246, 0.18), transparent 50%),
      linear-gradient(160deg, #12161c 0%, #1a222c 100%);
    border-bottom: 1px solid var(--border-color);
  }
  .hero-copy h2 {
    margin: 0 0 6px;
    font-size: 22px;
    color: #fff;
  }
  .hero-copy p {
    margin: 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.72);
    max-width: 42ch;
  }
  .hero-close {
    position: absolute;
    top: 14px;
    right: 14px;
    color: rgba(255, 255, 255, 0.8);
  }
  .tabs {
    display: flex; gap: 6px; padding: 12px 20px 8px;
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
  .field.grow { flex: 1; min-width: 0; }
  .field label, .field-label { font-size: 12px; color: var(--text-muted); font-weight: 600; }
  .field input:not([type="radio"]):not([type="range"]):not([type="checkbox"]),
  .field select,
  .cf-detail select {
    box-sizing: border-box; width: 100%; height: 42px; padding: 0 12px; border-radius: 10px;
    border: 1px solid var(--border-color); background: var(--bg-tertiary); color: var(--text-primary);
    font-size: 14px; line-height: 1;
  }
  .field input:disabled {
    color: var(--text-muted); background: var(--bg-elevated); cursor: not-allowed;
    opacity: 0.85;
  }
  .name-row {
    display: flex;
    gap: 12px;
    align-items: flex-end;
  }
  .pack-icon {
    width: 56px;
    height: 56px;
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    font-weight: 800;
    color: #04140a;
    background: linear-gradient(135deg, #1bd96a, #0ea5e9);
    flex-shrink: 0;
  }
  .loader-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .loader-chip {
    padding: 8px 14px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .loader-chip.active {
    border-color: rgba(27, 217, 106, 0.45);
    background: rgba(27, 217, 106, 0.14);
    color: var(--accent-primary);
  }
  .mem-slider { display: grid; gap: 8px; margin-top: 8px; }
  .mem-value {
    font-size: 20px;
    font-weight: 800;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }
  .mem-range {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 8px;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--bg-elevated);
    outline: none;
  }
  .mem-range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent-primary);
    border: 2px solid #0b1a10;
    cursor: pointer;
    box-shadow: 0 0 0 3px rgba(27, 217, 106, 0.25);
  }
  .mem-range::-moz-range-thumb {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent-primary);
    border: 2px solid #0b1a10;
    cursor: pointer;
  }
  .mem-scale {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-muted);
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
  .path-hint { font-size: 11px; color: var(--text-muted); }
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
    padding: 10px; border-radius: var(--border-radius-md); border: 1px solid var(--border-color);
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
