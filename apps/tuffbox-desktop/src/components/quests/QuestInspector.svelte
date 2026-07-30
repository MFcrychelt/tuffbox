<script lang="ts">
  import { Trash2, Link2, AlertTriangle } from "@lucide/svelte";
  import type { QuestData, QuestValidationIssue } from "../../lib/api";
  import TaskRewardEditor from "./TaskRewardEditor.svelte";

  let {
    quest,
    chapterQuests,
    issues = [],
    rewardTableIds = [],
    onDirty,
    onRemove,
    onAddDep,
    onRemoveDep,
  }: {
    quest: QuestData;
    chapterQuests: QuestData[];
    issues?: QuestValidationIssue[];
    rewardTableIds?: string[];
    onDirty: () => void;
    onRemove: () => void;
    onAddDep: (depId: string) => void;
    onRemoveDep: (depId: string) => void;
  } = $props();

  const SHAPES = ["", "circle", "square", "rsquare", "diamond", "hexagon", "pentagon", "gear", "none"];
  const DEP_REQ = ["", "all_completed", "one_completed", "all_started", "one_started"];

  let depPick = $state("");
  let descText = $state("");
  let extraKey = $state("");
  let extraVal = $state("");
  let showAdvanced = $state(false);

  let depOptions = $derived(buildDepOptions(chapterQuests, quest));
  let myIssues = $derived(issues.filter((i) => i.questId === quest.id));

  $effect(() => {
    descText = (quest.description ?? []).join("\n");
  });

  function buildDepOptions(list: QuestData[], current: QuestData) {
    const opts: { id: string; label: string }[] = [];
    for (const q of list) {
      if (q.id === current.id) continue;
      if (!current.dependencies.includes(q.id)) {
        opts.push({ id: q.id, label: q.title });
      }
      for (const t of q.tasks ?? []) {
        if (!t.id || current.dependencies.includes(t.id)) continue;
        opts.push({
          id: t.id,
          label: `${q.title} · ${t.title || t.type || "task"}`,
        });
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
    return id;
  }

  function applyDep() {
    if (!depPick) return;
    onAddDep(depPick);
    depPick = "";
  }

  function commitDescription() {
    quest.description = descText
      .split("\n")
      .map((s) => s.trimEnd())
      .filter((s, i, arr) => s.length > 0 || i < arr.length - 1);
    while (quest.description.length && quest.description[quest.description.length - 1] === "") {
      quest.description.pop();
    }
    onDirty();
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
</script>

<aside class="insp ftbq-view">
  <div class="insp-h">
    <h3 title={quest.id}>{quest.title}</h3>
    <button type="button" class="ico danger" title="Delete quest" onclick={onRemove}>
      <Trash2 size={14} />
    </button>
  </div>
  <code class="qid">{quest.id}</code>

  {#if myIssues.length > 0}
    <div class="val-warn">
      {#each myIssues as issue}
        <div><AlertTriangle size={12} /> {issue.message}</div>
      {/each}
    </div>
  {/if}

  <div class="fields">
    <label>Title<input bind:value={quest.title} oninput={onDirty} /></label>
    <label
      >Subtitle<input bind:value={quest.subtitle} oninput={onDirty} placeholder="Optional" /></label
    >
    <label
      >Description
      <div class="fmt-bar">
        <button type="button" class="ghost" onclick={() => wrapFmt("&l")}>Bold</button>
        <button type="button" class="ghost" onclick={() => wrapFmt("&a")}>Green</button>
        <button type="button" class="ghost" onclick={() => wrapFmt("&7")}>Gray</button>
        <button type="button" class="ghost" onclick={() => wrapFmt("&e")}>Gold</button>
        <button type="button" class="ghost" onclick={() => insertTemplate("objective")}>Objective</button>
        <button type="button" class="ghost" onclick={() => insertTemplate("story")}>Story</button>
      </div>
      <textarea
        rows="4"
        value={descText}
        oninput={(e) => (descText = textareaVal(e))}
        onchange={commitDescription}
        placeholder="One line per paragraph · & codes · JSON text lines ok"
      ></textarea>
      <small class="fmt-hint">Lines starting with {"{"} or [ are treated as raw JSON text by FTB Quests.</small>
    </label>
    <label class="checkbox">
      <input type="checkbox" bind:checked={quest.optional} onchange={onDirty} />
      Optional quest
    </label>
    <label
      >Shape
      <select
        value={quest.shape ?? ""}
        onchange={(e) => {
          quest.shape = selectVal(e) || null;
          onDirty();
        }}
      >
        {#each SHAPES as s}
          <option value={s}>{s || "(chapter default)"}</option>
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
      >Position (quest space)
      <div class="xy">
        <input type="number" step="0.5" bind:value={quest.x} oninput={onDirty} />
        <input type="number" step="0.5" bind:value={quest.y} oninput={onDirty} />
      </div>
    </label>
  </div>

  <button type="button" class="adv-tog" onclick={() => (showAdvanced = !showAdvanced)}>
    {showAdvanced ? "▾" : "▸"} FTB flags
  </button>
  {#if showAdvanced}
    <div class="fields flags">
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
          {#each DEP_REQ as d}
            <option value={d}>{d || "(default)"}</option>
          {/each}
        </select>
      </label>

      <h4>Extra SNBT</h4>
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

  <TaskRewardEditor {quest} {onDirty} {rewardTableIds} />

  <h4><Link2 size={12} /> Dependencies</h4>
  <p class="hint">Quest or task id (FTB allows both).</p>
  <div class="deps">
    {#each quest.dependencies as dep}
      <span class="dep-tag" title={dep}>
        {titleOf(dep)}
        <button type="button" class="dep-rm" onclick={() => onRemoveDep(dep)}>×</button>
      </span>
    {/each}
  </div>
  <div class="dep-add">
    <select bind:value={depPick}>
      <option value="">Add dependency…</option>
      {#each depOptions as o}
        <option value={o.id}>{o.label}</option>
      {/each}
    </select>
    <button type="button" class="add-btn" disabled={!depPick} onclick={applyDep}>Add</button>
  </div>
</aside>

<style>
  .insp {
    background: var(--ftbq-bg-panel, #212126);
    border-left: 1px solid var(--ftbq-border, #3a3a42);
    padding: 0;
    max-height: 100%;
    overflow: auto;
    overflow-x: hidden;
    min-height: 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .insp-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.2);
  }
  .insp-h h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
    color: var(--ftbq-title-gold, #f2c94c);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }
  .qid {
    display: block;
    font-size: 9px;
    color: var(--ftbq-text-muted, #9a9aa0);
    margin: 0;
    padding: 4px 12px 8px;
    word-break: break-all;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .val-warn {
    padding: 8px 10px;
    margin: 8px 12px 0;
    border-radius: 2px;
    background: rgba(242, 201, 76, 0.1);
    border: 1px solid rgba(242, 201, 76, 0.3);
    font-size: 11px;
    color: #fde68a;
    display: grid;
    gap: 4px;
  }
  .fields {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 10px 12px;
  }
  .fields label {
    display: grid;
    gap: 4px;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .fields input,
  .fields textarea,
  .fields select {
    font-size: 12px;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
    text-transform: none;
  }
  .checkbox {
    display: flex !important;
    flex-direction: row !important;
    align-items: center;
    gap: 8px;
    text-transform: none;
    color: var(--ftbq-text, #e8e8e8);
  }
  .xy {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .adv-tog {
    background: transparent;
    border: none;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    text-align: left;
    padding: 6px 12px;
    cursor: pointer;
    border-top: 1px solid var(--ftbq-border, #3a3a42);
  }
  .adv-tog:hover {
    color: var(--ftbq-text, #e8e8e8);
  }
  .flags {
    padding-top: 0;
    border-top: none;
  }
  .flags h4 {
    margin: 8px 0 0;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--ftbq-accent-teal, #3db8a8);
    letter-spacing: 0.05em;
  }
  .extra-row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 4px;
    align-items: center;
    font-size: 11px;
  }
  .extra-add {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 4px;
  }
  .hint {
    margin: 0 0 6px;
    padding: 0 12px;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  h4 {
    margin: 0;
    padding: 8px 12px 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-accent-teal, #3db8a8);
    display: flex;
    align-items: center;
    gap: 6px;
    background: rgba(0, 0, 0, 0.15);
    border-top: 1px solid var(--ftbq-border, #3a3a42);
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    font-weight: 700;
  }
  .deps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 8px 12px;
  }
  .dep-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 2px;
    background: rgba(92, 138, 158, 0.15);
    border: 1px solid rgba(92, 138, 158, 0.35);
    font-size: 11px;
    color: var(--ftbq-text, #e8e8e8);
  }
  .dep-rm {
    border: none;
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .dep-rm:hover {
    color: #f87171;
  }
  .dep-add {
    display: flex;
    gap: 6px;
    padding: 0 12px 12px;
  }
  .dep-add select {
    flex: 1;
    font-size: 11px;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .add-btn {
    font-size: 11px;
    padding: 4px 10px;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text, #e8e8e8);
    cursor: pointer;
  }
  .add-btn:hover:not(:disabled) {
    border-color: var(--ftbq-accent-teal, #3db8a8);
  }
  .add-btn:disabled {
    opacity: 0.4;
  }
  .fmt-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 6px;
  }
  .fmt-bar button {
    font-size: 10px;
    padding: 2px 6px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: none;
  }
  .fmt-bar button:hover {
    border-color: var(--ftbq-accent-green, #55c95a);
  }
  .fmt-hint {
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 10px;
    font-weight: 500;
  }
  .ico {
    border: none;
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .ico.danger:hover {
    color: #f87171;
  }
</style>
