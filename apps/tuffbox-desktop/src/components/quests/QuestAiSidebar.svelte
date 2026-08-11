<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    Sparkles,
    Plus,
    Trash2,
    Send,
    Loader2,
    PanelRightClose,
    GitBranch,
    MessageSquareText,
  } from "@lucide/svelte";
  import {
    api,
    type QuestChatSession,
    type QuestData,
    type QuestPlanMergeResult,
    type AiTokenUsage,
  } from "../../lib/api";
  import { projectPath, questChatFocusId } from "../../lib/store";
  import QuestPlanReview from "./QuestPlanReview.svelte";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import { invoke } from "@tauri-apps/api/core";

  type QuestAiProgressPayload = {
    chatId: string;
    line: string;
    phase: string;
    i?: number;
    n?: number;
  };

  type QuestAiTokenPayload = {
    chatId: string;
    text: string;
    phase: string;
  };

  let {
    open = true,
    onclose,
    onapply,
    anchorQuest = null,
    anchorChapterTitle = null,
    targetChapterId = null,
  }: {
    open?: boolean;
    onclose?: () => void;
    onapply?: (result: QuestPlanMergeResult) => void;
    anchorQuest?: QuestData | null;
    anchorChapterTitle?: string | null;
    /** Current editor chapter — generate/extend upsert target. */
    targetChapterId?: string | null;
  } = $props();

  let sessions = $state<QuestChatSession[]>([]);
  let activeId = $state<string | null>(null);
  let session = $state<QuestChatSession | null>(null);
  let input = $state("");
  let showJson = $state(false);
  let rawJson = $state("");
  let forceAi = $state(true);
  let busy = $state(false);
  let error = $state("");
  let aiReadyHint = $state("");
  let merge = $state<QuestPlanMergeResult | null>(null);
  let progressLog = $state<string[]>([]);
  let streamDraft = $state("");
  let loreWarning = $state("");
  /** Active intent for the next send: "generate" | "extend" | "lore" | "branch". */
  let pendingIntent = $state<"generate" | "extend" | "lore" | "branch">("generate");
  let discardConfirmOpen = $state(false);
  let deleteConfirmOpen = $state(false);
  let deleteTarget = $state<{ id: string; title: string } | null>(null);
  let composerHint = $state("");
  let transcriptEl = $state<HTMLDivElement | null>(null);
  let composerEl = $state<HTMLTextAreaElement | null>(null);
  let wasOpen = $state(false);
  let unlistenProgress: UnlistenFn | undefined;
  let unlistenTokens: UnlistenFn | undefined;
  type ExampleChip = { label: string; text: string };

  function uiLang(): "ru" | "en" {
    const t = (typeof navigator !== "undefined" ? navigator.language : "en").toLowerCase();
    return t.startsWith("ru") ? "ru" : "en";
  }

  const EXAMPLE_CHIPS_BY_LANG: Record<"en" | "ru", ExampleChip[]> = {
    en: [
      {
        label: "24-quest line",
        text: "24-quest line: early game → nether, with descriptions and rewards",
      },
      {
        label: "3 chapters",
        text: "3 chapters: early / mid / late progression with about 18 quests total",
      },
      {
        label: "Create early game",
        text: "Create a 16-quest chapter for Create mod early progression with lore and XP rewards",
      },
      {
        label: "Numbered list",
        text: "chapter 1: start — 1. gather 10 wood, 2. mine 20 cobblestone — reward 10 sticks",
      },
    ],
    ru: [
      {
        label: "Линейка 24 квеста",
        text: "линейка на 24 квеста: early game → nether, с описаниями и наградами",
      },
      {
        label: "3 главы",
        text: "3 главы: early / mid / late прогрессия, около 18 квестов всего",
      },
      {
        label: "Create early game",
        text: "глава Create early game на 16 квестов с лором и XP наградами",
      },
      {
        label: "Нумерованный список",
        text: "глава 1: начало — 1. добудь 10 дерева, 2. накопай 20 булыги — награда 10 палок",
      },
    ],
  };

  let exampleChips = $derived(EXAMPLE_CHIPS_BY_LANG[uiLang()]);
  let lastUsage = $state<AiTokenUsage | null>(null);

  function formatTokens(n: number | null | undefined): string {
    if (n == null || !Number.isFinite(n)) return "—";
    if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
    return String(n);
  }

  function formatUsage(u: AiTokenUsage | null | undefined): string | null {
    if (!u) return null;
    const pin = u.promptTokens;
    const cout = u.completionTokens;
    const tot = u.totalTokens ?? (pin != null && cout != null ? pin + cout : null);
    if (pin == null && cout == null && tot == null) return null;
    const parts: string[] = [];
    if (pin != null) parts.push(`${formatTokens(pin)} in`);
    if (cout != null) parts.push(`${formatTokens(cout)} out`);
    if (tot != null && (pin == null || cout == null)) parts.push(`${formatTokens(tot)} tot`);
    // Rough OpenAI-class mid-tier estimate ($/1M); local Ollama shows tokens only when cost ~0.
    let costHint = "";
    if (pin != null || cout != null) {
      const usd = ((pin ?? 0) * 0.15 + (cout ?? 0) * 0.6) / 1_000_000;
      if (usd >= 0.0001) costHint = ` · ~$${usd < 0.01 ? usd.toFixed(4) : usd.toFixed(3)}`;
    }
    return parts.join(" · ") + costHint;
  }

  let sessionUsageLabel = $derived.by(() => {
    const msgs = session?.messages ?? [];
    const acc: AiTokenUsage = { promptTokens: 0, completionTokens: 0, totalTokens: 0 };
    let any = false;
    for (const m of msgs) {
      const u = m.usage;
      if (!u) continue;
      any = true;
      if (u.promptTokens != null) acc.promptTokens = (acc.promptTokens ?? 0) + u.promptTokens;
      if (u.completionTokens != null)
        acc.completionTokens = (acc.completionTokens ?? 0) + u.completionTokens;
      if (u.totalTokens != null) acc.totalTokens = (acc.totalTokens ?? 0) + u.totalTokens;
    }
    return any ? formatUsage(acc) : null;
  });

  function useChip(text: string) {
    input = text;
    showJson = false;
  }

  function setIntent(i: "generate" | "extend" | "lore" | "branch") {
    pendingIntent = i;
  }

  const INTENT_ORDER = ["generate", "branch", "extend", "lore"] as const;

  function intentEnabled(i: (typeof INTENT_ORDER)[number]): boolean {
    if (i === "branch") return !!anchorQuest;
    if (i === "extend" || i === "lore") return !!session?.pendingPlan;
    return true;
  }

  function onIntentKeydown(e: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"].includes(e.key)) {
      return;
    }
    e.preventDefault();
    const enabled = INTENT_ORDER.filter(intentEnabled);
    if (enabled.length === 0) return;
    const idx = Math.max(0, enabled.indexOf(pendingIntent as (typeof INTENT_ORDER)[number]));
    let next = idx;
    if (e.key === "Home") next = 0;
    else if (e.key === "End") next = enabled.length - 1;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      next = (idx - 1 + enabled.length) % enabled.length;
    } else {
      next = (idx + 1) % enabled.length;
    }
    setIntent(enabled[next]!);
  }

  function appendProgressLine(line: string) {
    const last = progressLog[progressLog.length - 1];
    if (last === line) return;
    progressLog = [...progressLog, line];
  }

  /** When an anchor quest is selected, default the next send to "branch". */
  $effect(() => {
    if (anchorQuest && pendingIntent === "generate") {
      pendingIntent = "branch";
    }
    if (!anchorQuest && pendingIntent === "branch") {
      pendingIntent = "generate";
    }
  });

  $effect(() => {
    const _msgs = session?.messages?.length ?? 0;
    const _stream = streamDraft.length;
    const _prog = progressLog.length;
    void _msgs;
    void _stream;
    void _prog;
    if (!transcriptEl) return;
    queueMicrotask(() => {
      transcriptEl?.scrollTo({ top: transcriptEl.scrollHeight, behavior: "smooth" });
    });
  });

  async function refreshList() {
    if (!$projectPath) return;
    try {
      sessions = (await api.quests.listChats($projectPath)).sessions;
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
      streamDraft = "";
      error = "";
      composerHint = "";
      if (session.pendingPlan) {
        merge = await api.quests.filterAndMergePlan(
          session.pendingPlan,
          [],
          [],
          $projectPath,
        );
      }
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
      streamDraft = "";
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  function requestDeleteSession(id: string, title: string) {
    deleteTarget = { id, title };
    deleteConfirmOpen = true;
  }

  async function confirmDeleteSession() {
    deleteConfirmOpen = false;
    const target = deleteTarget;
    deleteTarget = null;
    if (!target || !$projectPath) return;
    try {
      await api.quests.deleteChat(target.id, $projectPath);
      if (activeId === target.id) {
        activeId = null;
        session = null;
        merge = null;
      }
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  async function reopenPendingReview() {
    if (!session?.pendingPlan || !$projectPath || busy) return;
    busy = true;
    error = "";
    try {
      merge = await api.quests.filterAndMergePlan(
        session.pendingPlan,
        [],
        [],
        $projectPath,
      );
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function preflightAi(): Promise<boolean> {
    aiReadyHint = "";
    try {
      const result = await invoke<string>("test_integration", { provider: "ai" });
      aiReadyHint = result;
      return true;
    } catch (e) {
      error = `AI not ready: ${String(e)}. Open Settings → AI and configure Ollama / a cloud API.`;
      return false;
    }
  }

  let canSend = $derived(
    !!$projectPath &&
      (pendingIntent === "lore" ||
        (showJson ? rawJson.trim().length > 0 : input.trim().length > 0)),
  );

  async function send(intent: "generate" | "extend" | "lore" | "branch" | null = "generate") {
    if (!$projectPath || busy) return;
    const useIntent = intent ?? pendingIntent;
    const text = showJson && rawJson.trim() ? rawJson.trim() : input.trim();
    if (!text && useIntent !== "lore") {
      composerHint = showJson
        ? "Paste QuestPlan JSON first"
        : "Describe the quest line first";
      return;
    }
    composerHint = "";
    busy = true;
    error = "";
    loreWarning = "";
    progressLog = ["Starting…"];
    streamDraft = "";
    try {
      if (showJson && rawJson.trim()) {
        merge = await api.quests.parseAndMergePlan(rawJson, $projectPath);
        progressLog = ["Parsed pasted QuestPlan JSON"];
        input = "";
      } else {
        const useForceAi = forceAi;
        if (useForceAi && useIntent !== "lore") {
          progressLog = ["Checking AI…"];
          const ok = await preflightAi();
          if (!ok) return;
        }
        const msg =
          useIntent === "lore"
            ? input.trim() || "Regenerate lore for pending plan"
            : useIntent === "extend"
              ? input.trim() || "Extend the quest line by 8 quests"
              : useIntent === "branch"
                ? input.trim() ||
                  `Create a branch of 6 quests from "${anchorQuest?.title ?? "selected"}"`
                : text;
        const result = await api.quests.chatTurn(
          msg,
          {
            chatId: activeId,
            forceAi: useForceAi,
            intent: useIntent,
            anchorQuestId: useIntent === "branch" ? anchorQuest?.id ?? null : null,
            targetChapterId:
              useIntent === "generate" || useIntent === "extend"
                ? targetChapterId
                : null,
          },
          $projectPath,
        );
        session = result.session;
        activeId = result.session.id;
        merge = result.merge;
        progressLog = result.progressLog ?? progressLog;
        lastUsage = result.usage ?? result.session.messages.at(-1)?.usage ?? null;
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
      const msg = String(e);
      if (/cancelled/i.test(msg)) {
        error = "";
        progressLog = [...progressLog, "Cancelled"];
      } else {
        error = msg;
        progressLog = [...progressLog, `Error: ${msg}`];
      }
    } finally {
      busy = false;
      streamDraft = "";
    }
  }

  async function stopGeneration() {
    if (!busy) return;
    try {
      await api.quests.cancelChatTurn();
      appendProgressLine("Stopping…");
    } catch (e) {
      error = String(e);
    }
  }

  function onApplyReview(detail: { chapterKeys: string[]; questKeys: string[] }) {
    if (busy) return;
    if (!merge?.plan || !$projectPath) return;
    void (async () => {
      busy = true;
      try {
        const filtered = await api.quests.filterAndMergePlan(
          merge!.plan,
          detail.chapterKeys,
          detail.questKeys,
          $projectPath!,
        );
        onapply?.(filtered);
        merge = null;
        if (session) {
          const next: QuestChatSession = { ...session, pendingPlan: null };
          session = next;
          await api.quests.saveChat(next, $projectPath!);
          await refreshList();
        }
      } catch (err) {
        error = String(err);
      } finally {
        busy = false;
      }
    })();
  }

  async function confirmDiscardPendingPlan() {
    discardConfirmOpen = false;
    if (!session?.pendingPlan || !$projectPath) return;
    const next: QuestChatSession = { ...session, pendingPlan: null };
    session = next;
    merge = null;
    if (pendingIntent === "extend" || pendingIntent === "lore") {
      pendingIntent = "generate";
    }
    try {
      await api.quests.saveChat(next, $projectPath);
      await refreshList();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    void refreshList();
    void listen<QuestAiProgressPayload>("quest-ai-progress", (event) => {
      const payload = event.payload;
      if (!busy) return;
      // Accept when no session yet, or chat matches active (incl. after assign).
      if (activeId != null && payload.chatId !== activeId) return;
      appendProgressLine(payload.line);
      // Clear live draft when a phase completes / advances past outline streaming.
      if (payload.phase && payload.phase !== "outline") {
        streamDraft = "";
      }
    }).then((unlisten) => {
      unlistenProgress = unlisten;
    });
    void listen<QuestAiTokenPayload>("quest-ai-token", (event) => {
      const payload = event.payload;
      if (!busy) return;
      if (activeId != null && payload.chatId !== activeId) return;
      if (streamDraft.length >= 262144) return;
      streamDraft += payload.text;
      if (streamDraft.length > 262144) {
        streamDraft = `${streamDraft.slice(0, 262144)}\n…`;
      }
    }).then((unlisten) => {
      unlistenTokens = unlisten;
    });
  });

  onDestroy(() => {
    unlistenProgress?.();
    unlistenTokens?.();
  });

  $effect(() => {
    if ($projectPath) void refreshList();
  });

  $effect(() => {
    if ($questChatFocusId && $projectPath && open) {
      const id = $questChatFocusId;
      questChatFocusId.set(null);
      void selectSession(id);
    }
  });

  $effect(() => {
    const justOpened = open && !wasOpen;
    wasOpen = open;
    if (!justOpened || !open) return;
    if (discardConfirmOpen || deleteConfirmOpen) return;
    queueMicrotask(() => {
      if (discardConfirmOpen || deleteConfirmOpen) return;
      composerEl?.focus({ preventScroll: true });
    });
  });
</script>

<aside class="qai" class:open aria-busy={busy}>
  <div class="qai-h">
    <Sparkles size={16} />
    <strong>Quest AI</strong>
    {#if sessionUsageLabel}
      <span class="usage-pill" title="Session token usage (estimate)">{sessionUsageLabel}</span>
    {/if}
    <button type="button" class="ghost ico" title="Close" onclick={() => onclose?.()}>
      <PanelRightClose size={16} />
    </button>
  </div>

  {#if anchorQuest}
    <div class="anchor-banner" title="Branch will root at this quest">
      <GitBranch size={14} />
      <div class="anchor-text">
        <span class="anchor-label">Branch from</span>
        <strong class="anchor-title">{anchorQuest.title}</strong>
        {#if anchorChapterTitle}<span class="anchor-ch">{anchorChapterTitle}</span>{/if}
      </div>
      <code class="anchor-id">{anchorQuest.id.slice(0, 8)}</code>
    </div>
  {/if}

  <div class="sessions">
    <button type="button" class="ghost" onclick={newSession} disabled={!$projectPath}>
      <Plus size={14} /> New
    </button>
    <div class="sess-list">
      {#each sessions as s (s.id)}
        <div class="sess" class:active={activeId === s.id}>
          <button type="button" class="sess-open" onclick={() => selectSession(s.id)}>
            {s.title}
          </button>
          <button
            type="button"
            class="ghost ico"
            aria-label={`Delete chat ${s.title}`}
            title={`Delete chat ${s.title}`}
            onclick={() => requestDeleteSession(s.id, s.title)}
          >
            <Trash2 size={12} />
          </button>
        </div>
      {/each}
    </div>
  </div>

  <div class="transcript" bind:this={transcriptEl}>
    {#if !session?.messages?.length}
      <div class="empty-chat">
        <MessageSquareText size={28} />
        <p class="hint">
          Describe a quest line. <strong>Apply</strong> updates the editor; <strong>Save</strong> writes
          SNBT.
        </p>
        {#if anchorQuest}
          <p class="hint hint-anchor">
            Tip: with a quest selected, <kbd>Branch</kbd> creates a chain rooted at it.
          </p>
        {/if}
        <div class="chips">
          {#each exampleChips as chip (chip.label)}
            <button type="button" class="chip" onclick={() => useChip(chip.text)}>{chip.label}</button>
          {/each}
        </div>
      </div>
    {:else}
      {#each session.messages as m, i (`${m.role}-${i}`)}
        <div class="msg" class:user={m.role === "user"} class:assistant={m.role === "assistant"}>
          <strong>{m.role === "user" ? "You" : "AI"}</strong>
          <p>{m.content}</p>
          {#if m.progressLog?.length}
            <details class="prog">
              <summary>{m.progressLog.length} log lines</summary>
              <ul>{#each m.progressLog as p, pi (`p-${pi}`)}<li>{p}</li>{/each}</ul>
            </details>
          {/if}
          {#if formatUsage(m.usage)}
            <div class="msg-usage">{formatUsage(m.usage)}</div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  {#if busy && (progressLog.length || streamDraft)}
    <div class="live-prog">
      <Loader2 size={14} class="spin" />
      {progressLog.length ? progressLog[progressLog.length - 1] : "Streaming…"}
    </div>
    {#if streamDraft}
      <details class="stream-wrap" open={busy && !merge}>
        <summary>Stream draft ({streamDraft.length} chars)</summary>
        <pre class="stream-draft">{streamDraft}</pre>
      </details>
    {/if}
  {/if}
  {#if !busy && lastUsage && formatUsage(lastUsage)}
    <div class="live-prog usage-last">Last turn: {formatUsage(lastUsage)}</div>
  {/if}

  {#if error}
    <div class="err" role="alert">
      <span class="err-text">{error}</span>
      <button type="button" class="ghost ico err-dismiss" title="Dismiss" aria-label="Dismiss error" onclick={() => (error = "")}>
        ×
      </button>
    </div>
  {/if}
  {#if loreWarning}
    <div class="warn" role="status">
      <span class="warn-text">{loreWarning}</span>
      <button type="button" class="ghost ico err-dismiss" title="Dismiss" aria-label="Dismiss warning" onclick={() => (loreWarning = "")}>
        ×
      </button>
    </div>
  {/if}
  {#if aiReadyHint && busy}<div class="live-prog">{aiReadyHint}</div>{/if}

  {#if merge}
    <QuestPlanReview
      {merge}
      needsReviewAck={!!merge.plan?.needsUserReview}
      onapply={onApplyReview}
      ondiscard={() => (discardConfirmOpen = true)}
    />
  {/if}

  {#if session?.pendingPlan && !merge}
    <div class="pending-plan-bar">
      <span class="pending-plan-label">Pending plan ready</span>
      <button
        type="button"
        class="ghost review-plan"
        disabled={busy}
        onclick={() => void reopenPendingReview()}
      >
        Review
      </button>
      <button
        type="button"
        class="ghost discard-plan"
        disabled={busy}
        onclick={() => (discardConfirmOpen = true)}
      >
        Discard
      </button>
    </div>
  {/if}

  <div class="composer">
    <div
      class="intent-row"
      role="radiogroup"
      aria-label="Quest AI intent"
      tabindex="-1"
      onkeydown={onIntentKeydown}
    >      <button
        type="button"
        class="intent"
        class:active={pendingIntent === "generate"}
        role="radio"
        aria-checked={pendingIntent === "generate"}
        tabindex={pendingIntent === "generate" ? 0 : -1}
        onclick={() => setIntent("generate")}
      >Generate</button>
      <button
        type="button"
        class="intent"
        class:active={pendingIntent === "branch"}
        role="radio"
        aria-checked={pendingIntent === "branch"}
        tabindex={pendingIntent === "branch" ? 0 : -1}
        disabled={!anchorQuest}
        title={anchorQuest ? "Branch from selected quest" : "Select a quest on canvas first"}
        onclick={() => setIntent("branch")}
      ><GitBranch size={12} /> Branch</button>
      <button
        type="button"
        class="intent"
        class:active={pendingIntent === "extend"}
        role="radio"
        aria-checked={pendingIntent === "extend"}
        tabindex={pendingIntent === "extend" ? 0 : -1}
        disabled={!session?.pendingPlan}
        title={session?.pendingPlan ? "Append to pending plan" : "Generate a plan first"}
        onclick={() => setIntent("extend")}
      >Extend</button>
      <button
        type="button"
        class="intent"
        class:active={pendingIntent === "lore"}
        role="radio"
        aria-checked={pendingIntent === "lore"}
        tabindex={pendingIntent === "lore" ? 0 : -1}
        disabled={!session?.pendingPlan}
        title={session?.pendingPlan ? "Regenerate lore only" : "Generate a plan first"}
        onclick={() => setIntent("lore")}
      >Lore</button>
    </div>

    {#if showJson}
      <textarea
        rows="4"
        placeholder={'{ "schemaVersion": 1, … }'}
        bind:this={composerEl}
        bind:value={rawJson}
        oninput={() => (composerHint = "")}
      ></textarea>
    {:else}
      <textarea
        rows="3"
        placeholder={pendingIntent === "branch"
          ? `Describe the branch from "${anchorQuest?.title ?? "…"}"…`
          : "Describe the quest line…"}
        bind:this={composerEl}
        bind:value={input}
        oninput={() => (composerHint = "")}
        onkeydown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) void send(pendingIntent);
        }}
      ></textarea>
    {/if}
    {#if composerHint}
      <div class="composer-hint" role="status">{composerHint}</div>
    {/if}
    <p class="send-chord">Ctrl+Enter to send</p>
    <div class="composer-actions">
      {#if busy}
        <button type="button" class="stop" onclick={() => void stopGeneration()}>
          Stop
        </button>
      {:else}
        <button
          type="button"
          class="primary"
          disabled={!canSend}
          title={!canSend && $projectPath ? "Describe the quest line first" : undefined}
          onclick={() => send(pendingIntent)}
        >
          <Send size={14} />
          {pendingIntent === "branch" ? "Branch" : pendingIntent === "extend" ? "Extend" : pendingIntent === "lore" ? "Lore" : "Generate"}
        </button>
      {/if}
      <button type="button" class="ghost" onclick={() => (showJson = !showJson)} disabled={busy}>
        {showJson ? "Text" : "JSON"}
      </button>
    </div>
    <details class="adv">
      <summary>Advanced</summary>
      <label class="opt"
        ><input type="checkbox" bind:checked={forceAi} /> Force AI (skip offline heuristic)</label
      >
    </details>
  </div>
</aside>

{#if discardConfirmOpen}
  <ConfirmDialog
    title="Discard pending plan?"
    message="This clears the pending QuestPlan from the chat session. You can generate a new one afterward."
    danger={true}
    confirmLabel="Discard"
    onconfirm={() => void confirmDiscardPendingPlan()}
    oncancel={() => (discardConfirmOpen = false)}
  />
{/if}

{#if deleteConfirmOpen && deleteTarget}
  <ConfirmDialog
    title="Delete chat?"
    message={`Delete chat “${deleteTarget.title}”? History cannot be recovered.`}
    danger={true}
    confirmLabel="Delete"
    onconfirm={() => void confirmDeleteSession()}
    oncancel={() => {
      deleteConfirmOpen = false;
      deleteTarget = null;
    }}
  />
{/if}

<style>
  .qai {
    display: flex;
    flex-direction: column;
    width: 360px;
    min-width: 300px;
    max-width: 440px;
    border-left: 1px solid var(--ftbq-frame);
    box-shadow: inset 1px 0 0 rgba(255, 255, 255, 0.05);
    background: var(--ftbq-bg-panel);
    color: var(--ftbq-text, #e8e8e8);
    min-height: 0;
    height: 100%;
  }
  .qai-h {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--ftbq-frame);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(0, 0, 0, 0.25));
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.05);
  }
  .qai-h strong {
    color: var(--ftbq-title-gold, #f2c94c);
    font-size: 13px;
    text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.65);
    letter-spacing: 0.02em;
  }
  .qai-h .ico {
    margin-left: auto;
  }
  .usage-pill,
  .msg-usage,
  .usage-last {
    font-size: 10px;
    font-weight: 600;
    color: var(--ftbq-text-muted, #9a9aa0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 160px;
  }
  .msg-usage {
    margin-top: 4px;
    max-width: none;
  }
  .usage-last {
    max-width: none;
    padding: 4px 10px 8px;
  }

  .anchor-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: linear-gradient(90deg, rgba(61, 184, 168, 0.18), rgba(61, 184, 168, 0.05));
    border-bottom: 1px solid var(--ftbq-frame);
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.05);
    color: #c9f2ec;
  }
  .anchor-banner :global(svg) {
    color: var(--ftbq-accent-teal, #3db8a8);
    flex-shrink: 0;
  }
  .anchor-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .anchor-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .anchor-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--ftbq-text, #e8e8e8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .anchor-ch {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .anchor-id {
    font-size: 9px;
    color: var(--ftbq-text-muted, #9a9aa0);
    background: rgba(0, 0, 0, 0.35);
    padding: 2px 5px;
    border-radius: 3px;
    border: 1px solid var(--ftbq-frame);
  }
  .sessions {
    padding: 8px;
    border-bottom: 1px solid var(--ftbq-frame);
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
  .empty-chat {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    text-align: center;
    padding: 16px 8px;
  }
  .empty-chat :global(svg) {
    color: var(--ftbq-text-muted, #9a9aa0);
    opacity: 0.6;
  }
  .msg {
    font-size: 12px;
    padding: 8px 10px;
    border-radius: 3px;
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    box-shadow: inset 1px 1px 0 rgba(0, 0, 0, 0.4), inset -1px -1px 0 rgba(255, 255, 255, 0.04);
  }
  .msg.user {
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent-primary) 12%, transparent), color-mix(in srgb, var(--accent-primary) 5%, transparent));
    border-color: #1f5a2c;
  }
  .msg p {
    margin: 4px 0 0;
    white-space: pre-wrap;
  }
  .prog {
    margin: 6px 0 0;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
  }
  .prog summary {
    cursor: pointer;
    padding: 2px 0;
  }
  .prog ul {
    margin: 4px 0 0;
    padding-left: 16px;
  }
  .hint {
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 12px;
    margin: 0;
  }
  .hint-anchor {
    font-size: 11px;
  }
  .hint-anchor kbd {
    display: inline-block;
    padding: 1px 5px;
    font-size: 10px;
    font-family: monospace;
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    border-radius: 3px;
    margin: 0 2px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
    justify-content: center;
  }
  .chip {
    border: 1px solid var(--ftbq-frame);
    background: linear-gradient(180deg, var(--ftbq-border), var(--ftbq-btn-bottom));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.1);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
    padding: 4px 8px;
    font-size: 11px;
    cursor: pointer;
  }
  .chip:hover {
    background: linear-gradient(180deg, var(--ftbq-btn-hover-top), var(--ftbq-btn-hover-bottom));
    color: var(--ftbq-accent-green);
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
  .stream-wrap {
    margin: 0 10px 8px;
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--border-radius-sm);
    background: rgba(0, 0, 0, 0.2);
  }
  .stream-wrap summary {
    cursor: pointer;
    padding: 6px 8px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-weight: 600;
  }
  .stream-draft {
    margin: 0;
    max-height: 140px;
    overflow: auto;
    padding: 8px;
    font-size: 11px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--ftbq-text-muted, #9a9aa0);
    background: rgba(0, 0, 0, 0.25);
    border: none;
    border-top: 1px solid var(--ftbq-frame);
    border-radius: 0;
  }
  .err {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: #f87171;
    padding: 6px 10px;
    font-size: 12px;
    background: rgba(239, 68, 68, 0.08);
    border-top: 1px solid rgba(239, 68, 68, 0.25);
  }
  .err-text {
    flex: 1;
    min-width: 0;
  }
  .err-dismiss {
    flex-shrink: 0;
    line-height: 1;
    font-size: 16px;
    padding: 0 4px;
    color: #f87171;
  }
  .warn {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--ftbq-quest-started, #f2c94c);
    background: rgba(242, 201, 76, 0.08);
    padding: 6px 10px;
    font-size: 12px;
  }
  .warn-text {
    flex: 1;
    min-width: 0;
  }
  .send-chord {
    margin: 0;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .pending-plan-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-top: 1px solid var(--ftbq-frame);
    background: rgba(242, 201, 76, 0.06);
  }
  .pending-plan-label {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    color: var(--ftbq-quest-started, #f2c94c);
  }
  .review-plan {
    font-size: 11px;
    font-weight: 600;
    color: #86efac;
    background: transparent;
    border: 1px solid #1f5a2c;
    border-radius: 3px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .review-plan:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
  }
  .review-plan:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .discard-plan {
    font-size: 11px;
    font-weight: 600;
    color: #f87171;
    background: transparent;
    border: 1px solid #5a1a1a;
    border-radius: 3px;
    padding: 3px 8px;
    cursor: pointer;
  }
  .discard-plan:hover:not(:disabled) {
    background: rgba(248, 113, 113, 0.12);
  }
  .discard-plan:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .composer {
    padding: 10px;
    border-top: 1px solid var(--ftbq-frame);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(0, 0, 0, 0.2));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .composer-hint {
    font-size: 11px;
    color: #fbbf24;
    padding: 2px 0;
  }
  .intent-row {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .intent {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 3px;
    border: 1px solid var(--ftbq-frame);
    background: linear-gradient(180deg, var(--ftbq-border), var(--ftbq-btn-bottom));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.1), inset 0 -1px 0 rgba(0, 0, 0, 0.45);
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .intent:hover:not(:disabled) {
    background: linear-gradient(180deg, var(--ftbq-btn-hover-top), var(--ftbq-btn-hover-bottom));
    color: var(--ftbq-text, #e8e8e8);
  }
  .intent.active {
    border-color: #12380f;
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent-primary) 88%, #fff 12%), color-mix(in srgb, var(--accent-primary) 72%, #000 28%));
    color: var(--ftbq-text);
  }
  .intent:disabled {
    opacity: 0.4;
    cursor: not-allowed;
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
    border-radius: 3px;
    border: 1px solid var(--ftbq-frame);
    background: var(--ftbq-input-bg);
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
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
  .composer-actions .primary {
    flex: 1;
    justify-content: center;
    padding: 6px 12px;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 50%, #000);
    background: linear-gradient(180deg, color-mix(in srgb, var(--accent-primary) 88%, #fff 12%), color-mix(in srgb, var(--accent-primary) 72%, #000 28%));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.25), inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    color: var(--ftbq-text);
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
    border-radius: 3px;
    font-weight: 700;
    cursor: pointer;
  }
  .composer-actions .primary:hover:not(:disabled) {
    filter: brightness(1.12);
  }
  .composer-actions .primary:disabled {
    opacity: 0.5;
  }
  .composer-actions .stop {
    flex: 1;
    justify-content: center;
    padding: 6px 12px;
    border: 1px solid #5a1a1a;
    background: linear-gradient(180deg, #a84848, #7a2e2e);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.2), inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    color: #ffe9e9;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
    border-radius: 3px;
    font-weight: 700;
    cursor: pointer;
  }
  .composer-actions .stop:hover {
    filter: brightness(1.1);
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
