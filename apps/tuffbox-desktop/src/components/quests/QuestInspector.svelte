<script lang="ts">
  import { Trash2, Link2, AlertTriangle, Copy, Check, ChevronDown, ChevronRight } from "@lucide/svelte";
  import QuestItemIcon from "./QuestItemIcon.svelte";
  import type { QuestChapter, QuestData, QuestValidationIssue } from "../../lib/api";
  import { DEP_REQUIREMENT_OPTIONS, SHAPE_OPTIONS } from "../../lib/questTypeLabels";
  import {
    localeValueAsString,
    type LocaleMap,
  } from "../../lib/questLocale";
  import TaskRewardEditor from "./TaskRewardEditor.svelte";
  import ItemStackEditor from "./ItemStackEditor.svelte";

  let {
    quest,
    chapterQuests,
    chapters = [],
    issues = [],
    rewardTableIds = [],
    activeLocale = null,
    compareLocale = null,
    compareMap = null,
    availableLocales = [],
    onDirty,
    onCompareDirty,
    onCompareLocaleChange,
    onCompareMapChange,
    onRemove,
    onAddDep,
    onRemoveDep,
    onOpenKubeJs,
    focusFieldToken = 0,
    focusField = null as string | null,
  }: {
    quest: QuestData;
    chapterQuests: QuestData[];
    chapters?: QuestChapter[];
    issues?: QuestValidationIssue[];
    rewardTableIds?: string[];
    activeLocale?: string | null;
    compareLocale?: string | null;
    compareMap?: LocaleMap | null;
    availableLocales?: string[];
    onDirty: () => void;
    onCompareDirty?: (code: string) => void;
    onCompareLocaleChange?: (code: string | null) => void;
    onCompareMapChange?: (code: string, map: LocaleMap) => void;
    onRemove: () => void;
    onAddDep: (depId: string) => void;
    onRemoveDep: (depId: string) => void;
    onOpenKubeJs?: (id: string) => void;
    /** Increment to focus `focusField` (title | tasks | icon). */
    focusFieldToken?: number;
    focusField?: string | null;
  } = $props();

  let depPick = $state("");
  let depFilter = $state("");
  let descText = $state("");
  let extraKey = $state("");
  let idCopied = $state(false);
  let idCopyTimer: ReturnType<typeof setTimeout> | null = null;
  let titleInputEl = $state<HTMLInputElement | null>(null);
  let lastFocusToken = $state(0);

  async function copyQuestId() {
    try {
      await navigator.clipboard.writeText(quest.id);
      idCopied = true;
      if (idCopyTimer) clearTimeout(idCopyTimer);
      idCopyTimer = setTimeout(() => {
        idCopied = false;
        idCopyTimer = null;
      }, 1500);
    } catch {
      /* clipboard may be unavailable */
    }
  }
  let extraVal = $state("");
  let showAdvanced = $state(false);
  let depsOpen = $state(true);
  let cmpTitle = $state("");
  let cmpSubtitle = $state("");
  let cmpDesc = $state("");

  // Locale gap jump sets compareLocale — open More so compare columns are visible.
  $effect(() => {
    if (showCompare) showAdvanced = true;
  });

  $effect(() => {
    if (!focusFieldToken || focusFieldToken === lastFocusToken) return;
    lastFocusToken = focusFieldToken;
    const field = focusField;
    queueMicrotask(() => {
      if (field === "title" || field === "icon") {
        titleInputEl?.focus();
        titleInputEl?.select();
      } else if (field === "tasks" || field === "item") {
        document.getElementById("quest-how-to-prove")?.scrollIntoView({ block: "nearest" });
      }
    });
  });

  let depOptions = $derived(buildDepOptions(chapters, chapterQuests, quest));
  let filteredDepOptions = $derived.by(() => {
    const q = depFilter.trim().toLowerCase();
    if (!q) return depOptions;
    return depOptions.filter(
      (o) =>
        o.label.toLowerCase().includes(q) ||
        o.id.toLowerCase().includes(q),
    );
  });
  let myIssues = $derived(issues.filter((i) => i.questId === quest.id));
  let showCompare = $derived(
    !!compareLocale &&
      !!compareMap &&
      compareLocale !== activeLocale &&
      availableLocales.length > 1,
  );

  function normalizeDescLines(text: string): string[] {
    const lines = text
      .split("\n")
      .map((s) => s.trimEnd())
      .filter((s, i, arr) => s.length > 0 || i < arr.length - 1);
    while (lines.length && lines[lines.length - 1] === "") lines.pop();
    return lines;
  }

  function commitDescriptionTo(target: QuestData, text: string): boolean {
    const lines = normalizeDescLines(text);
    const prev = (target.description ?? []).join("\n");
    const next = lines.join("\n");
    if (prev === next) return false;
    target.description = lines;
    target.descriptionFromSnbt = true;
    onDirty();
    return true;
  }

  function commitDescription() {
    commitDescriptionTo(quest, descText);
  }

  /** Flush pending description when leaving a quest (deselect / switch) before resync. */
  $effect(() => {
    const target = quest;
    void target.id;
    descText = (target.description ?? []).join("\n");
    return () => {
      commitDescriptionTo(target, descText);
    };
  });

  $effect(() => {
    const id = quest.id;
    const map = compareMap;
    const code = compareLocale;
    if (!map || !code) {
      cmpTitle = "";
      cmpSubtitle = "";
      cmpDesc = "";
      return;
    }
    cmpTitle = localeValueAsString(map, `quest.${id}.title`);
    cmpSubtitle = localeValueAsString(map, `quest.${id}.quest_subtitle`);
    cmpDesc = localeValueAsString(map, `quest.${id}.quest_desc`);
    return () => {
      if (!code || !map) return;
      const lines = normalizeDescLines(cmpDesc);
      const key = `quest.${id}.quest_desc`;
      const prev = localeValueAsString(map, key);
      if (prev === lines.join("\n")) return;
      const next: LocaleMap = { ...map };
      for (const [k, v] of Object.entries(next)) {
        if (Array.isArray(v)) next[k] = [...v];
      }
      next[key] = lines;
      onCompareMapChange?.(code, next);
      onCompareDirty?.(code);
    };
  });

  function patchCompare(mutator: (map: LocaleMap) => void) {
    if (!compareLocale || !compareMap) return;
    const next: LocaleMap = { ...compareMap };
    for (const [k, v] of Object.entries(next)) {
      if (Array.isArray(v)) next[k] = [...v];
    }
    mutator(next);
    onCompareMapChange?.(compareLocale, next);
    onCompareDirty?.(compareLocale);
  }

  function commitCompareDesc() {
    const lines = normalizeDescLines(cmpDesc);
    patchCompare((map) => {
      map[`quest.${quest.id}.quest_desc`] = lines;
    });
  }

  function buildDepOptions(
    allChapters: QuestChapter[],
    sameChapter: QuestData[],
    current: QuestData,
  ) {
    const opts: { id: string; label: string }[] = [];
    const walk: { title: string; same: boolean; quests: QuestData[] }[] =
      allChapters.length > 0
        ? (() => {
            const currentChId = allChapters.find((ch) =>
              ch.quests.some((q) => q.id === current.id),
            )?.id;
            return allChapters.map((ch) => ({
              title: ch.title || ch.filename || ch.id.slice(0, 8),
              same: ch.id === currentChId,
              quests: ch.quests,
            }));
          })()
        : [{ title: "", same: true, quests: sameChapter }];

    for (const ch of walk) {
      for (const q of ch.quests) {
        if (q.id === current.id) continue;
        const base = ch.same ? q.title : `${ch.title} · ${q.title}`;
        if (!current.dependencies.includes(q.id)) {
          opts.push({ id: q.id, label: base });
        }
        for (const t of q.tasks ?? []) {
          if (!t.id || current.dependencies.includes(t.id)) continue;
          opts.push({
            id: t.id,
            label: `${base} · ${t.title || t.type || "task"}`,
          });
        }
      }
    }
    return opts;
  }

  function titleOf(id: string) {
    const direct = chapterQuests.find((q) => q.id === id);
    if (direct) return direct.title;
    const viaTask = chapterQuests.find((q) => q.tasks?.some((t) => t.id === id));
    if (viaTask) {
      const task = viaTask.tasks.find((t) => t.id === id);
      return `${viaTask.title}${task?.title ? ` · ${task.title}` : " (task)"}`;
    }
    for (const ch of chapters) {
      const q = ch.quests.find((x) => x.id === id);
      if (q) return `${ch.title || ch.filename || ch.id.slice(0, 8)} · ${q.title}`;
      const owner = ch.quests.find((x) => x.tasks?.some((t) => t.id === id));
      if (owner) {
        const task = owner.tasks.find((t) => t.id === id);
        return `${ch.title || ch.filename || ch.id.slice(0, 8)} · ${owner.title}${task?.title ? ` · ${task.title}` : " (task)"}`;
      }
    }
    return id;
  }

  function applyDep() {
    if (!depPick) return;
    onAddDep(depPick);
    depPick = "";
    depFilter = "";
  }

  function applyDepFromFilter() {
    if (depPick) {
      applyDep();
      return;
    }
    const first = filteredDepOptions[0];
    if (!first) return;
    onAddDep(first.id);
    depPick = "";
    depFilter = "";
  }

  function wrapFmt(code: string) {
    descText = `${descText}${descText && !descText.endsWith("\n") ? "" : ""}${code}`;
    commitDescription();
  }

  function insertTemplate(kind: string) {
    const lines =
      kind === "objective"
        ? ["&7Objective:", "&fComplete the listed tasks.", "&8Rewards unlock the next step."]
        : kind === "story"
          ? ["&l&6Chapter beat", "&7A story beat for this pack.", "&aFinish to continue the line."]
          : ["&eHint: &7check JEI for recipes."];
    descText = [...(descText ? descText.split("\n") : []), ...lines].join("\n");
    commitDescription();
  }

  function tri(field: keyof QuestData, e: Event) {
    const s = (e.target as HTMLSelectElement).value;
    (quest as unknown as Record<string, unknown>)[field as string] =
      s === "true" ? true : s === "false" ? false : null;
    onDirty();
  }

  function triVal(v: boolean | null | undefined): string {
    return v === true ? "true" : v === false ? "false" : "";
  }

  function ensureExtras() {
    if (!quest.extras) quest.extras = {};
    return quest.extras;
  }

  function addExtra() {
    const k = extraKey.trim();
    if (!k) return;
    let parsed: unknown = extraVal;
    try {
      parsed = JSON.parse(extraVal);
    } catch {
      /* string */
    }
    ensureExtras()[k] = parsed as never;
    quest.extras = { ...quest.extras };
    extraKey = "";
    extraVal = "";
    onDirty();
  }

  function removeExtra(k: string) {
    if (!quest.extras) return;
    delete quest.extras[k];
    quest.extras = { ...quest.extras };
    onDirty();
  }

  function selectVal(e: Event): string {
    return (e.currentTarget as HTMLSelectElement).value;
  }
  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }
  function textareaVal(e: Event): string {
    return (e.currentTarget as HTMLTextAreaElement).value;
  }

  function autoGrowDescription(e: Event) {
    const el = e.currentTarget as HTMLTextAreaElement;
    descText = el.value;
    el.style.height = "auto";
    el.style.height = `${Math.max(100, el.scrollHeight)}px`;
  }
</script>

<aside class="insp ftbq-view">
  <div class="insp-h">
    <QuestItemIcon itemId={typeof quest.icon === "string" ? quest.icon : null} fallback={quest.title?.charAt(0) ?? "?"} size={26} />
    <h3 title={quest.id}>{quest.title || "Untitled quest"}</h3>
    <button
      type="button"
      class="qid-mini"
      title={idCopied ? "Copied" : "Copy quest id"}
      onclick={() => void copyQuestId()}
    >
      {#if idCopied}
        <Check size={12} />
      {:else}
        <Copy size={12} />
      {/if}
      <code>{quest.id.slice(0, 8)}</code>
    </button>
    <button type="button" class="ico danger" title="Delete quest" aria-label="Delete quest" onclick={onRemove}>
      <Trash2 size={14} />
    </button>
  </div>

  {#if myIssues.length > 0}
    <div class="val-warn">
      {#each myIssues as issue (issue.message)}
        <div><AlertTriangle size={12} /> {issue.message}</div>
      {/each}
    </div>
  {/if}

  <!-- 1. What to do -->
  <section class="block">
    <h4 class="block-h">What to do</h4>
    <div class="fields">
      <div class="title-icon">
        <label class="grow"
          >Title<input
            bind:this={titleInputEl}
            bind:value={quest.title}
            oninput={() => {
              quest.titleFromSnbt = true;
              onDirty();
            }}
          /></label
        >
        <div class="icon-slot">
          <ItemStackEditor
            label="Icon"
            value={quest.icon ?? null}
            allowFilters={false}
            onChange={(v) => {
              quest.icon = v;
              onDirty();
            }}
          />
        </div>
      </div>
      <label
        >Subtitle<input
          bind:value={quest.subtitle}
          oninput={() => {
            quest.subtitleFromSnbt = true;
            onDirty();
          }}
          placeholder="Optional"
        /></label
      >
      <label
        >Description
        <details class="fmt-details">
          <summary>Formatting</summary>
          <div class="fmt-bar">
            <button type="button" onclick={() => wrapFmt("&l")}>Bold</button>
            <button type="button" onclick={() => wrapFmt("&a")}>Green</button>
            <button type="button" onclick={() => wrapFmt("&7")}>Gray</button>
            <button type="button" onclick={() => wrapFmt("&e")}>Gold</button>
            <button type="button" onclick={() => insertTemplate("objective")}>Objective</button>
            <button type="button" onclick={() => insertTemplate("story")}>Story</button>
          </div>
        </details>
        <textarea
          rows="3"
          value={descText}
          oninput={autoGrowDescription}
          onchange={commitDescription}
          onblur={commitDescription}
          placeholder="What the player should do…"
        ></textarea>
      </label>
      <label class="checkbox">
        <input type="checkbox" bind:checked={quest.optional} onchange={onDirty} />
        Optional quest
      </label>
    </div>
  </section>

  <!-- 2–3. How to prove / What you get -->
  <TaskRewardEditor {quest} {onDirty} {rewardTableIds} {onOpenKubeJs} />

  <!-- 4. What unlocks it -->
  <section class="block">
    <button
      type="button"
      class="block-h tog"
      aria-expanded={depsOpen}
      aria-controls="quest-deps"
      onclick={() => (depsOpen = !depsOpen)}
    >
      {#if depsOpen}<ChevronDown size={12} class="flex-shrink-0" />{:else}<ChevronRight size={12} class="flex-shrink-0" />{/if}
      <Link2 size={12} class="flex-shrink-0" /> What unlocks it
      {#if quest.dependencies.length > 0}<span class="sec-count">{quest.dependencies.length}</span>{/if}
    </button>
    {#if depsOpen}
      <div class="deps" id="quest-deps">
      {#each quest.dependencies as dep (dep)}
        <span class="dep-tag" title={dep}>
          {titleOf(dep)}
          <button type="button" class="dep-rm" onclick={() => onRemoveDep(dep)}>×</button>
        </span>
      {/each}
      {#if quest.dependencies.length === 0}
        <span class="deps-empty">No dependencies — available immediately</span>
      {/if}
    </div>
    <div class="dep-add">
      <input
        type="search"
        class="dep-filter"
        placeholder="Filter quests…"
        bind:value={depFilter}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            applyDepFromFilter();
          }
        }}
      />
      <select bind:value={depPick}>
        <option value="">Add dependency…</option>
        {#each filteredDepOptions as o (o.id)}
          <option value={o.id}>{o.label}</option>
        {/each}
      </select>
      <button
        type="button"
        class="add-btn"
        disabled={!depPick && filteredDepOptions.length === 0}
        onclick={applyDepFromFilter}>Add</button
      >
    </div>
    {/if}
  </section>

  <!-- More: appearance, flags, locale -->
  <button
    type="button"
    class="adv-tog"
    aria-expanded={showAdvanced}
    aria-controls="quest-more"
    onclick={() => (showAdvanced = !showAdvanced)}
  >
    {#if showAdvanced}<ChevronDown size={12} class="flex-shrink-0" />{:else}<ChevronRight size={12} class="flex-shrink-0" />{/if}
    More
  </button>
  {#if showAdvanced}
    <div class="fields flags" id="quest-more">
      {#if availableLocales.length > 1}
        <label class="compare-pick"
          >Compare locale
          <select
            value={compareLocale ?? ""}
            onchange={(e) => {
              const v = (e.currentTarget as HTMLSelectElement).value;
              onCompareLocaleChange?.(v || null);
            }}
          >
            <option value="">(off)</option>
            {#each availableLocales as c (c)}
              {#if c !== activeLocale}
                <option value={c}>{c}</option>
              {/if}
            {/each}
          </select>
        </label>
      {/if}

      {#if showCompare}
        <div class="locale-cols">
          <div class="locale-col">
            <span class="col-h">{activeLocale ?? "active"}</span>
            <label
              >Title<input
                bind:value={quest.title}
                oninput={() => {
                  quest.titleFromSnbt = true;
                  onDirty();
                }}
              /></label
            >
            <label
              >Subtitle<input
                bind:value={quest.subtitle}
                oninput={() => {
                  quest.subtitleFromSnbt = true;
                  onDirty();
                }}
              /></label
            >
            <label
              >Description
              <textarea
                rows="3"
                value={descText}
                oninput={autoGrowDescription}
                onchange={commitDescription}
                onblur={commitDescription}
              ></textarea>
            </label>
          </div>
          <div class="locale-col">
            <span class="col-h">{compareLocale}</span>
            <label
              >Title<input
                bind:value={cmpTitle}
                oninput={() =>
                  patchCompare((map) => {
                    map[`quest.${quest.id}.title`] = cmpTitle;
                  })}
              /></label
            >
            <label
              >Subtitle<input
                bind:value={cmpSubtitle}
                oninput={() =>
                  patchCompare((map) => {
                    map[`quest.${quest.id}.quest_subtitle`] = cmpSubtitle;
                  })}
              /></label
            >
            <label
              >Description
              <textarea
                rows="3"
                bind:value={cmpDesc}
                onchange={commitCompareDesc}
                onblur={commitCompareDesc}
              ></textarea>
            </label>
          </div>
        </div>
      {/if}

      <label
        >Shape
        <select
          value={quest.shape ?? ""}
          onchange={(e) => {
            quest.shape = selectVal(e) || null;
            onDirty();
          }}
        >
          {#each SHAPE_OPTIONS as s (s.id || "_default")}
            <option value={s.id}>{s.label}</option>
          {/each}
        </select>
      </label>
      <label
        >Size<input
          type="number"
          step="0.25"
          min="0.25"
          bind:value={quest.size}
          oninput={onDirty}
          placeholder="1"
        /></label
      >
      <label
        >Position
        <div class="xy">
          <input type="number" step="0.5" bind:value={quest.x} oninput={onDirty} />
          <input type="number" step="0.5" bind:value={quest.y} oninput={onDirty} />
        </div>
      </label>

      <h4 class="sub-h">FTB flags</h4>
      <label
        >Hide dependency lines
        <select value={triVal(quest.hideDependencyLines)} onchange={(e) => tri("hideDependencyLines", e)}>
          <option value="">unset</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label
        >Hide dependent lines
        <select value={triVal(quest.hideDependentLines)} onchange={(e) => tri("hideDependentLines", e)}>
          <option value="">unset</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label
        >Can repeat
        <select value={triVal(quest.canRepeat)} onchange={(e) => tri("canRepeat", e)}>
          <option value="">unset</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label
        >Invisible
        <select value={triVal(quest.invisible)} onchange={(e) => tri("invisible", e)}>
          <option value="">unset</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label
        >Disable toast
        <select value={triVal(quest.disableToast)} onchange={(e) => tri("disableToast", e)}>
          <option value="">unset</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label
        >Min required deps<input
          type="number"
          min="0"
          value={quest.minRequiredDependencies ?? ""}
          oninput={(e) => {
            const v = inputVal(e);
            quest.minRequiredDependencies = v === "" ? null : Number(v);
            onDirty();
          }}
        /></label
      >
      <label
        >Dependency requirement
        <select
          value={quest.dependencyRequirement ?? ""}
          onchange={(e) => {
            quest.dependencyRequirement = selectVal(e) || null;
            onDirty();
          }}
        >
          {#each DEP_REQUIREMENT_OPTIONS as d (d.id || "_default")}
            <option value={d.id}>{d.label}</option>
          {/each}
        </select>
      </label>

      <h4 class="sub-h">Extra SNBT</h4>
      {#each Object.entries(quest.extras ?? {}) as [k, v] (k)}
        <div class="extra-row">
          <code>{k}</code>
          <span>{typeof v === "string" ? v : JSON.stringify(v)}</span>
          <button type="button" onclick={() => removeExtra(k)}>×</button>
        </div>
      {/each}
      <div class="extra-add">
        <input placeholder="key" bind:value={extraKey} />
        <input placeholder="value / JSON" bind:value={extraVal} />
        <button type="button" onclick={addExtra}>Add</button>
      </div>
    </div>
  {/if}
</aside>

<style>
  /* Modern typography system for quest inspector */
  .insp {
    background: var(--ftbq-bg-panel);
    border-left: 1px solid var(--ftbq-frame);
    min-width: 0;
    min-height: 0;
    max-height: 100%;
    overflow: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
    font-size: 13px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  .insp-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--ftbq-frame);
    background: var(--ftbq-bg-panel);
  }
  .insp-h h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--text-primary, var(--ftbq-text));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: -0.01em;
    text-shadow: 0 1px 0 rgba(255, 255, 255, 0.05);
  }

  .qid-mini {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    flex-shrink: 0;
    margin-left: auto;
    font-size: 11px;
    font-weight: 500;
    font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
    color: var(--ftbq-text-muted);
    padding: 4px 8px;
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-control);
    background: rgba(0, 0, 0, 0.15);
    cursor: pointer;
    transition: all 0.15s ease;
    letter-spacing: 0.02em;
  }
  .qid-mini:hover {
    color: var(--ftbq-text);
    background: rgba(0, 0, 0, 0.25);
  }
  .qid-mini code {
    font-size: inherit;
    color: inherit;
    font-family: inherit;
  }

  .block {
    display: grid;
    gap: 0;
  }
  .block-h {
    margin: 0;
    padding: 12px 14px 10px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ftbq-accent-teal);
    display: flex;
    align-items: center;
    gap: 8px;
    background: color-mix(in srgb, var(--ftbq-bg) 40%, transparent);
    border-top: 1px solid var(--ftbq-frame);
    border-bottom: 1px solid var(--ftbq-frame);
  }
  .block-h :global(svg) {
    flex-shrink: 0;
    opacity: 0.85;
  }

  .title-icon {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
    align-items: start;
  }
  .title-icon .grow {
    min-width: 0;
  }
  .icon-slot {
    min-width: 120px;
  }

  .fields {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 10px 14px;
  }
  .compare-pick {
    text-transform: none !important;
  }

  .fields label {
    display: grid;
    gap: 5px;
    font-size: 12px;
    font-weight: 500;
    color: var(--ftbq-text-muted);
    letter-spacing: 0;
    line-height: 1.4;
  }

  .fields input:not([type]),
  .fields input[type="number"],
  .fields textarea,
  .fields select {
    font-family: inherit;
    font-size: 13px;
    font-weight: 400;
    background: var(--ftbq-input-bg) !important;
    border: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text);
    border-radius: var(--border-radius-sm);
    padding: 9px 12px;
    outline: none;
    transition: all 0.15s ease;
    line-height: 1.4;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.06);
  }
  .fields input::placeholder,
  .fields textarea::placeholder {
    color: var(--ftbq-text-muted);
    opacity: 0.7;
  }
  .fields input:hover,
  .fields textarea:hover,
  .fields select:hover {
    border-color: color-mix(in srgb, var(--ftbq-text-muted) 40%, var(--ftbq-frame));
  }
  .fields input:focus,
  .fields textarea:focus,
  .fields select:focus {
    border-color: var(--ftbq-accent-teal);
    box-shadow:
      0 0 0 3px color-mix(in srgb, var(--ftbq-accent-teal) 20%, transparent),
      inset 0 1px 2px rgba(0, 0, 0, 0.04);
  }

  .fields textarea {
    min-height: 100px;
    field-sizing: content;
    resize: vertical;
    line-height: 1.55;
    overflow-y: auto;
  }

  .checkbox {
    display: flex !important;
    flex-direction: row !important;
    align-items: center;
    gap: 10px;
    color: var(--ftbq-text);
    font-weight: 500;
    padding: 4px 0;
  }
  .checkbox input[type="checkbox"] {
    width: 17px;
    height: 17px;
    accent-color: var(--ftbq-accent-teal);
    cursor: pointer;
    border-radius: 4px;
  }

  .fmt-details {
    margin: 0 0 8px;
  }
  .fmt-details summary {
    cursor: pointer;
    list-style: none;
    font-size: 11px;
    font-weight: 600;
    color: var(--ftbq-accent-teal);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 0;
  }
  .fmt-details summary::-webkit-details-marker {
    display: none;
  }
  .fmt-details summary::before {
    content: '▸ ';
    font-weight: 400;
  }
  .fmt-details[open] summary::before {
    content: '▾ ';
  }

  .fmt-bar {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 6px;
    padding: 4px;
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--border-radius-sm);
    background: var(--ftbq-input-bg);
  }
  .fmt-bar button {
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    padding: 6px 12px;
    border: none;
    border-radius: var(--ftbq-radius-control);
    background: transparent;
    color: var(--ftbq-text);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .fmt-bar button:hover {
    background: color-mix(in srgb, var(--ftbq-accent-teal) 15%, transparent);
    color: var(--ftbq-accent-teal);
  }
  .fmt-bar button:active {
    transform: scale(0.97);
  }
  .fmt-bar button:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: 1px;
  }

  .xy {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .deps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 10px 14px;
  }
  .deps-empty {
    font-size: 12px;
    color: var(--ftbq-text-muted);
    font-style: italic;
    padding: 4px 0;
  }
  .dep-tag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 20px;
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    font-size: 12px;
    font-weight: 500;
    color: var(--ftbq-text);
    transition: all 0.15s ease;
  }
  .dep-tag:hover {
    border-color: var(--ftbq-accent-teal);
    background: color-mix(in srgb, var(--ftbq-accent-teal) 8%, transparent);
    color: var(--ftbq-accent-teal);
  }
  .dep-rm {
    border: none;
    background: transparent;
    color: var(--ftbq-text-muted);
    cursor: pointer;
    padding: 2px 4px;
    border-radius: var(--ftbq-radius-control);
    font-size: 14px;
    font-weight: 400;
    transition: all 0.15s ease;
    line-height: 1;
  }
  .dep-rm:hover {
    color: #ef4444;
    background: rgba(239, 68, 68, 0.1);
  }

  .dep-add {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 14px 14px;
  }
  .dep-filter {
    width: 100%;
    font-family: inherit;
    font-size: 12px;
    padding: 9px 12px;
    background: var(--ftbq-input-bg) !important;
    border: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text);
    border-radius: var(--ftbq-radius-control);
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.06);
    outline: none;
    transition: all 0.15s ease;
  }
  .dep-filter:focus {
    border-color: var(--ftbq-accent-teal);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ftbq-accent-teal) 20%, transparent);
  }
  .dep-add select {
    width: 100%;
    font-family: inherit;
    font-size: 12px;
    background: var(--ftbq-input-bg) !important;
    border: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text);
    border-radius: var(--ftbq-radius-control);
    padding: 9px 12px;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.06);
    outline: none;
    transition: all 0.15s ease;
  }
  .dep-add select:focus {
    border-color: var(--ftbq-accent-teal);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ftbq-accent-teal) 20%, transparent);
  }

  .add-btn {
    align-self: flex-end;
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
    padding: 9px 18px;
    border-radius: var(--ftbq-radius-control);
    border: none;
    background: var(--ftbq-accent-teal);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
    color: #fff;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .add-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--ftbq-accent-teal) 85%, #000);
    transform: translateY(-1px);
    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.2);
  }
  .add-btn:active:not(:disabled) {
    transform: translateY(0);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
  }
  .add-btn:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: 2px;
  }
  .add-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .adv-tog {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: none;
    border-top: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text-muted);
    font-family: inherit;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    text-align: left;
    padding: 12px 14px;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .adv-tog:hover {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text);
  }
  .adv-tog:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: -2px;
  }

  .flags {
    padding-top: 0;
    border-top: none;
  }
  .sub-h,
  .flags h4 {
    margin: 12px 0 0 !important;
    padding: 0 !important;
    font-size: 11px !important;
    font-weight: 700 !important;
    text-transform: uppercase !important;
    letter-spacing: 0.06em !important;
    color: var(--ftbq-text-muted) !important;
    border: none !important;
    background: transparent !important;
  }

  .extra-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    font-size: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--ftbq-border);
  }
  .extra-row code {
    font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
    font-size: 11px;
    color: var(--ftbq-accent-teal);
    background: rgba(61, 184, 168, 0.1);
    padding: 2px 6px;
    border-radius: 4px;
  }
  .extra-add {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 8px;
    margin-top: 8px;
  }

  h4 {
    margin: 0;
    padding: 12px 14px 10px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-text-muted);
    display: flex;
    align-items: center;
    gap: 8px;
    background: color-mix(in srgb, var(--ftbq-bg) 40%, transparent);
    border-top: 1px solid var(--ftbq-frame);
    border-bottom: 1px solid var(--ftbq-frame);
  }
  .block-h.tog {
    width: 100%;
    margin: 0;
    padding: 12px 14px 10px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-text-muted);
    display: flex;
    align-items: center;
    gap: 8px;
    background: color-mix(in srgb, var(--ftbq-bg) 40%, transparent);
    border-top: 1px solid var(--ftbq-frame);
    border-bottom: 1px solid var(--ftbq-frame);
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .block-h.tog:hover {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text);
  }
  .block-h.tog .sec-count {
    margin-left: auto;
    font-size: 9px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text-muted);
    font-variant-numeric: tabular-nums;
  }
  .insp :global(.flex-shrink-0) {
    flex-shrink: 0;
  }

  .locale-cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    padding: 10px 14px;
  }
  .locale-col {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .col-h {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ftbq-text-muted);
    padding-bottom: 4px;
    border-bottom: 1px solid var(--ftbq-border);
  }

  .val-warn {
    padding: 10px 14px;
    margin: 10px 14px 0;
    border-radius: var(--border-radius-sm);
    background: rgba(251, 191, 36, 0.1);
    border: 1px solid rgba(251, 191, 36, 0.3);
    font-size: 12px;
    color: #f59e0b;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .val-warn div {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    line-height: 1.4;
  }
  .val-warn :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .ico {
    width: 36px;
    height: 36px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--ftbq-radius-control);
    border: 1px solid var(--ftbq-frame);
    background: var(--ftbq-input-bg);
    color: var(--ftbq-text-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .ico.danger:hover,
  .ico:hover {
    border-color: #ef4444;
    background: rgba(239, 68, 68, 0.08);
    color: #ef4444;
  }
  .ico:active {
    transform: scale(0.95);
  }
  .ico:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: 1px;
  }

  .insp :global(input:not([type="checkbox"]):not([type="radio"])),
  .insp :global(textarea),
  .insp :global(select) {
    font-family: inherit;
    background: var(--ftbq-input-bg) !important;
    border-color: var(--ftbq-frame);
    color-scheme: inherit;
  }
</style>
