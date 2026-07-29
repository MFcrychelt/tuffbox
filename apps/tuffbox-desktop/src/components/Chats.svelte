<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    MessagesSquare,
    Plus,
    Send,
    Sparkles,
    Package,
    Loader2,
    CheckCircle2,
    Trash2,
    GitGraph,
  } from "lucide-svelte";
  import { projectPath, projectInfo, ideStageRequest } from "../lib/store";
  import { toasts } from "../lib/toast";

  export let currentView: string;

  type ChatMessage = { role: string; content: string; createdAt?: string | null };
  type PackDraftMod = {
    slug: string;
    projectId: string;
    name: string;
    reason: string;
    category: string;
    downloads: number;
    provider?: string;
  };
  type PackBrief = {
    title: string;
    mcVersion: string;
    loader: string;
    targetCount: number;
    mustHave: { query: string; slugHint?: string | null; reason: string }[];
    categories: { id: string; query: string; count: number; reason: string }[];
    exclude: string[];
  };
  type PackDraft = { brief: PackBrief; mods: PackDraftMod[]; unresolved: string[] };
  type CandidateAddon = {
    slug: string;
    name: string;
    summary?: string;
    score: number;
    source: string;
  };
  type ChatSession = {
    id: string;
    title: string;
    messages: ChatMessage[];
    draft?: PackDraft | null;
    updatedAt: string;
  };

  let sessions: ChatSession[] = [];
  let activeId: string | null = null;
  let messages: ChatMessage[] = [];
  let brief: PackBrief | null = null;
  let draft: PackDraft | null = null;
  let candidates: CandidateAddon[] = [];
  let input = "";
  let targetCount = 80;
  let busy = false;
  let phase = "";
  let progressDone = 0;
  let progressTotal = 0;
  let progressCurrent = "";
  let unlisten: UnlistenFn | null = null;
  let lastPath = "";
  let draftConfirmOpen = false;
  let draftSelected: Record<string, boolean> = {};
  let postInstallTrail = false;
  let lastInstallCount = 0;

  $: active = sessions.find((s) => s.id === activeId) ?? null;
  $: mcLabel = $projectInfo?.minecraftVersion ?? "?";
  $: loaderLabel = $projectInfo?.loaderKind ?? "?";
  $: draftConfirmCount = draft?.mods?.filter((m) => draftSelected[m.projectId || m.slug] !== false).length ?? 0;

  async function refreshSessions() {
    if (!$projectPath) {
      sessions = [];
      return;
    }
    try {
      sessions = await invoke<ChatSession[]>("list_create_chats", { path: $projectPath });
      if (activeId && !sessions.some((s) => s.id === activeId)) {
        activeId = sessions[0]?.id ?? null;
      }
    } catch {
      sessions = [];
    }
  }

  async function selectSession(id: string) {
    if (!$projectPath) return;
    activeId = id;
    try {
      const s = await invoke<ChatSession>("load_create_chat", {
        path: $projectPath,
        chatId: id,
      });
      messages = s.messages ?? [];
      draft = s.draft ?? null;
      brief = s.draft?.brief ?? brief;
      candidates = [];
      if (!sessions.some((x) => x.id === id)) {
        await refreshSessions();
      }
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function newChat() {
    if (!$projectPath) return;
    try {
      const s = await invoke<ChatSession>("new_create_chat", {
        path: $projectPath,
        title: "New chat",
      });
      await refreshSessions();
      activeId = s.id;
      messages = [];
      draft = null;
      brief = null;
      candidates = [];
      input = "";
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function deleteChat(id: string) {
    if (!$projectPath) return;
    if (!confirm("Delete this chat?")) return;
    try {
      await invoke("delete_create_chat", { path: $projectPath, chatId: id });
      if (activeId === id) {
        activeId = null;
        messages = [];
        draft = null;
        brief = null;
        candidates = [];
      }
      await refreshSessions();
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function persistDraft() {
    if (!$projectPath || !activeId) return;
    const session: ChatSession = {
      id: activeId,
      title: brief?.title || active?.title || "Create Mode",
      messages,
      draft,
      updatedAt: String(Date.now()),
    };
    try {
      await invoke("save_create_chat", { path: $projectPath, session });
      await refreshSessions();
    } catch {
      /* ignore */
    }
  }

  async function sendMessage(refine = false) {
    if (!$projectPath || !input.trim() || busy) return;
    const text = input.trim();
    busy = true;
    phase = "chat";
    try {
      const res = await invoke<{
        chatId: string;
        reply: string;
        brief?: PackBrief | null;
        candidates?: CandidateAddon[];
        session?: ChatSession;
      }>("create_mode_chat", {
        path: $projectPath,
        chatId: activeId,
        message: refine && brief
          ? `${text}\n\n(Please refine the existing pack brief.)`
          : text,
        targetCount,
        history: messages,
        existingBrief: brief,
      });
      activeId = res.chatId;
      if (res.brief) brief = res.brief;
      candidates = res.candidates ?? [];
      input = "";
      if (res.session) {
        messages = res.session.messages ?? [];
      } else {
        messages = [
          ...messages,
          { role: "user", content: text },
          { role: "assistant", content: res.reply },
        ];
      }
      await refreshSessions();
    } catch (e) {
      toasts.error(`${String(e)} ? try Quick assemble (no AI).`);
    } finally {
      busy = false;
      phase = "";
    }
  }

  async function quickAssemble() {
    const text = input.trim();
    if (!$projectPath || !text || busy) return;
    busy = true;
    phase = "plan";
    try {
      const res = await invoke<{
        chatId: string;
        reply?: string;
        brief: PackBrief;
        candidates?: CandidateAddon[];
        session?: ChatSession;
      }>("create_mode_quick_brief", {
        path: $projectPath,
        chatId: activeId,
        message: text,
        targetCount,
      });
      activeId = res.chatId;
      brief = res.brief;
      candidates = res.candidates ?? [];
      if (res.session) {
        messages = res.session.messages ?? [];
      } else {
        messages = [
          ...messages,
          { role: "user", content: text },
          {
            role: "assistant",
            content: res.reply ?? `Quick brief: ${res.brief.title}`,
          },
        ];
      }
      input = "";
      await refreshSessions();

      phase = "search";
      progressDone = 0;
      progressTotal = 1;
      progressCurrent = "Searching Modrinth...";
      draft = await invoke<PackDraft>("assemble_pack_draft", {
        path: $projectPath,
        brief: { ...brief, targetCount },
      });
      brief = draft.brief;
      messages = [
        ...messages,
        {
          role: "system",
          content: `Assembled draft: ${draft.mods.length} mods` +
            (draft.unresolved?.length
              ? ` (${draft.unresolved.length} must-have unresolved)`
              : ""),
        },
      ];
      await persistDraft();
      toasts.success(`Draft ready: ${draft.mods.length} mods`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
      phase = "";
      progressCurrent = "";
    }
  }

  async function buildDraft() {
    if (!$projectPath || !brief || busy) return;
    busy = true;
    phase = "search";
    progressDone = 0;
    progressTotal = 1;
    progressCurrent = "Searching Modrinth...";
    try {
      draft = await invoke<PackDraft>("assemble_pack_draft", {
        path: $projectPath,
        brief: { ...brief, targetCount },
      });
      brief = draft.brief;
      messages = [
        ...messages,
        {
          role: "system",
          content: `Assembled draft: ${draft.mods.length} mods` +
            (draft.unresolved?.length
              ? ` (${draft.unresolved.length} must-have unresolved)`
              : ""),
        },
      ];
      await persistDraft();
      toasts.success(`Draft ready: ${draft.mods.length} mods`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
      phase = "";
      progressCurrent = "";
    }
  }

  async function previewDraft() {
    if (!$projectPath || !draft || busy) return;
    busy = true;
    phase = "resolve";
    try {
      const res = await invoke<{
        checked: number;
        ok: number;
        failures: { slug: string; error: string }[];
      }>("preview_pack_draft", {
        path: $projectPath,
        draft,
        sampleLimit: Math.min(40, draft.mods.length),
      });
      const failN = res.failures?.length ?? 0;
      messages = [
        ...messages,
        {
          role: "system",
          content: `Preview: ${res.ok}/${res.checked} OK` +
            (failN ? `, ${failN} failed` : ""),
        },
      ];
      if (failN) toasts.error(`${failN} mods failed preview`);
      else toasts.success("Preview OK");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
      phase = "";
    }
  }

  async function confirmInstall() {
    if (!$projectPath || !draft?.mods?.length || busy) return;
    // Open DraftConfirmPanel — replace browser confirm.
    draftSelected = Object.fromEntries(
      draft.mods.map((m) => [m.projectId || m.slug, true]),
    );
    draftConfirmOpen = true;
  }

  async function executeDraftInstall() {
    if (!$projectPath || !draft?.mods?.length || busy) return;
    const mods = draft.mods.filter((m) => draftSelected[m.projectId || m.slug] !== false);
    if (!mods.length) {
      toasts.error("Select at least one mod to install");
      return;
    }
    draftConfirmOpen = false;
    const n = mods.length;
    const filteredDraft = { ...draft, mods };
    busy = true;
    phase = "install";
    progressDone = 0;
    progressTotal = n;
    progressCurrent = "Installing...";
    postInstallTrail = false;
    try {
      const res = await invoke<{ installedCount: number; requested: number }>(
        "install_pack_draft",
        {
          path: $projectPath,
          draft: filteredDraft,
          confirmed: true,
          side: "both",
        },
      );
      messages = [
        ...messages,
        {
          role: "system",
          content: `Installed ${res.installedCount} of ${res.requested} requested mods (deps may add more). Snapshot was created first.`,
        },
      ];
      await persistDraft();
      lastInstallCount = res.installedCount;
      postInstallTrail = true;
      toasts.success(`Installed ${res.installedCount} mods`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      busy = false;
      phase = "";
      progressCurrent = "";
    }
  }

  function openMods() {
    ideStageRequest.set("content");
    currentView = "ide";
  }

  function openResolve() {
    ideStageRequest.set("resolve");
    currentView = "ide";
  }

  onMount(async () => {
    unlisten = await listen<{
      phase: string;
      done: number;
      total: number;
      current: string;
    }>("create-mode://progress", (ev) => {
      phase = ev.payload.phase;
      progressDone = ev.payload.done;
      progressTotal = ev.payload.total;
      progressCurrent = ev.payload.current;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  $: {
    const p = $projectPath ?? "";
    if (p !== lastPath) {
      lastPath = p;
      activeId = null;
      messages = [];
      draft = null;
      brief = null;
      candidates = [];
      if (p) {
        void refreshSessions().then(() => {
          if (sessions[0]) void selectSession(sessions[0].id);
        });
      } else {
        sessions = [];
      }
    }
  }
</script>

{#if !$projectPath}
  <div class="chats empty">
    <MessagesSquare size={40} strokeWidth={1.5} />
    <h2>Create Mode</h2>
    <p>Open an instance to plan a PackBrief and assemble a Modrinth pack draft with AI.</p>
  </div>
{:else}
  <div class="chats">
    <aside class="sessions">
      <div class="sessions-head">
        <span>Chats</span>
        <button type="button" class="icon-btn" title="New chat" on:click={newChat}>
          <Plus size={16} />
        </button>
      </div>
      <div class="session-list">
        {#each sessions as s (s.id)}
          <div class="session-row" class:active={s.id === activeId}>
            <button type="button" class="session-main" on:click={() => selectSession(s.id)}>
              <span class="session-title">{s.title || "Untitled"}</span>
            </button>
            <button
              type="button"
              class="icon-btn danger"
              title="Delete"
              on:click={() => deleteChat(s.id)}
            >
              <Trash2 size={14} />
            </button>
          </div>
        {:else}
          <p class="muted pad">No chats yet. Start one below.</p>
        {/each}
      </div>
    </aside>

    <section class="thread">
      <div class="thread-meta">
        <Sparkles size={16} />
        <span>Create Mode ? {mcLabel} / {loaderLabel}</span>
        <label class="target">
          Target
          <input type="range" min="40" max="120" step="5" bind:value={targetCount} disabled={busy} />
          <strong>{targetCount}</strong>
        </label>
      </div>

      <div class="messages">
        {#if messages.length === 0}
          <div class="welcome">
            <h3>Describe the pack you want</h3>
            <p>
              Example: "Tech + airplanes for NeoForge 1.21.1, ~80 mods, Create required."
              Plan builds a PackBrief (search JSON + must-haves from hub co-occurrence).
              Quick assemble builds a Modrinth draft in one step. Install only after you confirm.
            </p>
          </div>
        {/if}
        {#each messages as m, i (m.createdAt ?? `${m.role}-${i}-${m.content.slice(0, 48)}`)}
          <div class="bubble" class:user={m.role === "user"} class:assistant={m.role === "assistant"} class:system={m.role === "system"}>
            {m.content}
          </div>
        {/each}
        {#if busy && phase}
          <div class="bubble system progress">
            <span class="spin"><Loader2 size={14} /></span>
            {phase}: {progressCurrent || "..."}
            {#if progressTotal > 0}
              ({progressDone}/{progressTotal})
            {/if}
          </div>
        {/if}
      </div>

      <div class="composer">
        <textarea
          rows="2"
          placeholder="Pack brief..."
          bind:value={input}
          disabled={busy}
          on:keydown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void sendMessage(false);
            }
          }}
        ></textarea>
        <div class="actions">
          <button type="button" class="btn primary" disabled={busy || !input.trim()} on:click={() => sendMessage(false)}>
            <Send size={14} /> Plan
          </button>
          <button type="button" class="btn" disabled={busy || !input.trim()} on:click={() => void quickAssemble()}>
            Quick assemble
          </button>
          <button type="button" class="btn" disabled={busy || !brief || !input.trim()} on:click={() => sendMessage(true)}>
            Refine
          </button>
          <button type="button" class="btn" disabled={busy || !brief} on:click={buildDraft}>
            <Package size={14} /> Build draft
          </button>
          <button type="button" class="btn" disabled={busy || !draft?.mods?.length} on:click={previewDraft}>
            Preview
          </button>
          <button type="button" class="btn accent" disabled={busy || !draft?.mods?.length} on:click={confirmInstall}>
            <CheckCircle2 size={14} /> Confirm install
          </button>
          <button type="button" class="btn ghost" on:click={openMods}>Open in Content</button>
        </div>
        {#if postInstallTrail}
          <div class="post-trail">
            Installed {lastInstallCount} mods.
            <button type="button" class="btn ghost mini" on:click={openMods}><Package size={12} /> Content</button>
            <button type="button" class="btn ghost mini" on:click={openResolve}><GitGraph size={12} /> Resolve</button>
          </div>
        {/if}
      </div>
    </section>

    <aside class="draft">
      <div class="draft-head">
        <span>Pack draft</span>
        <strong>{draft?.mods?.length ?? 0}</strong>
      </div>
      {#if brief}
        <div class="brief-card">
          <div class="brief-title">{brief.title}</div>
          <div class="muted">{brief.mcVersion} ? {brief.loader} ? target {brief.targetCount}</div>
          {#if brief.categories?.length}
            <div class="cats">
              {#each brief.categories as c (c.id)}
                <span>{c.id}:{c.count}</span>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <p class="muted pad">Plan a brief first, then Build draft.</p>
      {/if}
      {#if candidates.length}
        <div class="candidates">
          <div class="draft-head"><span>Catalog candidates</span><strong>{candidates.length}</strong></div>
          {#each candidates.slice(0, 12) as c (c.slug)}
            <div class="mod-row compact">
              <div class="mod-name">{c.name}</div>
              <div class="mod-meta">
                <code>{c.slug}</code>
                <span class="muted">{c.source}</span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
      <div class="mod-table">
        {#if draft?.mods?.length}
          {#each draft.mods as m (m.projectId || m.slug)}
            <div class="mod-row">
              <div class="mod-name">{m.name}</div>
              <div class="mod-meta">
                <code>{m.slug}</code>
                <span class="muted">{m.category}</span>
              </div>
              <div class="mod-reason muted">{m.reason}</div>
            </div>
          {/each}
        {/if}
      </div>
      {#if draft?.unresolved?.length}
        <div class="unresolved">
          Must-have unresolved: {draft.unresolved.join(", ")}
        </div>
      {/if}
    </aside>
  </div>
{/if}

{#if draftConfirmOpen && draft}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    on:click|self={() => (draftConfirmOpen = false)}
    on:keydown={() => {}}
  >
    <div class="modal draft-confirm" role="dialog" aria-modal="true">
      <div class="draft-confirm-head">
        <h2>Confirm pack install</h2>
        <p>
          Install <strong>{draftConfirmCount}</strong> of {draft.mods.length} mods (+ dependencies) into this instance.
          A snapshot will be created first. Uncheck anything you do not want.
        </p>
      </div>
      <div class="draft-confirm-list">
        {#each draft.mods as m (m.projectId || m.slug)}
          <label class="draft-confirm-row">
            <input
              type="checkbox"
              checked={draftSelected[m.projectId || m.slug] !== false}
              on:change={(e) => {
                draftSelected[m.projectId || m.slug] = e.currentTarget.checked;
                draftSelected = { ...draftSelected };
              }}
            />
            <div>
              <strong>{m.name}</strong>
              <code>{m.slug}</code>
              <span class="muted">{m.category}</span>
            </div>
          </label>
        {/each}
      </div>
      {#if draft.unresolved?.length}
        <p class="warn">Unresolved must-haves: {draft.unresolved.join(", ")}</p>
      {/if}
      <div class="draft-confirm-actions">
        <button type="button" class="btn ghost" on:click={() => (draftConfirmOpen = false)}>Cancel</button>
        <button
          type="button"
          class="btn accent"
          disabled={busy || draftConfirmCount === 0}
          on:click={executeDraftInstall}
        >
          Install {draftConfirmCount} mod{draftConfirmCount === 1 ? "" : "s"} (snapshot first)
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .chats {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr) 280px;
    gap: 0;
    height: calc(100vh - 88px);
    min-height: 420px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color, #2a2f3a);
    border-radius: 12px;
    overflow: hidden;
  }
  .chats.empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--text-secondary, #9aa3b5);
    padding: 48px;
  }
  .chats.empty h2 {
    margin: 0;
    color: var(--text-primary, #e8ecf4);
  }
  .sessions,
  .draft {
    background: var(--bg-primary);
    border-right: 1px solid var(--border-color, #2a2f3a);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .draft {
    border-right: none;
    border-left: 1px solid var(--border-color, #2a2f3a);
  }
  .sessions-head,
  .draft-head,
  .thread-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color, #2a2f3a);
    font-size: 13px;
    color: var(--text-secondary, #9aa3b5);
  }
  .draft-head {
    justify-content: space-between;
  }
  .draft-head strong {
    color: var(--text-primary, #e8ecf4);
  }
  .session-list,
  .mod-table,
  .messages {
    flex: 1;
    overflow: auto;
    min-height: 0;
  }
  .session-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
  }
  .session-row.active {
    background: var(--bg-tertiary);
  }
  .session-main {
    flex: 1;
    text-align: left;
    background: none;
    border: none;
    color: var(--text-primary, #e8ecf4);
    padding: 8px;
    cursor: pointer;
    font-size: 13px;
  }
  .session-title {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--text-secondary, #9aa3b5);
    cursor: pointer;
    padding: 6px;
    border-radius: 6px;
  }
  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary, #e8ecf4);
  }
  .icon-btn.danger:hover {
    color: #f87171;
  }
  .thread {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .thread-meta {
    flex-wrap: wrap;
  }
  .target {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .target input {
    width: 100px;
  }
  .messages {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .welcome {
    max-width: 520px;
    color: var(--text-secondary, #9aa3b5);
  }
  .welcome h3 {
    margin: 0 0 8px;
    color: var(--text-primary, #e8ecf4);
    font-weight: 600;
  }
  .bubble {
    max-width: 85%;
    padding: 10px 12px;
    border-radius: 10px;
    font-size: 13px;
    line-height: 1.45;
    white-space: pre-wrap;
  }
  .bubble.user {
    align-self: flex-end;
    background: var(--bg-elevated);
    color: var(--text-primary, #e8ecf4);
  }
  .bubble.assistant {
    align-self: flex-start;
    background: var(--bg-tertiary);
    color: var(--text-primary, #e8ecf4);
  }
  .bubble.system {
    align-self: center;
    background: transparent;
    color: var(--text-secondary, #9aa3b5);
    font-size: 12px;
    border: 1px dashed var(--border-color, #2a2f3a);
  }
  .candidates {
    border-top: 1px solid var(--border-color, #2a2f3a);
  }
  .mod-row.compact {
    padding-top: 4px;
    padding-bottom: 4px;
  }
  .bubble.progress {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .composer {
    border-top: 1px solid var(--border-color, #2a2f3a);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .composer textarea {
    width: 100%;
    resize: vertical;
    min-height: 56px;
    background: var(--bg-primary);
    color: var(--text-primary, #e8ecf4);
    border: 1px solid var(--border-color, #2a2f3a);
    border-radius: 8px;
    padding: 10px;
    font: inherit;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border-color, #2a2f3a);
    background: var(--bg-tertiary);
    color: var(--text-primary, #e8ecf4);
    border-radius: 8px;
    padding: 7px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .btn.primary {
    background: #1bd96a;
    border-color: #1bd96a;
    color: #0b1a10;
    font-weight: 600;
  }
  .btn.accent {
    background: #3b82f6;
    border-color: #3b82f6;
    color: #fff;
  }
  .btn.ghost {
    background: transparent;
  }
  .brief-card {
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color, #2a2f3a);
  }
  .brief-title {
    font-weight: 600;
    color: var(--text-primary, #e8ecf4);
    margin-bottom: 4px;
  }
  .cats {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 8px;
  }
  .cats span {
    font-size: 11px;
    background: var(--bg-tertiary);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--text-secondary, #9aa3b5);
  }
  .mod-row {
    padding: 8px 12px;
    border-bottom: 1px solid var(--border-color, #2a2f3a);
  }
  .mod-name {
    font-size: 13px;
    color: var(--text-primary, #e8ecf4);
  }
  .mod-meta {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 11px;
  }
  .mod-meta code {
    font-size: 11px;
    color: #7dd3fc;
  }
  .mod-reason {
    font-size: 11px;
    margin-top: 2px;
  }
  .unresolved {
    padding: 10px 12px;
    font-size: 11px;
    color: #fbbf24;
    border-top: 1px solid var(--border-color, #2a2f3a);
  }
  .muted {
    color: var(--text-secondary, #9aa3b5);
  }
  .pad {
    padding: 12px;
    font-size: 13px;
  }
  .spin {
    display: inline-flex;
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .post-trail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 8px;
    font-size: 12px;
    color: var(--text-secondary, #9aa3b5);
  }
  .btn.mini { padding: 4px 8px; font-size: 11px; }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    padding: 16px;
  }
  .draft-confirm {
    width: min(520px, 100%);
    max-height: 85vh;
    overflow: auto;
    padding: 16px 18px;
    border-radius: 12px;
    border: 1px solid var(--border-color, #2a2f3a);
    background: var(--bg-secondary, #151922);
  }
  .draft-confirm-head h2 { margin: 0 0 6px; font-size: 16px; }
  .draft-confirm-head p { margin: 0 0 12px; font-size: 13px; color: var(--text-secondary, #9aa3b5); }
  .draft-confirm-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 40vh;
    overflow: auto;
    margin-bottom: 12px;
  }
  .draft-confirm-row {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px;
    border-radius: 8px;
    border: 1px solid var(--border-color, #2a2f3a);
    cursor: pointer;
  }
  .draft-confirm-row strong { display: block; font-size: 13px; }
  .draft-confirm-row code { font-size: 11px; color: #7dd3fc; margin-right: 6px; }
  .draft-confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .warn { color: #fbbf24; font-size: 12px; margin: 0 0 10px; }
  @media (max-width: 1100px) {
    .chats {
      grid-template-columns: 1fr;
      height: auto;
    }
    .sessions,
    .draft {
      max-height: 220px;
      border-right: none;
      border-bottom: 1px solid var(--border-color, #2a2f3a);
    }
    .draft {
      border-left: none;
      max-height: 320px;
    }
  }
</style>
