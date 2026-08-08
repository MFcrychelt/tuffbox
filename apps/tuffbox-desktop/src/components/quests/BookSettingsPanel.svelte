<script lang="ts">
  import { X } from "@lucide/svelte";

  const BOOK_BOOL_KEYS = [
    "pause_game",
    "show_lock_icons",
    "hide_offline",
    "drop_loot_crates",
    "disable_gui",
    "disable_toast",
    "disable_cheating",
    "default_consume_items",
  ] as const;
  const BOOK_STRING_KEYS = ["default_quest_shape", "theme", "progression_mode"] as const;
  const BOOK_NUMBER_KEYS = ["default_quest_size"] as const;
  const BOOK_CURATED = new Set<string>([...BOOK_BOOL_KEYS, ...BOOK_STRING_KEYS, ...BOOK_NUMBER_KEYS]);

  let {
    bookTitle = null,
    bookSubtitle = null,
    bookSettings,
    bookDirty = false,
    saving = false,
    onclose,
    onsave,
    onpushhistory,
    ontitlechange,
    onsubtitlechange,
    onsetsettings,
  }: {
    bookTitle?: string | null;
    bookSubtitle?: string | null;
    bookSettings: Record<string, unknown>;
    bookDirty?: boolean;
    saving?: boolean;
    onclose: () => void;
    onsave: () => void;
    onpushhistory: () => void;
    ontitlechange: (value: string) => void;
    onsubtitlechange: (value: string) => void;
    onsetsettings: (next: Record<string, unknown>) => void;
  } = $props();

  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }

  function bookBool(key: string): boolean {
    const v = bookSettings[key];
    return v === true || v === 1 || v === "1" || v === "true";
  }

  function setBookBool(key: string, value: boolean) {
    onpushhistory();
    onsetsettings({ ...bookSettings, [key]: value });
  }

  function bookString(key: string): string {
    const v = bookSettings[key];
    return typeof v === "string" ? v : v == null ? "" : String(v);
  }

  function setBookString(key: string, value: string) {
    onpushhistory();
    onsetsettings({ ...bookSettings, [key]: value });
  }

  function bookNumber(key: string): string {
    const v = bookSettings[key];
    return typeof v === "number" ? String(v) : v == null || v === "" ? "" : String(v);
  }

  function setBookNumber(key: string, raw: string) {
    onpushhistory();
    const next = { ...bookSettings };
    if (raw.trim() === "") delete next[key];
    else {
      const n = Number(raw);
      next[key] = Number.isFinite(n) ? n : raw;
    }
    onsetsettings(next);
  }

  function removeBookSetting(key: string) {
    onpushhistory();
    const next = { ...bookSettings };
    delete next[key];
    onsetsettings(next);
  }

  const bookExtraEntries = $derived(
    Object.entries(bookSettings).filter(([k]) => !BOOK_CURATED.has(k)),
  );
</script>

<div class="drawer drawer-wide">
  <div class="drawer-h">
    <strong>Book (data.snbt)</strong>
    <button type="button" class="ghost ico" onclick={onclose}><X size={14} /></button>
  </div>
  <label
    >Title<input
      value={bookTitle ?? ""}
      oninput={(e) => {
        if (!bookDirty) onpushhistory();
        ontitlechange(inputVal(e));
      }}
    /></label
  >
  <label
    >Subtitle<input
      value={bookSubtitle ?? ""}
      oninput={(e) => {
        if (!bookDirty) onpushhistory();
        onsubtitlechange(inputVal(e));
      }}
    /></label
  >
  <div class="book-flags">
    {#each BOOK_BOOL_KEYS as key (key)}
      <label class="book-check">
        <input
          type="checkbox"
          checked={bookBool(key)}
          onchange={(e) => setBookBool(key, (e.currentTarget as HTMLInputElement).checked)}
        />
        {key}
      </label>
    {/each}
  </div>
  {#each BOOK_STRING_KEYS as key (key)}
    <label
      >{key}<input
        value={bookString(key)}
        oninput={(e) => setBookString(key, inputVal(e))}
      /></label
    >
  {/each}
  <label
    >default_quest_size<input
      type="number"
      step="0.25"
      min="0"
      value={bookNumber("default_quest_size")}
      oninput={(e) => setBookNumber("default_quest_size", inputVal(e))}
      placeholder="optional"
    /></label
  >
  {#if bookExtraEntries.length > 0}
    <p class="drawer-hint">Other data.snbt keys</p>
    {#each bookExtraEntries as [k, v] (k)}
      <div class="group-row book-extra">
        <code>{k}</code>
        <span class="extra-val">{typeof v === "string" ? v : JSON.stringify(v)}</span>
        <button type="button" class="ghost" onclick={() => removeBookSetting(k)}>Remove</button>
      </div>
    {/each}
  {/if}
  <p class="drawer-hint">Included in Save all · or save here</p>
  <button type="button" onclick={onsave} disabled={saving || !bookDirty}>Save book</button>
</div>

<style>
  .drawer {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    width: 280px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-frame);
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 12px 28px rgba(0, 0, 0, 0.55);
  }
  .drawer-wide {
    width: 360px;
    max-height: min(70vh, 560px);
    overflow: auto;
  }
  .drawer-h {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .drawer-h strong {
    flex: 1;
  }
  .drawer label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .drawer input {
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    color: inherit;
    border-radius: 3px;
    padding: 6px 8px;
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
  }
  .drawer-hint {
    margin: 0;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .book-flags {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow: auto;
    padding: 4px 0;
  }
  .book-check {
    display: flex !important;
    flex-direction: row !important;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    text-transform: none;
    color: var(--ftbq-text, #e8e8e8);
    letter-spacing: 0;
  }
  .group-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .book-extra .extra-val {
    flex: 1;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
