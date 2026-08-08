<script lang="ts">
  import { Trash2 } from "@lucide/svelte";
  import type { QuestData, QuestReward, QuestTask } from "../../lib/api";
  import {
    REWARD_TYPE_OPTIONS,
    TASK_TYPE_OPTIONS,
    TASK_TYPES,
    REWARD_TYPES,
    rewardTypeLabel,
    taskTypeLabel,
  } from "../../lib/questTypeLabels";
  import type { ItemValue } from "../../lib/itemStack";
  import ItemStackEditor from "./ItemStackEditor.svelte";

  let {
    quest,
    onDirty,
    rewardTableIds = [],
  }: {
    quest: QuestData;
    onDirty: () => void;
    rewardTableIds?: string[];
  } = $props();

  function newId(len = 12) {
    return crypto.randomUUID().replace(/-/g, "").slice(0, len);
  }

  function ensureProps(obj: { properties?: Record<string, unknown> }) {
    if (!obj.properties) obj.properties = {};
    return obj.properties;
  }

  function setProp(obj: { properties?: Record<string, unknown> }, key: string, value: unknown) {
    const p = ensureProps(obj);
    if (value === "" || value == null) delete p[key];
    else p[key] = value;
    obj.properties = { ...p };
    onDirty();
  }

  function setItemValue(
    obj: { properties?: Record<string, unknown> },
    next: ItemValue | null,
  ) {
    const p = ensureProps(obj);
    if (next == null || next === "") {
      delete p.item;
    } else {
      p.item = next;
      if (typeof next === "object" && (next.Count != null || next.count != null)) {
        delete p.count;
      }
    }
    obj.properties = { ...p };
    onDirty();
  }

  function itemValueOf(props: Record<string, unknown> | undefined): ItemValue | null {
    const v = props?.item;
    if (typeof v === "string") return v;
    if (v && typeof v === "object" && !Array.isArray(v)) return v as Record<string, unknown>;
    return null;
  }

  function addTask(type = "item") {
    const t: QuestTask = {
      id: newId(),
      type,
      properties: type === "item" ? { item: "minecraft:stone", count: 1 } : {},
    };
    if (type === "kill") t.properties = { entity: "minecraft:zombie", value: 1 };
    if (type === "dimension") t.properties = { dimension: "minecraft:overworld" };
    if (type === "biome") t.properties = { biome: "minecraft:plains" };
    if (type === "advancement") t.properties = { advancement: "minecraft:story/root" };
    if (type === "stat") t.properties = { stat: "minecraft:walk_one_cm", value: 100 };
    if (type === "fluid") t.properties = { fluid: "minecraft:water", amount: 1000 };
    if (type === "location") {
      t.properties = { dimension: "minecraft:overworld", x: 0, y: 64, z: 0 };
    }
    if (type === "structure") t.properties = { structure: "minecraft:village" };
    if (type === "stage") t.properties = { stage: "" };
    if (type === "observation") t.properties = { timer: 0 };
    if (type === "xp") t.value = 1;
    quest.tasks = [...quest.tasks, t];
    onDirty();
  }

  function removeTask(i: number) {
    quest.tasks = quest.tasks.filter((_, idx) => idx !== i);
    onDirty();
  }

  function addReward(type = "item") {
    const r: QuestReward = {
      id: newId(),
      type,
      properties:
        type === "item"
          ? { item: "minecraft:diamond", count: 1 }
          : type === "xp"
            ? { xp: 10 }
            : type === "xp_levels"
              ? { xp_levels: 1 }
              : type === "command"
                ? { command: "say hello" }
                : type === "random" || type === "choice"
                  ? { table: rewardTableIds[0] ?? "" }
                  : type === "loot"
                    ? { loot_crate: "" }
                    : type === "all_tables"
                      ? {}
                      : type === "stage"
                        ? { stage: "" }
                        : type === "toast"
                          ? { description: "" }
                          : {},
    };
    quest.rewards = [...quest.rewards, r];
    onDirty();
  }

  function removeReward(i: number) {
    quest.rewards = quest.rewards.filter((_, idx) => idx !== i);
    onDirty();
  }

  function numProp(props: Record<string, unknown> | undefined, key: string, fallback = 1): number {
    const v = props?.[key];
    if (typeof v === "number") return v;
    if (typeof v === "string" && v !== "" && !Number.isNaN(Number(v))) return Number(v);
    return fallback;
  }

  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }
  function inputNum(e: Event): number {
    return Number((e.currentTarget as HTMLInputElement).value);
  }
  function inputChecked(e: Event): boolean {
    return (e.currentTarget as HTMLInputElement).checked;
  }
  function selectVal(e: Event): string {
    return (e.currentTarget as HTMLSelectElement).value;
  }
  function textareaVal(e: Event): string {
    return (e.currentTarget as HTMLTextAreaElement).value;
  }

  function onPickTaskType(e: Event) {
    const el = e.currentTarget as HTMLSelectElement;
    if (el.value) {
      addTask(el.value);
      el.value = "";
    }
  }

  function onPickRewardType(e: Event) {
    const el = e.currentTarget as HTMLSelectElement;
    if (el.value) {
      addReward(el.value);
      el.value = "";
    }
  }
</script>

<section class="tr ftbq-tr">
  <div class="tr-h">
    <h4>Tasks</h4>
    <div class="add-row">
      <select onchange={onPickTaskType}>
        <option value="">+ Task type…</option>
        {#each TASK_TYPE_OPTIONS as t (t.id)}
          <option value={t.id}>{t.label}</option>
        {/each}
      </select>
    </div>
  </div>

  {#each quest.tasks as task, i (task.id)}
    <div class="card">
      <div class="card-h">
        <select
          value={task.type}
          onchange={(e) => {
            task.type = selectVal(e);
            onDirty();
          }}
        >
          {#each TASK_TYPE_OPTIONS as t (t.id)}
            <option value={t.id}>{t.label}</option>
          {/each}
          {#if !TASK_TYPES.includes(task.type)}
            <option value={task.type}>{taskTypeLabel(task.type)}</option>
          {/if}
        </select>
        <button type="button" class="ico danger" onclick={() => removeTask(i)}
          ><Trash2 size={12} /></button
        >
      </div>

      {#if task.type === "item"}
        <ItemStackEditor
          value={itemValueOf(task.properties)}
          allowFilters={true}
          onChange={(v) => setItemValue(task, v)}
        />
      {:else if task.type === "kill"}
        <label
          >Entity<input
            value={String(task.properties?.entity ?? "")}
            oninput={(e) => setProp(task, "entity", inputVal(e))}
            placeholder="minecraft:zombie"
          /></label
        >
        <label
          >Count<input
            type="number"
            min="1"
            value={numProp(task.properties, "value", 1)}
            oninput={(e) => setProp(task, "value", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "dimension"}
        <label
          >Dimension<input
            value={String(task.properties?.dimension ?? "")}
            oninput={(e) => setProp(task, "dimension", inputVal(e))}
            placeholder="minecraft:the_nether"
          /></label
        >
      {:else if task.type === "biome"}
        <label
          >Biome<input
            value={String(task.properties?.biome ?? "")}
            oninput={(e) => setProp(task, "biome", inputVal(e))}
            placeholder="minecraft:plains"
          /></label
        >
      {:else if task.type === "xp"}
        <label
          >XP<input
            type="number"
            min="1"
            value={typeof task.value === "number" ? task.value : Number(task.value) || 1}
            oninput={(e) => {
              task.value = inputNum(e) || 1;
              onDirty();
            }}
          /></label
        >
      {:else if task.type === "checkmark"}
        <p class="hint">Manual checkmark — no extra fields.</p>
      {:else if task.type === "stage"}
        <label
          >Stage<input
            value={String(task.properties?.stage ?? "")}
            oninput={(e) => setProp(task, "stage", inputVal(e))}
          /></label
        >
      {:else if task.type === "advancement"}
        <label
          >Advancement<input
            value={String(task.properties?.advancement ?? "")}
            oninput={(e) => setProp(task, "advancement", inputVal(e))}
            placeholder="minecraft:story/mine_stone"
          /></label
        >
      {:else if task.type === "stat"}
        <label
          >Stat<input
            value={String(task.properties?.stat ?? "")}
            oninput={(e) => setProp(task, "stat", inputVal(e))}
            placeholder="minecraft:walk_one_cm"
          /></label
        >
        <label
          >Value<input
            type="number"
            value={numProp(task.properties, "value", 1)}
            oninput={(e) => setProp(task, "value", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "fluid"}
        <label
          >Fluid<input
            value={String(task.properties?.fluid ?? task.properties?.fluid_name ?? "")}
            oninput={(e) => setProp(task, "fluid", inputVal(e))}
            placeholder="minecraft:water"
          /></label
        >
        <label
          >Amount (mB)<input
            type="number"
            value={numProp(task.properties, "amount", 1000)}
            oninput={(e) => setProp(task, "amount", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "location"}
        <label
          >Dimension<input
            value={String(task.properties?.dimension ?? "")}
            oninput={(e) => setProp(task, "dimension", inputVal(e))}
            placeholder="minecraft:overworld"
          /></label
        >
        <label
          >Position
          <div class="item-row">
            <input
              type="number"
              title="x"
              value={numProp(task.properties, "x", 0)}
              oninput={(e) => setProp(task, "x", inputNum(e))}
            />
            <input
              type="number"
              title="y"
              value={numProp(task.properties, "y", 0)}
              oninput={(e) => setProp(task, "y", inputNum(e))}
            />
            <input
              type="number"
              title="z"
              value={numProp(task.properties, "z", 0)}
              oninput={(e) => setProp(task, "z", inputNum(e))}
            />
          </div>
        </label>
      {:else if task.type === "observation"}
        <label
          >Observe timer (ticks)<input
            type="number"
            value={numProp(task.properties, "timer", 0)}
            oninput={(e) => setProp(task, "timer", inputNum(e) || 0)}
          /></label
        >
        <label
          >Title<input
            bind:value={task.title}
            oninput={onDirty}
            placeholder="Look at…"
          /></label
        >
      {:else if task.type === "structure"}
        <label
          >Structure<input
            value={String(task.properties?.structure ?? "")}
            oninput={(e) => setProp(task, "structure", inputVal(e))}
            placeholder="minecraft:village"
          /></label
        >
      {:else if task.type === "custom"}
        <label
          >Title<input bind:value={task.title} oninput={onDirty} placeholder="Custom task" /></label
        >
      {:else}
        <label
          >Title<input
            bind:value={task.title}
            oninput={onDirty}
            placeholder="Optional title"
          /></label
        >
      {/if}

      {#if task.type === "item"}
        <label class="checkbox">
          <input
            type="checkbox"
            checked={!!task.properties?.consume_items}
            onchange={(e) => setProp(task, "consume_items", inputChecked(e))}
          />
          Consume items
        </label>
      {/if}

      <details class="raw">
        <summary>Raw properties</summary>
        <textarea
          rows="3"
          value={JSON.stringify(task.properties ?? {}, null, 0)}
          onchange={(e) => {
            try {
              task.properties = JSON.parse(textareaVal(e));
              onDirty();
            } catch {
              /* ignore invalid */
            }
          }}
        ></textarea>
      </details>
    </div>
  {/each}

  <div class="tr-h">
    <h4>Rewards</h4>
    <div class="add-row">
      <select onchange={onPickRewardType}>
        <option value="">+ Reward type…</option>
        {#each REWARD_TYPE_OPTIONS as t (t.id)}
          <option value={t.id}>{t.label}</option>
        {/each}
      </select>
    </div>
  </div>

  {#each quest.rewards as reward, i (reward.id)}
    <div class="card">
      <div class="card-h">
        <select
          value={reward.type}
          onchange={(e) => {
            reward.type = selectVal(e);
            onDirty();
          }}
        >
          {#each REWARD_TYPE_OPTIONS as t (t.id)}
            <option value={t.id}>{t.label}</option>
          {/each}
          {#if !REWARD_TYPES.includes(reward.type)}
            <option value={reward.type}>{rewardTypeLabel(reward.type)}</option>
          {/if}
        </select>
        <button type="button" class="ico danger" onclick={() => removeReward(i)}
          ><Trash2 size={12} /></button
        >
      </div>

      {#if reward.type === "item"}
        <ItemStackEditor
          value={itemValueOf(reward.properties)}
          allowFilters={true}
          onChange={(v) => setItemValue(reward, v)}
        />
      {:else if reward.type === "xp"}
        <label
          >XP<input
            type="number"
            min="1"
            value={numProp(reward.properties, "xp", 10)}
            oninput={(e) => setProp(reward, "xp", inputNum(e) || 1)}
          /></label
        >
      {:else if reward.type === "xp_levels"}
        <label
          >Levels<input
            type="number"
            min="1"
            value={numProp(reward.properties, "xp_levels", 1)}
            oninput={(e) => setProp(reward, "xp_levels", inputNum(e) || 1)}
          /></label
        >
      {:else if reward.type === "command"}
        <label
          >Command<input
            value={String(reward.properties?.command ?? "")}
            oninput={(e) => setProp(reward, "command", inputVal(e))}
            placeholder="/say hi"
          /></label
        >
      {:else if reward.type === "random" || reward.type === "choice"}
        <label
          >Reward table
          <select
            value={String(reward.properties?.table ?? "")}
            onchange={(e) => setProp(reward, "table", selectVal(e))}
          >
            <option value="">Select table…</option>
            {#each rewardTableIds as tid (tid)}
              <option value={tid}>{tid}</option>
            {/each}
          </select>
        </label>
      {:else if reward.type === "loot"}
        <label
          >Loot crate id<input
            value={String(
              reward.properties?.loot_crate ?? reward.properties?.table ?? "",
            )}
            oninput={(e) => setProp(reward, "loot_crate", inputVal(e))}
            placeholder="crate_id"
          /></label
        >
      {:else if reward.type === "all_tables"}
        <p class="hint">Grants a roll from every reward table in the book.</p>
      {:else if reward.type === "stage"}
        <label
          >Stage<input
            value={String(reward.properties?.stage ?? "")}
            oninput={(e) => setProp(reward, "stage", inputVal(e))}
          /></label
        >
      {:else if reward.type === "toast"}
        <label
          >Description<input
            value={String(reward.properties?.description ?? reward.title ?? "")}
            oninput={(e) => setProp(reward, "description", inputVal(e))}
          /></label
        >
      {:else}
        <label
          >Title<input bind:value={reward.title} oninput={onDirty} placeholder="Optional" /></label
        >
      {/if}
      <details class="raw">
        <summary>Raw properties</summary>
        <textarea
          rows="3"
          value={JSON.stringify(reward.properties ?? {}, null, 0)}
          onchange={(e) => {
            try {
              reward.properties = JSON.parse(textareaVal(e));
              onDirty();
            } catch {
              /* ignore */
            }
          }}
        ></textarea>
      </details>
    </div>
  {/each}

  <div class="icon-row">
    <ItemStackEditor
      label="Quest icon"
      value={quest.icon ?? null}
      allowFilters={false}
      onChange={(v) => {
        quest.icon = v;
        onDirty();
      }}
    />
  </div>
</section>

<style>
  .tr {
    display: grid;
    gap: 0;
    margin-top: 0;
  }
  .tr-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px 6px;
    background: rgba(0, 0, 0, 0.15);
    border-top: 1px solid var(--ftbq-border);
    border-bottom: 1px solid var(--ftbq-border);
  }
  .tr-h h4 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-accent-teal, #3db8a8);
    font-weight: 700;
  }
  .add-row select {
    font-size: 10px;
    max-width: 130px;
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .card {
    display: grid;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border);
    background: rgba(0, 0, 0, 0.1);
  }
  .card-h {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .card-h select {
    flex: 1;
    font-size: 12px;
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .card label {
    display: grid;
    gap: 3px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .card input,
  .card select {
    font-size: 12px;
    text-transform: none;
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .item-row {
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .item-row input {
    flex: 1;
    min-width: 0;
  }
  .ico {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
    flex-shrink: 0;
  }
  .ico.danger:hover {
    color: #f87171;
    background: rgba(239, 68, 68, 0.1);
  }
  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    text-transform: none;
  }
  .icon-row {
    margin: 0;
    padding: 8px 12px 12px;
    border-top: 1px solid var(--ftbq-border);
  }
  .raw {
    margin-top: 4px;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    text-transform: none;
  }
  .raw textarea {
    width: 100%;
    font-family: ui-monospace, monospace;
    font-size: 10px;
    text-transform: none;
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .checkbox {
    display: flex !important;
    flex-direction: row !important;
    align-items: center;
    gap: 6px;
    text-transform: none;
    color: var(--ftbq-text, #e8e8e8);
  }
</style>
