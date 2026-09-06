<script lang="ts">
  import { onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import CodeMirror from "svelte-codemirror-editor";
  import { markdown } from "@codemirror/lang-markdown";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { EditorView } from "@codemirror/view";
  import PromptDialog from "./PromptDialog.svelte";
  import { marked } from "marked";
  import { sanitizeHtml } from "../lib/sanitizeHtml";
  import {
      FolderOpen,
      Copy,
      Save,
      History,
      UploadCloud,
      Rocket,
    } from "@lucide/svelte";
    import { api, type ListingGalleryItem, type PackBrief, type ProjectListing } from "../lib/api";
    import { projectPath, projectInfo, ideStageRequest, briefDirty } from "../lib/store";
    import ListingCardPreview from "./ListingCardPreview.svelte";
    import MdToolbar from "./listing/MdToolbar.svelte";
    import GalleryGrid from "./listing/GalleryGrid.svelte";

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

  let cmView: EditorView | null = $state(null);

  let showGalleryUrlPrompt = $state(false);
  let galleryUrlMode = $state<"gallery" | "insert">("gallery");

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
    galleryUrlMode = "gallery";
    showGalleryUrlPrompt = true;
  }

  async function addGalleryUrlDirect(url: string) {
    if (!$projectPath) return;
    try {
      await flushForm();
      const listing = await api.project.addListingGalleryImage(
        { url },
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

  function insertAtCursor(insert: string, selectPlaceholder = true) {
    if (!cmView) {
      bodyMarkdown = bodyMarkdown + insert;
      markDirty();
      return;
    }
    const pos = cmView.state.selection.main.head;
    const selected = cmView.state.sliceDoc(
      cmView.state.selection.main.from,
      cmView.state.selection.main.to,
    );
    const needsSelect = selectPlaceholder && !selected;
    const text = needsSelect ? "text" : selected;
    let finalInsert = insert.replace("{sel}", text);
    let from = pos;
    let to = pos;
    if (selected) {
      from = cmView.state.selection.main.from;
      to = cmView.state.selection.main.to;
    }
    cmView.dispatch({
      changes: { from, to, insert: finalInsert },
      selection: { anchor: from + finalInsert.length },
    });
    bodyMarkdown = cmView.state.doc.toString();
    markDirty();
  }

  function insertAround(before: string, after = before) {
    if (!cmView) {
      bodyMarkdown = bodyMarkdown + before + "text" + after;
      markDirty();
      return;
    }
    const { from, to } = cmView.state.selection.main;
    const selected = cmView.state.sliceDoc(from, to);
    if (selected) {
      cmView.dispatch({
        changes: { from, to, insert: before + selected + after },
        selection: { anchor: from + before.length + selected.length + after.length },
      });
    } else {
      const placeholder = "text";
      const insert = before + placeholder + after;
      cmView.dispatch({
        changes: { from, insert },
        selection: { anchor: from + before.length, head: from + before.length + placeholder.length },
      });
    }
    bodyMarkdown = cmView.state.doc.toString();
    markDirty();
  }

  function insertHeading() {
    const prefix = bodyMarkdown.endsWith("\n") || !bodyMarkdown ? "" : "\n";
    insertAtCursor(prefix + "## Heading\n", false);
  }

  function insertLink() {
    insertAtCursor("[label](https://example.com)", false);
  }

  async function insertImageUrl() {
    galleryUrlMode = "insert";
    showGalleryUrlPrompt = true;
  }

  function onGalleryUrlConfirm(url: string) {
    showGalleryUrlPrompt = false;
    if (!url.trim()) return;
    if (galleryUrlMode === "gallery") {
      void addGalleryUrlDirect(url.trim());
    } else {
      insertAtCursor(`\n![image](${url.trim()})\n`, false);
    }
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
        insertAtCursor(`\n![image](${last.path})\n`, false);
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
    insertAtCursor(`\n![${alt}](${src})\n`, false);
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
      return sanitizeHtml(marked.parse(rewritten) as string);
    } catch {
      return "<p>Preview failed.</p>";
    }
  }

  async function handlePaste(e: ClipboardEvent) {
    if (!$projectPath) return;
    const target = e.target as HTMLElement;
    if (!target.closest?.(".cm-wrap")) return;
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

  function extForType(type: string): string {
      if (type.includes("jpeg")) return "jpg";
      if (type.includes("webp")) return "webp";
      if (type.includes("gif")) return "gif";
      return "png";
    }

    async function handleGalleryDrop(e: DragEvent) {
      if (!$projectPath) return;
      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;
      e.preventDefault();
      for (const file of Array.from(files)) {
        if (!file.type.startsWith("image/")) continue;
        const buf = await file.arrayBuffer();
        const bytes = new Uint8Array(buf);
        let binary = "";
        for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
        const b64 = btoa(binary);
        try {
          await flushForm();
          const listing = await api.project.addListingGalleryBytes(
            b64,
            extForType(file.type),
            null,
            $projectPath,
          );
          applyIconGallery(listing);
          message = "Dropped image added to gallery.";
          await refreshAssets();
        } catch (err) {
          error = String(err);
        }
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

  function handleBeforeUnload(e: BeforeUnloadEvent) {
    if (dirty) {
      e.preventDefault();
    }
  }

  let autoSaveTimer: ReturnType<typeof setTimeout> | null = null;
  const AUTO_SAVE_MS = 5000;

  function scheduleAutoSave() {
    if (autoSaveTimer) clearTimeout(autoSaveTimer);
    if (!dirty || !$projectPath) return;
    autoSaveTimer = setTimeout(async () => {
      if (dirty && $projectPath && !saving) {
        await saveAll();
      }
    }, AUTO_SAVE_MS);
  }

  $effect(() => {
    if (dirty) scheduleAutoSave();
    return () => {
      if (autoSaveTimer) clearTimeout(autoSaveTimer);
    };
  });

  $effect(() => {
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  });

  onDestroy(() => {
    briefDirty.set(false);
    if (mdDebounceTimer) clearTimeout(mdDebounceTimer);
  });
</script>

<div class="brief-editor" onpaste={handlePaste}>
  <!-- Unified header: title left, fast actions right -->
  <header class="page-header">
    <div class="ph-text">
      <h2 class="text-lg font-bold text-[color:var(--text-primary)] leading-tight">Storefront listing</h2>
      <p class="ph-sub">
        The pack card shown on Modrinth / CurseForge — preview updates live.
      </p>
    </div>
    <div class="header-actions">
      <span
        class="sync-pill"
        class:unsaved={dirty}
        title={dirty ? "You have unsaved changes (auto-saves in a moment)" : "All changes saved"}
      >
        <span class="sync-dot" class:on={!dirty}></span>
        {dirty ? "Unsaved changes" : "Synced"}
              </span>
              <button
        type="button"
        class="primary-btn"
        onclick={saveAll}
        disabled={!$projectPath || saving || nameEmpty}
      >
        <Save size={15} /> {saving ? "Saving…" : dirty ? "Save" : "Save"}
      </button>
    </div>
  </header>

  {#if !$projectPath}
    <div class="empty">Open a project to edit the storefront listing.</div>
  {:else if loading}
    <div class="empty">Loading listing…</div>
  {:else}
    <div class="main-body">
      {#if error}<div class="inline-error">{error}</div>{/if}
      {#if message}<div class="inline-success">{message}</div>{/if}

      <div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,520px)]">
        <!-- Identity block -->
        <section class="panel glass-card p-5">
          <h3 class="text-sm font-semibold text-[color:var(--text-primary)] mb-4">Identity</h3>
          <div class="flex flex-col gap-4 sm:flex-row">
            <!-- Icon dropzone left (112x112) -->
            <div class="icon-block">
              <span class="icon-label">Icon</span>
              <button
                              type="button"
                              class="icon-dropzone"
                              class:has-img={!!iconUrl}
                              ondragenter={(e) => e.preventDefault()}
                              ondragover={(e) => e.preventDefault()}
                              ondrop={(e) => {
                                if ($projectPath) void handleGalleryDrop(e);
                              }}
                              onclick={pickIcon}
                              title="Click to choose a square icon"
                              aria-label="Choose pack icon"
                            >
                              {#if iconUrl}
                                <img src={iconUrl} alt="Pack icon" />
                              {:else}
                                <span>No icon</span>
                              {/if}
                            </button>
              <div class="icon-actions">
                <button type="button" class="sm-btn" onclick={pickIcon}>Choose…</button>
                <button
                  type="button"
                  class="sm-btn ghost"
                  onclick={clearIcon}
                  disabled={!iconPath}
                >
                  Clear
                </button>
              </div>
              <small class="hint">Square PNG/WebP/JPG</small>
            </div>

            <!-- Pack name + Summary right -->
            <div class="identity-fields flex-1 min-w-0">
              <label class="field-label">
                <span class="fl-title">Pack name</span>
                <input
                  bind:value={name}
                  oninput={markDirty}
                  placeholder="My Pack"
                  class:invalid={nameEmpty}
                />
                {#if nameEmpty}<small class="hint warn">Name is required</small>{/if}
              </label>
              <label class="field-label">
                <span class="fl-title">Summary</span>
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
          </div>

          <!-- Categories / Tags -->
          <div class="mt-4">
            <span class="fl-title block mb-2">Categories</span>
            <div class="flex flex-wrap gap-2" role="group" aria-label="Modrinth modpack categories">
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
            <div class="mt-2 flex items-center gap-2">
              {#if categoriesError}
                <small class="hint warn">Using offline Modrinth list ({categoriesError})</small>
              {:else}
                <small class="hint">Official Modrinth modpack tags</small>
              {/if}
            </div>
          </div>
        </section>

        <!-- Listing preview column -->
        <aside class="min-w-0">
          <div class="preview-sticky glass-card p-5 flex flex-col gap-3">
            <div class="preview-heading-row">
              <div class="preview-heading">Listing preview</div>
              <div class="seg-control" role="group" aria-label="Preview platform">
                <button
                  type="button"
                  class:active={cardStyle === "modrinth"}
                  onclick={() => (cardStyle = "modrinth")}
                >
                  Modrinth
                </button>
                <button
                  type="button"
                  class:active={cardStyle === "curseforge"}
                  onclick={() => (cardStyle = "curseforge")}
                >
                  CurseForge
                </button>
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

      <section class="panel description-panel glass-card p-5">
              <div class="panel-head">
                <h3 class="text-sm font-semibold text-[color:var(--text-primary)]">Description</h3>
                <div class="seg-control" role="group" aria-label="Editor mode">
                  <button
                    type="button"
                    class:active={mdView === "edit"}
                    onclick={() => (mdView = "edit")}
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    class:active={mdView === "split"}
                    onclick={() => (mdView = "split")}
                  >
                    Split
                  </button>
                  <button
                    type="button"
                    class:active={mdView === "preview"}
                    onclick={() => (mdView = "preview")}
                  >
                    Preview
                  </button>
                </div>
              </div>
              <MdToolbar
                              onBold={() => insertAround("**")}
                              onItalic={() => insertAround("_")}
                              onHeading={insertHeading}
                              onLink={insertLink}
                              onImageUrl={insertImageUrl}
                              onLocalImage={insertLocalImage}
                            />
              <div class="md-split" class:edit-only={mdView === "edit"} class:preview-only={mdView === "preview"}>
                {#if mdView !== "preview"}
                  <div class="cm-wrap">
                    <CodeMirror
                      value={bodyMarkdown}
                      lang={markdown()}
                      theme={oneDark}
                      on:change={onBodyChange}
                      on:ready={(e) => (cmView = e.detail)}
                    />
                  </div>
                {/if}
                {#if mdView !== "edit"}
                  <div class="md-preview prose md-preview-glass">
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
              <section class="panel glass-card p-5">
                <div class="panel-head">
                  <h3 class="text-sm font-semibold text-[color:var(--text-primary)]">Gallery</h3>
                </div>
                <GalleryGrid
                                  items={gallery}
                                  urls={galleryUrls}
                  onAddFile={addGalleryFile}
                  onAddUrl={addGalleryUrl}
                  onDropFiles={handleGalleryDrop}
                  onRemove={removeGallery}
                  onMove={moveGallery}
                  onInsert={insertGalleryIntoBody}
                />
              </section>

              <summary class="sr-only">Author notes</summary>
              <details class="panel glass-card p-5 author-notes" open>
                <summary class="panel-summary">Author notes (planning)</summary>
                <div class="brief-grid">
                  <label
                    class="field-label"
                    ><span class="fl-title">Pack goal</span><textarea bind:value={briefGoal} oninput={markDirty} rows="3"></textarea
                    ></label
                  >
                  <label
                    class="field-label"
                    ><span class="fl-title">Target player</span><textarea
                      bind:value={briefAudience}
                      oninput={markDirty}
                      rows="3"
                    ></textarea></label
                  >
                  <label
                    class="field-label"
                    ><span class="fl-title">Gameplay pillars</span><textarea
                      bind:value={briefPillars}
                      oninput={markDirty}
                      rows="3"
                      placeholder="One per line"
                    ></textarea></label
                  >
                  <label
                    class="field-label"
                    ><span class="fl-title">Hard constraints</span><textarea
                      bind:value={briefConstraints}
                      oninput={markDirty}
                      rows="3"
                      placeholder="One per line"
                    ></textarea></label
                  >
                  <label
                    class="field-label"
                    ><span class="fl-title">Release targets</span><textarea
                      bind:value={briefReleaseTargets}
                      oninput={markDirty}
                      rows="3"
                      placeholder="One per line"
                    ></textarea></label
                  >
                  <label
                    class="field-label"
                    ><span class="fl-title">Notes</span><textarea bind:value={briefNotes} oninput={markDirty} rows="3"></textarea
                    ></label
                  >
                </div>
              </details>

              <details class="panel glass-card p-5 extras-panel">
                <summary class="panel-summary">More · copy, folder, workflow</summary>
                <div class="extras-grid">
                  <div class="extras-actions">
                    <button type="button" class="sm-btn" onclick={copySummary} disabled={!summary}>
                      <Copy size={14} /> Copy summary
                    </button>
                    <button
                      type="button"
                      class="sm-btn"
                      onclick={openListingFolder}
                      disabled={!$projectPath}
                    >
                      <FolderOpen size={14} /> Listing folder
                    </button>
                  </div>
                  <div class="trail">
                    <button type="button" class="sm-btn" onclick={() => goTrail("history")}
                      ><History size={14} /> History</button
                    >
                    <button type="button" class="sm-btn" onclick={() => goTrail("export")}
                      ><UploadCloud size={14} /> Export</button
                    >
                    <button type="button" class="sm-btn" onclick={() => goTrail("release")}
                      ><Rocket size={14} /> Release</button
                    >
                  </div>
                </div>
              </details>
            </div>
    </div>
  {/if}
</div>

{#if showGalleryUrlPrompt}
  <PromptDialog
    title={galleryUrlMode === "gallery" ? "Add gallery image URL" : "Insert image URL"}
    message={galleryUrlMode === "gallery" ? "Paste an image URL to add to the gallery." : "Paste an image URL to insert into the description."}
    confirmLabel={galleryUrlMode === "gallery" ? "Add to gallery" : "Insert"}
    onconfirm={onGalleryUrlConfirm}
    oncancel={() => (showGalleryUrlPrompt = false)}
  />
{/if}

<style>
  .brief-editor {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 12px 16px 16px;
    box-sizing: border-box;
    /* Task #62 + responsive pass: cap the column and center it */
    max-width: 1280px;
    margin: 0 auto;
    width: 100%;
    /* Native glass shell under the whole editor */
    background: rgba(0, 0, 0, 0.28);
    -webkit-backdrop-filter: blur(24px) saturate(150%);
    backdrop-filter: blur(24px) saturate(150%);
  }

  .main-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-gutter: stable;
    font-size: 13px;
    padding-right: 2px;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: center;
    flex-wrap: wrap;
    flex-shrink: 0;
    margin-bottom: 4px;
  }

  .ph-text {
    min-width: 0;
  }

  .ph-sub {
    margin: 3px 0 0;
    color: var(--text-muted);
    max-width: 62ch;
    font-size: 12.5px;
  }

  .header-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
    align-items: center;
    margin-left: auto;
  }

  /* Glass card shell reused across every section panel */
  .glass-card {
      background: color-mix(in srgb, var(--bg-secondary) 35%, transparent);
      -webkit-backdrop-filter: blur(14px) saturate(140%);
      backdrop-filter: blur(14px) saturate(140%);
      border: 1px solid var(--border-color);
          border-radius: var(--border-radius-lg);
      box-shadow:
        inset 0 1px 0 color-mix(in srgb, var(--text-muted) 10%, transparent),
        0 14px 44px rgba(3, 6, 10, 0.16);
      padding: 18px;
    }

  .primary-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 16px;
    border: none;
    border-radius: 10px;
    background: linear-gradient(180deg, #10b981, #059669);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 0 14px rgba(16, 185, 129, 0.35), inset 0 1px 0 rgba(255, 255, 255, 0.22);
    transition: filter var(--motion-fast, 160ms) ease, transform var(--motion-fast, 160ms) ease;
  }
  .primary-btn:hover:not(:disabled) {
    filter: brightness(1.06);
    transform: translateY(-1px);
  }
  .primary-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .sync-pill {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      padding: 5px 11px;
      border-radius: 999px;
      border: 1px solid var(--border-color);
      background: color-mix(in srgb, var(--bg-secondary) 35%, transparent);
      color: var(--text-muted);
      font-size: 12px;
      font-weight: 600;
      white-space: nowrap;
    }
  .sync-pill.unsaved {
    color: #fbbf24;
    border-color: rgba(251, 191, 36, 0.35);
    background: rgba(251, 191, 36, 0.08);
  }
  .sync-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #fbbf24;
    box-shadow: 0 0 8px rgba(251, 191, 36, 0.6);
  }
  .sync-dot.on {
    background: #34d399;
    box-shadow: 0 0 8px rgba(16, 185, 129, 0.6);
  }

  /* Segmented control (platform / editor mode) */
  .seg-control {
      display: inline-flex;
      gap: 3px;
      padding: 3px;
      border-radius: var(--border-radius-md);
      background: var(--bg-tertiary);
      border: 1px solid var(--border-color);
    }
    .seg-control button {
      border: none;
      background: transparent;
      color: var(--text-muted);
      padding: 6px 12px;
      border-radius: 9px;
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
      transition: background var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
    }
    .seg-control button.active {
      background: color-mix(in srgb, var(--accent-primary) 16%, transparent);
      color: var(--accent-primary);
      box-shadow: 0 0 10px color-mix(in srgb, var(--accent-primary) 20%, transparent);
    }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .panel-head h3 {
    margin: 0;
  }

  /* Fields */
  .field-label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
    color: var(--text-secondary);
    font-weight: 600;
    font-size: 13px;
  }
  .field-label:last-child {
    margin-bottom: 0;
  }
  .fl-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-secondary);
    letter-spacing: 0.01em;
  }

  input,
    textarea {
      border: 1px solid var(--border-color);
      border-radius: 10px;
      background: var(--bg-elevated);
      color: var(--text-primary);
      padding: 10px 12px;
      font-family: inherit;
      font-size: 13.5px;
      transition: border-color var(--motion-fast, 160ms) ease, box-shadow var(--motion-fast, 160ms) ease;
    }
  input:focus,
  textarea:focus {
    outline: none;
    border-color: rgba(16, 185, 129, 0.5);
    box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.12);
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

  /* Icon dropzone — square 112x112 with drag & drop */
  .icon-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    min-width: 120px;
  }
  .icon-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .icon-dropzone {
    width: 112px;
    height: 112px;
    border-radius: 14px;
    border: 1.5px dashed rgba(255, 255, 255, 0.18);
    background: rgba(0, 0, 0, 0.24);
    overflow: hidden;
    display: grid;
    place-items: center;
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
    transition: border-color var(--motion-fast, 160ms) ease, background var(--motion-fast, 160ms) ease;
  }
  .icon-dropzone:hover {
      border-color: rgba(16, 185, 129, 0.5);
      background: rgba(16, 185, 129, 0.05);
    }
    .icon-dropzone:focus-visible {
      outline: none;
      border-color: rgba(16, 185, 129, 0.6);
      box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.15);
    }
    .icon-dropzone.has-img {
    border-style: solid;
    border-color: rgba(16, 185, 129, 0.4);
  }
  .icon-dropzone img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .icon-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: center;
  }

  .sm-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--motion-fast, 160ms) ease, border-color var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
  }
  .sm-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.09);
    border-color: rgba(255, 255, 255, 0.18);
    color: #fff;
  }
  .sm-btn.ghost {
    background: transparent;
    border-color: transparent;
    color: var(--text-muted);
  }
  .sm-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  /* Category chips */
  .cat-chip {
      border: 1px solid color-mix(in srgb, var(--border-color) 55%, transparent);
      background: color-mix(in srgb, var(--bg-secondary) 25%, transparent);
      color: var(--text-muted);
      border-radius: 999px;
      padding: 6px 12px;
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
    transition: background var(--motion-fast, 160ms) ease, border-color var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
  }
  .cat-chip:hover {
    background: rgba(255, 255, 255, 0.1);
    border-color: rgba(255, 255, 255, 0.14);
    color: #d1d5db;
  }
  .cat-chip.on {
    background: rgba(16, 185, 129, 0.2);
    border-color: rgba(16, 185, 129, 0.4);
    color: #34d399;
    box-shadow: 0 0 10px rgba(16, 185, 129, 0.15);
  }

  /* Preview column */
  .preview-sticky {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .preview-heading-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }
  .preview-heading {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .listing-preview-compact :global(.mr-card),
  .listing-preview-compact :global(.cf-card) {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
    padding: 14px;
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

  /* Description editor */
  .description-panel {
    min-height: 220px;
    width: 100%;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    gap: 10px;
  }
  .description-panel .md-split {
    min-height: 280px;
    height: clamp(280px, 42vh, 420px);
  }
  .description-panel .cm-wrap,
  .description-panel .md-preview {
    min-height: 0;
    height: 100%;
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
      border-radius: var(--border-radius-md);
      overflow: hidden;
      min-height: 0;
      height: 100%;
    }
  .cm-wrap :global(.cm-editor) {
    height: 100%;
  }
  .cm-wrap :global(.cm-scroller) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 13px;
    line-height: 1.6;
  }

  .md-preview {
      border: 1px solid var(--border-color);
      border-radius: var(--border-radius-md);
      padding: 16px;
      min-height: 0;
      height: 100%;
      overflow: auto;
    }
    .md-preview-glass {
      background: color-mix(in srgb, var(--bg-secondary) 30%, transparent);
      backdrop-filter: blur(10px);
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
      color: var(--text-primary);
    }

  .brief-below {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 2px;
  }

  .panel-summary {
    cursor: pointer;
    font-weight: 700;
    color: var(--text-secondary);
    font-size: 13px;
    list-style: none;
    margin-bottom: 10px;
  }
  .panel-summary::-webkit-details-marker {
    display: none;
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
    flex-shrink: 0;
    font-size: 13px;
    background: rgba(255, 255, 255, 0.03);
  }
  .inline-error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.28);
  }
  .inline-success {
    color: #6ee7b7;
    background: rgba(16, 185, 129, 0.08);
    border-color: rgba(16, 185, 129, 0.25);
  }

  /* Slim scrollbars */
  .main-body::-webkit-scrollbar,
  .md-preview::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }
  .main-body::-webkit-scrollbar-thumb,
  .md-preview::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.12);
    border-radius: 999px;
  }
  .main-body::-webkit-scrollbar-thumb:hover,
  .md-preview::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  @media (max-width: 1100px) {
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
      min-height: 320px;
    }
  }
</style>
