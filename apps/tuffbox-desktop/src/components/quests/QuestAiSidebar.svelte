<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import {
    Sparkles,
    Plus,
    Trash2,
    Send,
    Loader2,
    PanelRightClose,
  } from "lucide-svelte";
  import {
    api,
    type QuestChatSession,
    type QuestPlanMergeResult,
  } from "../../lib/api";
  import { projectPath, questChatFocusId } from "../../lib/store";
  import QuestPlanReview from "./QuestPlanReview.svelte";
  import { invoke } from "@tauri-apps/api/core";

  export let open = true;

  const dispatch = createEventDispatcher<{
    close: void;
    apply: QuestPlanMergeResult;
  }>();

  let sessions: QuestChatSession[] = [];
  let activeId: string | null = null;
  let session: QuestChatSession | null = null;
  let input = "";
  let showJson = false;
  let rawJson = "";
  /** Default true so Generate uses LLM; uncheck to allow offline heuristic. */
  let forceAi = true;
  let allowOfflineHeuristic = false;
  let busy = false;
  let error = "";
  let aiReadyHint = "";
  let merge: QuestPlanMergeResult | null = null;
  let progressLog: string[] = [];
  let loreWarning = "";
  const EXAMPLE_CHIPS = [
    {
      label: "24-quest line",
      text: "линейка на 24 квеста: early game → nether, с описаниями и наградами",
    },
    {
      label: "Create early game",
      text: "Create a 16-quest chapter for Create mod early progression with lore and XP rewards",
    },
    {
      label: "Numbered list",
      text: "глава 1: начало — 1. добудь 10 дерева, 2. накопай 20 булыги — награда 10 палок",
    },
  ];

  function useChip(text: string) {
    input = text;
    showJson = false;
  }

  async function refreshList() {
    if (!$projectPath) return;
    try {
      sessions = await api.quests.listChats($projectPath);
    } catch (e) {
      sessions = [];
      error = String(e);
    }
  }

  async function selectSession(id: string) {
    if (!$projectPath) return;
    activeId = id;
    try {
      session = await api.quests.loadChat(id, $projectPath);
      merge = null;
      progressLog = [];
    } catch (e) {
      error = String(e);
    }
  }

  async function newSession() {
    if (!$projectPath) return;
    try {
      session = await api.quests.newChat("Quest line", $projectPath);
      activeId = session.id;
      merge = null;
      progressLog = [];
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeSession(id: string) {
    if (!$projectPath) return;
    try {
      await api.quests.deleteChat(id, $projectPath);
      if (activeId === id) {
        activeId = null;
        session = null;
        merge = null;
      }
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  async function preflightAi(): Promise<boolean> {
    aiReadyHint = "";
    try {
      const result = await invoke<string>("test_integration", { provider: "ai" });
      aiReadyHint = result;
      return true;
    } catch (e) {
      error = `AI not ready: ${String(e)}. Open Settings → Integrations and configure Ollama / OpenAI-compatible.`;
      return false;
    }
  }

  async function send(intent: string | null = "generate") {
    if (!$projectPath || busy) return;
    const text = showJson && rawJson.trim() ? rawJson.trim() : input.trim();
    if (!text && intent === "generate") return;
    busy = true;
    error = "";
    loreWarning = "";
    progressLog = ["Starting…"];
    try {
      if (showJson && rawJson.trim()) {
        merge = await api.quests.parseAndMergePlan(rawJson, $projectPath);
        progressLog = ["Parsed pasted QuestPlan JSON"];
        input = "";
      } else {
        const useForceAi = forceAi || !allowOfflineHeuristic;
        if (useForceAi && intent !== "lore") {
          progressLog = ["Checking AI…"];
          const ok = await preflightAi();
          if (!ok) return;
        }
        const msg =
          intent === "lore"
            ? input.trim() || "Regenerate lore for pending plan"
            : intent === "extend"
              ? input.trim() || "Extend the quest line by 8 quests"
              : text;
        const result = await api.quests.chatTurn(
          msg,
          { chatId: activeId, forceAi: useForceAi, intent },
          $projectPath,
        );
        session = result.session;
        activeId = result.session.id;
        merge = result.merge;
        progressLog = result.progressLog ?? [];
        const logJoined = progressLog.join("\n");
        if (/offline heuristic/i.test(logJoined)) {
          loreWarning = "Used offline heuristic (no LLM). Enable Force AI for full generation.";
        }
        if (/Lore AI unavailable|Lore fail|template fill/i.test(logJoined)) {
          loreWarning =
            (loreWarning ? loreWarning + " " : "") +
            "Lore used templates — AI lore pass failed or was skipped.";
        }
        input = "";
        await refreshList();
      }
    } catch (e) {
      error = String(e);
      progressLog = [...progressLog, `Error: ${String(e)}`];
    } finally {
      busy = false;
    }
  }

  function onApplyReview(e: CustomEvent<{ chapterKeys: string[]; questKeys: string[] }>) {
    if (!merge?.plan || !$projectPath) return;
    void (async () => {
      busy = true;
      try {
        const filtered = await api.quests.filterAndMergePlan(
          merge!.plan,
          e.detail.chapterKeys,
          e.detail.questKeys,
          $projectPath!,
        );
        dispatch("apply", filtered);
        merge = null;
      } catch (err) {
        error = String(err);
      } finally {
        busy = false;
      }
    })();
  }

  onMount(() => {
    void refreshList();
  });

  $: if ($projectPath) void refreshList();

  $: if ($questChatFocusId && $projectPath && open) {
    const id = $questChatFocusId;
    questChatFocusId.set(null);
    void selectSession(id);
  }
</script>

<aside class="qai" class:open>
  <div class="qai-h">
    <Sparkles size={16} />
    <strong>Quest AI</strong>
    <button type="button" class="ghost ico" title="Close" on:click={() => dispatch("close")}>
      <PanelRightClose size={16} />
    </button>
  </div>

  <div class="sessions">
    <button type="button" class="ghost" on:click={newSession} disabled={!$projectPath}>
      <Plus size={14} /> New
    </button>
    <div class="sess-list">
      {#each sessions as s (s.id)}
        <div class="sess" class:active={activeId === s.id}>
          <button type="button" class="sess-open" on:click={() => selectSession(s.id)}>
            {s.title}
          </button>
          <button type="button" class="ghost ico" on:click={() => removeSession(s.id)}>
            <Trash2 size={12} />
          </button>
        </div>
      {/each}
    </div>
  </div>

  <div class="transcript">
    {#if !session?.messages?.length}
      <p class="hint">
        Describe a quest line. <strong>Apply</strong> updates the editor; <strong>Save</strong> writes
        SNBT.
      </p>
      <div class="chips">
        {#each EXAMPLE_CHIPS as chip (chip.label)}
          <button type="button" class="chip" on:click={() => useChip(chip.text)}>{chip.label}</button>
        {/each}
      </div>
    {:else}
      {#each session.messages as m, i (`${m.role}-${i}`)}
        <div class="msg" class:user={m.role === "user"} class:assistant={m.role === "assistant"}>
          <strong>{m.role === "user" ? "You" : "AI"}</strong>
          <p>{m.content}</p>
          {#if m.progressLog?.length}
            <ul class="prog">{#each m.progressLog as p, pi (`p-${pi}`)}<li>{p}</li>{/each}</ul>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  {#if busy && progressLog.length}
    <div class="live-prog">
      <Loader2 size={14} class="spin" />
      {progressLog[progressLog.length - 1]}
    </div>
  {/if}

  {#if error}<div class="err">{error}</div>{/if}
  {#if loreWarning}<div class="warn">{loreWarning}</div>{/if}
  {#if aiReadyHint && busy}<div class="live-prog">{aiReadyHint}</div>{/if}

  {#if merge}
    <QuestPlanReview
      {merge}
      needsReviewAck={!!merge.plan?.needsUserReview}
      on:apply={onApplyReview}
      on:discard={() => (merge = null)}
    />
  {/if}

  <div class="composer">
    {#if showJson}
      <textarea rows="4" placeholder={'{ "schemaVersion": 1, … }'} bind:value={rawJson}></textarea>
    {:else}
      <textarea
        rows="3"
        placeholder="Describe the quest line…"
        bind:value={input}
        on:keydown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void send("generate");
        }}
      ></textarea>
    {/if}
    <div class="composer-actions">
      <button type="button" disabled={busy || !$projectPath} on:click={() => send("generate")}>
        {#if busy}<Loader2 size={14} class="spin" />{:else}<Send size={14} />{/if}
        Generate
      </button>
    </div>
    <details class="adv">
      <summary>Advanced</summary>
      <label class="opt"><input type="checkbox" bind:checked={forceAi} /> Force AI (skip offline heuristic)</label>
      <label class="opt"
        ><input type="checkbox" bind:checked={allowOfflineHeuristic} /> Allow offline heuristic</label
      >
      <label class="opt"><input type="checkbox" bind:checked={showJson} /> Paste JSON</label>
      <div class="composer-actions">
        <button
          type="button"
          class="ghost"
          disabled={busy || !session?.pendingPlan}
          on:click={() => send("lore")}>Lore only</button
        >
        <button
          type="button"
          class="ghost"
          disabled={busy || !$projectPath}
          on:click={() => send("extend")}>Extend</button
        >
      </div>
    </details>
  </div>
</aside>

<style>
  .qai {
    display: flex;
    flex-direction: column;
    width: 340px;
    min-width: 280px;
    max-width: 420px;
    border-left: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg-panel, #212126);
    color: var(--ftbq-text, #e8e8e8);
    min-height: 0;
    height: 100%;
  }
  .qai-h {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .qai-h .ico {
    margin-left: auto;
  }
  .sessions {
    padding: 8px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .sess-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 88px;
    overflow: auto;
    margin-top: 6px;
  }
  .sess {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sess.active .sess-open {
    color: var(--ftbq-accent-green, #55c95a);
  }
  .sess-open {
    flex: 1;
    text-align: left;
    background: transparent;
    border: none;
    color: inherit;
    font-size: 12px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .transcript {
    flex: 1;
    overflow: auto;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 120px;
  }
  .msg {
    font-size: 12px;
    padding: 8px;
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
  }
  .msg.user {
    background: rgba(27, 217, 106, 0.08);
  }
  .msg p {
    margin: 4px 0 0;
    white-space: pre-wrap;
  }
  .prog {
    margin: 4px 0 0;
    padding-left: 16px;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
  }
  .hint {
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 12px;
    margin: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .chip {
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
    padding: 4px 8px;
    font-size: 11px;
    cursor: pointer;
  }
  .chip:hover {
    border-color: var(--ftbq-accent-teal, #3db8a8);
  }
  .adv {
    margin-top: 4px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .adv summary {
    cursor: pointer;
    padding: 4px 0;
  }
  .live-prog,
  .err,
  .warn {
    padding: 6px 10px;
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .err {
    color: #f87171;
  }
  .warn {
    color: var(--ftbq-quest-started, #f2c94c);
    background: rgba(242, 201, 76, 0.08);
  }
  .composer {
    padding: 10px;
    border-top: 1px solid var(--ftbq-border, #3a3a42);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .opt {
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  textarea {
    width: 100%;
    resize: vertical;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg, #1a1a1e);
    color: inherit;
    padding: 8px;
    font-family: inherit;
    font-size: 12px;
  }
  .composer-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .composer-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  :global(.spin) {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
