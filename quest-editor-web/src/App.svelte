<script lang="ts">
  import type { QuestChapter, QuestChapterGroup, QuestData, QuestValidationIssue, QuestBook } from "./lib/store";
  import { loadQuestBookFromSnbt, exportChapterSnbt, validateQuestBook, saveToStorage, loadFromStorage, clearStorage } from "./lib/store";
  import ChapterRail from "./components/quests/ChapterRail.svelte";
  import QuestCanvas from "./components/quests/QuestCanvas.svelte";
  import QuestInspector from "./components/quests/QuestInspector.svelte";

  let chapters = $state<QuestChapter[]>([]);
  let chapterGroups = $state<QuestChapterGroup[]>([]);
  let bookTitle = $state<string | null>(null);
  let selectedChapter = $state("");
  let selectedQuest = $state<QuestData | null>(null);
  let validationIssues = $state<QuestValidationIssue[]>([]);
  let dirtyChapters = $state(new Set<string>());
  let fitToken = $state(0);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let snbtFiles = $state<Map<string, string>>(new Map());
  let showImportHelp = $state(false);

  // Load from localStorage on mount
  $effect(() => {
    const saved = loadFromStorage();
    if (saved && saved.chapters.length > 0) {
      chapters = saved.chapters;
      chapterGroups = saved.chapterGroups ?? [];
      bookTitle = saved.title ?? null;
      if (chapters.length > 0) selectedChapter = chapters[0].id;
      validationIssues = validateQuestBook({ chapters, chapterGroups });
    }
  });

  // Auto-save to localStorage
  $effect(() => {
    if (chapters.length > 0 || bookTitle) {
      saveToStorage({ chapters, chapterGroups, title: bookTitle });
    }
  });

  const chapterQuests = $derived(chapters.find((c) => c.id === selectedChapter)?.quests ?? []);
  const selectedChapterObj = $derived(chapters.find((c) => c.id === selectedChapter) ?? null);
  const totalQuests = $derived(chapters.reduce((n, c) => n + c.quests.length, 0));

  function markDirty(chapterId: string) {
    dirtyChapters = new Set([...dirtyChapters, chapterId]);
    chapters = [...chapters];
  }

  function selectChapter(id: string) {
    selectedChapter = id;
    selectedQuest = null;
    fitToken += 1;
  }

  function createChapter() {
    const n: QuestChapter = {
      id: `chapter_${Date.now().toString(16)}`,
      title: `Chapter ${chapters.length + 1}`,
      quests: [],
      extras: {},
      orderIndex: chapters.length,
      filename: `chapter_${chapters.length + 1}`,
    };
    chapters = [...chapters, n];
    selectChapter(n.id);
    markDirty(n.id);
  }

  function addQuestAt(x: number, y: number) {
    if (!selectedChapter) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    const newQ: QuestData = {
      id: crypto.randomUUID().replace(/-/g, "").slice(0, 16).toUpperCase(),
      title: "New Quest",
      description: [],
      x,
      y,
      dependencies: [],
      tasks: [{ id: crypto.randomUUID().replace(/-/g, "").slice(0, 12).toUpperCase(), type: "checkmark" }],
      rewards: [],
      optional: false,
      size: 1,
      extras: {},
    };
    ch.quests = [...ch.quests, newQ];
    markDirty(selectedChapter);
    selectedQuest = newQ;
  }

  function removeQuest(q: QuestData) {
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    ch.quests = ch.quests.filter((x) => x.id !== q.id);
    for (const other of ch.quests) {
      other.dependencies = other.dependencies.filter((d) => d !== q.id);
    }
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = null;
  }

  function moveQuest(q: QuestData, x: number, y: number) {
    q.x = x;
    q.y = y;
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = q;
  }

  function wouldCycle(questId: string, depId: string, list: QuestData[]): boolean {
    const visited = new Set<string>();
    function dfs(id: string): boolean {
      if (id === questId) return true;
      if (visited.has(id)) return false;
      visited.add(id);
      const q = list.find((x) => x.id === id);
      if (q) {
        for (const d of q.dependencies) {
          if (dfs(d)) return true;
        }
      }
      return false;
    }
    return dfs(depId);
  }

  function addDep(q: QuestData, depId: string) {
    if (!depId || q.dependencies.includes(depId) || depId === q.id) return;
    const list = chapters.find((c) => c.id === selectedChapter)?.quests ?? [];
    if (wouldCycle(q.id, depId, list)) {
      error = "Dependency would create a cycle";
      return;
    }
    error = null;
    q.dependencies = [...q.dependencies, depId];
    markDirty(selectedChapter);
    selectedQuest = q;
  }

  function removeDep(q: QuestData, depId: string) {
    q.dependencies = q.dependencies.filter((d) => d !== depId);
    markDirty(selectedChapter);
    selectedQuest = q;
  }

  function linkQuests(fromId: string, toDepId: string) {
    const ch = chapters.find((c) => c.id === selectedChapter);
    const q = ch?.quests.find((x) => x.id === fromId);
    if (q) addDep(q, toDepId);
  }

  function deleteChapter(id: string) {
    chapters = chapters.filter((c) => c.id !== id);
    dirtyChapters = new Set([...dirtyChapters].filter((x) => x !== id));
    if (selectedChapter === id) {
      selectedChapter = chapters[0]?.id ?? "";
      selectedQuest = null;
    }
  }

  function moveChapter(id: string, dir: -1 | 1) {
    const idx = chapters.findIndex((c) => c.id === id);
    if (idx < 0) return;
    const j = idx + dir;
    if (j < 0 || j >= chapters.length) return;
    const next = [...chapters];
    [next[idx], next[j]] = [next[j]!, next[idx]!];
    next.forEach((c, i) => {
      c.orderIndex = i;
      markDirty(c.id);
    });
    chapters = next;
  }

  function exportAll() {
    const parts: string[] = [];
    for (const ch of chapters) {
      parts.push(exportChapterSnbt(ch));
    }
    const blob = new Blob([parts.join("\n\n")], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "quest_chapters.snbt";
    a.click();
    URL.revokeObjectURL(url);
    message = `Exported ${chapters.length} chapter(s)`;
  }

  function exportChapter(chapterId: string) {
    const ch = chapters.find((c) => c.id === chapterId);
    if (!ch) return;
    const snbt = exportChapterSnbt(ch);
    const blob = new Blob([snbt], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${ch.filename ?? ch.id}.snbt`;
    a.click();
    URL.revokeObjectURL(url);
    message = `Exported "${ch.title}"`;
  }

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    showImportHelp = false;
    const items = e.dataTransfer?.items;
    if (!items) return;

    const files: Promise<{ path: string; content: string }>[] = [];
    for (const item of Array.from(items)) {
      if (item.kind === "file") {
        const entry = item.webkitGetAsEntry?.();
        if (entry) {
          files.push(readEntryRecursive(entry, ""));
        }
      }
    }

    Promise.all(files).then((results) => {
      const fileMap = new Map<string, string>();
      for (const r of results) {
        if (r.path.endsWith(".snbt")) {
          fileMap.set(r.path, r.content);
        }
      }
      if (fileMap.size === 0) {
        error = "No .snbt files found in drop";
        return;
      }
      snbtFiles = fileMap;
      const book = loadQuestBookFromSnbt(fileMap);
      if (book.chapters.length === 0) {
        error = "Could not parse any chapters from dropped files";
        return;
      }
      chapters = book.chapters;
      chapterGroups = book.chapterGroups;
      bookTitle = book.title;
      selectedChapter = chapters[0].id;
      selectedQuest = null;
      dirtyChapters = new Set();
      validationIssues = validateQuestBook(book);
      message = `Loaded ${chapters.length} chapter(s) from ${fileMap.size} file(s)`;
    });
  }

  function readEntryRecursive(entry: FileSystemEntry, basePath: string): Promise<{ path: string; content: string }> {
    return new Promise((resolve, reject) => {
      if (entry.isFile) {
        (entry as FileSystemFileEntry).file((file) => {
          const reader = new FileReader();
          reader.onload = () => resolve({ path: basePath + file.name, content: reader.result as string });
          reader.onerror = reject;
          reader.readAsText(file);
        });
      } else if (entry.isDirectory) {
        const dir = entry as FileSystemDirectoryEntry;
        const reader = dir.createReader();
        const entries: FileSystemEntry[] = [];
        function readBatch() {
          reader.readEntries((batch) => {
            if (batch.length === 0) {
              Promise.all(entries.map((e) => readEntryRecursive(e, basePath + dir.name + "/")))
                .then((results) => resolve(results.length === 1 ? results[0]! : { path: "", content: results.map((r) => r.content).join("\n") }))
                .catch(reject);
            } else {
              entries.push(...batch);
              readBatch();
            }
          }, reject);
        }
        readBatch();
      } else {
        resolve({ path: "", content: "" });
      }
    });
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    showImportHelp = true;
  }

  function handleDragLeave() {
    showImportHelp = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      exportAll();
    }
  }

  function clearAll() {
    if (!confirm("Clear all chapters? This cannot be undone.")) return;
    chapters = [];
    chapterGroups = [];
    bookTitle = null;
    selectedChapter = "";
    selectedQuest = null;
    dirtyChapters = new Set();
    clearStorage();
    message = "Cleared";
  }

  function handleFileImport(e: Event) {
    const input = e.target as HTMLInputElement;
    const files = input.files;
    if (!files || files.length === 0) return;

    const fileMap = new Map<string, string>();
    let pending = files.length;

    for (const file of Array.from(files)) {
      const reader = new FileReader();
      reader.onload = () => {
        fileMap.set(file.name, reader.result as string);
        pending--;
        if (pending === 0) {
          const book = loadQuestBookFromSnbt(fileMap);
          if (book.chapters.length > 0) {
            chapters = book.chapters;
            chapterGroups = book.chapterGroups;
            bookTitle = book.title;
            selectedChapter = chapters[0]?.id ?? "";
            selectedQuest = null;
            dirtyChapters = new Set();
            validationIssues = validateQuestBook(book);
            message = `Loaded ${chapters.length} chapter(s)`;
          } else {
            error = "Could not parse any chapters from files";
          }
        }
      };
      reader.readAsText(file);
    }
    input.value = "";
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="app"
  ondrop={handleDrop}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  role="application"
>
  <header class="toolbar">
    <div class="toolbar-left">
      <h1 class="logo">Quest Editor</h1>
      {#if bookTitle}
        <span class="book-title">{bookTitle}</span>
      {/if}
    </div>
    <div class="toolbar-center">
      <span class="stat">{chapters.length} chapters · {totalQuests} quests</span>
      {#if validationIssues.length > 0}
        <span class="stat warn">{validationIssues.length} issues</span>
      {/if}
      {#if dirtyChapters.size > 0}
        <span class="dirty">{dirtyChapters.size} unsaved</span>
      {/if}
    </div>
    <div class="toolbar-right">
      <button type="button" class="btn ghost" onclick={createChapter}>+ Chapter</button>
      <button type="button" class="btn ghost" onclick={exportAll} disabled={chapters.length === 0}>Export All</button>
      <button type="button" class="btn danger" onclick={clearAll} disabled={chapters.length === 0}>Clear</button>
    </div>
  </header>

  {#if error}
    <div class="notice error" onclick={() => (error = null)} role="alert">{error}</div>
  {/if}
  {#if message}
    <div class="notice success" onclick={() => (message = null)} role="status">{message}</div>
  {/if}

  {#if showImportHelp}
    <div class="drop-overlay">
      <div class="drop-hint">Drop .snbt files or folders here</div>
    </div>
  {/if}

  {#if chapters.length === 0}
    <div class="empty">
      <div class="empty-icon">Quests</div>
      <h2>Start a quest line</h2>
      <p>Drop FTB Quests <code>.snbt</code> files here, or create a new chapter.</p>
      <div class="empty-actions">
        <button type="button" class="btn primary" onclick={createChapter}>+ Create first chapter</button>
        <label class="btn ghost file-label">
          Import .snbt files
          <input type="file" accept=".snbt" multiple onchange={handleFileImport} hidden />
        </label>
      </div>
    </div>
  {:else}
    <div class="workspace">
      <ChapterRail
        {chapters}
        {chapterGroups}
        {selectedChapter}
        dirtyIds={dirtyChapters}
        onSelect={selectChapter}
        onCreate={createChapter}
        onDirty={markDirty}
        onDelete={deleteChapter}
        onMove={moveChapter}
        onExport={exportChapter}
      />
      <div class="canvas-area">
        <QuestCanvas
          quests={chapterQuests}
          selectedId={selectedQuest?.id ?? null}
          issues={validationIssues}
          {fitToken}
          emptyHint="Double-click to add a quest"
          onSelect={(q) => (selectedQuest = q)}
          onMove={moveQuest}
          onAddAt={addQuestAt}
          onLink={linkQuests}
        />
      </div>
      {#if selectedQuest}
        <QuestInspector
          quest={selectedQuest}
          {chapterQuests}
          issues={validationIssues}
          onDirty={() => markDirty(selectedChapter)}
          onRemove={() => { if (selectedQuest) removeQuest(selectedQuest); }}
          onAddDep={(id) => { if (selectedQuest) addDep(selectedQuest, id); }}
          onRemoveDep={(id) => { if (selectedQuest) removeDep(selectedQuest, id); }}
        />
      {:else if selectedChapterObj}
        <div class="chapter-info">
          <h3>{selectedChapterObj.title}</h3>
          <p>{selectedChapterObj.quests.length} quests</p>
          <p class="hint">Double-click canvas to add quest</p>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  :global(*) { box-sizing: border-box; margin: 0; padding: 0; }
  :global(body) {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: #1a1a1e;
    color: #e8e8e8;
    overflow: hidden;
    height: 100vh;
  }
  :global(button) { cursor: pointer; border-radius: 2px; font-weight: 600; }
  :global(input), :global(select), :global(textarea) {
    border-radius: 2px;
    border: 1px solid #3a3a42;
    background: #1a1a1e;
    color: #e8e8e8;
    padding: 6px 8px;
    font-size: 12px;
  }
  :global(input:focus), :global(select:focus), :global(textarea:focus) {
    outline: none;
    border-color: #3db8a8;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #1a1a1e;
    position: relative;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    background: #212126;
    border-bottom: 1px solid #3a3a42;
    flex-shrink: 0;
    gap: 12px;
    z-index: 10;
  }
  .toolbar-left { display: flex; align-items: center; gap: 12px; }
  .toolbar-center { display: flex; align-items: center; gap: 16px; }
  .toolbar-right { display: flex; align-items: center; gap: 8px; }
  .logo {
    font-size: 14px;
    font-weight: 800;
    color: #f2c94c;
    letter-spacing: 0.02em;
  }
  .book-title {
    font-size: 12px;
    color: #9a9aa0;
  }
  .stat { font-size: 11px; color: #9a9aa0; }
  .stat.warn { color: #fbbf24; }
  .dirty {
    font-size: 10px;
    color: #f2c94c;
    padding: 2px 8px;
    background: rgba(242, 201, 76, 0.12);
    border: 1px solid rgba(242, 201, 76, 0.25);
    border-radius: 2px;
  }
  .btn {
    padding: 6px 12px;
    border: 1px solid #3a3a42;
    background: rgba(0,0,0,0.25);
    color: #e8e8e8;
    font-size: 12px;
    font-weight: 600;
    border-radius: 2px;
  }
  .btn:hover:not(:disabled) { border-color: #3db8a8; background: rgba(61,184,168,0.12); }
  .btn:disabled { opacity: 0.4; cursor: default; }
  .btn.primary { border-color: #55c95a; background: rgba(85,201,90,0.18); color: #55c95a; }
  .btn.primary:hover { background: rgba(85,201,90,0.28); }
  .btn.danger { color: #f87171; }
  .btn.danger:hover { border-color: #f87171; background: rgba(248,113,113,0.1); }
  .file-label { display: inline-flex; align-items: center; cursor: pointer; }

  .notice {
    padding: 8px 16px;
    font-size: 12px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .notice.error { background: rgba(239,68,68,0.1); color: #fecaca; border-bottom: 1px solid rgba(239,68,68,0.3); }
  .notice.success { background: rgba(85,201,90,0.1); color: #55c95a; border-bottom: 1px solid rgba(85,201,90,0.3); }

  .drop-overlay {
    position: absolute;
    inset: 0;
    background: rgba(61,184,168,0.15);
    border: 3px dashed #3db8a8;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    pointer-events: none;
  }
  .drop-hint {
    font-size: 18px;
    font-weight: 700;
    color: #3db8a8;
    padding: 24px 48px;
    background: rgba(26,26,30,0.9);
    border-radius: 8px;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: #9a9aa0;
  }
  .empty-icon { font-size: 48px; color: #f2c94c; }
  .empty h2 { font-size: 20px; color: #f2c94c; }
  .empty p { font-size: 13px; max-width: 400px; text-align: center; line-height: 1.5; }
  .empty code { background: rgba(255,255,255,0.08); padding: 2px 6px; border-radius: 3px; }
  .empty-actions { display: flex; gap: 12px; margin-top: 8px; }

  .workspace {
    flex: 1;
    display: flex;
    min-height: 0;
    overflow: hidden;
  }
  .canvas-area {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .chapter-info {
    width: 260px;
    background: #212126;
    border-left: 1px solid #3a3a42;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .chapter-info h3 { font-size: 14px; color: #f2c94c; }
  .chapter-info p { font-size: 12px; color: #9a9aa0; }
  .hint { font-size: 11px; color: #9a9aa0; }
</style>
