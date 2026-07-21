# Bug Report — TuffBox Desktop Frontend

**Date:** 2026-07-20  
**Scope:** `apps/tuffbox-desktop/src/` — all Svelte components + lib files  
**Verified:** All findings double-checked against source code

---

## Bug 1 — Infinite retry loop: OreGenVisualizer `loadWorlds` never marks failed path

**File:** `src/components/OreGenVisualizer.svelte`  
**Lines:** 59–71, 168  
**Type:** Infinite loop  
**Severity:** HIGH  

```js
let lastWorldsPath: string | null = null;

async function loadWorlds() {
  if (!$projectPath) return;
  worldsLoading = true;
  worldsError = null;
  try {
    worlds = (await invoke("list_worlds", { path: $projectPath }));
    lastWorldsPath = $projectPath;       // ← only set on SUCCESS
  } catch (e) {
    worldsError = String(e);             // ← lastWorldsPath NOT set
  } finally {
    worldsLoading = false;
  }
}

$: if ($projectPath && $projectPath !== lastWorldsPath && !worldsLoading) loadWorlds();
```

When `invoke("list_worlds")` throws, `lastWorldsPath` is never updated. The reactive statement re-evaluates: `$projectPath !== lastWorldsPath` is still `true` and `worldsLoading` is `false` (reset in `finally`), so `loadWorlds()` fires again immediately. This loops indefinitely, spamming the backend with failed calls.

**Fix:** Move `lastWorldsPath = $projectPath;` into the `finally` block so it's set regardless of success/failure.

---

## Bug 2 — Stale state: Dashboard and Library `launching` flag resets instantly

**File:** `src/components/Dashboard.svelte` (lines 101–113), `src/components/Library.svelte` (lines 58–68)  
**Type:** State bug  
**Severity:** MEDIUM  

```js
// Dashboard.svelte
async function launch() {
  if (!selectedPath) return;
  launching = true;
  showLogModal = true;
  try {
    await invoke("set_last_opened_project", { path: selectedPath });
    await invoke("launch_profile", { path: selectedPath, profile: "client" });
  } catch (e) {
    toasts.error(`Launch failed: ${e}`);
  } finally {
    launching = false;  // ← resets immediately after invoke returns
  }
}
```

`launch_profile` is fire-and-forget — it spawns the Minecraft process and returns immediately. `launching` is set to `false` in the `finally` block within milliseconds, so the "Launching..." spinner on the Play button flashes and disappears almost instantly, never reflecting whether the game is actually running. Same pattern exists in `Library.svelte:launchPack`.

**Fix:** Listen for a Tauri event (e.g. `process-exited`) to set `launching = false`, or poll a backend state endpoint.

---

## Bug 3 — Memory leak: WorldMap `flashTimer` not cleaned on destroy

**File:** `src/components/WorldMap.svelte`  
**Lines:** 347–353  
**Type:** Memory leak  
**Severity:** LOW  

```js
let flashMsg: string | null = null;
let flashTimer: any;
function flash(msg: string) {
  flashMsg = msg;
  clearTimeout(flashTimer);
  flashTimer = setTimeout(() => (flashMsg = null), 2500);
}
```

No `onDestroy` cleanup for `flashTimer`. If the component unmounts while a flash message is active (e.g. after "Deleted chunks"), the timeout callback fires and tries to set `flashMsg` on a destroyed component. Svelte won't crash, but the closure retains a reference to the destroyed instance.

**Fix:** Add `import { onDestroy } from "svelte"` and `onDestroy(() => clearTimeout(flashTimer))`.

---

## Bug 4 — Race condition: SkinPreview3D uncancelled async `loadSkin`

**File:** `src/components/SkinPreview3D.svelte`  
**Lines:** 53–69  
**Type:** Race condition  
**Severity:** LOW  

```js
$: if (viewer && skinUrl) {
  loadSkin(skinUrl);  // async, not awaited by reactive
}
```

The reactive block calls `loadSkin` (async) without awaiting. If `skinUrl` changes rapidly (e.g. user switches accounts quickly), multiple concurrent `loadSkin` calls overlap. The older in-flight fetch may complete after the newer one, overwriting the correct skin with a stale one. Also, if the component is destroyed mid-fetch, `viewer.loadSkin(dataUrl)` could operate on a disposed WebGL context.

**Fix:** Use a generation counter or AbortController to cancel stale loads. Increment a counter before each call; if the counter changed when the fetch resolves, discard the result.

---

## Bug 5 — Theme desync: Settings reads localStorage, not the store

**File:** `src/components/Settings.svelte`  
**Lines:** 24, 194–198  
**Type:** State desync  
**Severity:** LOW  

```js
let theme = localStorage.getItem("tuffbox-theme") || "dark";

function toggleTheme() {
  theme = theme === "dark" ? "light" : "dark";
  localStorage.setItem("tuffbox-theme", theme);
  document.documentElement.setAttribute("data-theme", theme);
}
```

The theme is initialized from `localStorage` directly, not from a reactive store. `App.svelte` also reads from `localStorage` on mount to set the DOM attribute. When the user toggles the theme in Settings, only the DOM `data-theme` attribute is updated — no Svelte store is notified. Sidebar/Header/etc. don't react because they don't observe a store. This works only because CSS variables respond to the DOM attribute, but it's fragile and un-Svelte-like.

**Fix:** Create a `theme` writable store in `store.ts` and have both Settings and App.svelte subscribe to it.

---

## Bug 6 — Timing: ExportBuilder `loadDefaultPaths` runs before `$projectInfo` loads

**File:** `src/components/ExportBuilder.svelte`  
**Lines:** 25–34, 80  
**Type:** Timing / null safety  
**Severity:** LOW  

```js
$: onProjectPathChange($projectPath);

async function loadDefaultPaths(path: string) {
  projectDir = await invoke("get_project_dir", { path });
  const id = $projectInfo?.id ?? "modpack";
  const version = $projectInfo?.version ?? "1.0.0";
  // ...sets default filenames using id and version
}
```

`loadDefaultPaths` is called reactively when `$projectPath` changes. But `$projectInfo` is loaded asynchronously in `App.svelte`'s `onMount` and may not be available yet. The `?.` operator prevents a crash, but the default filenames use fallback values ("modpack" / "1.0.0") even when the real project info arrives milliseconds later. The paths won't update when `projectInfo` finishes loading.

**Fix:** Make `loadDefaultPaths` reactive to both `$projectPath` and `$projectInfo`, or call it again when `$projectInfo` changes.

---

## Bug 7 — Performance: RecipeBrowser `filteredCounts` built twice on initial load

**File:** `src/components/RecipeBrowser.svelte`  
**Lines:** ~595–600, ~623–625  
**Type:** Redundant computation  
**Severity:** LOW  

```js
// In rebuildIndexes():
filteredCounts = buildFilteredCounts(recipes, categoryFilter, modFilter);

// Reactive block that also fires on initial load:
$: if (recipes.length && categoryFilter && modFilter) {
  filteredCounts = buildFilteredCounts(recipes, categoryFilter, modFilter);
}
```

On initial load, `rebuildIndexes()` builds `filteredCounts`, then the reactive block fires and rebuilds it again with the same arguments. The computation is duplicated.

**Fix:** Remove the redundant reactive block, or gate it to skip if `rebuildIndexes` already ran in the same tick.

---

## Bug 8 — Unused import: Dashboard imports `onDestroy` but never calls it

**File:** `src/components/Dashboard.svelte`  
**Line:** 2  
**Type:** Dead code  
**Severity:** INFO  

```js
import { onMount, onDestroy } from "svelte";
```

`onDestroy` is imported but never used anywhere in the component. This is dead code — possibly a leftover from a previous iteration that had event listeners needing cleanup.

**Fix:** Remove `onDestroy` from the import statement.

---

## Summary

| # | File | Bug Type | Severity |
|---|------|----------|----------|
| 1 | OreGenVisualizer.svelte | Infinite retry loop on failed `loadWorlds` | **HIGH** |
| 2 | Dashboard.svelte, Library.svelte | `launching` flag resets instantly (fire-and-forget) | MEDIUM |
| 3 | WorldMap.svelte | `flashTimer` not cleaned on destroy | LOW |
| 4 | SkinPreview3D.svelte | Uncancelled async `loadSkin` race condition | LOW |
| 5 | Settings.svelte | Theme desync (localStorage, no store) | LOW |
| 6 | ExportBuilder.svelte | `loadDefaultPaths` runs before `$projectInfo` loads | LOW |
| 7 | RecipeBrowser.svelte | `filteredCounts` built twice on initial load | LOW |
| 8 | Dashboard.svelte | Unused `onDestroy` import | INFO |
