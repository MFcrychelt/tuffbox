<script lang="ts">
  import { X, Folder, Loader2, Download, Search, Package, ImagePlus } from "@lucide/svelte";
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { convertFileSrc, invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import { projectPath, libraryTabRequest } from "../lib/store";
  import { trapFocus } from "../lib/focusTrap";
  import LoadingButton from "./LoadingButton.svelte";
  import CatalogProjectView from "./CatalogProjectView.svelte";

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
  type CreateMode = "blank" | "import" | "catalog";
  let mode: CreateMode = $state("blank");

  // --- Blank instance (explicit, typed inputs) ---
  let name = $state("New Instance");
  let minecraftVersion = $state("1.20.1");
  let loader = $state<"vanilla" | "fabric" | "forge" | "neoforge" | "quilt">("fabric");
  let loaderVersion = $state("");
  let mcVersions = $state<{ id: string; popular: boolean }[]>([]);
  let loaderVersions = $state<{ id: string; stable: boolean }[]>([]);
  let loadingMc = $state(true);
  let loadingLoader = $state(false);
  let loaderRequestId = 0;
  let memoryMode = $state<"auto" | "manual">("auto");
  let recommendedMemoryMb = $state(8192);
  let memoryMb = $state(8192);
  /** Hard cap for Create modpack memory slider / presets. */
  const MEMORY_MAX_MB = 64 * 1024;
  const MEMORY_MIN_MB = 4 * 1024;
  const MEMORY_PRESETS_GB = [4, 8, 10, 12, 16, 24, 32, 36, 48, 64] as const;
  let memoryMaxMb = $state(MEMORY_MAX_MB);
  let jvmArgs = $state("-XX:+UseG1GC");
  let iconSourcePath = $state<string | null>(null);
  let iconPreviewUrl = $state<string | null>(null);

  // --- Templates (blank helper) ---
  let templates = $state<any[]>([]);
  let templatesLoaded = $state(false);
  let useTemplate = $state(false);

  // --- Import pack (.mrpack / zip) ---
  let importName = $state("New Instance");
  let importPath = $state("");

  // --- Catalog browse (Modrinth / CurseForge) ---
  let catalogProvider = $state<"modrinth" | "curseforge">("modrinth");
  let cfName = $state("New Instance");
  let cfQuery = $state("");
  let cfHits = $state<any[]>([]);
  let cfLoading = $state(false);
  let cfSelected = $state<any>(null);
  let cfFiles = $state<any[]>([]);
  let cfFilesLoading = $state(false);
  let cfFileId = $state<number | null>(null);
  let catalogViewResult = $state<{
    id: string;
    slug: string;
    name: string;
    description: string;
    projectType: string;
    iconUrl?: string | null;
    author?: string | null;
    downloads?: number | null;
    follows?: number | null;
    categories?: string[];
    provider?: string;
  } | null>(null);
  let catalogInstalling = $state(false);

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
          recommendedMemoryMb = Math.min(MEMORY_MAX_MB, Math.max(MEMORY_MIN_MB, settings.defaultMemoryMb));
          memoryMb = recommendedMemoryMb;
        }
      } catch {
        /* keep defaults */
      }
      memoryMaxMb = MEMORY_MAX_MB;
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
    if (mode === "import" || mode === "catalog") {
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
    if (loader === "vanilla") {
      loaderVersions = [];
      loaderVersion = "";
      loadingLoader = false;
      return;
    }
    if (!minecraftVersion) {
      loaderVersions = [];
      loaderVersion = "";
      return;
    }
    const reqId = ++loaderRequestId;
    loadingLoader = true;
    try {
      const versions = await invoke<{ id: string; stable: boolean }[]>("get_loader_versions", {
        loader,
        minecraftVersion,
      });
      if (reqId !== loaderRequestId) return;
      loaderVersions = versions ?? [];
      loaderVersion =
        loaderVersions.find((v) => v.stable)?.id ?? loaderVersions[0]?.id ?? "";
      if (loaderVersions.length === 0) {
        error = `No ${loader} versions found for Minecraft ${minecraftVersion}.`;
      } else if (error.startsWith("No ") || error.startsWith("Failed to load")) {
        error = "";
      }
    } catch (e) {
      if (reqId !== loaderRequestId) return;
      loaderVersions = [];
      loaderVersion = "";
      error = `Failed to load ${loader} versions: ${e}`;
    } finally {
      if (reqId === loaderRequestId) loadingLoader = false;
    }
  }

  async function pickIcon() {
    try {
      const selected = await open({
        multiple: false,
        title: "Choose pack icon",
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      iconSourcePath = selected;
      try {
        iconPreviewUrl = convertFileSrc(selected);
      } catch {
        iconPreviewUrl = null;
      }
    } catch (e) {
      error = `Could not choose icon: ${e}`;
    }
  }

  function goToDiscover() {
    libraryTabRequest.set("discover");
    onclose?.();
    window.dispatchEvent(new CustomEvent("tuffbox:open-library"));
  }

  function setMemoryPresetGb(gb: number) {
    memoryMode = "manual";
    memoryMb = Math.min(MEMORY_MAX_MB, Math.max(MEMORY_MIN_MB, gb * 1024));
  }

  function clearIcon() {
    iconSourcePath = null;
    iconPreviewUrl = null;
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
    if (mode === "catalog") return cfName || name;
    return name;
  }

  // Page-per-type validation: each mode validates only its own inputs.
  const blankValid = $derived(!!minecraftVersion && (loader === "vanilla" || !!loaderVersion));
  const importValid = $derived(!!importPath);
  const cfValid = $derived(
    catalogProvider === "curseforge"
      ? !!cfSelected && cfFileId !== null
      : !!cfSelected,
  );

  async function create() {
    if (!blankValid) {
      error = "Pick a Minecraft version and a loader version (or Vanilla).";
      return;
    }
    loading = true;
    error = "";
    installMessage = "";
    try {
      const rawMem = memoryMode === "auto" ? recommendedMemoryMb : memoryMb;
      const mem = Math.min(MEMORY_MAX_MB, Math.max(MEMORY_MIN_MB, rawMem));
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
      if (iconSourcePath) {
        try {
          await invoke("set_project_listing_icon", {
            path,
            sourceFile: iconSourcePath,
          });
        } catch (iconErr) {
          error = `Pack created, but icon failed: ${iconErr}`;
        }
      }
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
    void loadLoaderVersions();
  }

  function openPackInCatalog(hit: any) {
    const id = String(hit.id ?? "");
    const slug = String(hit.slug || hit.id || "");
    if (!id && !slug) return;
    cfSelected = hit;
    cfName = hit.name || cfName;
    catalogViewResult = {
      id,
      slug,
      name: hit.name || slug,
      description: hit.summary || hit.description || "",
      projectType: "modpack",
      iconUrl: hit.iconUrl ?? null,
      author: hit.authors?.[0] ?? hit.author ?? null,
      downloads: hit.downloadCount ?? hit.downloads ?? null,
      follows: hit.follows ?? null,
      categories: hit.categories ?? [],
      provider: catalogProvider,
    };
    if (catalogProvider === "curseforge") {
      void selectCfPack(hit);
    } else {
      cfFiles = [];
      cfFileId = null;
      cfFilesLoading = false;
    }
  }

  async function openCatalogExternal() {
    if (!catalogViewResult) return;
    const slugOrId = (catalogViewResult.slug || catalogViewResult.id || "").trim();
    if (!slugOrId) return;
    const url =
      catalogViewResult.provider === "curseforge"
        ? /^\d+$/.test(slugOrId)
          ? `https://www.curseforge.com/projects/${slugOrId}`
          : `https://www.curseforge.com/minecraft/modpacks/${slugOrId}`
        : `https://modrinth.com/modpack/${slugOrId}`;
    try {
      await openExternal(url);
    } catch (e) {
      error = `Could not open link: ${e}`;
    }
  }

  async function installFromCatalogView() {
    if (!catalogViewResult) return;
    catalogInstalling = true;
    try {
      if (catalogViewResult.provider === "curseforge") {
        if (!cfSelected || cfFileId == null) {
          await selectCfPack({
            id: Number(catalogViewResult.id) || catalogViewResult.id,
            slug: catalogViewResult.slug,
            name: catalogViewResult.name,
          });
        }
        catalogViewResult = null;
        await installFromCurseForge();
      } else {
        catalogViewResult = null;
        await installFromModrinth(cfSelected);
      }
    } finally {
      catalogInstalling = false;
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

  async function searchCatalog() {
    cfLoading = true;
    error = "";
    installMessage = "";
    cfSelected = null;
    cfFiles = [];
    cfFileId = null;
    try {
      if (catalogProvider === "curseforge") {
        cfHits = await invoke("search_curseforge_modpacks", {
          query: cfQuery,
          gameVersion: null,
          offset: 0,
        });
      } else {
        const page = await invoke<{ results: any[]; total: number }>("search_modrinth_mods", {
          path: "",
          query: cfQuery.trim(),
          gameVersion: null,
          loader: null,
          category: null,
          environment: null,
          license: null,
          sort: "downloads",
          contentType: "modpack",
          page: 1,
          pageSize: 30,
        });
        cfHits = (page.results ?? []).map((r) => ({
          ...r,
          summary: r.description,
          downloadCount: r.downloads,
          authors: r.author ? [r.author] : [],
        }));
      }
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

  async function installFromModrinth(hit: any) {
    if (!hit?.id && !hit?.slug) {
      error = "Select a Modrinth modpack first.";
      return;
    }
    loading = true;
    error = "";
    installMessage = "Downloading Modrinth modpack…";
    try {
      const targetDir = location.replace(/[\\/]+$/, "") || defaultHome;
      const projectId = String(hit.id || hit.slug);
      const source = await invoke<string>("get_modrinth_pack_download", { projectId });
      const result: any = await invoke("install_modpack", {
        source,
        targetDir,
        instanceName: cfName || hit.name,
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

  async function installFromCatalogFooter() {
    if (catalogProvider === "modrinth") {
      await installFromModrinth(cfSelected);
    } else {
      await installFromCurseForge();
    }
  }

  $effect(() => {
    if (mode !== "blank") return;
    const mc = minecraftVersion;
    const ld = loader;
    if (!mc) return;
    void ld;
    void loadLoaderVersions();
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
      <button type="button" onclick={goToDiscover}>Browse packs</button>
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
          <button
            type="button"
            class="pack-icon"
            class:has-img={!!iconPreviewUrl}
            onclick={pickIcon}
            title="Choose pack icon"
            aria-label="Choose pack icon"
          >
            {#if iconPreviewUrl}
              <img src={iconPreviewUrl} alt="" />
            {:else}
              <span class="pack-icon-letter">{(name.trim()[0] || "?").toUpperCase()}</span>
              <span class="pack-icon-hint"><ImagePlus size={14} /></span>
            {/if}
          </button>
          <div class="field grow">
            <label for="inst-name">Profile name</label>
            <input id="inst-name" bind:value={name} placeholder="Enter pack name" oninput={() => (location = guessLocation())} />
            {#if iconSourcePath}
              <button type="button" class="clear-icon" onclick={clearIcon}>Remove icon</button>
            {/if}
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
            {:else if loaderVersions.length === 0}
              <input id="inst-loader-version" value="No versions available" disabled />
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
            <div class="mem-presets" role="group" aria-label="Memory presets">
              {#each MEMORY_PRESETS_GB as gb (gb)}
                <button
                  type="button"
                  class="mem-preset"
                  class:active={memoryMb === gb * 1024}
                  onclick={() => setMemoryPresetGb(gb)}
                >{gb} GB</button>
              {/each}
            </div>
            <div class="mem-slider">
              <div class="mem-value">{memoryMb} MB · {(memoryMb / 1024).toFixed(memoryMb % 1024 === 0 ? 0 : 1)} GB</div>
              <input
                class="mem-range"
                type="range"
                min={MEMORY_MIN_MB}
                max={memoryMaxMb}
                step="1024"
                bind:value={memoryMb}
              />
              <div class="mem-scale">
                <span>4 GB</span>
                <span>64 GB</span>
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
         <!-- Page 3: Modrinth / CurseForge browse — opens CatalogProjectView -->
         <p class="muted">Search Modrinth or CurseForge modpacks. Click a result for the in-app project page.</p>
         <div class="loader-chips" role="radiogroup" aria-label="Catalog provider">
           <button
             type="button"
             class="loader-chip"
             class:active={catalogProvider === "modrinth"}
             role="radio"
             aria-checked={catalogProvider === "modrinth"}
             onclick={() => {
               catalogProvider = "modrinth";
               cfHits = [];
               cfSelected = null;
               cfFiles = [];
               cfFileId = null;
             }}
           >Modrinth</button>
           <button
             type="button"
             class="loader-chip"
             class:active={catalogProvider === "curseforge"}
             role="radio"
             aria-checked={catalogProvider === "curseforge"}
             onclick={() => {
               catalogProvider = "curseforge";
               cfHits = [];
               cfSelected = null;
               cfFiles = [];
               cfFileId = null;
             }}
           >CurseForge</button>
         </div>
         <div class="search-row">
           <div class="search">
             <Search size={16} />
             <input
               aria-label="Search modpacks"
               bind:value={cfQuery}
               placeholder="Search modpacks…"
               onkeydown={(e) => e.key === "Enter" && searchCatalog()}
             />
           </div>
           <button class="secondary" onclick={searchCatalog} disabled={cfLoading}>
             {#if cfLoading}<Loader2 size={16} class="spin" />{:else}<Search size={16} />{/if}
             Search
           </button>
         </div>
         <div class="cf-layout">
           <div class="cf-list">
             {#each cfHits as hit (hit.id)}
               <button class="cf-row" class:active={cfSelected?.id === hit.id} onclick={() => openPackInCatalog(hit)}>
                 {#if hit.iconUrl}
                   <img src={hit.iconUrl} alt="" />
                 {:else}
                   <span class="cf-icon"><Package size={18} /></span>
                 {/if}
                 <div>
                   <strong>{hit.name}</strong>
                   <span>{(hit.summary || hit.description || "").slice(0, 100)}</span>
                 </div>
               </button>
             {:else}
               <div class="muted compact">{cfLoading ? "Searching…" : "Search for a modpack to begin."}</div>
             {/each}
           </div>
           <div class="cf-detail">
             {#if cfSelected}
               <h3>{cfSelected.name}</h3>
               {#if catalogProvider === "curseforge"}
                 {#if cfFilesLoading}
                   <div class="field-loader"><Loader2 size={16} class="spin" /> Loading versions…</div>
                 {:else}
                   <label for="cf-file">Pack version</label>
                   <select id="cf-file" value={cfFileId ?? ""} onchange={onCfFileChange}>
                     {#each cfFiles as f (f.id)}
                       <option value={f.id}>{f.displayName} · {(f.gameVersions || []).slice(0, 3).join(", ")}</option>
                     {/each}
                   </select>
                 {/if}
               {:else}
                 <p class="muted compact">Open the project page for details, or install the latest pack below.</p>
               {/if}
               <div class="field" style="margin-top:12px">
                 <label for="inst-name-cf">Instance name</label>
                 <input id="inst-name-cf" bind:value={cfName} oninput={() => (location = guessLocation())} />
               </div>
             {:else}
               <div class="muted compact">Select a pack (opens the in-app catalog page).</div>
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
          <LoadingButton {loading} disabled={!cfValid} onclick={installFromCatalogFooter}>
            <Download size={16} /> Install pack
          </LoadingButton>
        {/if}
      </div>
  </div>
</div>

{#if catalogViewResult}
  <div
    class="modal-backdrop catalog-backdrop"
    role="button"
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) catalogViewResult = null;
    }}
    onkeydown={() => {}}
  >
    <div
      class="modal catalog-modal"
      role="dialog"
      aria-modal="true"
      use:trapFocus={{ onEscape: () => (catalogViewResult = null) }}
    >
      <CatalogProjectView
        result={catalogViewResult}
        installing={catalogInstalling || loading}
        onback={() => (catalogViewResult = null)}
        oninstall={() => void installFromCatalogView()}
        onopenexternal={() => void openCatalogExternal()}
      />
    </div>
  </div>
{/if}

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
      radial-gradient(ellipse at 20% 0%, color-mix(in srgb, var(--accent-primary) 28%, transparent), transparent 55%),
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
    border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent);
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
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
    position: relative;
    width: 56px;
    height: 56px;
    border-radius: var(--border-radius-md);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 22px;
    font-weight: 800;
    color: #04140a;
    background: linear-gradient(135deg, var(--accent-primary), #0ea5e9);
    flex-shrink: 0;
    border: 1px solid transparent;
    padding: 0;
    cursor: pointer;
    overflow: hidden;
  }
  .pack-icon:hover {
    outline: 2px solid color-mix(in srgb, var(--accent-primary) 45%, transparent);
    outline-offset: 1px;
  }
  .pack-icon.has-img {
    background: var(--bg-tertiary);
  }
  .pack-icon-letter {
    line-height: 1;
  }
  .pack-icon img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .pack-icon-hint {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .clear-icon {
    margin-top: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
    text-decoration: underline;
  }
  .catalog-backdrop {
    z-index: 90;
  }
  .catalog-modal {
    width: min(920px, 96vw);
    max-height: min(90vh, 900px);
    overflow: auto;
    padding: 0;
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
    border-color: color-mix(in srgb, var(--accent-primary) 45%, transparent);
    background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
    color: var(--accent-primary);
  }
  .mem-slider { display: grid; gap: 8px; margin-top: 8px; }
  .mem-presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .mem-preset {
    padding: 5px 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .mem-preset:hover {
    border-color: var(--accent-primary);
    color: var(--text-primary);
  }
  .mem-preset.active {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 12%, var(--bg-tertiary));
  }
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
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-primary) 25%, transparent);
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
    background: color-mix(in srgb, var(--accent-primary) 8%, transparent); border: 1px solid color-mix(in srgb, var(--accent-primary) 25%, transparent); color: var(--accent-primary);
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
  .cf-row.active, .cf-row:hover { border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent); background: color-mix(in srgb, var(--accent-primary) 6%, transparent); }
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
