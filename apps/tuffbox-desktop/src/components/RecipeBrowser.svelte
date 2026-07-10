<script lang="ts">
  import { api, type ScannedRecipe, type IngredientDisplay } from "../lib/api";
  import {
    PackageOpen,
    Search,
    RefreshCw,
    ChevronLeft,
    ChevronRight,
    Copy,
    Flame,
    ArrowRight,
    Grid3x3,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type FocusMode = "recipes" | "uses";
  type ItemEntry = { id: string; name: string; modNs: string; recipeCount: number; useCount: number };

  let recipes: ScannedRecipe[] = [];
  let loading = false;
  let error: string | null = null;
  let message: string | null = null;
  let filter = "";
  let categoryFilter = "all";
  let focusMode: FocusMode = "recipes";
  let selectedItem = "";
  let recipeIndex = 0;
  let lastLoadedPath: string | null = null;

  async function loadRecipes() {
    if (!$projectPath) return;
    loading = true;
    error = null;
    message = null;
    try {
      recipes = await api.recipes.scan($projectPath);
      lastLoadedPath = $projectPath;
      selectedItem = "";
      recipeIndex = 0;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
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

  function matchesJeiSearch(itemId: string, query: string): boolean {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    const tokens = q.split(/\s+/).filter(Boolean);
    for (const token of tokens) {
      if (token.startsWith("-")) {
        const neg = token.slice(1);
        if (neg && matchesToken(itemId, neg)) return false;
        continue;
      }
      if (!matchesToken(itemId, token)) return false;
    }
    return true;
  }

  function matchesToken(itemId: string, token: string): boolean {
    const name = prettifyItem(itemId).toLowerCase();
    const id = itemId.toLowerCase();
    if (token.startsWith("@")) return itemNamespace(itemId).toLowerCase().includes(token.slice(1));
    if (token.startsWith("#")) return id.includes(token.slice(1)) || id.includes(token);
    if (token.startsWith("&")) return id.includes(token.slice(1));
    return name.includes(token) || id.includes(token);
  }

  function buildItemCatalog(list: ScannedRecipe[]): ItemEntry[] {
    const map = new Map<string, ItemEntry>();
    const touch = (id: string, field: "recipeCount" | "useCount") => {
      if (!id || id === "unknown:unknown") return;
      const existing = map.get(id);
      if (existing) existing[field]++;
      else
        map.set(id, {
          id,
          name: prettifyItem(id),
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
      if (mode === "recipes") return r.outputId === itemId;
      return r.inputIds.includes(itemId);
    });
  }

  function selectItem(id: string, mode?: FocusMode) {
    selectedItem = id;
    if (mode) focusMode = mode;
    recipeIndex = 0;
  }

  function slotLabel(ing: IngredientDisplay | null): string {
    if (!ing) return "";
    if (ing.kind === "tag") return ing.id.replace(/^#?/, "#");
    return prettifyItem(ing.id).slice(0, 3);
  }

  function copyKubeJS(r: ScannedRecipe) {
    const script = `ServerEvents.recipes(event => {\n  event.remove({ id: '${r.id}' })\n})`;
    navigator.clipboard.writeText(script).then(() => (message = "KubeJS remove copied"));
  }

  function navigateSlot(ing: IngredientDisplay | null, mode: FocusMode) {
    if (!ing) return;
    const id = ing.kind === "tag" ? ing.id : ing.id;
    if (id.startsWith("#")) return;
    selectItem(id, mode);
  }

  $: items = buildItemCatalog(recipes);
  $: filteredItems = items.filter((i) => matchesJeiSearch(i.id, filter));
  $: activeRecipes = recipesForItem(selectedItem, focusMode);
  $: currentRecipe = activeRecipes[recipeIndex] ?? null;
  $: categories = ["all", ...new Set(recipes.map((r) => r.category))].sort();
  $: if ($projectPath && $projectPath !== lastLoadedPath) loadRecipes();

  function prevRecipe() {
    if (recipeIndex > 0) recipeIndex--;
  }
  function nextRecipe() {
    if (recipeIndex < activeRecipes.length - 1) recipeIndex++;
  }
</script>

<div class="jei">
  <header class="jei-hd">
    <div class="jei-title"><PackageOpen size={18} /> Recipe browser <span class="jei-badge">JEI mode</span></div>
    <button class="ghost" on:click={loadRecipes} disabled={!$projectPath || loading}>
      <RefreshCw size={16} class={loading ? "spin" : ""} /> Scan
    </button>
  </header>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}

  <div class="jei-search-row">
    <div class="jei-search">
      <Search size={14} />
      <input bind:value={filter} placeholder="JEI search: name, @mod, #tag, &id" />
    </div>
    <div class="jei-cats">
      {#each categories as cat}
        <button class="cat-btn" class:active={categoryFilter === cat} on:click={() => (categoryFilter = cat)}>
          {cat === "all" ? "All" : cat}
        </button>
      {/each}
    </div>
  </div>

  {#if !$projectPath}
    <div class="empty">Open a project to browse recipes like JEI.</div>
  {:else if loading && recipes.length === 0}
    <div class="empty"><RefreshCw size={32} class="spin" /><p>Scanning mod JARs…</p></div>
  {:else if recipes.length === 0}
    <div class="empty"><p>No recipes found. Click Scan to index mod JARs.</p><button on:click={loadRecipes}>Scan JARs</button></div>
  {:else}
    <div class="jei-body">
      <!-- Item list (JEI ingredient overlay) -->
      <aside class="jei-items">
        <div class="jei-items-h">
          <span>Items</span>
          <small>{filteredItems.length}</small>
        </div>
        <div class="item-grid">
          {#each filteredItems as item (item.id)}
            <button
              class="item-slot"
              class:sel={selectedItem === item.id}
              style="--hue: {itemHue(item.id)}"
              title="{item.id}\nR: {item.recipeCount} recipes · U: {item.useCount} uses"
              on:click={() => selectItem(item.id, "recipes")}
              on:contextmenu|preventDefault={() => selectItem(item.id, "uses")}
            >
              <span class="item-letter">{item.name.slice(0, 2)}</span>
              {#if item.recipeCount > 1 || item.useCount > 1}
                <span class="item-count">{item.recipeCount + item.useCount}</span>
              {/if}
            </button>
          {/each}
        </div>
        <p class="jei-hint">Click = Recipes (R) · Right-click = Uses (U)</p>
      </aside>

      <!-- Recipe viewer -->
      <main class="jei-view">
        {#if !selectedItem}
          <div class="view-empty">
            <Grid3x3 size={48} />
            <h3>Select an item</h3>
            <p>Choose from the item list — like JEI's ingredient overlay.</p>
            <p class="muted">{recipes.length} recipes indexed from mod JARs</p>
          </div>
        {:else}
          <div class="view-toolbar">
            <div class="focus-tabs">
              <button class:active={focusMode === "recipes"} on:click={() => (focusMode = "recipes")}>
                Recipes ({recipesForItem(selectedItem, "recipes").length})
              </button>
              <button class:active={focusMode === "uses"} on:click={() => (focusMode = "uses")}>
                Uses ({recipesForItem(selectedItem, "uses").length})
              </button>
            </div>
            <div class="item-focus">
              <span class="focus-slot" style="--hue: {itemHue(selectedItem)}">{prettifyItem(selectedItem).slice(0, 2)}</span>
              <div>
                <strong>{prettifyItem(selectedItem)}</strong>
                <code>{selectedItem}</code>
              </div>
            </div>
          </div>

          {#if activeRecipes.length === 0}
            <div class="view-empty compact">
              <p>No {focusMode} for this item{categoryFilter !== "all" ? ` in ${categoryFilter}` : ""}.</p>
            </div>
          {:else if currentRecipe}
            <div class="recipe-stage">
              <div class="recipe-nav">
                <button class="ghost" on:click={prevRecipe} disabled={recipeIndex === 0}><ChevronLeft size={18} /></button>
                <span>{recipeIndex + 1} / {activeRecipes.length}</span>
                <button class="ghost" on:click={nextRecipe} disabled={recipeIndex >= activeRecipes.length - 1}><ChevronRight size={18} /></button>
              </div>

              <div class="recipe-panel" class:cooking={currentRecipe.layout.category === "cooking"}>
                {#if currentRecipe.layout.category === "crafting"}
                  <div class="craft-grid">
                    {#each currentRecipe.layout.grid as slot, i}
                      <button
                        class="r-slot"
                        class:empty={!slot}
                        style={slot ? `--hue: ${itemHue(slot.id)}` : ""}
                        on:click={() => navigateSlot(slot, "uses")}
                        title={slot?.id ?? ""}
                      >
                        {#if slot}<span>{slotLabel(slot)}</span>{/if}
                      </button>
                    {/each}
                  </div>
                  <div class="recipe-arrow"><ArrowRight size={28} /></div>
                  {#if currentRecipe.layout.shapeless}<span class="shapeless-badge">Shapeless</span>{/if}
                {:else if currentRecipe.layout.category === "cooking"}
                  <button
                    class="r-slot large"
                    style="--hue: {itemHue(currentRecipe.layout.grid[4]?.id ?? '')}"
                    on:click={() => navigateSlot(currentRecipe.layout.grid[4] ?? null, "uses")}
                  >
                    {slotLabel(currentRecipe.layout.grid[4] ?? null)}
                  </button>
                  <div class="cook-mid">
                    <Flame size={24} class="flame" />
                    {#if currentRecipe.layout.cookTime}
                      <span>{(currentRecipe.layout.cookTime / 20).toFixed(1)}s</span>
                    {/if}
                  </div>
                  <div class="recipe-arrow"><ArrowRight size={28} /></div>
                {:else}
                  <div class="craft-grid loose">
                    {#each currentRecipe.layout.grid.filter(Boolean) as slot}
                      <button class="r-slot" style="--hue: {itemHue(slot?.id ?? '')}" on:click={() => navigateSlot(slot, "uses")}>
                        {slotLabel(slot)}
                      </button>
                    {/each}
                  </div>
                  <div class="recipe-arrow"><ArrowRight size={28} /></div>
                {/if}

                <button
                  class="r-slot output"
                  style="--hue: {itemHue(currentRecipe.layout.output.id)}"
                  on:click={() => navigateSlot(currentRecipe.layout.output, "recipes")}
                >
                  <span>{prettifyItem(currentRecipe.layout.output.id).slice(0, 4)}</span>
                  {#if currentRecipe.layout.outputCount > 1}
                    <em class="stack">{currentRecipe.layout.outputCount}</em>
                  {/if}
                </button>
              </div>

              <div class="recipe-meta">
                <span class="type-tag">{currentRecipe.recipeType.replace("minecraft:", "")}</span>
                {#if currentRecipe.isConditional}<span class="warn-tag">conditional</span>{/if}
                <span class="mod-tag">{currentRecipe.modSource}</span>
                {#if currentRecipe.layout.experience}
                  <span class="xp-tag">+{currentRecipe.layout.experience} XP</span>
                {/if}
              </div>
              <code class="recipe-id">{currentRecipe.id}</code>
              <div class="recipe-actions">
                <button class="secondary" on:click={() => copyKubeJS(currentRecipe)}><Copy size={14} /> KubeJS remove</button>
              </div>
            </div>
          {/if}
        {/if}
      </main>
    </div>
  {/if}
</div>

<style>
  .jei { max-width: none; width: 100%; }
  .jei-hd { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .jei-title { display: flex; align-items: center; gap: 10px; font-weight: 700; color: var(--text-secondary); }
  .jei-badge { font-size: 10px; font-weight: 800; text-transform: uppercase; letter-spacing: 0.06em; padding: 3px 8px; border-radius: 6px; background: rgba(251, 191, 36, 0.12); color: #fbbf24; border: 1px solid rgba(251, 191, 36, 0.25); }
  .notice { padding: 10px 14px; border-radius: 10px; margin-bottom: 12px; font-size: 13px; }
  .notice.error { background: rgba(239, 68, 68, 0.08); color: #fecaca; border: 1px solid rgba(239, 68, 68, 0.25); }
  .notice.ok { background: rgba(27, 217, 106, 0.08); color: var(--accent-primary); border: 1px solid rgba(27, 217, 106, 0.25); }
  .jei-search-row { display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 12px; align-items: center; }
  .jei-search { flex: 1; min-width: 220px; display: flex; align-items: center; position: relative; }
  .jei-search :global(svg) { position: absolute; left: 10px; color: var(--text-muted); }
  .jei-search input { width: 100%; padding-left: 34px; }
  .jei-cats { display: flex; gap: 4px; flex-wrap: wrap; }
  .cat-btn { padding: 6px 12px; font-size: 11px; text-transform: capitalize; border-radius: 8px; background: var(--bg-secondary); border: 1px solid var(--border-color); color: var(--text-muted); cursor: pointer; }
  .cat-btn.active { background: rgba(27, 217, 106, 0.1); border-color: rgba(27, 217, 106, 0.35); color: var(--accent-primary); }
  .empty { padding: 80px; text-align: center; color: var(--text-muted); background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .jei-body { display: grid; grid-template-columns: 280px 1fr; gap: 14px; min-height: 620px; }
  .jei-items { background: #121214; border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); display: flex; flex-direction: column; overflow: hidden; }
  .jei-items-h { display: flex; justify-content: space-between; padding: 10px 12px; border-bottom: 1px solid var(--border-color); font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .item-grid { flex: 1; overflow: auto; padding: 8px; display: grid; grid-template-columns: repeat(auto-fill, minmax(36px, 1fr)); gap: 4px; align-content: start; }
  .item-slot { width: 36px; height: 36px; border: 1px solid #3a3a42; border-radius: 4px; background: linear-gradient(135deg, hsl(var(--hue) 30% 22%), hsl(var(--hue) 25% 16%)); position: relative; cursor: pointer; padding: 0; }
  .item-slot:hover, .item-slot.sel { border-color: #fbbf24; box-shadow: 0 0 0 1px rgba(251, 191, 36, 0.4); }
  .item-letter { font-size: 10px; font-weight: 800; color: #e8e8ec; text-transform: uppercase; }
  .item-count { position: absolute; bottom: 1px; right: 2px; font-size: 8px; font-weight: 700; color: #fbbf24; }
  .jei-hint { font-size: 10px; color: var(--text-muted); padding: 8px 10px; border-top: 1px solid var(--border-color); margin: 0; }
  .jei-view { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 16px; display: flex; flex-direction: column; }
  .view-empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px; color: var(--text-muted); }
  .view-empty.compact { padding: 40px; }
  .view-empty h3 { margin: 0; color: var(--text-secondary); }
  .view-empty .muted { font-size: 12px; }
  .view-toolbar { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; }
  .focus-tabs { display: flex; gap: 4px; }
  .focus-tabs button { padding: 8px 14px; font-size: 12px; border-radius: 8px; background: var(--bg-tertiary); border: 1px solid var(--border-color); color: var(--text-muted); cursor: pointer; }
  .focus-tabs button.active { background: rgba(27, 217, 106, 0.1); border-color: rgba(27, 217, 106, 0.35); color: var(--accent-primary); font-weight: 600; }
  .item-focus { display: flex; align-items: center; gap: 10px; }
  .focus-slot { width: 40px; height: 40px; border: 1px solid #fbbf24; border-radius: 6px; display: flex; align-items: center; justify-content: center; font-size: 11px; font-weight: 800; background: linear-gradient(135deg, hsl(var(--hue) 35% 24%), hsl(var(--hue) 28% 18%)); }
  .item-focus strong { display: block; font-size: 14px; }
  .item-focus code { font-size: 10px; color: var(--text-muted); }
  .recipe-stage { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 14px; }
  .recipe-nav { display: flex; align-items: center; gap: 12px; color: var(--text-muted); font-size: 13px; }
  .recipe-panel { display: flex; align-items: center; gap: 16px; padding: 24px 32px; background: #0c0c0e; border: 1px solid #2a2a32; border-radius: 12px; position: relative; min-height: 140px; }
  .recipe-panel.cooking { gap: 20px; }
  .craft-grid { display: grid; grid-template-columns: repeat(3, 44px); grid-template-rows: repeat(3, 44px); gap: 4px; }
  .craft-grid.loose { grid-template-columns: repeat(auto-fill, 44px); grid-template-rows: auto; }
  .r-slot { width: 44px; height: 44px; border: 1px solid #4a4a54; border-radius: 4px; background: linear-gradient(180deg, #3a3a44 0%, #2a2a32 100%); display: flex; align-items: center; justify-content: center; font-size: 9px; font-weight: 700; color: #ddd; cursor: pointer; padding: 2px; text-align: center; line-height: 1.1; position: relative; }
  .r-slot.empty { background: #1a1a1e; border-color: #2e2e36; cursor: default; }
  .r-slot:not(.empty):hover { border-color: #fbbf24; }
  .r-slot.large { width: 52px; height: 52px; font-size: 11px; }
  .r-slot.output { width: 52px; height: 52px; border-color: #6ee7b7; background: linear-gradient(135deg, hsl(var(--hue) 40% 28%), hsl(var(--hue) 32% 20%)); font-size: 10px; }
  .stack { position: absolute; bottom: 2px; right: 4px; font-style: normal; font-size: 11px; font-weight: 800; color: #fff; text-shadow: 0 1px 2px #000; }
  .recipe-arrow { color: #888; }
  .shapeless-badge { position: absolute; top: 8px; right: 8px; font-size: 9px; text-transform: uppercase; color: #67e8f9; background: rgba(103, 232, 249, 0.1); padding: 2px 6px; border-radius: 4px; }
  .cook-mid { display: flex; flex-direction: column; align-items: center; gap: 4px; color: #f97316; font-size: 11px; }
  .cook-mid :global(.flame) { animation: flicker 1.2s ease-in-out infinite alternate; }
  @keyframes flicker { from { opacity: 0.7; } to { opacity: 1; } }
  .recipe-meta { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  .type-tag, .mod-tag, .warn-tag, .xp-tag { font-size: 10px; padding: 3px 8px; border-radius: 6px; text-transform: uppercase; font-weight: 700; }
  .type-tag { background: rgba(103, 232, 249, 0.1); color: #67e8f9; }
  .mod-tag { background: var(--bg-tertiary); color: var(--text-muted); }
  .warn-tag { background: rgba(251, 191, 36, 0.1); color: #fbbf24; }
  .xp-tag { background: rgba(134, 239, 172, 0.1); color: #86efac; }
  .recipe-id { font-size: 10px; color: var(--text-muted); word-break: break-all; text-align: center; max-width: 480px; }
  .recipe-actions { display: flex; gap: 8px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) { .jei-body { grid-template-columns: 1fr; } }
</style>
