<script lang="ts">
  import { Trash2, Package } from "lucide-svelte";
  import type { QuestData, QuestReward, QuestTask } from "../../lib/api";
  import ItemPicker from "./ItemPicker.svelte";

  export let quest: QuestData;
  export let onDirty: () => void;
  export let rewardTableIds: string[] = [];

  const TASK_TYPES = [
    "item",
    "checkmark",
    "kill",
    "dimension",
    "biome",
    "xp",
    "advancement",
    "stat",
    "stage",
    "fluid",
    "location",
    "observation",
    "structure",
    "custom",
  ];

  const REWARD_TYPES = [
    "item",
    "xp",
    "xp_levels",
    "command",
    "random",
    "choice",
    "stage",
    "toast",
    "custom",
  ];

  let pickerOpen = false;
  let pickerTarget: { kind: "task" | "reward" | "icon"; index: number } | null = null;

  function newId(len = 12) {
    return crypto.randomUUID().replace(/-/g, "").slice(0, len);
  }

  function ensureProps(obj: { properties?: Record<string, unknown> }) {
    if (!obj.properties) obj.properties = {};
    return obj.properties;
  }

  function getItemId(props: Record<string, unknown> | undefined): string {
    const v = props?.item;
    if (typeof v === "string") return v;
    if (!v || typeof v !== "object") return "";
    const obj = v as Record<string, unknown>;
    if (typeof obj.id === "string" && !String(obj.id).startsWith("itemfilters:")) {
      return String(obj.id);
    }
    // itemfilters:or / and — show first nested concrete item
    const tag = obj.tag as { items?: unknown[] } | undefined;
    const items = tag?.items ?? (obj.items as unknown[] | undefined);
    if (Array.isArray(items)) {
      for (const it of items) {
        if (typeof it === "string" && it.includes(":")) return it;
        if (it && typeof it === "object" && "id" in (it as object)) {
          const id = String((it as { id: unknown }).id ?? "");
          if (id && !id.startsWith("itemfilters:")) return id;
        }
      }
    }
    if (typeof obj.id === "string") return obj.id;
    return "";
  }

  function setProp(obj: { properties?: Record<string, unknown> }, key: string, value: unknown) {
    const p = ensureProps(obj);
    if (value === "" || value == null) delete p[key];
    else p[key] = value;
    obj.properties = { ...p };
    onDirty();
  }

  function addTask(type = "item") {
    const t: QuestTask = {
      id: newId(),
      type,
      properties: type === "item" ? { item: "minecraft:stone", count: 1 } : {},
    };
    if (type === "kill") t.properties = { entity: "minecraft:zombie", value: 1 };
    if (type === "dimension") t.properties = { dimension: "minecraft:overworld" };
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
                : type === "random"
                  ? { table: rewardTableIds[0] ?? "" }
                  : {},
    };
    quest.rewards = [...quest.rewards, r];
    onDirty();
  }

  function removeReward(i: number) {
    quest.rewards = quest.rewards.filter((_, idx) => idx !== i);
    onDirty();
  }

  function openPicker(kind: "task" | "reward" | "icon", index: number) {
    pickerTarget = { kind, index };
    pickerOpen = true;
  }

  function onPickItem(itemId: string) {
    if (!pickerTarget) return;
    if (pickerTarget.kind === "icon") {
      quest.icon = itemId;
      onDirty();
    } else if (pickerTarget.kind === "task") {
      const t = quest.tasks[pickerTarget.index];
      if (t) setProp(t, "item", itemId);
    } else {
      const r = quest.rewards[pickerTarget.index];
      if (r) setProp(r, "item", itemId);
    }
    pickerTarget = null;
  }

  function numProp(props: Record<string, unknown> | undefined, key: string, fallback = 1): number {
    const v = props?.[key];
    if (typeof v === "number") return v;
    if (typeof v === "string" && v !== "" && !Number.isNaN(Number(v))) return Number(v);
    return fallback;
  }

  // Svelte 4 markup can't parse TS `as` — keep casts in script helpers
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
      <select on:change={onPickTaskType}>
        <option value="">+ Task type…</option>
        {#each TASK_TYPES as t}
          <option value={t}>{t}</option>
        {/each}
      </select>
    </div>
  </div>

  {#each quest.tasks as task, i (task.id)}
    <div class="card">
      <div class="card-h">
        <select
          value={task.type}
          on:change={(e) => {
            task.type = selectVal(e);
            onDirty();
          }}
        >
          {#each TASK_TYPES as t}
            <option value={t}>{t}</option>
          {/each}
        </select>
        <button type="button" class="ico danger" on:click={() => removeTask(i)}><Trash2 size={12} /></button>
      </div>

      {#if task.type === "item"}
        <label
          >Item
          <div class="item-row">
            <input
              value={getItemId(task.properties)}
              on:input={(e) => setProp(task, "item", inputVal(e))}
              placeholder="modid:item"
            />
            <button type="button" class="pick" on:click={() => openPicker("task", i)}
              ><Package size={12} /></button
            >
          </div>
        </label>
        <label
          >Count<input
            type="number"
            min="1"
            value={numProp(task.properties, "count", 1)}
            on:input={(e) => setProp(task, "count", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "kill"}
        <label
          >Entity<input
            value={String(task.properties?.entity ?? "")}
            on:input={(e) => setProp(task, "entity", inputVal(e))}
            placeholder="minecraft:zombie"
          /></label
        >
        <label
          >Count<input
            type="number"
            min="1"
            value={numProp(task.properties, "value", 1)}
            on:input={(e) => setProp(task, "value", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "dimension"}
        <label
          >Dimension<input
            value={String(task.properties?.dimension ?? "")}
            on:input={(e) => setProp(task, "dimension", inputVal(e))}
            placeholder="minecraft:the_nether"
          /></label
        >
      {:else if task.type === "biome"}
        <label
          >Biome<input
            value={String(task.properties?.biome ?? "")}
            on:input={(e) => setProp(task, "biome", inputVal(e))}
            placeholder="minecraft:plains"
          /></label
        >
      {:else if task.type === "xp"}
        <label
          >XP<input
            type="number"
            min="1"
            value={typeof task.value === "number" ? task.value : Number(task.value) || 1}
            on:input={(e) => {
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
            on:input={(e) => setProp(task, "stage", inputVal(e))}
          /></label
        >
      {:else if task.type === "advancement"}
        <label
          >Advancement<input
            value={String(task.properties?.advancement ?? "")}
            on:input={(e) => setProp(task, "advancement", inputVal(e))}
            placeholder="minecraft:story/mine_stone"
          /></label
        >
      {:else if task.type === "stat"}
        <label
          >Stat<input
            value={String(task.properties?.stat ?? "")}
            on:input={(e) => setProp(task, "stat", inputVal(e))}
            placeholder="minecraft:walk_one_cm"
          /></label
        >
        <label
          >Value<input
            type="number"
            value={numProp(task.properties, "value", 1)}
            on:input={(e) => setProp(task, "value", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "fluid"}
        <label
          >Fluid<input
            value={String(task.properties?.fluid ?? task.properties?.fluid_name ?? "")}
            on:input={(e) => setProp(task, "fluid", inputVal(e))}
            placeholder="minecraft:water"
          /></label
        >
        <label
          >Amount (mB)<input
            type="number"
            value={numProp(task.properties, "amount", 1000)}
            on:input={(e) => setProp(task, "amount", inputNum(e) || 1)}
          /></label
        >
      {:else if task.type === "location"}
        <label
          >Dimension<input
            value={String(task.properties?.dimension ?? "")}
            on:input={(e) => setProp(task, "dimension", inputVal(e))}
            placeholder="minecraft:overworld"
          /></label
        >
        <label
          >Position / radius
          <div class="item-row">
            <input
              type="number"
              title="x"
              value={numProp(task.properties, "x", 0)}
              on:input={(e) => setProp(task, "x", inputNum(e))}
            />
            <input
              type="number"
              title="y"
              value={numProp(task.properties, "y", 0)}
              on:input={(e) => setProp(task, "y", inputNum(e))}
            />
            <input
              type="number"
              title="z"
              value={numProp(task.properties, "z", 0)}
              on:input={(e) => setProp(task, "z", inputNum(e))}
            />
          </div>
        </label>
      {:else if task.type === "observation"}
        <label
          >Observe timer (ticks)<input
            type="number"
            value={numProp(task.properties, "timer", 0)}
            on:input={(e) => setProp(task, "timer", inputNum(e) || 0)}
          /></label
        >
        <label
          >Title<input
            bind:value={task.title}
            on:input={onDirty}
            placeholder="Look at…"
          /></label
        >
      {:else if task.type === "structure"}
        <label
          >Structure<input
            value={String(task.properties?.structure ?? "")}
            on:input={(e) => setProp(task, "structure", inputVal(e))}
            placeholder="minecraft:village"
          /></label
        >
      {:else if task.type === "custom"}
        <label
          >Title<input bind:value={task.title} on:input={onDirty} placeholder="Custom task" /></label
        >
      {:else}
        <label
          >Title<input
            bind:value={task.title}
            on:input={onDirty}
            placeholder="Optional title"
          /></label
        >
      {/if}

      {#if task.type === "item"}
        <label class="checkbox">
          <input
            type="checkbox"
            checked={!!task.properties?.consume_items}
            on:change={(e) =>
              setProp(task, "consume_items", inputChecked(e))}
          />
          Consume items
        </label>
      {/if}

      <details class="raw">
        <summary>Raw properties</summary>
        <textarea
          rows="3"
          value={JSON.stringify(task.properties ?? {}, null, 0)}
          on:change={(e) => {
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
      <select on:change={onPickRewardType}>
        <option value="">+ Reward type…</option>
        {#each REWARD_TYPES as t}
          <option value={t}>{t}</option>
        {/each}
      </select>
    </div>
  </div>

  {#each quest.rewards as reward, i (reward.id)}
    <div class="card">
      <div class="card-h">
        <select
          value={reward.type}
          on:change={(e) => {
            reward.type = selectVal(e);
            onDirty();
          }}
        >
          {#each REWARD_TYPES as t}
            <option value={t}>{t}</option>
          {/each}
        </select>
        <button type="button" class="ico danger" on:click={() => removeReward(i)}
          ><Trash2 size={12} /></button
        >
      </div>

      {#if reward.type === "item"}
        <label
          >Item
          <div class="item-row">
            <input
              value={getItemId(reward.properties)}
              on:input={(e) => setProp(reward, "item", inputVal(e))}
              placeholder="modid:item"
            />
            <button type="button" class="pick" on:click={() => openPicker("reward", i)}
              ><Package size={12} /></button
            >
          </div>
        </label>
        <label
          >Count<input
            type="number"
            min="1"
            value={numProp(reward.properties, "count", 1)}
            on:input={(e) => setProp(reward, "count", inputNum(e) || 1)}
          /></label
        >
      {:else if reward.type === "xp"}
        <label
          >XP<input
            type="number"
            min="1"
            value={numProp(reward.properties, "xp", 10)}
            on:input={(e) => setProp(reward, "xp", inputNum(e) || 1)}
          /></label
        >
      {:else if reward.type === "xp_levels"}
        <label
          >Levels<input
            type="number"
            min="1"
            value={numProp(reward.properties, "xp_levels", 1)}
            on:input={(e) =>
              setProp(reward, "xp_levels", inputNum(e) || 1)}
          /></label
        >
      {:else if reward.type === "command"}
        <label
          >Command<input
            value={String(reward.properties?.command ?? "")}
            on:input={(e) => setProp(reward, "command", inputVal(e))}
            placeholder="/say hi"
          /></label
        >
      {:else if reward.type === "random"}
        <label
          >Reward table
          <select
            value={String(reward.properties?.table ?? "")}
            on:change={(e) => setProp(reward, "table", selectVal(e))}
          >
            <option value="">Select table…</option>
            {#each rewardTableIds as tid}
              <option value={tid}>{tid}</option>
            {/each}
          </select>
        </label>
      {:else if reward.type === "stage"}
        <label
          >Stage<input
            value={String(reward.properties?.stage ?? "")}
            on:input={(e) => setProp(reward, "stage", inputVal(e))}
          /></label
        >
      {:else if reward.type === "choice"}
        <label
          >Table / choices
          <select
            value={String(reward.properties?.table ?? "")}
            on:change={(e) => setProp(reward, "table", selectVal(e))}
          >
            <option value="">Select table…</option>
            {#each rewardTableIds as tid}
              <option value={tid}>{tid}</option>
            {/each}
          </select>
        </label>
      {:else if reward.type === "toast"}
        <label
          >Description<input
            value={String(reward.properties?.description ?? reward.title ?? "")}
            on:input={(e) => setProp(reward, "description", inputVal(e))}
          /></label
        >
      {:else}
        <label
          >Title<input bind:value={reward.title} on:input={onDirty} placeholder="Optional" /></label
        >
      {/if}
      <details class="raw">
        <summary>Raw properties</summary>
        <textarea
          rows="3"
          value={JSON.stringify(reward.properties ?? {}, null, 0)}
          on:change={(e) => {
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
    <label
      >Quest icon
      <div class="item-row">
        <input bind:value={quest.icon} on:input={onDirty} placeholder="modid:item" />
        <button type="button" class="pick" on:click={() => openPicker("icon", 0)}
          ><Package size={12} /></button
        >
      </div>
    </label>
  </div>
</section>

<ItemPicker
  open={pickerOpen}
  onPick={onPickItem}
  onClose={() => {
    pickerOpen = false;
    pickerTarget = null;
  }}
/>

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
    border-top: 1px solid var(--ftbq-border, #3a3a42);
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .tr-h h4 {
    margin: 0;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-accent-teal, #3db8a8);
    font-weight: 700;
  }
  .tr-h:first-child h4 {
    color: var(--ftbq-accent-teal, #3db8a8);
  }
  .add-row select {
    font-size: 10px;
    max-width: 130px;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .card {
    display: grid;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
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
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
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
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
  .item-row {
    display: flex;
    gap: 4px;
  }
  .item-row input {
    flex: 1;
    min-width: 0;
  }
  .pick,
  .ico {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
    flex-shrink: 0;
  }
  .pick:hover {
    color: var(--ftbq-text, #e8e8e8);
    border-color: var(--ftbq-accent-teal, #3db8a8);
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
    border-top: 1px solid var(--ftbq-border, #3a3a42);
  }
  .icon-row label {
    display: grid;
    gap: 3px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .icon-row input {
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
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
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
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
