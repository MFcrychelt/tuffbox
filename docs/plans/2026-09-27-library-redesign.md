# Library redesign — design spec + implementation plan

**Date:** 2026-09-27
**Surface:** `apps/tuffbox-desktop` → Library (`your packs` tab)
**Problem owner:** players who live in the Library, pack authors who edit in the IDE at the same time.

---

## 1. Evidence — what is actually wrong today

Measured on the running app (2432×1385 window, `tuffbox-light`), not from memory:

| # | Observation | Evidence |
|---|---|---|
| 1 | **Three stacked bars before any content.** Row 1 `Your packs / Discover / Create … Import`; row 2 `+ Add Instance / Folders … Update / Help / Settings / <account>`; row 3 `Library 4 of 4 / search / sort / view`. | DOM dump of `.library`, screenshot `lib-current.png` |
| 2 | **"Library" is written three times** — breadcrumb, `<h1>`, and again as the list header (`Library 4 of 4`). | same |
| 3 | **The list is 4 packs in a 155px card grid** that eats ~500×500px of a 2432px window and wraps the 4th card onto a second row, leaving a hole. | screenshot |
| 4 | **~60% of the window is empty dotted background.** The pack grid ends at y≈500; the detail column ends at y≈860; the status bar is clipped by the window edge. | screenshot |
| 5 | **Every card repeats the word "Play"**, and the same action appears again as the big green button in the detail column. | screenshot |
| 6 | **Functions are spread across four containers**: per-pack actions live in the right column, on the card, in the `⋯` menu, in the `Manage…` modal, and in the IDE. `Folders` is a dropdown; `Update all` exists only inside the detail column. | DOM dump |
| 7 | **The detail column is 10 controls in a narrow strip** (Play, update banner + Update all, Manage…, Open in IDE, Folder, share, settings, 4 stat boxes, notes, content) — it is the whole feature set squeezed into a sidebar. | screenshot |
| 8 | **Inconsistent metadata**: "2h ago" next to "Aug 16", "Sep 27, 2026" in the stat box. | screenshot |
| 9 | **No multimasking.** `selectedPath` in the pane writes the global `projectPath` on every selection (`LibraryInstancesPane.svelte:482, 487, 1166, 1182`), so browsing the Library **steals** the IDE's project. Editing pack A while managing pack B is impossible. | source |

## 2. Design brief

- **Subject:** a workshop bench for Minecraft packs — not a storefront. The player has several packs, one is running, one is being edited, one is being updated.
- **Audience:** (a) players who switch packs constantly and expect Play to be two clicks away; (b) pack authors who keep the IDE open on one pack while trimming another.
- **Single job of the screen:** *get me into the right pack, and keep me oriented while I juggle several.*
- **Pinned by the brief (not mine to redesign):** the palette, the type stack (Inter Variable / JetBrains Mono), the radius/glass tokens, and the CI contrast + font-size floors. The design freedom is spent on **structure, density and the multitasking model** — that is where the current screen fails.

## 3. Design plan

### 3.1 Layout concept — *rail · bench · inspector · dock*

```
┌ rail ┬──────────────────────────────────────────────────────┬──────────────┐
│ home │ (search packs…)        [All] [Updates 3] [Folders ▾]  │  INSPECTOR   │
│ libr │ ────────────────────────────────────────────────────  │              │
│ ide  │  + New pack   ⤓ Import   ⟳ Update all (3)   4 packs   │  cover       │
│ …    ├──────────────────────────────────────────────────────┤  name        │
│      │ ▸ Fabulously Optimized  26.2 fabric   2h ago  ●Play   │  26.2 fabric │
│      │ ▸ HORROR – Into The …   1.20.1 fabric Aug 16   ⋯      │  [  Play  ]  │
│      │ ▸ New Instance          26.1.2 fabric Jul 31   ⋯      │  [Open in IDE│
│      │ ▸ Fabric 26.1.2         26.1.2 fabric Jul 31   ⋯      │  [ Manage… ] │
│      │                                                      │  updates,    │
│      │                                                      │  stats, notes│
│      ├──────────────────────────────────────────────────────┤              │
│      │ ⚙ IDE is editing "Fabulously Optimized" · Content ⇢   │              │
└──────┴──────────────────────────────────────────────────────┴──────────────┘
```

Three columns, one purpose each: **filters/actions** on top of the list, **the list** as the primary surface, **the inspector** for the focused pack. The dock is the signature element (§3.4).

### 3.2 Decisions (each one answers a numbered observation)

| Decision | Kills |
|---|---|
| **One toolbar row**: search + filter chips + sort on the left of the list; `New pack / Import / Update all (N)` on the action strip directly under it. Delete the second bar entirely — `Update`, `Help`, `Settings`, account chip are already reachable (rail gear, command palette, header `Switch`). | 1, 6 |
| **One heading.** The shell's `<h1>Library</h1>` stays; the in-panel `Library 4 of 4` header becomes the count chip `4 packs` inside the toolbar. | 2 |
| **List is the default view, grid is opt-in.** Rows are 64px: 40px cover, name, `26.2 · fabric`, relative last-played, update badge, inline `Play`. The grid keeps the existing 155px tiles for players who recognise packs by art, but it stops being the default, so four packs no longer eat the screen. | 3, 4, 8 |
| **One Play per pack.** The row's Play button is the only one in the list; the inspector's Play is the same action for the focused pack. No "Play" label repeated on every tile. | 5 |
| **Chips, not dropdowns.** `All · Updates 3 · Favorites · <folder name>` as chips above the list; grouping is a toggle, not a hidden menu. `Folders` stops being a dropdown. | 6 |
| **Updates are a first-class signal**: a badge on the row (`3↑`), a chip with the total, and one `Update all (N)` button next to it. Per-pack update detail stays in the inspector. | 6, 7 |
| **Inspector stays a column, but loses the noise**: cover, name, version, `Play`, `Open in IDE`, `Manage…`, then updates, stats, notes. Share/settings icon buttons fold into the `⋯` menu. | 7 |

### 3.3 Copy (interface voice)

- Action strip: `New pack`, `Import pack`, `Update all (3)`.
- Chips: `All`, `Updates 3`, `Favorites`, plus folder names as-is.
- Empty state: `No packs yet. Create one, or import a .mrpack you already have.` — direction, not mood.
- Cards keep the pack name verbatim (authors name their packs; never truncate to a tooltip-only).

### 3.4 Signature — the IDE dock

A single persistent strip at the bottom of the Library:

`⚙ IDE is editing "Fabulously Optimized" · Content   →   Return to IDE`

It exists because the product genuinely runs two jobs at once. It encodes real state (which pack the IDE holds), it is the affordance that makes multimasking legible, and it is the thing a reviewer will remember. When the IDE holds nothing it reads `Nothing open in the IDE — pick a pack and choose Open in IDE`, which also teaches the hand-off.

### 3.5 Multitasking model

Today `projectPath` is the single "current project" for the whole app, and the Library overwrites it on every click. New model:

- `projectPath` / `projectInfo` = **the IDE's pack** (unchanged meaning, but no longer written by mere browsing).
- `selectedPath` (already exists locally in `LibraryInstancesPane`) = **the Library's focused pack**. Selecting a row changes only this.
- **Hand-off is explicit**: `Open in IDE` (row menu / inspector) moves the focused pack into `projectPath`.
- **Play acts on the focused pack**, not on the global: the row's Play and the inspector's Play both launch `selected.path`.
- The dock reads `projectPath` and jumps back via the existing `currentView = "ide"`.
- Result: edit pack A in the IDE, click through packs B/C/D in the Library, launch B — A stays in the IDE.

### 3.6 Self-critique against the default look

Checked against the three generic AI clusters (cream+serif, near-black+acid accent, broadsheet). The palette and type here come from the product's existing theme system, so the risk budget is spent on **layout and interaction**: a dense list-first bench with an inspector and a dock. If the plan reads like "sidebar + cards + right panel for any dashboard", the swap from card grid to a **row list with a persistent IDE dock** is what makes it specific to a launcher that runs two packs at once. Removed one accessory: the second toolbar is gone entirely rather than restyled.

## 4. Task breakdown

Task 1 is independently shippable and is the one that unblocks the user's stated workflow; 2–4 are the visual redesign.

### Task 1: Library stops stealing the IDE's project

**Files:**
- Modify: `apps/tuffbox-desktop/src/components/LibraryInstancesPane.svelte`
- Test: `apps/tuffbox-desktop/src/components/test/libraryFocus.test.ts` (new)

- [ ] **Step 1 — failing test.** Assert that selecting a pack updates a Library-local focus store and leaves `projectPath` untouched, and that `openInIde` is what sets `projectPath`.

```ts
import { get } from "svelte/store";
import { projectPath, libraryFocusPath, focusLibraryPack } from "$lib/store";

it("browsing the Library never steals the IDE project", () => {
  projectPath.set("C:/packs/editing");
  focusLibraryPack("C:/packs/other");
  expect(get(libraryFocusPath)).toBe("C:/packs/other");
  expect(get(projectPath)).toBe("C:/packs/editing");
});
```

- [ ] **Step 2 — run it, expect failure** (`focusLibraryPack is not exported`).
- [ ] **Step 3 — add to `src/lib/store.ts`:**

```ts
/** The Library's focused pack — deliberately separate from `projectPath`,
 *  which belongs to the IDE. Browsing packs must not close the pack being
 *  edited in another surface. */
export const libraryFocusPath = writable<string | null>(null);
export function focusLibraryPack(path: string | null) { libraryFocusPath.set(path); }
```

- [ ] **Step 4 — replace the four `projectPath.set` / `projectInfo.set` pairs** at `LibraryInstancesPane.svelte:482-488, 1165-1167, 1181-1183` with `focusLibraryPack(...)`, and seed `selectedPath` from `$libraryFocusPath ?? $projectPath ?? $recentProjects[0]`.
- [ ] **Step 5 — tests pass, `npm run check` clean.**

**Accepts:** with the IDE open on pack A, clicking pack B in the Library changes only the Library's selection; the IDE still shows A.

### Task 2: the dock

**Files:** Modify `LibraryInstancesPane.svelte` (markup + styles).

- [ ] Render a `lib-ide-dock` strip under the list; when `$projectPath` is set show `⚙ IDE is editing "<name>" · <stage>` + `Return to IDE` (`currentView = "ide"`); otherwise the teaching line from §3.4.
- [ ] `$projectPath` name comes from `$projectInfo?.name` / `$recentProjects`.

**Accepts:** the dock names the IDE's pack and returns to it in one click; it never appears on the Discover/Create tabs.

### Task 3: one toolbar, one heading, chips

**Files:** Modify `LibraryInstancesPane.svelte`; `Library.svelte` (drop the duplicate header).

- [ ] Merge rows 2 and 3 into one: `[search] [All] [Updates N] [Favorites] [<folder>…] [sort] [view]`, with the action strip `New pack · Import · Update all (N) · N packs` beneath.
- [ ] Delete `Update`, `Help`, `Settings` and the account chip from the Library toolbar (rail gear / header `Switch` / command palette already own them).
- [ ] Replace the `Folders` dropdown and the `No group` dropdown with chips; keep grouping as a toggle.

**Accepts:** exactly one toolbar row above the list, one `Library` heading on screen, `Update all (N)` reachable without selecting a pack.

### Task 4: list-first rows

**Files:** Modify `LibraryInstancesPane.svelte` (new row markup + styles; the grid stays behind the existing view toggle).

- [ ] 64px row: 40px cover (letter fallback), name, `MC · loader`, relative last played, update badge, inline `Play`, `⋯` menu (Open in IDE, Manage…, Folder, Export, Remove).
- [ ] Relative time everywhere (`2h ago`, `Aug 16` → `6w ago` past 30 days), one formatter.
- [ ] Row `Play` launches `selected.path`.

**Accepts:** four packs occupy four rows (~256px) instead of a 500px grid with a hole; no empty dotted background under the list at 1440p.

## 6. Status after the first implementation pass (2026-09-27)

**Shipped** (verified on the running app, 2432×1385):

| Task | State | Notes |
|---|---|---|
| 1 — Library stops stealing the IDE project | **done** | `libraryFocusPath` + `focusLibraryPack()` in `lib/store.ts`; `selectInstance` no longer writes `projectPath`; `openInIde` is the explicit hand-off; remove/delete follow the IDE slot only when the pack IT held vanished. Verified live: dock kept naming "Fabulously Optimized" after clicking another pack in the list. |
| 2 — the IDE dock | **done** | Replaces the low-value status strip's left half; reads `projectPath`, hops back with `currentView = "ide"`, marks "· this pack" when the IDE already holds the focused one. |
| 3 — one toolbar, one heading | **partly** | Rows 2 and 3 merged into **one** toolbar (search · sort · view · count · Update next to Add/Folders); duplicate `Library` `<h2>` + "4 of 4" header deleted; `Help`, `Settings` and the account chip removed (command palette / rail gear / header `Switch` already own them). Still separate: the tab rail row (`Your packs / Discover / Create` + Import) — three tabs are navigation, not clutter, but merging it into the same row remains open. |
| 4 — list-first rows | **partly** | `viewMode` now defaults to `list` (grid stays one click away and is remembered per user). Rows show cover, name, `MC · loader`, relative time + playtime, update badge, inline `Play`. Still open: moving share/note into the `⋯` menu, and a single relative-time formatter (the detail column still prints `Sep 27, 2026`). |

**Not started** (still true after this pass, listed so it does not get lost):

- The Library column is still ~1065px inside a 2432px window; the list would breathe at ~1680px. Needs `.prism-grid-pane` / `.library` widths revisited.
- The inspector still carries ten controls (Play, update banner, Manage…, Open in IDE, Folder, share, settings, 4 stat boxes, notes, content). The plan folds share/settings into `⋯`.
- The bottom status strip is still clipped by the window edge.
- `Discover` / `Create` tabs were not touched beyond losing the duplicate header.

**Deviation from this plan:** Task 1 ships without the unit-test file the plan proposed — importing `lib/store.ts` in vitest pulls in the Tauri `invoke` bridge with no precedent in the suite. The behaviour was verified end-to-end instead: with the IDE holding pack A, clicking pack B in the Library left the dock (and therefore `projectPath`) on A.

## 7. Out of scope (explicit)

- Rewriting `LibraryInstanceContent` / `InstanceManager` internals.
- Multi-IDE (several packs in the IDE at once) — Task 1 keeps one IDE slot and makes the *Library* independent, which is what the brief asks for.
- Discover/Create tabs beyond losing their duplicated header.