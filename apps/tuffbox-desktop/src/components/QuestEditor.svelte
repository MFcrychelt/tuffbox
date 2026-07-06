<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ScrollText, RefreshCw, Plus, Trash2 } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type QuestData = { id: string; title: string; subtitle?: string|null; description: string[]; x: number; y: number; icon?: string|null; dependencies: string[]; tasks: any[]; rewards: any[]; optional: boolean };
  type Chapter = { id: string; title: string; quests: QuestData[] };

  let chapters: Chapter[] = [];
  let loading = false; let error: string|null = null; let message: string|null = null;
  let selectedChapter = ""; let selectedQuest: QuestData|null = null;

  async function load() {
    if (!$projectPath) return; loading = true; error = null;
    try {
      const files: {path:string,name:string,size:number}[] = await invoke("list_config_files", { path: $projectPath });
      const snbtFiles = files.filter(f => f.path.includes("ftbquests") && f.name.endsWith(".snbt"));
      if (snbtFiles.length > 0) {
        let loadedChapters: Chapter[] = [];
        for (const f of snbtFiles.slice(0, 20)) {
          try {
            const content: string = await invoke("read_config_file", { path: $projectPath, relativePath: f.path });
            if (content.includes('"quests"') || content.includes('quests:')) {
              const id = f.name.replace('.snbt','');
              const title = content.match(/title:\s*"([^"]+)"/)?.[1] || f.name;
              loadedChapters.push({ id, title, quests: [] });
            }
          } catch {}
        }
        if (loadedChapters.length > 0) chapters = loadedChapters;
      }
    } catch(e) { error = String(e); }
    finally { loading = false; }
  }

  function addQuest() {
    if (!selectedChapter) return;
    const ch = chapters.find(c => c.id === selectedChapter);
    if (!ch) return;
    const newQ: QuestData = { id: crypto.randomUUID().replace(/-/g,'').slice(0,16), title: "New Quest", description: [], x: 100, y: 100, dependencies: [], tasks: [], rewards: [], optional: false };
    ch.quests = [...ch.quests, newQ]; chapters = [...chapters]; selectedQuest = newQ;
  }

  function removeQuest(q: QuestData) {
    const ch = chapters.find(c => c.id === selectedChapter); if (!ch) return;
    ch.quests = ch.quests.filter(x => x.id !== q.id); chapters = [...chapters];
    if (selectedQuest?.id === q.id) selectedQuest = null;
  }

  function addDep(q: QuestData, depId: string) {
    if (!depId.trim() || q.dependencies.includes(depId)) return;
    q.dependencies = [...q.dependencies, depId.trim()]; chapters = [...chapters];
  }

  $: chapterQuests = chapters.find(c => c.id === selectedChapter)?.quests ?? [];
  $: if ($projectPath) load();
</script>

<div class="qe">
  <div class="qe-tb"><div class="qe-title"><ScrollText size={18} /> Quest editor</div>
    <button class="ghost" on:click={load} disabled={!$projectPath || loading}><RefreshCw size={16} class={loading?"spin":""} /></button></div>
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice success">{message}</div>{/if}
  {#if !$projectPath}<div class="empty">Open a project to edit quests.</div>
  {:else if chapters.length === 0}
    <div class="empty"><ScrollText size={40} /><h3>No FTB Quests chapters found</h3>
      <p>Place .snbt chapter files in config/ftbquests/quests/chapters/</p>
      <p class="hint">TuffBox parses them automatically — no Minecraft needed.</p>
      <button on:click={() => { chapters = [{ id: "new", title: "New Chapter", quests: [] }]; selectedChapter = "new"; }}><Plus size={16} /> Create chapter</button></div>
  {:else}
    <div class="qe-lay">
      <aside class="qe-side"><h3>Chapters</h3>
        {#each chapters as ch}
          <button class="qe-ch-row" class:sel={selectedChapter===ch.id} on:click={()=>(selectedChapter=ch.id,selectedQuest=null)}><strong>{ch.title}</strong><span>{ch.quests.length} quests</span></button>
        {/each}
        <button class="secondary qe-add-ch" on:click={() => { const n = { id: "ch"+Date.now(), title: "Chapter "+(chapters.length+1), quests: [] }; chapters = [...chapters, n]; selectedChapter = n.id; }}><Plus size={14} /> Add</button></aside>
      <section class="qe-main">
        {#if selectedChapter}
          <div class="qe-main-h"><h3>{chapters.find(c=>c.id===selectedChapter)?.title}</h3><button on:click={addQuest}><Plus size={14} /> Add quest</button></div>
          {#if chapterQuests.length === 0}<div class="empty compact">No quests yet.</div>
          {:else}
            <div class="qe-grid">
              {#each chapterQuests as q}
                <button class="qe-card" class:sel={selectedQuest?.id===q.id} on:click={()=>(selectedQuest=q)}>
                  <strong>{q.title}</strong><div class="qe-card-meta"><span>{q.tasks.length} tasks</span><span>{q.dependencies.length} deps</span>{#if q.optional}<span class="opt">optional</span>{/if}</div></button>
              {/each}</div>
          {/if}
        {:else}<div class="empty compact">Select a chapter.</div>{/if}</section>
      {#if selectedQuest}
        <aside class="qe-detail">
          <div class="qe-det-h"><h3>Edit: {selectedQuest.title}</h3><button class="ico danger" on:click={() => removeQuest(selectedQuest)}><Trash2 size={14} /></button></div>
          <div class="qe-det-fields">
            <label>Title<input bind:value={selectedQuest.title} on:input={()=>(chapters=[...chapters])} /></label>
            <label>Subtitle<input bind:value={selectedQuest.subtitle} on:input={()=>(chapters=[...chapters])} placeholder="Optional" /></label>
            <label class="checkbox"><input type="checkbox" bind:checked={selectedQuest.optional} on:change={()=>(chapters=[...chapters])} /> Optional quest</label>
            <label>X/Y position<div class="xy"><input type="number" bind:value={selectedQuest.x} /><input type="number" bind:value={selectedQuest.y} /></div></label></div>
          <h4>Tasks ({selectedQuest.tasks.length})</h4>
          <div class="qe-tasks">{#each selectedQuest.tasks as task,i}<div class="qe-task-row"><code>{task.type||"item"}</code><span>{task.title||"Task "+(i+1)}</span></div>{/each}</div>
          <h4>Dependencies</h4>
          <div class="qe-deps">{#each selectedQuest.dependencies as dep}<span class="dep-tag">{dep} <button class="dep-rm" on:click={() => { selectedQuest.dependencies = selectedQuest.dependencies.filter(d=>d!==dep); chapters=[...chapters]; }}>x</button></span>{/each}
            <div class="dep-add"><input placeholder="Quest ID..." on:keydown={(e) => { if(e.key==="Enter"){ addDep(selectedQuest!, (e.target as HTMLInputElement).value); (e.target as HTMLInputElement).value = ""; }}} /></div></div></aside>
      {/if}</div>
    <div class="qe-footer">
      <p class="hint">Changes are tracked in-memory. Export to .snbt via File → Save Quest Book (coming soon).</p>
    </div>
  {/if}
</div>

<style>
  .qe{max-width:none;width:100%}.qe-tb,.qe-title{display:flex;align-items:center;gap:10px}.qe-tb{justify-content:space-between;margin-bottom:16px}.qe-title{color:var(--text-secondary);font-weight:700}
  .notice{padding:12px 14px;border-radius:var(--border-radius-lg);margin-bottom:14px;border:1px solid var(--border-color)}.notice.error{color:#fecaca;background:rgba(239,68,68,.08);border-color:rgba(239,68,68,.28)}.notice.success{color:var(--accent-primary);background:rgba(27,217,106,.08);border-color:rgba(27,217,106,.25)}
  .empty{color:var(--text-muted);padding:80px;text-align:center;background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);display:flex;flex-direction:column;align-items:center;gap:12px}.empty.compact{padding:40px}.hint{font-size:11px}
  .qe-lay{display:grid;grid-template-columns:240px 1fr 320px;gap:16px}.qe-side{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);padding:14px}.qe-side h3{color:var(--text-muted);font-size:11px;text-transform:uppercase;letter-spacing:.06em;margin-bottom:10px}
  .qe-ch-row{width:100%;display:grid;gap:2px;text-align:left;padding:8px 10px;border-radius:8px;background:transparent;color:var(--text-secondary);border:1px solid transparent;margin-bottom:4px;transform:none}.qe-ch-row:hover,.qe-ch-row.sel{background:var(--bg-tertiary);border-color:rgba(27,217,106,.25);color:var(--text-primary)}.qe-ch-row strong{font-size:13px}.qe-ch-row span{font-size:11px;color:var(--text-muted)}
  .qe-add-ch{margin-top:8px;width:100%}.qe-main{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);padding:14px}
  .qe-main-h{display:flex;justify-content:space-between;align-items:center;margin-bottom:14px}.qe-main-h h3{font-size:16px}
  .qe-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));gap:8px}
  .qe-card{display:grid;gap:6px;padding:12px;border-radius:10px;border:1px solid var(--border-color);background:var(--bg-tertiary);text-align:left;transform:none}.qe-card:hover,.qe-card.sel{border-color:rgba(27,217,106,.35);background:rgba(27,217,106,.04)}.qe-card strong{color:var(--text-primary);font-size:13px}
  .qe-card-meta{display:flex;gap:8px;flex-wrap:wrap}.qe-card-meta span{font-size:10px;color:var(--text-muted)}.qe-card-meta .opt{color:#fbbf24;font-weight:700}
  .qe-detail{background:var(--bg-secondary);border:1px solid var(--border-color);border-radius:var(--border-radius-lg);padding:14px;max-height:680px;overflow:auto}.qe-det-h{display:flex;justify-content:space-between;align-items:center;margin-bottom:14px}
  .ico{width:28px;height:28px;padding:0;display:flex;align-items:center;justify-content:center;background:transparent;border:1px solid var(--border-color);border-radius:6px;color:var(--text-muted);cursor:pointer}.ico.danger:hover{background:rgba(239,68,68,.1);color:#f87171}
  .qe-det-fields{display:grid;gap:10px;margin-bottom:16px}.qe-det-fields label{display:grid;gap:4px;color:var(--text-muted);font-size:11px;text-transform:uppercase;letter-spacing:.03em}.qe-det-fields input{font-size:13px}
  .checkbox{display:flex;align-items:center;gap:8px;flex-direction:row!important;font-size:12px!important}.checkbox input{width:auto}.xy{display:grid;grid-template-columns:1fr 1fr;gap:6px}
  h4{color:var(--text-secondary);font-size:12px;margin:14px 0 8px;text-transform:uppercase;letter-spacing:.04em}
  .qe-tasks{display:grid;gap:4px}.qe-task-row{display:flex;gap:8px;align-items:center;padding:6px 8px;border-radius:6px;background:var(--bg-tertiary);font-size:11px}.qe-task-row code{font-size:10px;color:var(--accent-primary)}.muted{color:var(--text-muted);font-size:11px}
  .qe-deps{display:flex;flex-wrap:wrap;gap:4px;align-items:center}.dep-tag{font-size:10px;padding:3px 6px;border-radius:4px;background:var(--bg-tertiary);color:var(--text-secondary);border:1px solid var(--border-color);display:flex;align-items:center;gap:4px}
  .dep-rm{background:transparent;border:none;color:var(--text-muted);cursor:pointer;font-size:10px;padding:0}.dep-rm:hover{color:#f87171}.dep-add{margin-top:4px}.dep-add input{font-size:11px;padding:4px 8px;width:140px}
  :global(.spin){animation:spin 900ms linear infinite}@keyframes spin{to{transform:rotate(360deg)}}@media(max-width:1100px){.qe-lay{grid-template-columns:1fr}}
</style>
