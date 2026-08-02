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
  } from "@lucide/svelte";
  import { api, type ListingGalleryItem, type PackBrief, type ProjectListing } from "../lib/api";
  import { projectPath, projectInfo, ideStageRequest, briefDirty } from "../lib/store";
  import ListingCardPreview from "./ListingCardPreview.svelte";

  const SUMMARY_LIMIT = 256;

  let name = $state("");
  let summary = $state("");
  let bodyMarkdown = $state("");
  let categories = $state<string[]>([]);
  let iconPath = $state<string | null>(null);
  let gallery = $state<ListingGalleryItem[]>([]);
  let iconUrl = $state<string | null>(null);
  let galleryUrls = $state<Record<string, string>>({});

  let briefGoal = $state("");
  let briefAudience = $state("");
  let briefPillars = $state("");
  let briefConstraints = $state("");
  let briefReleaseTargets = $state("");
  let briefNotes = $state("");

  let cardStyle = $state<"modrinth" | "curseforge">("modrinth");
  let message = $state("");
  let error = $state("");
  let loading = $state(false);
  let saving = $state(false);
  let lastPath = $state<string | null>(null);
  let dirty = $state(false);
  let mdView = $state<"split" | "edit" | "preview">("split");
  let renderedHtml = $state("");
  let mdDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  const MD_DEBOUNCE_MS = 200;

  let modrinthCategories = $state<Array<{ name: string; header: string; icon: string }>>([]);
  let categoriesLoading = $state(false);
  let categoriesError = $state("");

  $effect(() => {
    briefDirty.set(dirty);
  });
  const summaryLen = $derived(summary.length);
  const summaryWarn = $derived(summaryLen > 200);
  const summaryOver = $derived(summaryLen > SUMMARY_LIMIT);
  const nameEmpty = $derived(!name.trim());

  function scheduleMarkdownRender(src: string, assets: Record<string, string>) {
    if (mdDebounceTimer) clearTimeout(mdDebounceTimer);
    mdDebounceTimer = setTimeout(() => {
      renderedHtml = renderMarkdown(src, assets);
    }, MD_DEBOUNCE_MS);
  }

  $effect(() => {
    scheduleMarkdownRender(bodyMarkdown, galleryUrls);
  });

  function lines(value: string) {
    return value
      .split("\n")
      .map((line) => line.trim())
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
      categories: [...categories],
      authors: [],
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
    categories = [...(listing.categories ?? [])];
  }

  function toggleCategory(cat: string) {
    const key = cat.trim().toLowerCase();
    if (!key) return;
    if (categories.some((c) => c.toLowerCase() === key)) {
      categories = categories.filter((c) => c.toLowerCase() !== key);
    } else {
      categories = [...categories, key];
    }
    markDirty();
  }

  function isCategorySelected(cat: string) {
    const key = cat.toLowerCase();
    return categories.some((c) => c.toLowerCase() === key);
  }

  function prettyCat(c: string) {
    return c
      .split("-")
      .map((p) => (p ? p[0].toUpperCase() + p.slice(1) : p))
      .join(" ");
  }

  async function loadModrinthCategories() {
    categoriesLoading = true;
    categoriesError = "";
    try {
      const rows = await api.mods.listCategories("modpack");
      modrinthCategories = rows
        .map((r) => ({
          name: r.name,
          header: r.header || "categories",
          icon: r.icon || "",
        }))
        .sort((a, b) => a.name.localeCompare(b.name));
    } catch (e) {
      categoriesError = e instanceof Error ? e.message : String(e);
      // Known Modrinth modpack tags as offline fallback
      modrinthCategories = [
        "adventure",
        "challenging",
        "combat",
        "kitchen-sink",
        "lightweight",
        "magic",
        "multiplayer",
        "optimization",
        "quests",
        "technology",
      ].map((name) => ({ name, header: "categories", icon: "" }));
    } finally {
      categoriesLoading = false;
    }
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
      await Promise.all([refreshAssets(), loadModrinthCategories()]);
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

  $effect(() => {
    if ($projectPath) void loadAll();
  });
  $effect(() => {
    if (!$projectPath) {
      lastPath = null;
      dirty = false;
    }
  });

  onDestroy(() => {
    briefDirty.set(false);
    if (mdDebounceTimer) clearTimeout(mdDebounceTimer);
  });
</script>

<div class="brief-editor" onpaste={handlePaste}>
  <div class="page-header">
    <div>
      <h2>Storefront listing</h2>
      <p>
        Edit the pack card players see on Modrinth / CurseForge. Preview updates live; Save writes
        listing + syncs project name/summary.
      </p>
    </div>
    <div class="header-actions">
      <button type="button" onclick={saveAll} disabled={!$projectPath || saving || nameEmpty}>
        <Save size={14} /> {saving ? "Saving…" : dirty ? "Save*" : "Save"}
      </button>
    </div>
  </div>

  {#if !$projectPath}
    <div class="empty">Open a project to edit the storefront listing.</div>
  {:else if loading}
    <div class="empty">Loading listing…</div>
  {:else}
    <div class="main-body">
      {#if error}<div class="inline-error">{error}</div>{/if}
      {#if message}<div class="inline-success">{message}</div>{/if}

      <div class="top-split">
        <section class="panel identity-panel">
          <h3>Identity</h3>
          <div class="identity-grid">
            <div class="identity-fields">
              <label>
                Pack name
                <input
                  bind:value={name}
                  oninput={markDirty}
                  placeholder="My Pack"
                  class:invalid={nameEmpty}
                />
                {#if nameEmpty}<small class="hint warn">Name is required</small>{/if}
              </label>
              <label>
                Summary
                <textarea
                  bind:value={summary}
                  oninput={markDirty}
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
            </div>
            <div class="icon-block">
              <span class="icon-label">Icon</span>
              <div class="icon-preview">
                {#if iconUrl}
                  <img src={iconUrl} alt="Pack icon" />
                {:else}
                  <span>No icon</span>
                {/if}
              </div>
              <div class="icon-actions">
                <button type="button" onclick={pickIcon}>Choose…</button>
                <button type="button" class="ghost" onclick={clearIcon} disabled={!iconPath}>
                  Clear
                </button>
              </div>
              <small class="hint">Square PNG/WebP/JPG</small>
            </div>
          </div>
          <label>
            Categories
            <div class="cat-picker" role="group" aria-label="Modrinth modpack categories">
              {#if categoriesLoading && modrinthCategories.length === 0}
                <span class="muted">Loading Modrinth categories…</span>
              {:else}
                {#each modrinthCategories as cat (cat.name)}
                  <button
                    type="button"
                    class="cat-chip"
                    class:on={isCategorySelected(cat.name)}
                    onclick={() => toggleCategory(cat.name)}
                    title={cat.name}
                  >
                    {prettyCat(cat.name)}
                  </button>
                {/each}
              {/if}
            </div>
            {#if categoriesError}
              <small class="hint warn">Using offline Modrinth list ({categoriesError})</small>
            {:else}
              <small class="hint">Official Modrinth modpack tags</small>
            {/if}
            {#if categories.length}
              <small class="hint">Selected: {categories.map(prettyCat).join(", ")}</small>
            {/if}
          </label>
        </section>

        <aside class="preview-column">
          <div class="preview-sticky">
            <div class="preview-heading-row">
              <div class="preview-heading">Listing preview</div>
              <div class="style-toggle">
                <button
                  type="button"
                  class:active={cardStyle === "modrinth"}
                  onclick={() => (cardStyle = "modrinth")}>Modrinth</button
                >
                <button
                  type="button"
                  class:active={cardStyle === "curseforge"}
                  onclick={() => (cardStyle = "curseforge")}>CurseForge</button
                >
              </div>
            </div>
            <div class="listing-preview-compact">
              <ListingCardPreview
                style={cardStyle}
                variant="card"
                {name}
                {summary}
                {categories}
                {iconUrl}
                author={name}
                minecraftVersion={$projectInfo?.minecraftVersion ?? null}
                loaderKind={$projectInfo?.loaderKind ?? null}
                version={$projectInfo?.version ?? null}
              />
            </div>
          </div>
        </aside>
      </div>

      <section class="panel description-panel">
        <div class="panel-head">
          <h3>Description</h3>
          <div class="seg">
            <button type="button" class:active={mdView === "edit"} onclick={() => (mdView = "edit")}
              >Edit</button
            >
            <button
              type="button"
              class:active={mdView === "split"}
              onclick={() => (mdView = "split")}>Split</button
            >
            <button
              type="button"
              class:active={mdView === "preview"}
              onclick={() => (mdView = "preview")}>Preview</button
            >
          </div>
        </div>
        <div class="md-toolbar">
          <button type="button" class="ghost" title="Bold" onclick={() => insertAround("**")}
            ><Bold size={14} /></button
          >
          <button type="button" class="ghost" title="Italic" onclick={() => insertAround("_")}
            ><Italic size={14} /></button
          >
          <button type="button" class="ghost" title="Heading" onclick={insertHeading}
            ><Heading size={14} /></button
          >
          <button type="button" class="ghost" title="Link" onclick={insertLink}
            ><Link size={14} /></button
          >
          <button type="button" class="ghost" title="Image URL" onclick={insertImageUrl}
            ><ImageIcon size={14} /></button
          >
          <button type="button" class="ghost" onclick={insertLocalImage}>Insert local image</button>
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

      <div class="brief-below">
        <section class="panel">
          <div class="panel-head">
            <h3>Gallery</h3>
            <div class="row-actions">
              <button type="button" class="ghost" onclick={addGalleryFile}
                ><Plus size={14} /> File</button
              >
              <button type="button" class="ghost" onclick={addGalleryUrl}
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
                    <button type="button" class="ghost" onclick={() => insertGalleryIntoBody(item)}
                      >Insert</button
                    >
                    <button type="button" class="ghost" onclick={() => moveGallery(i, i - 1)} disabled={i === 0}
                      >↑</button
                    >
                    <button
                      type="button"
                      class="ghost"
                      onclick={() => moveGallery(i, i + 1)}
                      disabled={i === gallery.length - 1}>↓</button
                    >
                    <button type="button" class="ghost danger" onclick={() => removeGallery(i)}
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
              >Pack goal<textarea bind:value={briefGoal} oninput={markDirty} rows="3"></textarea
              ></label
            >
            <label
              >Target player<textarea
                bind:value={briefAudience}
                oninput={markDirty}
                rows="3"
              ></textarea></label
            >
            <label
              >Gameplay pillars<textarea
                bind:value={briefPillars}
                oninput={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Hard constraints<textarea
                bind:value={briefConstraints}
                oninput={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Release targets<textarea
                bind:value={briefReleaseTargets}
                oninput={markDirty}
                rows="3"
                placeholder="One per line"
              ></textarea></label
            >
            <label
              >Notes<textarea bind:value={briefNotes} oninput={markDirty} rows="3"></textarea
              ></label
            >
          </div>
        </details>

        <details class="panel extras-panel">
          <summary>More · copy, folder, workflow</summary>
          <div class="extras-grid">
            <div class="extras-actions">
              <button type="button" class="ghost" onclick={copySummary} disabled={!summary}>
                <Copy size={14} /> Copy summary
              </button>
              <button type="button" class="ghost" onclick={openListingFolder} disabled={!$projectPath}>
                <FolderOpen size={14} /> Listing folder
              </button>
            </div>
            <div class="trail">
              <button type="button" class="ghost" onclick={() => goTrail("history")}
                ><History size={14} /> History</button
              >
              <button type="button" class="ghost" onclick={() => goTrail("export")}
                ><UploadCloud size={14} /> Export</button
              >
              <button type="button" class="ghost" onclick={() => goTrail("release")}
                ><Rocket size={14} /> Release</button
              >
            </div>
          </div>
        </details>
      </div>
    </div>
  {/if}
</div>

<style>
  .brief-editor {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 12px 16px 16px;
    box-sizing: border-box;
  }

  .main-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
    flex-shrink: 0;
    margin-bottom: 4px;
  }

  .page-header h2 {
    margin: 0 0 4px;
    font-size: 18px;
  }

  .page-header p {
    margin: 0;
    color: var(--text-muted);
    max-width: 62ch;
    font-size: 13px;
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
  .gal-actions button,
  .extras-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .top-split {
    flex-shrink: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(440px, 560px);
    gap: 14px;
    align-items: start;
  }

  .identity-panel {
    min-width: 0;
  }

  .preview-column {
    min-width: 0;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    padding: 10px;
  }

  .preview-sticky {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .preview-heading-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }

  .listing-preview-compact :global(.mr-card),
  .listing-preview-compact :global(.cf-card) {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
    padding: 12px;
  }

  .listing-preview-compact :global(.mr-icon) {
    width: 80px;
    height: 80px;
  }

  .listing-preview-compact :global(.cf-icon) {
    width: 80px;
    height: 80px;
  }

  .listing-preview-compact :global(.mr-title-line h3),
  .listing-preview-compact :global(.cf-body h3) {
    font-size: 15px;
  }

  .listing-preview-compact :global(.mr-summary),
  .listing-preview-compact :global(.cf-summary) {
    font-size: 12px;
    line-height: 1.4;
  }

  .listing-preview-compact :global(.mr-center),
  .listing-preview-compact :global(.cf-body) {
    gap: 6px;
    min-width: 0;
  }

  .listing-preview-compact :global(.mr-actions) {
    flex-direction: row;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .listing-preview-compact :global(.mr-dl-btn.card-dl) {
    height: 28px;
    font-size: 12px;
    padding: 0 12px;
  }

  .listing-preview-compact :global(.card-summary) {
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }
  .identity-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 14px;
    align-items: start;
    margin-bottom: 8px;
  }

  .icon-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    min-width: 108px;
  }

  .icon-label {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .description-panel {
    min-height: 220px;
    width: 100%;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .description-panel .md-split {
    min-height: 240px;
    height: clamp(240px, 36vh, 400px);
  }

  .description-panel .cm-wrap,
  .description-panel .md-preview {
    min-height: 0;
    height: 100%;
  }

  .brief-below {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 2px;
  }

  .preview-heading {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .panel {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    padding: 12px;
  }

  .panel h3 {
    margin: 0 0 10px;
    font-size: 14px;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
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

  .identity-fields label:last-child {
    margin-bottom: 0;
  }

  .cat-picker {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .cat-chip {
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    border-radius: 999px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transform: none !important;
  }

  .cat-chip:hover {
    border-color: rgba(27, 217, 106, 0.4);
    color: var(--text-primary);
    transform: none !important;
  }

  .cat-chip.on {
    border-color: rgba(27, 217, 106, 0.5);
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }

  input,
  textarea {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    padding: 8px 10px;
    font-family: inherit;
  }

  input.invalid {
    border-color: rgba(239, 68, 68, 0.55);
  }

  .hint {
    font-weight: 500;
    color: var(--text-muted);
    font-size: 12px;
  }

  .hint.warn {
    color: #fbbf24;
  }

  .hint.bad {
    color: #f87171;
  }

  .icon-preview {
    width: 88px;
    height: 88px;
    border-radius: var(--border-radius-md);
    border: 1px dashed var(--border-color);
    background: var(--bg-tertiary);
    overflow: hidden;
    display: grid;
    place-items: center;
    color: var(--text-muted);
    font-size: 11px;
  }

  .icon-preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .icon-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    justify-content: center;
  }

  .md-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 6px;
  }

  .md-split {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    min-height: 0;
  }

  .md-split.edit-only,
  .md-split.preview-only {
    grid-template-columns: 1fr;
  }

  .cm-wrap {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    overflow: hidden;
    min-height: 0;
    height: 100%;
  }

  .cm-wrap :global(.cm-editor) {
    height: 100%;
  }

  .md-preview {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    padding: 12px;
    background: var(--bg-tertiary);
    min-height: 0;
    height: 100%;
    overflow: auto;
  }

  .prose :global(img) {
    max-width: 100%;
    border-radius: var(--border-radius-sm);
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
    border-radius: var(--border-radius-sm);
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

  .author-notes summary,
  .extras-panel summary {
    cursor: pointer;
    font-weight: 700;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .brief-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin-top: 12px;
  }

  .extras-grid {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 10px;
  }

  .extras-actions,
  .trail {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .style-toggle,
  .seg {
    display: inline-flex;
    gap: 4px;
    padding: 3px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
  }

  .style-toggle button,
  .seg button {
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 5px 10px;
    border-radius: 999px;
    font-size: 12px;
    cursor: pointer;
  }

  .style-toggle button.active,
  .seg button.active {
    background: var(--bg-elevated);
    color: var(--text-primary);
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
    padding: 8px 10px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    flex-shrink: 0;
    font-size: 13px;
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
    .top-split {
      grid-template-columns: 1fr;
    }

    .preview-column {
      order: -1;
    }

    .identity-grid {
      grid-template-columns: 1fr;
    }

    .icon-block {
      flex-direction: row;
      flex-wrap: wrap;
      justify-content: flex-start;
      min-width: 0;
    }

    .brief-grid {
      grid-template-columns: 1fr;
    }

    .md-split {
      grid-template-columns: 1fr;
    }

    .description-panel {
      min-height: 280px;
    }
  }
</style>
