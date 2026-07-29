<script lang="ts">
  import { onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import CodeMirror from "svelte-codemirror-editor";
  import { markdown } from "@codemirror/lang-markdown";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { marked } from "marked";
  import {
    Bold,
    Italic,
    Heading,
    Link,
    Image as ImageIcon,
    FolderOpen,
    Copy,
    Save,
    History,
    UploadCloud,
    Rocket,
    X,
    Plus,
  } from "lucide-svelte";
  import { api, type ListingGalleryItem, type PackBrief, type ProjectListing } from "../lib/api";
  import { projectPath, projectInfo, ideStageRequest, briefDirty } from "../lib/store";
  import ListingCardPreview from "./ListingCardPreview.svelte";

  const SUMMARY_LIMIT = 256;

  let name = "";
  let summary = "";
  let bodyMarkdown = "";
  let authorsText = "";
  let categoriesText = "";
  let iconPath: string | null = null;
  let gallery: ListingGalleryItem[] = [];
  let iconUrl: string | null = null;
  let galleryUrls: Record<string, string> = {};

  let briefGoal = "";
  let briefAudience = "";
  let briefPillars = "";
  let briefConstraints = "";
  let briefReleaseTargets = "";
  let briefNotes = "";

  let cardStyle: "modrinth" | "curseforge" = "modrinth";
  let previewTab: "card" | "page" = "card";
  let message = "";
  let error = "";
  let loading = false;
  let saving = false;
  let lastPath: string | null = null;
  let dirty = false;
  let mdView: "split" | "edit" | "preview" = "split";

  $: briefDirty.set(dirty);
  $: summaryLen = summary.length;
  $: summaryWarn = summaryLen > 200;
  $: summaryOver = summaryLen > SUMMARY_LIMIT;
  $: nameEmpty = !name.trim();

  $: renderedHtml = renderMarkdown(bodyMarkdown, galleryUrls);

  function lines(value: string) {
    return value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }

  function chips(value: string) {
    return value
      .split(/[,;\n]+/)
      .map((s) => s.trim())
      .filter(Boolean);
  }

  function markDirty() {
    dirty = true;
    message = "";
  }

  function buildListing(): ProjectListing {
    return {
      name: name.trim(),
      summary: summary,
      bodyMarkdown,
      iconPath,
      gallery,
      categories: chips(categoriesText),
      authors: chips(authorsText),
    };
  }

  function buildBrief(): PackBrief {
    return {
      goal: briefGoal,
      targetAudience: briefAudience,
      gameplayPillars: lines(briefPillars),
      constraints: lines(briefConstraints),
      releaseTargets: lines(briefReleaseTargets),
      notes: briefNotes,
    };
  }

  function applyListing(listing: ProjectListing) {
    name = listing.name ?? "";
    summary = listing.summary ?? "";
    bodyMarkdown = listing.bodyMarkdown ?? "";
    iconPath = listing.iconPath ?? null;
    gallery = listing.gallery ?? [];
    authorsText = (listing.authors ?? []).join(", ");
    categoriesText = (listing.categories ?? []).join(", ");
  }

  function applyBrief(brief: PackBrief) {
    briefGoal = brief.goal ?? "";
    briefAudience = brief.targetAudience ?? "";
    briefPillars = (brief.gameplayPillars ?? []).join("\n");
    briefConstraints = (brief.constraints ?? []).join("\n");
    briefReleaseTargets = (brief.releaseTargets ?? []).join("\n");
    briefNotes = brief.notes ?? "";
  }

  async function refreshAssets() {
    iconUrl = null;
    galleryUrls = {};
    if (!$projectPath) return;
    if (iconPath) {
      try {
        iconUrl = await api.project.readListingAsset(iconPath, $projectPath);
      } catch {
        iconUrl = null;
      }
    }
    const next: Record<string, string> = {};
    for (const item of gallery) {
      if (item.path) {
        try {
          next[item.path] = await api.project.readListingAsset(item.path, $projectPath);
        } catch {
          /* skip */
        }
      }
    }
    galleryUrls = next;
  }

  async function loadAll(force = false) {
    if (!$projectPath) return;
    if (!force && lastPath === $projectPath) return;
    loading = true;
    error = "";
    message = "";
    try {
      const [listing, brief] = await Promise.all([
        api.project.getListing($projectPath),
        api.project.getBrief($projectPath),
      ]);
      applyListing(listing);
      applyBrief(brief);
      lastPath = $projectPath;
      dirty = false;
      await refreshAssets();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function saveAll() {
    if (!$projectPath) return;
    if (nameEmpty) {
      error = "Pack name is required.";
      return;
    }
    saving = true;
    error = "";
    message = "";
    try {
      await api.project.updateBriefAndListing(buildBrief(), buildListing(), $projectPath);
      dirty = false;
      message = "Listing saved. Auto snapshot created.";
      if ($projectInfo) {
        projectInfo.set({
          ...$projectInfo,
          name: name.trim(),
        });
      }
      await refreshAssets();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function flushForm() {
    if (!$projectPath || !dirty) return;
    await api.project.updateBriefAndListing(buildBrief(), buildListing(), $projectPath);
    dirty = false;
  }

  function applyIconGallery(listing: ProjectListing) {
    iconPath = listing.iconPath ?? null;
    gallery = listing.gallery ?? [];
  }

  async function pickIcon() {
    if (!$projectPath) return;
    const selected = await open({
      multiple: false,
      title: "Choose pack icon",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    try {
      await flushForm();
      const listing = await api.project.setListingIcon(selected, $projectPath);
      applyIconGallery(listing);
      message = "Icon updated.";
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  async function clearIcon() {
    if (!$projectPath) return;
    try {
      await flushForm();
      const listing = await api.project.clearListingIcon($projectPath);
      applyIconGallery(listing);
      message = "Icon cleared.";
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  async function addGalleryFile() {
    if (!$projectPath) return;
    const selected = await open({
      multiple: false,
      title: "Add gallery image",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    try {
      await flushForm();
      const listing = await api.project.addListingGalleryImage(
        { sourceFile: selected },
        $projectPath,
      );
      applyIconGallery(listing);
      message = "Gallery image added.";
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  async function addGalleryUrl() {
    if (!$projectPath) return;
    const url = window.prompt("Image URL");
    if (!url?.trim()) return;
    try {
      await flushForm();
      const listing = await api.project.addListingGalleryImage(
        { url: url.trim() },
        $projectPath,
      );
      applyIconGallery(listing);
      message = "Gallery URL added.";
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  async function removeGallery(index: number) {
    if (!$projectPath) return;
    try {
      await flushForm();
      const listing = await api.project.removeListingGalleryImage(index, $projectPath);
      applyIconGallery(listing);
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  async function moveGallery(from: number, to: number) {
    if (!$projectPath || to < 0 || to >= gallery.length) return;
    try {
      await flushForm();
      const listing = await api.project.reorderListingGallery(from, to, $projectPath);
      applyIconGallery(listing);
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  function gallerySrc(item: ListingGalleryItem): string | null {
    if (item.url) return item.url;
    if (item.path && galleryUrls[item.path]) return galleryUrls[item.path];
    return null;
  }

  function insertAround(before: string, after = before) {
    const sel = window.getSelection()?.toString() ?? "";
    bodyMarkdown = `${bodyMarkdown}${before}${sel || "text"}${after}`;
    markDirty();
  }

  function insertHeading() {
    bodyMarkdown = `${bodyMarkdown}${bodyMarkdown.endsWith("\n") || !bodyMarkdown ? "" : "\n"}## Heading\n`;
    markDirty();
  }

  function insertLink() {
    bodyMarkdown = `${bodyMarkdown}[label](https://example.com)`;
    markDirty();
  }

  async function insertImageUrl() {
    const url = window.prompt("Image URL to insert");
    if (!url?.trim()) return;
    bodyMarkdown = `${bodyMarkdown}\n![image](${url.trim()})\n`;
    markDirty();
  }

  async function insertLocalImage() {
    if (!$projectPath) return;
    const selected = await open({
      multiple: false,
      title: "Insert local image",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    try {
      await flushForm();
      const listing = await api.project.addListingGalleryImage(
        { sourceFile: selected },
        $projectPath,
      );
      applyIconGallery(listing);
      const last = listing.gallery[listing.gallery.length - 1];
      if (last?.path) {
        bodyMarkdown = `${bodyMarkdown}\n![image](${last.path})\n`;
        dirty = true;
      }
      await refreshAssets();
    } catch (e) {
      error = String(e);
    }
  }

  function insertGalleryIntoBody(item: ListingGalleryItem) {
    const src = item.path || item.url;
    if (!src) return;
    const alt = item.caption?.trim() || "image";
    bodyMarkdown = `${bodyMarkdown}\n![${alt}](${src})\n`;
    markDirty();
  }

  async function copySummary() {
    try {
      await navigator.clipboard.writeText(summary);
      message = "Summary copied.";
    } catch {
      error = "Could not copy summary.";
    }
  }

  async function openListingFolder() {
    if (!$projectPath) return;
    try {
      const dir = await api.project.ensureListingFolder($projectPath);
      await openShell(dir);
    } catch (e) {
      error = String(e);
    }
  }

  function goTrail(stage: string) {
    ideStageRequest.set(stage);
  }

  function onBodyChange(e: CustomEvent<string>) {
    bodyMarkdown = e.detail;
    markDirty();
  }

  function renderMarkdown(src: string, assets: Record<string, string>): string {
    marked.setOptions({ breaks: true, gfm: true });
    let rewritten = src.replace(
      /!\[([^\]]*)\]\((\.tuffbox\/listing\/[^)\s]+)\)/g,
      (_m, alt, path) => {
        const data = assets[path];
        if (data) return `![${alt}](${data})`;
        return `![${alt}](${path})`;
      },
    );
    try {
      return marked.parse(rewritten) as string;
    } catch {
      return "<p>Preview failed.</p>";
    }
  }

  async function handlePaste(e: ClipboardEvent) {
    if (!$projectPath) return;
    const items = e.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (!item.type.startsWith("image/")) continue;
      e.preventDefault();
      const file = item.getAsFile();
      if (!file) return;
      const buf = await file.arrayBuffer();
      const bytes = new Uint8Array(buf);
      let binary = "";
      for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
      const b64 = btoa(binary);
      const ext = item.type.includes("jpeg")
        ? "jpg"
        : item.type.includes("webp")
          ? "webp"
          : item.type.includes("gif")
            ? "gif"
            : "png";
      try {
        await flushForm();
        const listing = await api.project.addListingGalleryBytes(b64, ext, null, $projectPath);
        applyIconGallery(listing);
        const last = listing.gallery[listing.gallery.length - 1];
        if (last?.path) {
          bodyMarkdown = `${bodyMarkdown}\n![pasted](${last.path})\n`;
          dirty = true;
        }
        message = "Pasted image added to gallery.";
        await refreshAssets();
      } catch (err) {
        error = String(err);
      }
      return;
    }
  }

  $: if ($projectPath) void loadAll();
  $: if (!$projectPath) {
    lastPath = null;
    dirty = false;
  }

  onDestroy(() => {
    briefDirty.set(false);
  });
</script>

<div class="brief-editor" on:paste={handlePaste}>
  <div class="page-header">
    <div>
      <h2>Storefront listing</h2>
      <p>
        Edit the pack card players see on Modrinth / CurseForge. Preview updates live; Save writes
        listing + syncs project name/summary.
      </p>
    </div>
    <div class="header-actions">
      <button type="button" class="ghost" on:click={copySummary} disabled={!summary}>
        <Copy size={14} /> Copy summary
      </button>
      <button type="button" class="ghost" on:click={openListingFolder} disabled={!$projectPath}>
        <FolderOpen size={14} /> Listing folder
      </button>
      <button type="button" on:click={saveAll} disabled={!$projectPath || saving || nameEmpty}>
        <Save size={14} /> {saving ? "Saving…" : dirty ? "Save*" : "Save"}
      </button>
    </div>
  </div>

  {#if !$projectPath}
    <div class="empty">Open a project to edit the storefront listing.</div>
  {:else if loading}
    <div class="empty">Loading listing…</div>
  {:else}
    {#if error}<div class="inline-error">{error}</div>{/if}
    {#if message}<div class="inline-success">{message}</div>{/if}

    <div class="layout">
      <div class="editor-col">
        <section class="panel">
          <h3>Identity</h3>
          <label>
            Pack name
            <input
              bind:value={name}
              on:input={markDirty}
              placeholder="My Pack"
              class:invalid={nameEmpty}
            />
            {#if nameEmpty}<small class="hint warn">Name is required</small>{/if}
          </label>
          <label>
            Summary
            <textarea
              bind:value={summary}
              on:input={markDirty}
              maxlength={512}
              rows="3"
              placeholder="Short card blurb (Modrinth soft limit 256)"
            ></textarea>
            <small class="hint" class:warn={summaryWarn} class:bad={summaryOver}>
              {summaryLen}/{SUMMARY_LIMIT}
              {#if summaryOver} — over Modrinth limit{/if}
              {#if summaryWarn && !summaryOver} — getting long{/if}
            </small>
          </label>
          <label>
            Authors
            <input
              bind:value={authorsText}
              on:input={markDirty}
              placeholder="You, Co-author"
            />
          </label>
          <label>
            Categories
            <input
              bind:value={categoriesText}
              on:input={markDirty}
              placeholder="adventure, optimization"
            />
          </label>
        </section>

        <section class="panel">
          <h3>Icon</h3>
          <div class="icon-row">
            <div class="icon-preview">
              {#if iconUrl}
                <img src={iconUrl} alt="Pack icon" />
              {:else}
                <span>No icon</span>
              {/if}
            </div>
            <div class="icon-actions">
              <button type="button" on:click={pickIcon}>Choose image…</button>
              <button type="button" class="ghost" on:click={clearIcon} disabled={!iconPath}>
                Clear
              </button>
              <small class="hint">Square PNG/WebP/JPG · exported as icon.png / pack.png</small>
            </div>
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h3>Description</h3>
            <div class="seg">
              <button type="button" class:active={mdView === "edit"} on:click={() => (mdView = "edit")}
                >Edit</button
              >
              <button
                type="button"
                class:active={mdView === "split"}
                on:click={() => (mdView = "split")}>Split</button
              >
              <button
                type="button"
                class:active={mdView === "preview"}
                on:click={() => (mdView = "preview")}>Preview</button
              >
            </div>
          </div>
          <div class="md-toolbar">
            <button type="button" class="ghost" title="Bold" on:click={() => insertAround("**")}
              ><Bold size={14} /></button
            >
            <button type="button" class="ghost" title="Italic" on:click={() => insertAround("_")}
              ><Italic size={14} /></button
            >
            <button type="button" class="ghost" title="Heading" on:click={insertHeading}
              ><Heading size={14} /></button
            >
            <button type="button" class="ghost" title="Link" on:click={insertLink}
              ><Link size={14} /></button
            >
            <button type="button" class="ghost" title="Image URL" on:click={insertImageUrl}
              ><ImageIcon size={14} /></button
            >
            <button type="button" class="ghost" on:click={insertLocalImage}>Insert local image</button>
          </div>
          <div class="md-split" class:edit-only={mdView === "edit"} class:preview-only={mdView === "preview"}>
            {#if mdView !== "preview"}
              <div class="cm-wrap">
                <CodeMirror
                  value={bodyMarkdown}
                  lang={markdown()}
                  theme={oneDark}
                  on:change={onBodyChange}
                />
              </div>
            {/if}
            {#if mdView !== "edit"}
              <div class="md-preview prose">
                {#if bodyMarkdown.trim()}
                  {@html renderedHtml}
                {:else}
                  <p class="muted">Markdown preview — paste images or insert from gallery.</p>
                {/if}
              </div>
            {/if}
          </div>
        </section>

        <section class="panel">
          <div class="panel-head">
            <h3>Gallery</h3>
            <div class="row-actions">
              <button type="button" class="ghost" on:click={addGalleryFile}
                ><Plus size={14} /> File</button
              >
              <button type="button" class="ghost" on:click={addGalleryUrl}
                ><Plus size={14} /> URL</button
              >
            </div>
          </div>
          {#if gallery.length === 0}
            <p class="muted">No gallery images yet. Add files/URLs or paste from clipboard.</p>
          {:else}
            <div class="gallery-strip">
              {#each gallery as item, i (item.path || item.url || `g-${i}`)}
                <div class="gal-item">
                  {#if gallerySrc(item)}
                    <img src={gallerySrc(item)} alt={item.caption || "gallery"} />
                  {:else}
                    <div class="gal-ph">?</div>
                  {/if}
                  <div class="gal-actions">
                    <button type="button" class="ghost" on:click={() => insertGalleryIntoBody(item)}
                      >Insert</button
                    >
                    <button type="button" class="ghost" on:click={() => moveGallery(i, i - 1)} disabled={i === 0}
                      >↑</button
                    >
                    <button
                      type="button"
                      class="ghost"
                      on:click={() => moveGallery(i, i + 1)}
                      disabled={i === gallery.length - 1}>↓</button
                    >
                    <button type="button" class="ghost danger" on:click={() => removeGallery(i)}
                      ><X size={12} /></button
                    >
                  </div>
                  {#if item.caption}<small>{item.caption}</small>{/if}
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <details class="panel author-notes">
          <summary>Author notes (planning)</summary>
          <div class="brief-grid">
            <label
              >Pack goal<textarea bind:value={briefGoal} on:input={markDirty} rows="3"></textarea
              ></label
            >
            <label
              >Target player<textarea
                bind:value={briefAudience}
                on:input={markDirty}
                rows="3"
              ></textarea></label
            >
            <label
              >Gameplay pillars<textarea
                bind:value={briefPillars}
                on:input={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Hard constraints<textarea
                bind:value={briefConstraints}
                on:input={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Release targets<textarea
                bind:value={briefReleaseTargets}
                on:input={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Notes<textarea bind:value={briefNotes} on:input={markDirty} rows="3"></textarea
              ></label
            >
          </div>
        </details>

        <div class="trail">
          <span>Next:</span>
          <button type="button" class="ghost" on:click={() => goTrail("history")}
            ><History size={14} /> History</button
          >
          <button type="button" class="ghost" on:click={() => goTrail("export")}
            ><UploadCloud size={14} /> Export</button
          >
          <button type="button" class="ghost" on:click={() => goTrail("release")}
            ><Rocket size={14} /> Release</button
          >
        </div>
      </div>

      <aside class="preview-col">
        <div class="preview-sticky">
          <div class="preview-tabs">
            <button type="button" class:active={previewTab === "card"} on:click={() => (previewTab = "card")}
              >Card</button
            >
            <button type="button" class:active={previewTab === "page"} on:click={() => (previewTab = "page")}
              >Page preview</button
            >
          </div>
          {#if previewTab === "card"}
            <div class="style-toggle">
              <button
                type="button"
                class:active={cardStyle === "modrinth"}
                on:click={() => (cardStyle = "modrinth")}>Modrinth</button
              >
              <button
                type="button"
                class:active={cardStyle === "curseforge"}
                on:click={() => (cardStyle = "curseforge")}>CurseForge</button
              >
            </div>
            <ListingCardPreview
              style={cardStyle}
              {name}
              {summary}
              authors={chips(authorsText)}
              categories={chips(categoriesText)}
              {iconUrl}
              minecraftVersion={$projectInfo?.minecraftVersion ?? null}
              loaderKind={$projectInfo?.loaderKind ?? null}
              version={$projectInfo?.version ?? null}
            />
            <p class="muted preview-note">
              Close-enough layout — not a pixel-perfect clone of the storefronts.
            </p>
          {:else}
            <div class="page-preview prose">
              <div class="page-hero">
                {#if iconUrl}<img class="page-icon" src={iconUrl} alt="" />{/if}
                <div>
                  <h2>{name || "Untitled pack"}</h2>
                  <p>{summary || "No summary."}</p>
                </div>
              </div>
              {#if bodyMarkdown.trim()}
                {@html renderedHtml}
              {:else}
                <p class="muted">Long description preview appears here.</p>
              {/if}
            </div>
          {/if}
        </div>
      </aside>
    </div>
  {/if}
</div>

<style>
  .brief-editor {
    min-height: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
  }

  .page-header h2 {
    margin: 0 0 6px;
  }

  .page-header p {
    margin: 0;
    color: var(--text-muted);
    max-width: 62ch;
  }

  .header-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
  }

  .header-actions button,
  .md-toolbar button,
  .trail button,
  .row-actions button,
  .icon-actions button,
  .gal-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(280px, 0.8fr);
    gap: 18px;
    align-items: start;
  }

  .editor-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .preview-col {
    min-width: 0;
  }

  .preview-sticky {
    position: sticky;
    top: 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    padding: 14px;
  }

  .panel h3 {
    margin: 0 0 12px;
    font-size: 14px;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }

  .panel-head h3 {
    margin: 0;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
    color: var(--text-secondary);
    font-weight: 600;
    font-size: 13px;
  }

  input,
  textarea {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    padding: 10px 12px;
    font-family: inherit;
  }

  input.invalid {
    border-color: rgba(239, 68, 68, 0.55);
  }

  .hint {
    font-weight: 500;
    color: var(--text-muted);
  }

  .hint.warn {
    color: #fbbf24;
  }

  .hint.bad {
    color: #f87171;
  }

  .icon-row {
    display: flex;
    gap: 14px;
    align-items: center;
  }

  .icon-preview {
    width: 96px;
    height: 96px;
    border-radius: 12px;
    border: 1px dashed var(--border-color);
    background: var(--bg-tertiary);
    overflow: hidden;
    display: grid;
    place-items: center;
    color: var(--text-muted);
    font-size: 12px;
  }

  .icon-preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .icon-actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
  }

  .md-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 8px;
  }

  .md-split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    min-height: 220px;
  }

  .md-split.edit-only,
  .md-split.preview-only {
    grid-template-columns: 1fr;
  }

  .cm-wrap {
    border: 1px solid var(--border-color);
    border-radius: 8px;
    overflow: hidden;
    min-height: 220px;
  }

  .md-preview,
  .page-preview {
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 12px;
    background: var(--bg-tertiary);
    min-height: 220px;
    overflow: auto;
  }

  .prose :global(img) {
    max-width: 100%;
    border-radius: 8px;
  }

  .prose :global(a) {
    color: var(--accent-primary);
  }

  .prose :global(h1),
  .prose :global(h2),
  .prose :global(h3) {
    margin: 0.6em 0 0.35em;
  }

  .gallery-strip {
    display: flex;
    gap: 10px;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .gal-item {
    flex: 0 0 120px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .gal-item img,
  .gal-ph {
    width: 120px;
    height: 72px;
    object-fit: cover;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }

  .gal-ph {
    display: grid;
    place-items: center;
    color: var(--text-muted);
  }

  .gal-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
  }

  .gal-actions .danger {
    color: #f87171;
  }

  .author-notes summary {
    cursor: pointer;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .brief-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin-top: 12px;
  }

  .trail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .preview-tabs,
  .style-toggle,
  .seg {
    display: inline-flex;
    gap: 4px;
    padding: 3px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
  }

  .preview-tabs button,
  .style-toggle button,
  .seg button {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 6px 10px;
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
  }

  .preview-tabs button.active,
  .style-toggle button.active,
  .seg button.active {
    background: var(--bg-elevated);
    color: var(--text-primary);
  }

  .preview-note {
    margin: 0;
    font-size: 12px;
  }

  .page-hero {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 14px;
  }

  .page-icon {
    width: 64px;
    height: 64px;
    border-radius: 10px;
    object-fit: cover;
  }

  .page-hero h2 {
    margin: 0 0 4px;
    font-size: 18px;
  }

  .page-hero p {
    margin: 0;
    color: var(--text-secondary);
  }

  .muted {
    color: var(--text-muted);
  }

  .empty {
    padding: 24px;
    color: var(--text-muted);
  }

  .inline-error,
  .inline-success {
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
  }

  .inline-error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.28);
  }

  .inline-success {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
  }

  @media (max-width: 1100px) {
    .layout {
      grid-template-columns: 1fr;
    }
    .preview-sticky {
      position: static;
    }
    .brief-grid {
      grid-template-columns: 1fr;
    }
    .md-split {
      grid-template-columns: 1fr;
    }
  }
</style>
