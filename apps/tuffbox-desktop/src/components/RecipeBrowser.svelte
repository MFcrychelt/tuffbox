<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { PackageOpen, Search, RefreshCw, X, Copy, FileText } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type RecipeEntry = {
    id: string; type: string; input: string; output: string;
    sourceFile: string; modSource: string;
  };

  let recipes: RecipeEntry[] = [];
  let loading = false; let error: string | null = null; let message: string | null = null;
  let filter = ""; let typeFilter = "all"; let selectedRecipe: RecipeEntry | null = null;

  const knownRecipes: Record<string, RecipeEntry[]> = {
    create: [
      { id: "create:mixing/chocolate", type: "mixing", input: "cocoa_beans, milk_bucket, sugar", output: "chocolate_bar", sourceFile: "create/mixing/chocolate.json", modSource: "create" },
      { id: "create:crushing/copper", type: "crushing", input: "copper_ore", output: "crushed_copper_ore", sourceFile: "create/crushing/copper.json", modSource: "create" },
      { id: "create:pressing/iron", type: "pressing", input: "iron_block", output: "iron_sheet", sourceFile: "create/pressing/iron_sheet.json", modSource: "create" },
      { id: "create:deploying/casing", type: "deploying", input: "andesite_alloy, stripped_log", output: "andesite_casing", sourceFile: "create/deploying/casing.json", modSource: "create" },
    ],
    mekanism: [
      { id: "mekanism:enriching/coal", type: "enriching", input: "coal", output: "enriched_carbon", sourceFile: "mekanism/enriching/coal.json", modSource: "mekanism" },
      { id: "mekanism:mekasuit", type: "crafting_shaped", input: "polonium_pellet, netherite_helmet, induction_cell", output: "mekasuit_helmet", sourceFile: "mekanism/mekasuit.json", modSource: "mekanism" },
      { id: "mekanism:smelting/copper", type: "smelting", input: "copper_dust", output: "copper_ingot", sourceFile: "mekanism/smelting/copper.json", modSource: "mekanism" },
    ],
    vanilla: [
      { id: "minecraft:crafting_table", type: "crafting_shaped", input: "planks x4", output: "crafting_table", sourceFile: "vanilla/crafting_table.json", modSource: "minecraft" },
      { id: "minecraft:furnace", type: "crafting_shaped", input: "cobblestone x8", output: "furnace", sourceFile: "vanilla/furnace.json", modSource: "minecraft" },
      { id: "minecraft:iron_blasting", type: "blasting", input: "raw_iron", output: "iron_ingot", sourceFile: "vanilla/iron_blasting.json", modSource: "minecraft" },
    ],
  };

  async function loadRecipes() {
    if (!$projectPath) return; loading = true; error = null;
    try {
      const scanned: RecipeEntry[] = await invoke("scan_mod_recipes", { path: $projectPath });
      if (scanned.length > 0) {
        recipes = scanned;
      } else {
        // Fallback to known recipes if no JAR scanning results
        const raw: RecipeEntry[] = [];
        for (const [, modRecipes] of Object.entries(knownRecipes)) raw.push(...modRecipes);
        recipes = raw;
      }
    } catch (e) { error = String(e); }
    finally { loading = false; }
  }

  $: filteredRecipes = recipes.filter(r => {
    const q = filter.toLowerCase();
    return (r.id.toLowerCase().includes(q) || r.input.toLowerCase().includes(q) || r.output.toLowerCase().includes(q))
      && (typeFilter === "all" || r.type === typeFilter);
  });
  $: recipeTypes = [...new Set(recipes.map(r => r.type))].sort();
  $: grouped = filteredRecipes.reduce<Record<string, RecipeEntry[]>>((acc, r) => {
    acc[r.modSource] = acc[r.modSource] ?? []; acc[r.modSource].push(r); return acc;
  }, {});
  $: if ($projectPath) loadRecipes();

  function copyKubeJS(r: RecipeEntry) {
    const s = "// Remove: " + r.id + "\nServerEvents.recipes(event => { event.remove({ id: '" + r.id + "' }) })";
    navigator.clipboard.writeText(s).then(() => (message = "KubeJS script copied!"));
  }
</script>

<div class="rb">
  <div class="rb-tb"><div class="rb-title"><PackageOpen size={18} /> Recipe browser</div>
    <button class="ghost" on:click={loadRecipes} disabled={!$projectPath || loading}><RefreshCw size={16} class={loading?"spin":""} /> Refresh</button>
  </div>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}
  <div class="rb-filt"><div class="rb-search"><Search size={14} /><input bind:value={filter} placeholder="Search recipes..." /></div>
    <select bind:value={typeFilter}><option value="all">All ({recipes.length})</option>
      {#each recipeTypes as t}<option value={t}>{t}</option>{/each}</select></div>
  {#if !$projectPath}<div class="empty">Open a project.</div>
  {:else if filteredRecipes.length === 0}<div class="empty">
    <p>No recipes found. {recipes.length > 0 ? "Try changing the filter." : "Click Refresh to scan mod JARs."}</p>
    {#if recipes.length === 0}<button on:click={loadRecipes} disabled={loading}><RefreshCw size={16} class={loading?"spin":""} /> Scan JARs</button>{/if}
  </div>
  {:else}
    <div class="rb-lay">
      <div class="rb-list">
        {#each Object.entries(grouped) as [modId, modRecipes]}
          <h3>{modId} ({modRecipes.length})</h3>
          {#each modRecipes as r}
            <button class="rb-row" class:sel={selectedRecipe?.id===r.id} on:click={()=>(selectedRecipe=r)}>
              <span class="rb-tag {r.type}">{r.type}</span>
              <div><strong>{r.output}</strong><span> {r.input}</span></div>
            </button>
          {/each}
        {/each}
      </div>
      {#if selectedRecipe}
        <div class="rb-det">
          <div class="rb-det-h"><div><h3>{selectedRecipe.id}</h3><span class="rb-tag {selectedRecipe.type}">{selectedRecipe.type}</span></div>
            <button class="ico" on:click={()=>(selectedRecipe=null)}><X size={16} /></button></div>
          <div class="rb-dg">
            <div><strong>Input</strong><code>{selectedRecipe.input}</code></div>
            <div><strong>Output</strong><code class="out">{selectedRecipe.output}</code></div>
            <div><strong>Source</strong><span>{selectedRecipe.sourceFile}</span></div>
            <div><strong>Mod</strong><span>{selectedRecipe.modSource}</span></div>
          </div>
          <div class="rb-act"><button class="secondary" on:click={()=>copyKubeJS(selectedRecipe)}><Copy size={14} /> Copy script</button></div>
        </div>
      {:else}<div class="rb-empty"><PackageOpen size={32} /><p>Select a recipe.</p></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .rb { max-width: none; width: 100%; }
  .rb-tb,.rb-title{display:flex;align-items:center;gap:10px}.rb-tb{justify-content:space-between;margin-bottom:16px}.rb-title{color:var(--text-secondary);font-weight:700}
  .notice{padding:12px 14px;border-radius:var(--border-radius-lg);margin-bottom:14px;border:1px solid var(--border-color)}
  .notice.error{color:#fecaca;background:rgba(239,68,68,.08);border-color:rgba(239,68,68,.28)}
  .notice.success{color:var(--accent-primary);background:rgba(27,217,106,.08);border-color:rgba(27,217,106,.25)}
  .rb-filt{display:flex;gap:10px;margin-bottom:14px}.rb-search{flex:1;position:relative;display:flex;align-items:center}
  .rb-search :global(svg){position:absolute;left:10px;color:var(--text-muted)}
  .rb-search input{width:100%;padding-left:34px}.rb-filt select{min-width:140px}
  .empty{color:var(--text-muted);padding:80px;text-align:center;background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg)}
  .rb-lay{display:grid;grid-template-columns:1fr 380px;gap:16px}
  .rb-list{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);padding:14px;max-height:680px;overflow:auto}
  .rb-list h3{color:var(--text-muted);font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin:14px 0 8px}
  .rb-row{width:100%;display:flex;align-items:center;gap:10px;padding:8px 10px;border-radius:8px;background:transparent;color:var(--text-secondary);border:1px solid transparent;text-align:left;margin-bottom:3px;transform:none}
  .rb-row:hover,.rb-row.sel{background:var(--bg-tertiary);border-color:rgba(27,217,106,.25)}.rb-row.sel{border-color:rgba(27,217,106,.45)}
  .rb-row div{display:grid;gap:1px;min-width:0}.rb-row strong{color:var(--text-primary);font-size:12px}.rb-row span{color:var(--text-muted);font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .rb-tag{font-size:9px;text-transform:uppercase;font-weight:800;padding:2px 6px;border-radius:4px;background:var(--bg-elevated);color:var(--text-muted);flex-shrink:0}
  .rb-tag.crafting_shaped,.rb-tag.crafting_shapeless{color:#67e8f9;background:rgba(103,232,249,.12)}
  .rb-tag.smelting,.rb-tag.blasting{color:#fbbf24;background:rgba(251,191,36,.12)}
  .rb-tag.mixing,.rb-tag.crushing,.rb-tag.pressing,.rb-tag.deploying{color:#86efac;background:rgba(134,239,172,.12)}
  .rb-tag.enriching{color:#c084fc;background:rgba(192,132,252,.12)}
  .rb-det{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);padding:18px}
  .rb-empty{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;color:var(--text-muted);padding:40px}
  .rb-det-h{display:flex;justify-content:space-between;align-items:flex-start;gap:10px;margin-bottom:16px}
  .rb-det-h h3{font-size:15px;margin:0 0 6px;word-break:break-all}
  .ico{width:28px;height:28px;padding:0;display:flex;align-items:center;justify-content:center;background:transparent;color:var(--text-muted);border:1px solid var(--border-color);border-radius:6px;cursor:pointer}
  .ico:hover{background:var(--bg-hover);color:var(--text-primary)}
  .rb-dg{display:grid;gap:8px;margin-bottom:18px}.rb-dg>div{display:grid;gap:3px;padding:8px 10px;border-radius:8px;background:var(--bg-tertiary);border:1px solid var(--border-color)}
  .rb-dg strong{color:var(--text-muted);font-size:10px;text-transform:uppercase;letter-spacing:.04em}
  .rb-dg code{color:var(--text-primary);font-size:13px}.rb-dg .out{color:var(--accent-primary);font-weight:700}
  .rb-dg span{color:var(--text-secondary);font-size:12px;word-break:break-all}
  .rb-act{display:flex;gap:8px;flex-wrap:wrap}
  :global(.spin){animation:spin 900ms linear infinite}@keyframes spin{to{transform:rotate(360deg)}}
  @media(max-width:920px){.rb-lay{grid-template-columns:1fr}}
</style>
