<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import {
    api,
    type ScannedRecipe,
    type IngredientDisplay,
    type RecipeScanResult,
    type RecipeRuntimeStatus,
    type RuntimeRecipeCategory,
  } from "../lib/api";
  import {
    Search,
    RefreshCw,
    ChevronLeft,
    ChevronRight,
    Copy,
    Flame,
    ArrowRight,
    Star,
    History,
    Hammer,
    Anvil,
    Scissors,
    Grid3x3,
    Package,
    Bookmark,
    Trash2,
    FileCode,
    X,
    Keyboard,
    Radio,
    Play,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type FocusMode = "recipes" | "uses";
  type ItemEntry = {
    id: string;
    name: string;
    modNs: string;
    recipeCount: number;
    useCount: number;
  };
  type ItemFocusCounts = { recipes: number; uses: number };

  const ITEMS_PER_PAGE = 60;
  const ICON_BATCH_SIZE = 48;
  const BOOKMARK_KEY = "tuffbox.jei.bookmarks";

  let recipes: ScannedRecipe[] = [];
  let items: ItemEntry[] = [];
  let itemCategorySets = new Map<string, Set<string>>();
  let filteredCounts = new Map<string, ItemFocusCounts>();
  let catalogReady = false;
  let scanMeta: Omit<RecipeScanResult, "recipes"> | null = null;
  let loading = false;
  let error: string | null = null;
  let message: string | null = null;
  let filter = "";
  let categoryFilter = "all";
  let modFilter = "all";
  let focusMode: FocusMode = "recipes";
  let selectedItem = "";
  let recipeIndex = 0;
  let itemPage = 0;
  let lastLoadedPath: string | null = null;
  let bookmarks: string[] = [];
  let historyStack: string[] = [];
  let showBookmarks = true;
  let showHelp = false;
  let cycleTick = 0;
  let cycleTimer: ReturnType<typeof setInterval> | null = null;
  let pendingRemoves = new Set<string>();
  let recipeSource: "offline" | "runtime" = "offline";
  let runtimeStatus: RecipeRuntimeStatus | null = null;
  let runtimeCategories: RuntimeRecipeCategory[] = [];
  let runtimePoller: ReturnType<typeof setInterval> | null = null;

  type IconState = "loading" | "missing" | string;
  let iconCache: Record<string, IconState> = {};
  const iconInFlight = new Set<string>();
  let iconPreloadQueue: string[] = [];
  let iconPreloadTimer: ReturnType<typeof setTimeout> | null = null;
  let iconBatchRunning = false;

  async function preloadIconsBatch(ids: string[]) {
    if (!ids.length || !$projectPath) return;
    const pending = [...new Set(ids)].filter(
      (id) => id && !id.startsWith("#") && iconCache[id] === undefined && !iconInFlight.has(id)
    );
    if (!pending.length) return;

    for (const id of pending) {
      iconInFlight.add(id);
      iconCache[id] = "loading";
    }
    iconCache = { ...iconCache };

    try {
      const result = await api.recipes.itemIconsBatch(pending, $projectPath);
      for (const id of pending) {
        const file = result[id];
        iconCache[id] = file ?? "missing";
        iconInFlight.delete(id);
      }
    } catch {
      for (const id of pending) {
        iconCache[id] = "missing";
        iconInFlight.delete(id);
      }
    }
    iconCache = { ...iconCache };
  }

  async function flushIconPreload() {
    if (iconBatchRunning || iconPreloadQueue.length === 0 || !$projectPath) return;
    iconBatchRunning = true;
    try {
      while (iconPreloadQueue.length > 0) {
        const chunk = iconPreloadQueue.splice(0, ICON_BATCH_SIZE);
        await preloadIconsBatch(chunk);
        await tick();
      }
    } finally {
      iconBatchRunning = false;
      if (iconPreloadQueue.length > 0) scheduleIconPreload();
    }
  }

  function scheduleIconPreload(ids: Array<string | null | undefined> = []) {
    for (const id of ids) {
      if (!id || id.startsWith("#")) continue;
      if (iconCache[id] !== undefined || iconInFlight.has(id)) continue;
      if (!iconPreloadQueue.includes(id)) iconPreloadQueue.push(id);
    }
    if (iconPreloadTimer) clearTimeout(iconPreloadTimer);
    iconPreloadTimer = setTimeout(() => {
      iconPreloadTimer = null;
      void flushIconPreload();
    }, 32);
  }

  async function ensureItemIcon(itemId: string | null | undefined) {
    if (!itemId || !$projectPath || itemId.startsWith("#")) return;
    if (iconCache[itemId] !== undefined || iconInFlight.has(itemId)) return;
    await preloadIconsBatch([itemId]);
  }

  function normalizeIconUrl(url: string | null | undefined): string | null {
    if (!url) return null;
    if (url.startsWith("data:") || url.startsWith("http://") || url.startsWith("https://") || url.startsWith("blob:")) {
      return url;
    }
    return null;
  }

  function iconSrc(itemId: string | null | undefined, explicit?: string | null): string | null {
    const normalized = normalizeIconUrl(explicit);
    if (normalized) return normalized;
    if (!itemId) return null;
    const state = iconCache[itemId];
    return typeof state === "string" && state !== "loading" && state !== "missing" ? state : null;
  }

  function onIconError(itemId: string | null | undefined) {
    if (!itemId) return;
    iconCache[itemId] = "missing";
    iconCache = { ...iconCache };
  }

  function preloadIcons(ids: Array<string | null | undefined>) {
    scheduleIconPreload(ids);
  }

  const CATEGORY_META: Record<string, { label: string; icon: "craft" | "cook" | "smith" | "cut" | "other" }> = {
    all: { label: "All", icon: "other" },
    crafting: { label: "Crafting", icon: "craft" },
    cooking: { label: "Cooking", icon: "cook" },
    smithing: { label: "Smithing", icon: "smith" },
    stonecutting: { label: "Cutting", icon: "cut" },
    other: { label: "Other", icon: "other" },
  };

  onMount(() => {
    try {
      bookmarks = JSON.parse(localStorage.getItem(BOOKMARK_KEY) || "[]");
    } catch {
      bookmarks = [];
    }
    cycleTimer = setInterval(() => {
      cycleTick = (cycleTick + 1) % 1000;
    }, 1200);
    window.addEventListener("keydown", onKey);
    runtimePoller = setInterval(checkRuntimeTransition, 5000);
  });

  onDestroy(() => {
    if (cycleTimer) clearInterval(cycleTimer);
    if (runtimePoller) clearInterval(runtimePoller);
    if (iconPreloadTimer) clearTimeout(iconPreloadTimer);
    window.removeEventListener("keydown", onKey);
  });

  function onKey(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (e.key === "r" || e.key === "R") {
      if (selectedItem) {
        focusMode = "recipes";
        recipeIndex = 0;
      }
    } else if (e.key === "u" || e.key === "U") {
      if (selectedItem) {
        focusMode = "uses";
        recipeIndex = 0;
      }
    } else if (e.key === "ArrowLeft") {
      prevRecipe();
    } else if (e.key === "ArrowRight") {
      nextRecipe();
    } else if (e.key === "b" || e.key === "B") {
      if (selectedItem) toggleBookmark(selectedItem);
    } else if (e.key === "Backspace" && historyStack.length > 1) {
      e.preventDefault();
      goBack();
    }
  }

  async function loadRecipes(preferLive = true, preserveSelection = false) {
    if (!$projectPath) return;
    loading = true;
    catalogReady = false;
    error = null;
    message = null;
    const previousSelection = preserveSelection ? selectedItem : "";
    try {
      runtimeStatus = await api.recipes.runtimeStatus($projectPath);
      if (preferLive && runtimeStatus.connected) {
        const live = await api.recipes.runtimeSnapshot($projectPath);
        recipes = (live.recipes ?? []).filter((recipe) => !!recipe.outputId);
        runtimeCategories = live.categories ?? [];
        recipeSource = "runtime";
        scanMeta = {
          jarCount: 0,
          datapackFiles: 0,
          truncated: live.truncated,
          totalScanned: live.totalScanned,
        };
        message = `Connected to live JEI: ${recipes.length} runtime recipes in ${runtimeCategories.length} categories.`;
      } else {
        const result = await api.recipes.scan($projectPath);
        recipes = result.recipes ?? [];
        runtimeCategories = [];
        recipeSource = "offline";
        scanMeta = {
          jarCount: result.jarCount,
          datapackFiles: result.datapackFiles,
          truncated: result.truncated,
          totalScanned: result.totalScanned,
        };
        if (result.truncated) {
          message = `Indexed ${recipes.length} recipes (limit reached; ${result.totalScanned} files scanned).`;
        } else {
          message = `Indexed ${recipes.length} recipes from ${result.jarCount} jars` +
            (result.datapackFiles ? ` + ${result.datapackFiles} datapack files` : "") + ".";
        }
      }
      await tick();
      rebuildIndexes(recipes);
      lastLoadedPath = $projectPath;
      if (!preserveSelection) categoryFilter = "all";
      selectedItem = previousSelection && recipes.some(
        (recipe) => recipe.outputId === previousSelection || recipe.inputIds.includes(previousSelection)
      ) ? previousSelection : "";
      recipeIndex = 0;
      itemPage = 0;
      if (!preserveSelection) historyStack = [];
    } catch (e) {
      if (preferLive && runtimeStatus?.connected) {
        try {
          const fallback = await api.recipes.scan($projectPath);
          recipes = fallback.recipes ?? [];
          runtimeCategories = [];
          recipeSource = "offline";
          scanMeta = {
            jarCount: fallback.jarCount,
            datapackFiles: fallback.datapackFiles,
            truncated: fallback.truncated,
            totalScanned: fallback.totalScanned,
          };
          await tick();
          rebuildIndexes(recipes);
          message = `Live JEI disconnected; showing offline recipes. ${String(e)}`;
        } catch (fallbackError) {
          error = String(fallbackError);
        }
      } else {
        error = String(e);
      }
    } finally {
      loading = false;
    }
  }

  function applyOfflineResult(result: RecipeScanResult) {
    recipes = result.recipes ?? [];
    runtimeCategories = [];
    recipeSource = "offline";
    scanMeta = {
      jarCount: result.jarCount,
      datapackFiles: result.datapackFiles,
      truncated: result.truncated,
      totalScanned: result.totalScanned,
    };
    if (result.truncated) {
      message = `Indexed ${recipes.length} recipes (limit reached; ${result.totalScanned} files scanned).`;
    } else {
      message = `Indexed ${recipes.length} recipes from ${result.jarCount} jars` +
        (result.datapackFiles ? ` + ${result.datapackFiles} datapack files` : "") + ".";
    }
    void tick().then(() => rebuildIndexes(recipes));
  }

  async function checkRuntimeTransition() {
    if (!$projectPath || loading) return;
    try {
      const status = await api.recipes.runtimeStatus($projectPath);
      const changed = status.connected !== runtimeStatus?.connected;
      runtimeStatus = status;
      if (changed && status.connected && recipeSource !== "runtime") {
        await loadRecipes(true, true);
      } else if (changed && !status.connected && recipeSource === "runtime") {
        await loadRecipes(false, true);
      }
    } catch {
      // Keep the current snapshot; the next poll can recover.
    }
  }

  async function launchJeiLive() {
    if (!$projectPath) return;
    error = null;
    try {
      const profiles = await api.project.listProfiles($projectPath);
      const profile = profiles.find((entry) => entry.side.toLowerCase() !== "server") ?? profiles[0];
      if (!profile) throw new Error("Create a client profile before launching JEI Live.");
      await api.launch.profile(profile.id, $projectPath);
      message = `Launching ${profile.name}. Live recipes connect after JEI finishes loading.`;
      runtimeStatus = { connected: false, supported: true, message: "Waiting for JEI…", minecraftVersion: null, pid: null };
    } catch (e) {
      error = String(e);
    }
  }

  function prettifyItem(id: string): string {
    if (!id || id === "?") return "Unknown";
    const bare = id.replace(/^#/, "");
    const name = bare.split(":").pop() ?? bare;
    return name.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }

  function itemNamespace(id: string): string {
    return id.replace(/^#/, "").split(":")[0] ?? "";
  }

  function itemHue(id: string): number {
    let h = 0;
    for (const c of id) h = (h * 31 + c.charCodeAt(0)) % 360;
    return h;
  }

  /** JEI ElementPrefixParser: @mod #tag &id, -negation, plain name */
  function matchesJeiSearch(itemId: string, query: string, displayName?: string): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    const tokens = q.split(/\s+/).filter(Boolean);
    for (const token of tokens) {
      if (token.startsWith("-")) {
        const neg = token.slice(1);
        if (neg && matchesToken(itemId, neg, displayName)) return false;
        continue;
      }
      if (!matchesToken(itemId, token, displayName)) return false;
    }
    return true;
  }

  function matchesToken(itemId: string, token: string, displayName?: string): boolean {
    const name = (displayName || prettifyItem(itemId)).toLowerCase();
    const id = itemId.toLowerCase();
    if (token.startsWith("@")) return itemNamespace(itemId).toLowerCase().includes(token.slice(1));
    if (token.startsWith("#")) return id.includes(token.slice(1)) || id.startsWith("#") || id.includes(token);
    if (token.startsWith("&")) return id.includes(token.slice(1));
    if (token.startsWith("$")) return name.includes(token.slice(1)) || id.includes(token.slice(1));
    return name.includes(token) || id.includes(token);
  }

  function buildItemCatalog(list: ScannedRecipe[]): ItemEntry[] {
    const map = new Map<string, ItemEntry>();
    const names = new Map<string, string>();
    for (const recipe of list) {
      if (recipe.layout.output?.name) names.set(recipe.layout.output.id, recipe.layout.output.name);
      for (const slot of recipe.layout.slots ?? []) {
        for (const ingredient of slot.ingredients) {
          if (ingredient.name) names.set(ingredient.id, ingredient.name);
        }
      }
    }
    const touch = (id: string, field: "recipeCount" | "useCount") => {
      if (!id || id === "unknown:unknown") return;
      const existing = map.get(id);
      if (existing) existing[field]++;
      else
        map.set(id, {
          id,
          name: names.get(id) ?? prettifyItem(id),
          modNs: itemNamespace(id),
          recipeCount: field === "recipeCount" ? 1 : 0,
          useCount: field === "useCount" ? 1 : 0,
        });
    };
    for (const r of list) {
      touch(r.outputId, "recipeCount");
      for (const inp of r.inputIds) touch(inp, "useCount");
    }
    return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
  }

  function recipesForItem(itemId: string, mode: FocusMode): ScannedRecipe[] {
    if (!itemId) return [];
    return recipes.filter((r) => {
      if (categoryFilter !== "all" && r.category !== categoryFilter) return false;
      if (modFilter !== "all" && itemNamespace(r.outputId) !== modFilter && !r.modSource.includes(modFilter))
        return false;
      if (mode === "recipes") return r.outputId === itemId;
      return r.inputIds.includes(itemId);
    });
  }

  function selectItem(id: string, mode?: FocusMode, pushHistory = true) {
    if (!id || id.startsWith("#")) return;
    if (pushHistory && id !== selectedItem) {
      historyStack = [...historyStack.filter((h) => h !== id), id].slice(-40);
    }
    selectedItem = id;
    if (mode) focusMode = mode;
    recipeIndex = 0;
  }

  function goBack() {
    if (historyStack.length < 2) return;
    const next = [...historyStack];
    next.pop();
    const prev = next[next.length - 1];
    historyStack = next;
    selectedItem = prev;
    recipeIndex = 0;
  }

  function toggleBookmark(id: string) {
    if (bookmarks.includes(id)) bookmarks = bookmarks.filter((b) => b !== id);
    else bookmarks = [...bookmarks, id];
    localStorage.setItem(BOOKMARK_KEY, JSON.stringify(bookmarks));
  }

  function resolveSlot(ing: IngredientDisplay | null): IngredientDisplay | null {
    if (!ing) return null;
    if (ing.kind === "one_of" && ing.alts && ing.alts.length > 0) {
      return ing.alts[cycleTick % ing.alts.length] ?? ing;
    }
    return ing;
  }

  function slotLabel(ing: IngredientDisplay | null): string {
    const r = resolveSlot(ing);
    if (!r) return "";
    if (r.kind === "tag" || r.id.startsWith("#")) return "#";
    return prettifyItem(r.id).slice(0, 3);
  }

  function slotTitle(ing: IngredientDisplay | null): string {
    if (!ing) return "";
    if (ing.kind === "one_of" && ing.alts) {
      return ing.alts.map((a) => a.id).join("\n");
    }
    return ing.id;
  }

  async function copyKubeJS(r: ScannedRecipe) {
    const script = await api.recipes.generateScript("remove", [r.id]);
    await navigator.clipboard.writeText(script.content);
    message = "KubeJS remove copied to clipboard";
  }

  async function queueRemove(r: ScannedRecipe) {
    pendingRemoves = new Set([...pendingRemoves, r.id]);
    message = `Queued ${pendingRemoves.size} recipe(s) for KubeJS remove`;
  }

  async function flushRemoves() {
    if (!$projectPath || pendingRemoves.size === 0) return;
    try {
      const path = await api.recipes.writeRemoves([...pendingRemoves], $projectPath);
      message = `Wrote ${pendingRemoves.size} removes → ${path}`;
      pendingRemoves = new Set();
    } catch (e) {
      error = String(e);
    }
  }

  function navigateSlot(ing: IngredientDisplay | null, mode: FocusMode) {
    const r = resolveSlot(ing);
    if (!r) return;
    if (r.id.startsWith("#") || r.kind === "tag") return;
    selectItem(r.id, mode);
  }

  function runtimeSlotIngredient(slot: { ingredients: IngredientDisplay[] }): IngredientDisplay | null {
    if (!slot.ingredients?.length) return null;
    return slot.ingredients[cycleTick % slot.ingredients.length] ?? slot.ingredients[0];
  }

  function runtimeCategory(id: string) {
    return runtimeCategories.find((category) => category.id === id);
  }

  function buildCategories(): string[] {
    const ids = new Set(recipes.map((recipe) => recipe.category));
    if (recipeSource === "runtime" && runtimeCategories.length > 0) {
      const ordered = runtimeCategories.map((category) => category.id).filter((id) => ids.has(id));
      const extras = [...ids].filter((id) => !ordered.includes(id)).sort((a, b) => a.localeCompare(b));
      return ["all", ...ordered, ...extras];
    }
    const buckets = ["crafting", "cooking", "smithing", "stonecutting", "other"];
    return ["all", ...buckets.filter((bucket) => ids.has(bucket))];
  }

  function categoryLabel(cat: string): string {
    if (cat === "all") return CATEGORY_META.all.label;
    return CATEGORY_META[cat]?.label ?? runtimeCategory(cat)?.title ?? prettifyItem(cat);
  }

  function categoryIcon(cat: string): "craft" | "cook" | "smith" | "cut" | "other" {
    if (CATEGORY_META[cat]?.icon) return CATEGORY_META[cat].icon;
    const hay = `${cat} ${runtimeCategory(cat)?.title ?? ""}`.toLowerCase();
    if ((hay.includes("craft") || hay.includes("workbench")) && !hay.includes("smith")) return "craft";
    if (hay.includes("smelt") || hay.includes("furnace") || hay.includes("blast") || hay.includes("cook")) return "cook";
    if (hay.includes("smith")) return "smith";
    if (hay.includes("stonecut") || hay.includes("cutting") || hay.includes("saw")) return "cut";
    return "other";
  }

  function buildItemCategorySets(list: ScannedRecipe[]): Map<string, Set<string>> {
    const map = new Map<string, Set<string>>();
    const touch = (id: string, category: string) => {
      if (!id || id === "unknown:unknown") return;
      let set = map.get(id);
      if (!set) {
        set = new Set();
        map.set(id, set);
      }
      set.add(category);
    };
    for (const recipe of list) {
      touch(recipe.outputId, recipe.category);
      for (const inp of recipe.inputIds) touch(inp, recipe.category);
    }
    return map;
  }

  function buildFilteredCounts(
    list: ScannedRecipe[],
    category: string,
    modNs: string
  ): Map<string, ItemFocusCounts> {
    const map = new Map<string, ItemFocusCounts>();
    const touch = (id: string, field: "recipes" | "uses") => {
      if (!id || id === "unknown:unknown") return;
      const existing = map.get(id) ?? { recipes: 0, uses: 0 };
      existing[field]++;
      map.set(id, existing);
    };
    for (const recipe of list) {
      if (category !== "all" && recipe.category !== category) continue;
      if (modNs !== "all") {
        const outputNs = itemNamespace(recipe.outputId);
        if (outputNs !== modNs && !recipe.modSource.includes(modNs)) continue;
      }
      touch(recipe.outputId, "recipes");
      for (const inp of recipe.inputIds) touch(inp, "uses");
    }
    return map;
  }

  function rebuildIndexes(list: ScannedRecipe[]) {
    items = buildItemCatalog(list);
    itemCategorySets = buildItemCategorySets(list);
    filteredCounts = buildFilteredCounts(list, categoryFilter, modFilter);
    catalogReady = true;
  }

  function itemInCategory(itemId: string, category: string): boolean {
    if (category === "all") return true;
    return itemCategorySets.get(itemId)?.has(category) ?? false;
  }

  function focusCountForItem(item: ItemEntry): number {
    if (categoryFilter === "all" && modFilter === "all") {
      return focusMode === "recipes" ? item.recipeCount : item.useCount;
    }
    const counts = filteredCounts.get(item.id);
    if (!counts) return 0;
    return focusMode === "recipes" ? counts.recipes : counts.uses;
  }

  function prevRecipe() {
    if (recipeIndex > 0) recipeIndex--;
  }
  function nextRecipe() {
    if (recipeIndex < activeRecipes.length - 1) recipeIndex++;
  }

  $: if (recipes.length && categoryFilter && modFilter) {
    filteredCounts = buildFilteredCounts(recipes, categoryFilter, modFilter);
  }
  $: modNamespaces = ["all", ...new Set(items.map((i) => i.modNs).filter(Boolean))].sort();
  $: filteredItems = catalogReady
    ? items.filter((i) => {
        if (modFilter !== "all" && i.modNs !== modFilter) return false;
        if (!itemInCategory(i.id, categoryFilter)) return false;
        return matchesJeiSearch(i.id, filter, i.name);
      })
    : [];
  $: totalItemPages = Math.max(1, Math.ceil(filteredItems.length / ITEMS_PER_PAGE));
  $: if (itemPage >= totalItemPages) itemPage = Math.max(0, totalItemPages - 1);
  $: pageItems = filteredItems.slice(itemPage * ITEMS_PER_PAGE, (itemPage + 1) * ITEMS_PER_PAGE);
  $: activeRecipes = recipesForItem(selectedItem, focusMode);
  $: categories = buildCategories();
  $: if (!categories.includes(categoryFilter)) categoryFilter = "all";
  $: if (activeRecipes.length === 0) recipeIndex = 0;
  else if (recipeIndex >= activeRecipes.length) recipeIndex = activeRecipes.length - 1;
  $: currentRecipe = activeRecipes[recipeIndex] ?? null;
  $: bookmarkItems = bookmarks
    .map((id) => items.find((i) => i.id === id) ?? { id, name: prettifyItem(id), modNs: itemNamespace(id), recipeCount: 0, useCount: 0 })
    .filter(Boolean);
  $: preloadIcons(pageItems.map((item) => item.id));
  $: preloadIcons(bookmarkItems.map((item) => item.id));
  $: if (selectedItem) ensureItemIcon(selectedItem);
  $: if (currentRecipe) {
    preloadIcons([
      currentRecipe.outputId,
      currentRecipe.layout.output?.id,
      ...currentRecipe.layout.grid.map((slot) => resolveSlot(slot)?.id),
      ...(currentRecipe.layout.slots ?? []).flatMap((slot) => slot.ingredients.map((ingredient) => ingredient.id)),
      ...(runtimeCategory(currentRecipe.category)?.stations ?? []).map((station) => station.id),
    ]);
  }
  $: if ($projectPath && $projectPath !== lastLoadedPath) loadRecipes();
  $: if (filter) itemPage = 0;
</script>

<div class="jei" class:busy={loading}>
  <header class="jei-hd">
    <div class="jei-brand">
      <div class="jei-logo">JEI</div>
      <div>
        <h2>Just Enough Items</h2>
        <p class="sub">
          {#if scanMeta}
            <span class="source-badge" class:live={recipeSource === "runtime"}>
              <Radio size={11} /> {recipeSource === "runtime" ? "Live JEI" : "Offline files"}
            </span>
            · {recipes.length} recipes · {items.length} items
            {#if recipeSource === "offline"} · {scanMeta.jarCount} jars{/if}
            {#if recipeSource === "offline" && scanMeta.datapackFiles} · {scanMeta.datapackFiles} datapacks{/if}
            {#if scanMeta.truncated} · truncated{/if}
          {:else}
            Recipe browser for your modpack
          {/if}
        </p>
      </div>
    </div>
    <div class="hd-actions">
      <button class="ghost" title="Keyboard shortcuts" on:click={() => (showHelp = !showHelp)}>
        <Keyboard size={16} />
      </button>
      <button class="ghost" class:active={showBookmarks} on:click={() => (showBookmarks = !showBookmarks)}>
        <Bookmark size={16} />
      </button>
      {#if runtimeStatus?.supported && !runtimeStatus.connected}
        <button class="live-launch" on:click={launchJeiLive} disabled={!$projectPath || loading} title={runtimeStatus.message}>
          <Play size={15} /> Launch JEI Live
        </button>
      {/if}
      <button class="primary-scan" on:click={() => loadRecipes(true, true)} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
        {loading ? "Loading…" : recipeSource === "runtime" ? "Refresh live" : "Rescan"}
      </button>
    </div>
  </header>

  {#if showHelp}
    <div class="help-bar">
      <span><kbd>R</kbd> Recipes</span>
      <span><kbd>U</kbd> Uses</span>
      <span><kbd>B</kbd> Bookmark</span>
      <span><kbd>←</kbd><kbd>→</kbd> Recipe pages</span>
      <span><kbd>Backspace</kbd> History back</span>
      <span>LMB = Recipes · RMB = Uses</span>
      <button class="ghost tiny" on:click={() => (showHelp = false)}><X size={14} /></button>
    </div>
  {/if}

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}

  <div class="jei-search-row">
    <div class="jei-search">
      <Search size={14} />
      <input bind:value={filter} placeholder="Search: name  @mod  #tag  &id  $tooltip  -exclude" spellcheck="false" />
      {#if filter}
        <button class="clear" on:click={() => (filter = "")}><X size={12} /></button>
      {/if}
    </div>
    <select bind:value={modFilter} class="mod-select" title="Filter by mod namespace">
      {#each modNamespaces as ns}
        <option value={ns}>{ns === "all" ? "All mods" : ns}</option>
      {/each}
    </select>
    {#if pendingRemoves.size > 0}
      <button class="warn-btn" on:click={flushRemoves}>
        <FileCode size={14} /> Write {pendingRemoves.size} removes
      </button>
    {/if}
  </div>

  {#if !$projectPath}
    <div class="empty">
      <Package size={40} />
      <h3>Open a project</h3>
      <p>Scan mod JARs, datapacks and KubeJS data like JEI.</p>
    </div>
  {:else if loading || !catalogReady}
    <div class="empty">
      <RefreshCw size={40} class="spin" />
      <h3>Indexing recipes…</h3>
      <p>Reading JAR data packs — this can take a moment on large packs.</p>
    </div>
  {:else if recipes.length === 0}
    <div class="empty">
      <Grid3x3 size={40} />
      <h3>No recipes found</h3>
      <p>Put mods in <code>mods/</code> or datapacks under <code>datapacks/</code>.</p>
      <button on:click={() => loadRecipes()}>Scan now</button>
    </div>
  {:else}
    <div class="jei-body" class:with-bookmarks={showBookmarks}>
      <!-- Category tabs (JEI left rail) -->
      <nav class="cat-rail" aria-label="Recipe categories">
        {#each categories as cat}
          <button
            type="button"
            class="cat-tab"
            class:active={categoryFilter === cat}
            title={categoryLabel(cat)}
            on:click={() => {
              categoryFilter = cat;
              recipeIndex = 0;
              itemPage = 0;
            }}
          >
            {#if categoryIcon(cat) === "craft"}
              <Grid3x3 size={18} />
            {:else if categoryIcon(cat) === "cook"}
              <Flame size={18} />
            {:else if categoryIcon(cat) === "smith"}
              <Anvil size={18} />
            {:else if categoryIcon(cat) === "cut"}
              <Scissors size={18} />
            {:else}
              <Hammer size={18} />
            {/if}
            <span>{categoryLabel(cat)}</span>
          </button>
        {/each}
      </nav>

      <!-- Recipe GUI -->
      <main class="jei-gui">
        {#if !selectedItem}
          <div class="gui-empty">
            <div class="mc-panel preview">
              <div class="craft-grid dim">
                {#each Array(9) as _}
                  <div class="mc-slot"></div>
                {/each}
              </div>
              <ArrowRight size={22} class="arr" />
              <div class="mc-slot out"></div>
            </div>
            <h3>Select an item</h3>
            <p>Click an item on the right — like JEI’s ingredient list.</p>
            <p class="hint">Right-click for Uses · Press <kbd>R</kbd> / <kbd>U</kbd></p>
          </div>
        {:else}
          <div class="gui-top">
            <button class="ghost" disabled={historyStack.length < 2} on:click={goBack} title="Back">
              <History size={16} />
            </button>
            <div class="focus-tabs">
              <button
                type="button"
                class:active={focusMode === "recipes"}
                on:click={() => { focusMode = "recipes"; recipeIndex = 0; }}
              >
                Recipes <em>{recipesForItem(selectedItem, "recipes").length}</em>
              </button>
              <button
                type="button"
                class:active={focusMode === "uses"}
                on:click={() => { focusMode = "uses"; recipeIndex = 0; }}
              >
                Uses <em>{recipesForItem(selectedItem, "uses").length}</em>
              </button>
            </div>
            <div class="focus-item">
              <span class="mc-slot mini" style="--hue: {itemHue(selectedItem)}">
                {#if iconSrc(selectedItem)}
                  <img src={iconSrc(selectedItem)} alt="" class="slot-icon" on:error={() => onIconError(selectedItem)} />
                {:else}
                  <span class="letter">{prettifyItem(selectedItem).slice(0, 2)}</span>
                {/if}
              </span>
              <div>
                <strong>{prettifyItem(selectedItem)}</strong>
                <code>{selectedItem}</code>
              </div>
              <button
                class="star"
                class:on={bookmarks.includes(selectedItem)}
                title="Bookmark (B)"
                on:click={() => toggleBookmark(selectedItem)}
              >
                <Star size={16} />
              </button>
            </div>
          </div>

          {#if activeRecipes.length === 0}
            <div class="gui-empty compact">
              <p>No {focusMode} for this item{categoryFilter !== "all" ? ` in ${categoryLabel(categoryFilter)}` : ""}.</p>
            </div>
          {:else if currentRecipe}
            <div class="recipe-stage">
              <div class="recipe-nav">
                <button class="nav-btn" on:click={prevRecipe} disabled={recipeIndex === 0}>
                  <ChevronLeft size={18} />
                </button>
                <span class="page">{recipeIndex + 1} / {activeRecipes.length}</span>
                <button class="nav-btn" on:click={nextRecipe} disabled={recipeIndex >= activeRecipes.length - 1}>
                  <ChevronRight size={18} />
                </button>
              </div>

              <div class="mc-panel" data-cat={currentRecipe.layout.category}>
                <div class="panel-title">
                  {CATEGORY_META[currentRecipe.layout.category]?.label ?? runtimeCategory(currentRecipe.category)?.title ?? currentRecipe.layout.category}
                  {#if currentRecipe.layout.shapeless}<span class="badge shapeless">Shapeless</span>{/if}
                </div>

                {#if currentRecipe.layout.slots?.length}
                  {@const liveCategory = runtimeCategory(currentRecipe.category)}
                  <div
                    class="runtime-layout"
                    style={`--runtime-width:${Math.max(120, liveCategory?.width ?? 160)}px;--runtime-height:${Math.max(70, liveCategory?.height ?? 90)}px`}
                  >
                    {#each currentRecipe.layout.slots as slot}
                      {@const ingredient = runtimeSlotIngredient(slot)}
                      <button
                        class="mc-slot runtime-slot"
                        class:empty={!ingredient}
                        class:output={slot.role === "OUTPUT"}
                        style={`left:${slot.x}px;top:${slot.y}px;width:${Math.max(18, slot.width)}px;height:${Math.max(18, slot.height)}px;--hue:${itemHue(ingredient?.id ?? "")}`}
                        title={ingredient?.tooltip?.join("\n") || ingredient?.name || ingredient?.id || slot.name || slot.role}
                        disabled={!ingredient}
                        on:click={() => navigateSlot(ingredient, slot.role === "OUTPUT" ? "recipes" : "uses")}
                        on:contextmenu|preventDefault={() => navigateSlot(ingredient, "recipes")}
                      >
                        {#if ingredient}
                          {#if iconSrc(ingredient.id, ingredient.iconUrl)}
                            <img src={iconSrc(ingredient.id, ingredient.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(ingredient.id)} />
                          {:else}
                            <span class="letter">{(ingredient.name || prettifyItem(ingredient.id)).slice(0, 3)}</span>
                          {/if}
                          {#if (ingredient.count ?? 1) > 1}<em class="stack">{ingredient.count}</em>{/if}
                          {#if slot.ingredients.length > 1}<span class="cycle-dot"></span>{/if}
                        {/if}
                      </button>
                    {/each}
                  </div>
                  {#if liveCategory?.stations?.length}
                    <div class="stations">
                      <span>Stations</span>
                      {#each liveCategory.stations as station}
                        <button
                          class="mc-slot mini"
                          title={station.tooltip?.join("\n") || station.name || station.id}
                          style={`--hue:${itemHue(station.id)}`}
                          on:click={() => navigateSlot(station, "recipes")}
                        >
                          {#if iconSrc(station.id, station.iconUrl)}
                            <img src={iconSrc(station.id, station.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(station.id)} />
                          {:else}
                            <span class="letter">{(station.name || prettifyItem(station.id)).slice(0, 2)}</span>
                          {/if}
                        </button>
                      {/each}
                    </div>
                  {/if}
                {:else if currentRecipe.layout.category === "crafting"}
                  <div class="panel-body craft">
                    <div class="craft-grid">
                      {#each currentRecipe.layout.grid as slot}
                        <button
                          class="mc-slot"
                          class:empty={!slot}
                          class:tag={resolveSlot(slot)?.kind === "tag" || resolveSlot(slot)?.id?.startsWith("#")}
                          style={slot ? `--hue: ${itemHue(resolveSlot(slot)?.id ?? "")}` : ""}
                          title={slotTitle(slot)}
                          disabled={!slot}
                          on:click={() => navigateSlot(slot, "uses")}
                          on:contextmenu|preventDefault={() => navigateSlot(slot, "recipes")}
                        >
                          {#if slot}
                            {@const resolved = resolveSlot(slot)}
                            {#if iconSrc(resolved?.id, resolved?.iconUrl)}
                              <img src={iconSrc(resolved?.id, resolved?.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(resolved?.id)} />
                            {:else}
                              <span class="letter">{slotLabel(slot)}</span>
                            {/if}
                            {#if slot.kind === "one_of" && slot.alts && slot.alts.length > 1}
                              <span class="cycle-dot"></span>
                            {/if}
                          {/if}
                        </button>
                      {/each}
                    </div>
                    <ArrowRight size={28} class="arr" />
                    <button
                      class="mc-slot out"
                      style="--hue: {itemHue(currentRecipe.layout.output.id)}"
                      title={currentRecipe.layout.output.id}
                      on:click={() => navigateSlot(currentRecipe.layout.output, "recipes")}
                      on:contextmenu|preventDefault={() => navigateSlot(currentRecipe.layout.output, "uses")}
                    >
                      {#if iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)}
                        <img src={iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(currentRecipe.layout.output.id)} />
                      {:else}
                        <span class="letter">{prettifyItem(currentRecipe.layout.output.id).slice(0, 3)}</span>
                      {/if}
                      {#if currentRecipe.layout.outputCount > 1}
                        <em class="stack">{currentRecipe.layout.outputCount}</em>
                      {/if}
                    </button>
                  </div>
                {:else if currentRecipe.layout.category === "cooking"}
                  {@const input = resolveSlot(currentRecipe.layout.grid[4])}
                  <div class="panel-body cook">
                    <button
                      class="mc-slot large"
                      style="--hue: {itemHue(resolveSlot(currentRecipe.layout.grid[4])?.id ?? '')}"
                      title={slotTitle(currentRecipe.layout.grid[4])}
                      on:click={() => navigateSlot(currentRecipe.layout.grid[4], "uses")}
                    >
                      {#if iconSrc(input?.id, input?.iconUrl)}
                        <img src={iconSrc(input?.id, input?.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(input?.id)} />
                      {:else}
                        <span class="letter">{slotLabel(currentRecipe.layout.grid[4])}</span>
                      {/if}
                    </button>
                    <div class="flame-col">
                      <Flame size={26} class="flame" />
                      {#if currentRecipe.layout.cookTime}
                        <span>{(currentRecipe.layout.cookTime / 20).toFixed(1)}s</span>
                      {/if}
                      {#if currentRecipe.layout.experience}
                        <span class="xp">+{currentRecipe.layout.experience} XP</span>
                      {/if}
                    </div>
                    <ArrowRight size={28} class="arr" />
                    <button
                      class="mc-slot out large"
                      style="--hue: {itemHue(currentRecipe.layout.output.id)}"
                      on:click={() => navigateSlot(currentRecipe.layout.output, "recipes")}
                    >
                      {#if iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)}
                        <img src={iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(currentRecipe.layout.output.id)} />
                      {:else}
                        <span class="letter">{prettifyItem(currentRecipe.layout.output.id).slice(0, 3)}</span>
                      {/if}
                      {#if currentRecipe.layout.outputCount > 1}
                        <em class="stack">{currentRecipe.layout.outputCount}</em>
                      {/if}
                    </button>
                  </div>
                {:else if currentRecipe.layout.category === "smithing"}
                  <div class="panel-body smith">
                    {#each [0, 1, 2] as i}
                      {@const slot = currentRecipe.layout.grid[3 + i]}
                      <button
                        class="mc-slot large"
                        class:empty={!slot}
                        style={slot ? `--hue: ${itemHue(resolveSlot(slot)?.id ?? "")}` : ""}
                        title={slotTitle(slot)}
                        disabled={!slot}
                        on:click={() => navigateSlot(slot, "uses")}
                      >
                        {#if slot}
                          {@const resolved = resolveSlot(slot)}
                          {#if iconSrc(resolved?.id, resolved?.iconUrl)}
                            <img src={iconSrc(resolved?.id, resolved?.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(resolved?.id)} />
                          {:else}
                            <span class="letter">{slotLabel(slot)}</span>
                          {/if}
                        {/if}
                      </button>
                      {#if i < 2}<span class="plus">+</span>{/if}
                    {/each}
                    <ArrowRight size={28} class="arr" />
                    <button
                      class="mc-slot out large"
                      style="--hue: {itemHue(currentRecipe.layout.output.id)}"
                      on:click={() => navigateSlot(currentRecipe.layout.output, "recipes")}
                    >
                      {#if iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)}
                        <img src={iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(currentRecipe.layout.output.id)} />
                      {:else}
                        <span class="letter">{prettifyItem(currentRecipe.layout.output.id).slice(0, 3)}</span>
                      {/if}
                    </button>
                  </div>
                {:else}
                  <div class="panel-body cook">
                    <div class="craft-grid loose">
                      {#each currentRecipe.layout.grid.filter(Boolean) as slot}
                        {@const resolved = resolveSlot(slot)}
                        <button
                          class="mc-slot"
                          style="--hue: {itemHue(resolved?.id ?? '')}"
                          title={slotTitle(slot)}
                          on:click={() => navigateSlot(slot, "uses")}
                        >
                          {#if iconSrc(resolved?.id, resolved?.iconUrl)}
                            <img src={iconSrc(resolved?.id, resolved?.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(resolved?.id)} />
                          {:else}
                            <span class="letter">{slotLabel(slot)}</span>
                          {/if}
                        </button>
                      {/each}
                    </div>
                    <ArrowRight size={28} class="arr" />
                    <button
                      class="mc-slot out"
                      style="--hue: {itemHue(currentRecipe.layout.output.id)}"
                      on:click={() => navigateSlot(currentRecipe.layout.output, "recipes")}
                    >
                      {#if iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)}
                        <img src={iconSrc(currentRecipe.layout.output.id, currentRecipe.layout.output.iconUrl)} alt="" class="slot-icon" on:error={() => onIconError(currentRecipe.layout.output.id)} />
                      {:else}
                        <span class="letter">{prettifyItem(currentRecipe.layout.output.id).slice(0, 3)}</span>
                      {/if}
                      {#if currentRecipe.layout.outputCount > 1}
                        <em class="stack">{currentRecipe.layout.outputCount}</em>
                      {/if}
                    </button>
                  </div>
                {/if}
              </div>

              <div class="recipe-meta">
                <span class="tag type">{currentRecipe.recipeType.replace(/^minecraft:/, "")}</span>
                {#if currentRecipe.isConditional}<span class="tag warn">conditional</span>{/if}
                <span class="tag mod" title={currentRecipe.sourceFile}>{currentRecipe.modSource}</span>
              </div>
              <code class="recipe-id">{currentRecipe.id}</code>

              <div class="recipe-actions">
                <button class="secondary" on:click={() => copyKubeJS(currentRecipe)}>
                  <Copy size={14} /> Copy KubeJS
                </button>
                <button class="secondary" on:click={() => queueRemove(currentRecipe)}>
                  <Trash2 size={14} /> Queue remove
                </button>
                <button
                  class="secondary"
                  class:on={bookmarks.includes(currentRecipe.outputId)}
                  on:click={() => toggleBookmark(currentRecipe.outputId)}
                >
                  <Star size={14} /> Bookmark
                </button>
              </div>
            </div>
          {/if}
        {/if}
      </main>

      <!-- Right: bookmarks + ingredient list (JEI overlay) -->
      <aside class="jei-overlay">
        {#if showBookmarks && bookmarkItems.length > 0}
          <div class="bm-strip">
            <div class="overlay-h"><Star size={12} /> Bookmarks</div>
            <div class="item-grid compact">
              {#each bookmarkItems as item (item.id)}
                <button
                  class="item-slot"
                  class:sel={selectedItem === item.id}
                  style="--hue: {itemHue(item.id)}"
                  title={item.id}
                  on:click={() => selectItem(item.id, "recipes")}
                  on:contextmenu|preventDefault={() => selectItem(item.id, "uses")}
                >
                  {#if iconSrc(item.id)}
                    <img src={iconSrc(item.id)} alt="" class="item-icon" on:error={() => onIconError(item.id)} />
                  {:else if iconCache[item.id] === "loading"}
                    <span class="item-letter icon-pending"></span>
                  {:else}
                    <span class="item-letter">{item.name.slice(0, 2)}</span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="overlay-h">
          <span>Items</span>
          <small>{filteredItems.length}</small>
        </div>
        <div class="item-grid">
          {#each pageItems as item (item.id)}
            <button
              class="item-slot"
              class:sel={selectedItem === item.id}
              class:bookmarked={bookmarks.includes(item.id)}
              style="--hue: {itemHue(item.id)}"
              title="{item.id}\nR: {item.recipeCount} · U: {item.useCount}"
              on:click={() => selectItem(item.id, "recipes")}
              on:contextmenu|preventDefault={() => selectItem(item.id, "uses")}
              on:auxclick={(e) => {
                if (e.button === 1) {
                  e.preventDefault();
                  toggleBookmark(item.id);
                }
              }}
            >
              {#if iconSrc(item.id)}
                <img src={iconSrc(item.id)} alt="" class="item-icon" on:error={() => onIconError(item.id)} />
              {:else if iconCache[item.id] === "loading"}
                <span class="item-letter icon-pending"></span>
              {:else}
                <span class="item-letter">{item.name.slice(0, 2)}</span>
              {/if}
              {#if focusCountForItem(item) > 0}
                <span class="item-count">{focusCountForItem(item)}</span>
              {/if}
            </button>
          {/each}
        </div>
        <div class="overlay-pager">
          <button class="nav-btn" disabled={itemPage === 0} on:click={() => (itemPage = Math.max(0, itemPage - 1))}>
            <ChevronLeft size={14} />
          </button>
          <span>{itemPage + 1} / {totalItemPages}</span>
          <button
            class="nav-btn"
            disabled={itemPage >= totalItemPages - 1}
            on:click={() => (itemPage = Math.min(totalItemPages - 1, itemPage + 1))}
          >
            <ChevronRight size={14} />
          </button>
        </div>
        {#if historyStack.length > 0}
          <div class="hist-strip">
            <div class="overlay-h"><History size={12} /> Recent</div>
            <div class="item-grid compact">
              {#each [...historyStack].reverse().slice(0, 12) as id (id)}
                <button
                  class="item-slot"
                  class:sel={selectedItem === id}
                  style="--hue: {itemHue(id)}"
                  title={id}
                  on:click={() => selectItem(id, focusMode, false)}
                >
                  {#if iconSrc(id)}
                    <img src={iconSrc(id)} alt="" class="item-icon" on:error={() => onIconError(id)} />
                  {:else if iconCache[id] === "loading"}
                    <span class="item-letter icon-pending"></span>
                  {:else}
                    <span class="item-letter">{prettifyItem(id).slice(0, 2)}</span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .jei {
    --jei-panel: #c6c6c6;
    --jei-panel-dark: #8b8b8b;
    --jei-slot: #8b8b8b;
    --jei-slot-in: #373737;
    --jei-gold: #fbbf24;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .jei.busy { opacity: 0.92; }

  .jei-hd {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .jei-brand { display: flex; align-items: center; gap: 12px; }
  .jei-logo {
    width: 44px; height: 44px; border-radius: 10px;
    background: linear-gradient(145deg, #fbbf24, #d97706);
    color: #1a1200; font-weight: 900; font-size: 14px;
    display: grid; place-items: center;
    box-shadow: 0 4px 14px rgba(251, 191, 36, 0.25);
    letter-spacing: -0.02em;
  }
  .jei-brand h2 { margin: 0; font-size: 16px; font-weight: 700; color: var(--text-primary); }
  .sub { margin: 2px 0 0; font-size: 12px; color: var(--text-muted); }
  .hd-actions { display: flex; gap: 8px; align-items: center; }
  .hd-actions .ghost.active { color: var(--jei-gold); border-color: rgba(251, 191, 36, 0.4); }
  .primary-scan {
    display: inline-flex; align-items: center; gap: 8px;
    padding: 8px 14px; border-radius: 10px; border: none;
    background: var(--accent-primary); color: #04140a; font-weight: 700; cursor: pointer;
  }
  .primary-scan:disabled { opacity: 0.5; cursor: not-allowed; }

  .help-bar {
    display: flex; flex-wrap: wrap; gap: 12px; align-items: center;
    padding: 8px 12px; border-radius: 10px;
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    font-size: 12px; color: var(--text-secondary);
  }
  .help-bar .tiny { margin-left: auto; }
  kbd {
    display: inline-block; padding: 1px 6px; margin: 0 2px;
    border-radius: 4px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); font-size: 11px; font-family: ui-monospace, monospace;
  }

  .notice { padding: 10px 14px; border-radius: 10px; font-size: 13px; }
  .notice.error { background: rgba(239, 68, 68, 0.08); color: #fecaca; border: 1px solid rgba(239, 68, 68, 0.25); }
  .notice.ok { background: rgba(27, 217, 106, 0.08); color: var(--accent-primary); border: 1px solid rgba(27, 217, 106, 0.25); }

  .jei-search-row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .jei-search { flex: 1; min-width: 220px; display: flex; align-items: center; position: relative; }
  .jei-search :global(svg) { position: absolute; left: 10px; color: var(--text-muted); pointer-events: none; }
  .jei-search input {
    width: 100%; padding: 10px 32px 10px 34px; border-radius: 10px;
    border: 1px solid var(--border-color); background: var(--bg-secondary); color: var(--text-primary);
  }
  .jei-search .clear {
    position: absolute; right: 8px; background: transparent; border: none;
    color: var(--text-muted); cursor: pointer; padding: 4px;
  }
  .mod-select {
    padding: 9px 12px; border-radius: 10px; border: 1px solid var(--border-color);
    background: var(--bg-secondary); color: var(--text-secondary); font-size: 12px;
  }
  .warn-btn {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 8px 12px; border-radius: 10px; border: 1px solid rgba(251, 191, 36, 0.35);
    background: rgba(251, 191, 36, 0.12); color: #fbbf24; font-weight: 600; cursor: pointer; font-size: 12px;
  }

  .empty {
    padding: 72px 24px; text-align: center; color: var(--text-muted);
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    display: flex; flex-direction: column; align-items: center; gap: 10px;
  }
  .empty h3 { margin: 0; color: var(--text-secondary); }
  .empty code { font-size: 12px; color: var(--accent-primary); }

  .jei-body {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr) 300px;
    gap: 10px;
    flex: 1;
    min-height: 0;
    align-items: stretch;
  }
  .jei-body:not(.with-bookmarks) { grid-template-columns: 72px minmax(0, 1fr) 280px; }

  .cat-rail {
    display: flex; flex-direction: column; gap: 4px;
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); padding: 8px 6px;
    min-height: 0;
    max-height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
  }
  .cat-tab {
    display: flex; flex-direction: column; align-items: center; gap: 4px;
    flex: 0 0 auto;
    padding: 10px 4px; border-radius: 10px; border: 1px solid transparent;
    background: transparent; color: var(--text-muted); cursor: pointer; font-size: 9px;
    text-transform: uppercase; letter-spacing: 0.04em; font-weight: 700;
    transform: none;
  }
  .cat-tab span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }
  .cat-tab:hover { background: var(--bg-hover); color: var(--text-secondary); transform: none; }
  .cat-tab.active {
    background: rgba(251, 191, 36, 0.12); border-color: rgba(251, 191, 36, 0.35); color: var(--jei-gold);
  }

  .jei-gui {
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); padding: 14px 16px;
    display: flex; flex-direction: column; min-height: 0;
  }
  .gui-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; color: var(--text-muted); text-align: center;
  }
  .gui-empty.compact { padding: 40px; }
  .gui-empty h3 { margin: 0; color: var(--text-secondary); }
  .gui-empty .hint { font-size: 12px; }
  .mc-panel.preview { opacity: 0.45; pointer-events: none; margin-bottom: 8px; }
  .mc-panel.preview .craft-grid.dim .mc-slot { background: #2a2a2e; }

  .gui-top {
    display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin-bottom: 14px;
  }
  .focus-tabs { display: flex; gap: 4px; }
  .focus-tabs button {
    padding: 8px 14px; font-size: 12px; border-radius: 8px;
    background: var(--bg-tertiary); border: 1px solid var(--border-color);
    color: var(--text-muted); cursor: pointer;
    transform: none;
    font-weight: 600;
  }
  .focus-tabs button:hover { transform: none; background: var(--bg-hover); color: var(--text-secondary); }
  .focus-tabs button em { font-style: normal; opacity: 0.7; margin-left: 4px; }
  .focus-tabs button.active {
    background: rgba(27, 217, 106, 0.1); border-color: rgba(27, 217, 106, 0.35);
    color: var(--accent-primary); font-weight: 700;
  }
  .focus-tabs button.active:hover {
    background: rgba(27, 217, 106, 0.14);
    color: var(--accent-primary);
  }
  .focus-item { display: flex; align-items: center; gap: 10px; margin-left: auto; }
  .focus-item strong { display: block; font-size: 14px; }
  .focus-item code { font-size: 10px; color: var(--text-muted); }
  .star {
    background: transparent; border: 1px solid var(--border-color); border-radius: 8px;
    color: var(--text-muted); padding: 6px; cursor: pointer;
  }
  .star.on { color: var(--jei-gold); border-color: rgba(251, 191, 36, 0.45); }

  .recipe-stage { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .recipe-nav { display: flex; align-items: center; gap: 12px; color: var(--text-muted); font-size: 13px; }
  .nav-btn {
    width: 32px; height: 32px; border-radius: 8px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-secondary); cursor: pointer;
    display: grid; place-items: center;
  }
  .nav-btn:disabled { opacity: 0.35; cursor: default; }
  .page { font-variant-numeric: tabular-nums; min-width: 64px; text-align: center; }

  /* Minecraft-style recipe panel */
  .mc-panel {
    background: linear-gradient(180deg, #c6c6c6 0%, #8b8b8b 100%);
    border: 3px solid #373737;
    box-shadow:
      inset 2px 2px 0 #ffffff88,
      inset -2px -2px 0 #00000044,
      0 8px 24px rgba(0, 0, 0, 0.35);
    border-radius: 4px;
    padding: 12px 18px 16px;
    min-width: 320px;
    color: #1a1a1a;
  }
  .panel-title {
    font-size: 12px; font-weight: 800; text-transform: uppercase; letter-spacing: 0.06em;
    margin-bottom: 10px; display: flex; align-items: center; gap: 8px; color: #222;
  }
  .badge.shapeless {
    font-size: 9px; padding: 2px 6px; border-radius: 4px;
    background: #67e8f9; color: #083344;
  }
  .panel-body { display: flex; align-items: center; gap: 14px; justify-content: center; }
  .panel-body.craft, .panel-body.cook, .panel-body.smith { min-height: 120px; }
  .panel-body :global(.arr) { color: #373737; flex-shrink: 0; }
  .plus { font-weight: 900; font-size: 18px; color: #373737; }

  .craft-grid {
    display: grid; grid-template-columns: repeat(3, 48px); grid-template-rows: repeat(3, 48px); gap: 3px;
  }
  .craft-grid.loose {
    grid-template-columns: repeat(auto-fill, 48px); grid-template-rows: auto; max-width: 160px;
  }

  .mc-slot {
    width: 48px; height: 48px; padding: 0; cursor: pointer;
    border: 2px solid #373737;
    background:
      linear-gradient(135deg, hsl(var(--hue, 0) 28% 38%), hsl(var(--hue, 0) 22% 28%)),
      #8b8b8b;
    box-shadow: inset 2px 2px 0 #ffffff55, inset -2px -2px 0 #00000055;
    display: flex; align-items: center; justify-content: center;
    position: relative; border-radius: 2px;
  }
  .mc-slot.empty, .mc-slot:disabled.empty {
    background: #373737;
    box-shadow: inset 2px 2px 0 #00000066, inset -1px -1px 0 #ffffff22;
    cursor: default;
  }
  .mc-slot:not(.empty):hover { outline: 2px solid #fbbf24; outline-offset: 1px; }
  .mc-slot.out {
    width: 56px; height: 56px;
    border-color: #14532d;
    background: linear-gradient(135deg, hsl(var(--hue) 40% 42%), hsl(var(--hue) 32% 28%));
  }
  .mc-slot.large { width: 56px; height: 56px; }
  .mc-slot.mini { width: 36px; height: 36px; flex-shrink: 0; }
  .mc-slot.tag { border-style: dashed; border-color: #67e8f9; }
  .letter {
    font-size: 10px; font-weight: 800; color: #fff;
    text-shadow: 0 1px 2px #000, 1px 1px 0 #000; text-align: center; line-height: 1.1;
    text-transform: uppercase; pointer-events: none;
  }
  .slot-icon {
    width: calc(100% - 4px);
    height: calc(100% - 4px);
    object-fit: contain;
    image-rendering: pixelated;
    pointer-events: none;
  }
  .stack {
    position: absolute; bottom: 1px; right: 3px; font-style: normal;
    font-size: 12px; font-weight: 900; color: #fff; text-shadow: 1px 1px 0 #000;
  }
  .cycle-dot {
    position: absolute; top: 3px; right: 3px; width: 5px; height: 5px;
    border-radius: 50%; background: #67e8f9; box-shadow: 0 0 4px #67e8f9;
  }

  .flame-col {
    display: flex; flex-direction: column; align-items: center; gap: 2px;
    font-size: 11px; font-weight: 700; color: #9a3412;
  }
  .flame-col :global(.flame) { color: #ea580c; animation: flicker 1.1s ease-in-out infinite alternate; }
  .xp { color: #166534; }
  @keyframes flicker { from { opacity: 0.65; transform: scale(0.95); } to { opacity: 1; transform: scale(1.05); } }

  .recipe-meta { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .tag {
    font-size: 10px; padding: 3px 8px; border-radius: 6px;
    text-transform: uppercase; font-weight: 700;
  }
  .tag.type { background: rgba(103, 232, 249, 0.12); color: #67e8f9; }
  .tag.mod { background: var(--bg-tertiary); color: var(--text-muted); max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag.warn { background: rgba(251, 191, 36, 0.12); color: #fbbf24; }
  .recipe-id {
    font-size: 10px; color: var(--text-muted); word-break: break-all;
    text-align: center; max-width: 520px;
  }
  .recipe-actions { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  .recipe-actions .secondary {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 8px 12px; border-radius: 8px; font-size: 12px;
    background: var(--bg-tertiary); border: 1px solid var(--border-color);
    color: var(--text-secondary); cursor: pointer;
  }
  .recipe-actions .secondary.on { color: var(--jei-gold); border-color: rgba(251, 191, 36, 0.4); }

  .jei-overlay {
    background: #0e0e10; border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg); display: flex; flex-direction: column;
    overflow: hidden; min-height: 0;
  }
  .overlay-h {
    display: flex; justify-content: space-between; align-items: center; gap: 6px;
    padding: 8px 10px; border-bottom: 1px solid var(--border-color);
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em;
    color: var(--text-muted); font-weight: 700;
  }
  .item-grid {
    flex: 1; overflow: auto; padding: 8px;
    display: grid; grid-template-columns: repeat(auto-fill, minmax(36px, 1fr));
    gap: 3px; align-content: start;
  }
  .item-grid.compact { flex: none; max-height: 88px; }
  .item-slot {
    width: 36px; height: 36px; border: 1px solid #3a3a42; border-radius: 3px;
    background: linear-gradient(135deg, hsl(var(--hue) 32% 24%), hsl(var(--hue) 26% 16%));
    position: relative; cursor: pointer; padding: 0;
    box-shadow: inset 1px 1px 0 rgba(255,255,255,0.08);
  }
  .item-slot:hover, .item-slot.sel {
    border-color: var(--jei-gold);
    box-shadow: 0 0 0 1px rgba(251, 191, 36, 0.45);
  }
  .item-slot.bookmarked::after {
    content: ""; position: absolute; top: 2px; left: 2px;
    width: 4px; height: 4px; border-radius: 50%; background: var(--jei-gold);
  }
  .item-letter {
    font-size: 9px; font-weight: 800; color: #e8e8ec; text-transform: uppercase;
    text-shadow: 0 1px 1px #000;
  }
  .item-letter.icon-pending {
    width: 60%;
    height: 60%;
    border-radius: 2px;
    background: linear-gradient(90deg, rgba(255,255,255,0.06) 25%, rgba(255,255,255,0.14) 50%, rgba(255,255,255,0.06) 75%);
    background-size: 200% 100%;
    animation: icon-shimmer 1.1s linear infinite;
  }
  @keyframes icon-shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }
  .item-icon {
    width: calc(100% - 4px);
    height: calc(100% - 4px);
    object-fit: contain;
    image-rendering: pixelated;
    pointer-events: none;
  }
  .item-count {
    position: absolute; bottom: 0; right: 2px;
    font-size: 8px; font-weight: 800; color: var(--jei-gold);
  }
  .overlay-pager {
    display: flex; align-items: center; justify-content: center; gap: 10px;
    padding: 8px; border-top: 1px solid var(--border-color);
    font-size: 11px; color: var(--text-muted);
  }
  .bm-strip, .hist-strip { border-bottom: 1px solid var(--border-color); }

  .source-badge {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--text-muted);
  }
  .source-badge.live { color: #4ade80; }
  .live-launch {
    display: inline-flex; align-items: center; gap: 6px;
    border: 1px solid rgba(74, 222, 128, 0.4); border-radius: 5px;
    color: #86efac; background: rgba(34, 197, 94, 0.1);
    padding: 7px 10px; cursor: pointer;
  }
  .runtime-layout {
    width: var(--runtime-width); height: var(--runtime-height);
    min-width: 120px; min-height: 70px; position: relative;
    margin: 10px auto 4px;
  }
  .runtime-slot { position: absolute; }
  .runtime-slot.output { border-color: var(--jei-gold); }
  .stations {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    margin-top: 8px; color: var(--text-muted); font-size: 10px;
  }

  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 1100px) {
    .jei-body { grid-template-columns: 64px 1fr; }
    .jei-overlay { grid-column: 1 / -1; max-height: 280px; }
  }
  @media (max-width: 700px) {
    .jei-body { grid-template-columns: 1fr; }
    .cat-rail { flex-direction: row; overflow-x: auto; }
  }
</style>
