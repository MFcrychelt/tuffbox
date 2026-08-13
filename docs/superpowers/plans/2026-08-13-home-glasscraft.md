# Glasscraft Home Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Сделать главный экран солидным modern-glassy: процедурные Minecraft-текстуры (CSS-only), glass-панели на Home, глобальный текстурный слой app-shell и 4 новые биом-темы.

**Architecture:** Всё через существующую токен-систему: новый слой `textures.css` (utility-классы `.tex-*` + дефолтный `--app-shell-bg` со speckle), апгрейд glass-токенов в `styles.css`, точечные правки `HomeHero.svelte`/`Dashboard.svelte`, 4 новых `[data-theme]` блока + shared Glasscraft-блок в `themes.css`, записи в каталоге `themes.ts`. Ни одного PNG-ассета.

**Tech Stack:** Svelte 5 (runes), чистый CSS (gradient layers, color-mix, backdrop-filter), Tauri.

**Spec:** `docs/superpowers/specs/2026-08-13-home-glasscraft-redesign-design.md` (commit 8632d99f)

## Global Constraints

- Svelte 5 only: `$state`/`$derived`/`$effect`, `onclick` (не `on:click`), `{#snippet}` (не `<slot>`).
- Border-radius только `var(--border-radius-sm|md|lg|xl)`; мелкий декор ≤4px и 999px допустим. Проверка: `npm run lint:tokens` (из `apps/tuffbox-desktop`).
- `html.potato-pc` и `prefers-reduced-motion`: текстуры и blur отключаются (см. Task 2).
- Никаких PNG/JPG ассетов — только CSS-градиенты. `image-rendering: pixelated` на текстурных слоях.
- Каждый коммит добавляет ТОЛЬКО файлы своей задачи (`git add <file...>`) — в репо много незакоммиченной чужой работы, не подметать её.
- Рабочая директория для npm-команд: `apps/tuffbox-desktop`.
- Не ломать тему `minecraft` (её overrides изолированы под `html[data-theme="minecraft"]`).

---

### Task 1: Glass-токены + утилита `.glass-panel`

**Files:**
- Modify: `apps/tuffbox-desktop/src/styles.css` (:root glass-блок ~L61-72; potato-блок ~L436-441; reduced-motion ~L341-347; utilities рядом с `.tb-truncate` ~L414)

**Interfaces:**
- Produces: CSS-токен `--glass-saturate` (default `140%`), утилита `.glass-panel`. Потребляется в Task 3/4 и существующими glass-поверхностями (hero toolbar, crash banner).

- [ ] **Step 1: Обновить :root glass-токены**

В `styles.css`, в `:root` заменить блок:

```css
  --glass-bg: rgba(11, 11, 13, 0.42);
  --glass-border: color-mix(in srgb, #fff 14%, var(--border-color));
  --glass-blur: 10px;
  --glass-highlight: color-mix(in srgb, #fff 14%, transparent);
```

на:

```css
  --glass-bg: rgba(11, 11, 13, 0.42);
  --glass-border: color-mix(in srgb, #fff 14%, var(--border-color));
  --glass-blur: 14px;
  --glass-saturate: 140%;
  --glass-highlight: color-mix(in srgb, #fff 14%, transparent);
```

- [ ] **Step 2: Добавить утилиту `.glass-panel`**

Сразу после блока `.tb-truncate-2` в `styles.css`:

```css
/* Frosted glass surface — panels and cards over textured/ambient shell. */
.glass-panel {
  background: var(--glass-bg);
  border: 1px solid var(--glass-border);
  -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
  backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
  box-shadow:
    var(--shadow-md),
    inset 0 1px 0 var(--glass-highlight);
}
```

- [ ] **Step 3: Гарды potato-pc / reduced-motion**

В `html.potato-pc { ... }` (там уже `--glass-blur: 0;`) добавить `--glass-saturate: 100%;`.
В `@media (prefers-reduced-motion: reduce) { html { ... } }` рядом с `--glass-blur: 0;` добавить `--glass-saturate: 100%;`.

- [ ] **Step 4: Проверка**

Run: `npm run lint:tokens` (cwd `apps/tuffbox-desktop`)
Expected: PASS (без ошибок; новых radius-литералов нет).

- [ ] **Step 5: Commit**

```bash
git add apps/tuffbox-desktop/src/styles.css
git commit -m "feat(ui): glass tokens v2 (blur 14px + saturate) and .glass-panel utility"
```

---

### Task 2: Процедурные текстуры — `textures.css`

**Files:**
- Create: `apps/tuffbox-desktop/src/styles/textures.css`
- Modify: `apps/tuffbox-desktop/src/main.ts` (добавить import после `./styles/themes.css`)

**Interfaces:**
- Produces: классы `.tex-stone`, `.tex-dirt`, `.tex-grass`, `.tex-deepslate`, `.tex-netherrack`, `.tex-sculk`, `.tex-amethyst`; токены `--tex-size` (16px), `--tex-speck-a/b/c` (per-class overrides); дефолтный `--app-shell-bg` со stone-speckle (наследуют темы без своего `--app-shell-bg`). Потребляется в Task 3 (hero fallback), Task 4 (skin vitrine), Task 5 (биом-темы переопределяют speckle-цвета и `--app-shell-bg`).

**Техника:** каждая текстура — один общий слоёный стек на базовом селекторе; вариант задаёт только 3 цвета-«крошки» через `--tex-speck-*`. Тайл 16px с кластерами ~2px + checker 8px = пиксель-шум в духе MC.

- [ ] **Step 1: Создать `textures.css`**

```css
/* Procedural Minecraft-flavoured pixel textures — pure CSS, zero assets.
   Layer stack is shared; each variant only re-colors the three speckle
   tones via --tex-speck-a (light fleck) / --tex-speck-b (dark fleck) /
   --tex-speck-c (checker). Themes may override the vars per data-theme. */

:root {
  --tex-size: 16px;
  --tex-speck-a: rgba(255, 255, 255, 0.05);
  --tex-speck-b: rgba(0, 0, 0, 0.16);
  --tex-speck-c: rgba(0, 0, 0, 0.06);

  /* Default app shell: faint stone speckle over the base color. Themes
     that set their own --app-shell-bg are untouched (higher specificity). */
  --app-shell-bg:
    radial-gradient(circle at 22% 30%, rgba(255, 255, 255, 0.035) 0 10%, transparent 11%),
    radial-gradient(circle at 68% 12%, rgba(0, 0, 0, 0.12) 0 12%, transparent 13%),
    radial-gradient(circle at 82% 64%, rgba(255, 255, 255, 0.03) 0 9%, transparent 10%),
    radial-gradient(circle at 38% 78%, rgba(0, 0, 0, 0.10) 0 13%, transparent 14%),
    repeating-conic-gradient(rgba(0, 0, 0, 0.045) 0% 25%, transparent 0% 50%),
    var(--bg-primary);
}

.app-shell {
  background-size:
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    calc(var(--tex-size) / 2) calc(var(--tex-size) / 2),
    auto;
}

/* Shared speckle stack for component surfaces (hero fallback, vitrine…). */
:where(.tex-stone, .tex-dirt, .tex-grass, .tex-deepslate, .tex-netherrack, .tex-sculk, .tex-amethyst) {
  background-image:
    radial-gradient(circle at 22% 30%, var(--tex-speck-a) 0 10%, transparent 11%),
    radial-gradient(circle at 68% 12%, var(--tex-speck-b) 0 12%, transparent 13%),
    radial-gradient(circle at 82% 64%, var(--tex-speck-a) 0 9%, transparent 10%),
    radial-gradient(circle at 38% 78%, var(--tex-speck-b) 0 13%, transparent 14%),
    radial-gradient(circle at 8% 88%, var(--tex-speck-c) 0 8%, transparent 9%),
    repeating-conic-gradient(var(--tex-speck-c) 0% 25%, transparent 0% 50%);
  background-size:
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    calc(var(--tex-size) / 2) calc(var(--tex-size) / 2);
  image-rendering: pixelated;
}

.tex-stone {
  --tex-speck-a: rgba(255, 255, 255, 0.05);
  --tex-speck-b: rgba(0, 0, 0, 0.16);
  --tex-speck-c: rgba(0, 0, 0, 0.06);
}

.tex-dirt {
  --tex-speck-a: rgba(196, 146, 98, 0.10);
  --tex-speck-b: rgba(28, 17, 9, 0.22);
  --tex-speck-c: rgba(64, 42, 24, 0.10);
}

.tex-grass {
  --tex-speck-a: rgba(122, 220, 110, 0.10);
  --tex-speck-b: rgba(24, 34, 14, 0.20);
  --tex-speck-c: rgba(70, 120, 50, 0.10);
}

.tex-deepslate {
  --tex-speck-a: rgba(160, 190, 210, 0.045);
  --tex-speck-b: rgba(0, 0, 0, 0.26);
  --tex-speck-c: rgba(10, 14, 18, 0.10);
}

.tex-netherrack {
  --tex-speck-a: rgba(255, 122, 82, 0.08);
  --tex-speck-b: rgba(22, 5, 7, 0.26);
  --tex-speck-c: rgba(120, 32, 28, 0.10);
}

.tex-sculk {
  --tex-speck-a: rgba(47, 214, 200, 0.10);
  --tex-speck-b: rgba(0, 0, 0, 0.28);
  --tex-speck-c: rgba(16, 60, 64, 0.12);
}

.tex-amethyst {
  --tex-speck-a: rgba(198, 156, 255, 0.10);
  --tex-speck-b: rgba(10, 5, 20, 0.24);
  --tex-speck-c: rgba(96, 62, 150, 0.10);
}

/* Potato PC: no texture repaint cost anywhere. */
html.potato-pc :where(.tex-stone, .tex-dirt, .tex-grass, .tex-deepslate, .tex-netherrack, .tex-sculk, .tex-amethyst) {
  background-image: none;
}

html.potato-pc {
  --app-shell-bg: var(--bg-primary);
}
```

- [ ] **Step 2: Подключить в `main.ts`**

```ts
import "./styles.css";
import "./styles/themes.css";
import "./styles/textures.css";
```

- [ ] **Step 3: Проверка**

Run: `npm run lint:tokens; npm run build`
Expected: оба PASS (vite build собирает CSS без ошибок).

- [ ] **Step 4: Commit**

```bash
git add apps/tuffbox-desktop/src/styles/textures.css apps/tuffbox-desktop/src/main.ts
git commit -m "feat(ui): procedural CSS pixel-texture layer (.tex-*) and textured default app shell"
```

---

### Task 3: HomeHero — «cave scene» fallback + grass-кромка + glass-выравнивание

**Files:**
- Modify: `apps/tuffbox-desktop/src/components/HomeHero.svelte` (style: `.poster-procedural` ~L366-373; `.poster-toolbar-bar` ~L409-418; `.storefront-ghost` ~L686-692; `.crash-fix-banner` ~L694-706)

**Interfaces:**
- Consumes: `.tex-deepslate`-подобный speckle (встроен прямо в `.poster-procedural`, т.к. у элемента уже есть свои gradient-слои — класс `.tex-*` не комбинируется), `--glass-saturate` из Task 1.
- Produces: ничего наружу.

- [ ] **Step 1: Текстурный fallback вместо плоского градиента**

Заменить `.poster-procedural` на:

```css
  .poster-procedural {
    position: absolute;
    inset: 0;
    background-image:
      radial-gradient(ellipse 70% 80% at 18% 85%, color-mix(in srgb, var(--accent-primary) 28%, transparent), transparent 55%),
      radial-gradient(ellipse 55% 60% at 92% 8%, color-mix(in srgb, var(--accent-secondary) 22%, transparent), transparent 50%),
      radial-gradient(circle at 22% 30%, var(--tex-speck-a, rgba(160, 190, 210, 0.045)) 0 10%, transparent 11%),
      radial-gradient(circle at 68% 12%, var(--tex-speck-b, rgba(0, 0, 0, 0.26)) 0 12%, transparent 13%),
      radial-gradient(circle at 82% 64%, var(--tex-speck-a, rgba(160, 190, 210, 0.045)) 0 9%, transparent 10%),
      radial-gradient(circle at 38% 78%, var(--tex-speck-b, rgba(0, 0, 0, 0.26)) 0 13%, transparent 14%),
      repeating-conic-gradient(var(--tex-speck-c, rgba(10, 14, 18, 0.10)) 0% 25%, transparent 0% 50%);
    background-size:
      auto, auto,
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      calc(var(--tex-size, 16px) / 2) calc(var(--tex-size, 16px) / 2);
    background-color: var(--bg-primary);
    image-rendering: pixelated;
  }

  /* Grass-block edge along the bottom of the fallback scene. */
  .poster-procedural::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 6px;
    background-image:
      repeating-conic-gradient(color-mix(in srgb, var(--accent-primary) 55%, #2c1f10) 0% 25%, #3a2a16 0% 50%);
    background-size: 8px 6px;
    image-rendering: pixelated;
    opacity: 0.85;
  }
```

- [ ] **Step 2: Glass-выравнивание**

В `.poster-toolbar-bar`, `.storefront-ghost`, `.crash-fix-banner` заменить оба backdrop-filter (и `-webkit-` версии):
`blur(var(--glass-blur, 10px))` → `blur(var(--glass-blur)) saturate(var(--glass-saturate, 100%))`.

- [ ] **Step 3: Гард potato-pc**

В существующем блоке `:global(html.potato-pc) .poster...` (рядом, ~L327-330) добавить:

```css
  :global(html.potato-pc) .poster-procedural {
    background-image: none;
  }
```

(после этого останется только `background-color: var(--bg-primary)`).

- [ ] **Step 4: Проверка**

Run: `npm run lint:tokens; npm run check`
Expected: PASS, 0 новых ошибок svelte-check.

- [ ] **Step 5: Commit**

```bash
git add apps/tuffbox-desktop/src/components/HomeHero.svelte
git commit -m "feat(home): textured cave fallback with grass edge in hero, glass saturate alignment"
```

---

### Task 4: Dashboard — glass-панели + витрина скина

**Files:**
- Modify: `apps/tuffbox-desktop/src/components/Dashboard.svelte` (style: `.skin-panel` ~L785-795, `.home-feed :global(.youtube-feed)` ~L735-742)

**Interfaces:**
- Consumes: `--glass-bg`, `--glass-border`, `--glass-blur`, `--glass-saturate`, `--glass-highlight` (Task 1), speckle-токены (Task 2).
- Produces: ничего наружу.

- [ ] **Step 1: Skin-панель → glass + каменная витрина под 3D-превью**

В `.skin-panel` заменить `background: var(--bg-secondary);` на:

```css
  background: var(--glass-bg);
  -webkit-backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
  backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturate));
```

И в том же `<style>` добавить витрину (speckle под полупрозрачным canvas превью):

```css
  .skin-panel :global(.skin-3d-container) {
    background-image:
      radial-gradient(circle at 22% 30%, var(--tex-speck-a, rgba(255, 255, 255, 0.05)) 0 10%, transparent 11%),
      radial-gradient(circle at 68% 12%, var(--tex-speck-b, rgba(0, 0, 0, 0.16)) 0 12%, transparent 13%),
      radial-gradient(circle at 82% 64%, var(--tex-speck-a, rgba(255, 255, 255, 0.05)) 0 9%, transparent 10%),
      radial-gradient(circle at 38% 78%, var(--tex-speck-b, rgba(0, 0, 0, 0.16)) 0 13%, transparent 14%),
      repeating-conic-gradient(var(--tex-speck-c, rgba(0, 0, 0, 0.06)) 0% 25%, transparent 0% 50%);
    background-size:
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      var(--tex-size, 16px) var(--tex-size, 16px),
      calc(var(--tex-size, 16px) / 2) calc(var(--tex-size, 16px) / 2);
    image-rendering: pixelated;
  }

  :global(html.potato-pc) .skin-panel :global(.skin-3d-container) {
    background-image: none;
  }
```

Если при визуальной проверке окажется, что `.skin-3d-container` рисует собственный непрозрачный фон (текстуры не видно) — применить тот же блок к `:global(.skin-3d-wrap)` вместо container (проверить в dev, оставить один вариант).

- [ ] **Step 2: YouTube-лента → glass**

В `.home-feed :global(.youtube-feed)` заменить `background: var(--bg-secondary);` на `background: var(--glass-bg);` и добавить туда же обе строки `backdrop-filter` (как в Step 1).

- [ ] **Step 3: Проверка**

Run: `npm run lint:tokens; npm run check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/tuffbox-desktop/src/components/Dashboard.svelte
git commit -m "feat(home): glass skin panel with stone vitrine and glass youtube feed"
```

---

### Task 5: Биом-темы — Overworld / Nether / Deep Dark / Amethyst

**Files:**
- Modify: `apps/tuffbox-desktop/src/styles/themes.css` (новые блоки — после `[data-theme="minecraft"]`-секции, перед shared Minimal-блоком ~L533)
- Modify: `apps/tuffbox-desktop/src/lib/themes.ts` (ThemeId + THEMES)

**Interfaces:**
- Consumes: `--tex-*` speckle-переопределения (Task 2), glass-токены (Task 1).
- Produces: `ThemeId` += `"overworld" | "nether" | "deepdark" | "amethyst"`; 4 записи `THEMES` с shades; per-theme `--app-shell-bg` со своей speckle-палитрой.

- [ ] **Step 1: Каталог тем**

В `themes.ts` расширить `ThemeId` и `THEMES`:

```ts
export type ThemeId =
  | "tuffbox"
  // …existing…
  | "minecraft"
  | "overworld"
  | "nether"
  | "deepdark"
  | "amethyst";
```

```ts
  { id: "minecraft", label: "Minecraft", shades: ["#212121", "#313131", "#3fad32"] },
  { id: "overworld", label: "Overworld", shades: ["#0d100c", "#1a2416", "#52d465"] },
  { id: "nether", label: "Nether", shades: ["#140608", "#241012", "#ff6a2b"] },
  { id: "deepdark", label: "Deep Dark", shades: ["#05090e", "#0a141c", "#2fd6c8"] },
  { id: "amethyst", label: "Amethyst", shades: ["#0f0a18", "#1c1230", "#b07cff"] },
```

- [ ] **Step 2: Четыре `[data-theme]` блока в themes.css**

Каждый блок: полная палитра + `--app-shell-bg` со speckle-слоями (первые 5 слоёв = текстура, затем ambient, затем цвет) + companion-правило `background-size` для `.app-shell`. Шаблон (показан для overworld; остальные — по таблице палитр ниже, тот же каркас):

```css
[data-theme="overworld"] {
  --bg-primary: #0d100c;
  --bg-secondary: #131811;
  --bg-tertiary: #1a2416;
  --bg-elevated: #22301c;
  --bg-hover: #2a3c22;
  --bg-active: #35502a;
  --text-primary: #f2f8ee;
  --text-secondary: #c4d4b8;
  --text-muted: #7f9474;
  --accent-primary: #52d465;
  --accent-hover: #6fe083;
  --accent-secondary: #f0a44a;
  --accent-danger: #ef5350;
  --accent-warning: #f0c14b;
  --border-color: #26341f;
  --shadow-sm: 0 1px 2px rgba(4, 8, 3, 0.5);
  --shadow-md: 0 6px 18px rgba(4, 8, 3, 0.5);
  --shadow-lg: 0 16px 40px rgba(4, 8, 3, 0.55);
  --on-accent: #07130a;
  --ambient-a: rgba(82, 212, 101, 0.07);
  --ambient-b: rgba(240, 164, 74, 0.05);
  --tex-speck-a: rgba(196, 146, 98, 0.10);
  --tex-speck-b: rgba(28, 17, 9, 0.22);
  --tex-speck-c: rgba(70, 120, 50, 0.10);
  --glass-bg: rgba(13, 16, 12, 0.55);
  --glass-highlight: color-mix(in srgb, #fff 12%, transparent);
  --hero-scrim:
    linear-gradient(90deg, rgba(13, 16, 12, 0.72) 0%, transparent 55%),
    linear-gradient(0deg, rgba(13, 16, 12, 0.68) 0%, transparent 52%);
  --app-shell-bg:
    radial-gradient(circle at 22% 30%, var(--tex-speck-a) 0 10%, transparent 11%),
    radial-gradient(circle at 68% 12%, var(--tex-speck-b) 0 12%, transparent 13%),
    radial-gradient(circle at 82% 64%, var(--tex-speck-a) 0 9%, transparent 10%),
    radial-gradient(circle at 38% 78%, var(--tex-speck-b) 0 13%, transparent 14%),
    repeating-conic-gradient(var(--tex-speck-c) 0% 25%, transparent 0% 50%),
    radial-gradient(ellipse 85% 55% at 8% -8%, var(--ambient-a), transparent 58%),
    radial-gradient(ellipse 70% 45% at 96% 4%, var(--ambient-b), transparent 52%),
    var(--bg-primary);
}

html[data-theme="overworld"] .app-shell {
  background-size:
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    var(--tex-size) var(--tex-size),
    calc(var(--tex-size) / 2) calc(var(--tex-size) / 2),
    auto, auto, auto;
}
```

Палитры остальных (тот же каркас, подставить значения):

| token | nether | deepdark | amethyst |
|-------|--------|----------|----------|
| bg-primary | `#140608` | `#05090e` | `#0f0a18` |
| bg-secondary | `#1c0a0c` | `#081018` | `#150e22` |
| bg-tertiary | `#241012` | `#0a141c` | `#1c1230` |
| bg-elevated | `#2e1618` | `#0e1c26` | `#25183e` |
| bg-hover | `#3a1d20` | `#14262f` | `#2f2050` |
| bg-active | `#482528` | `#1a3240` | `#3b2a62` |
| text-primary | `#fdf1ec` | `#e8f6f6` | `#f4efff` |
| text-secondary | `#e8c4b8` | `#a8ccd0` | `#c8bce8` |
| text-muted | `#a88478` | `#5f8890` | `#8d7fb0` |
| accent-primary | `#ff6a2b` | `#2fd6c8` | `#b07cff` |
| accent-hover | `#ff8547` | `#54e4d6` | `#c495ff` |
| accent-secondary | `#ffc24b` | `#1e9e93` | `#8f6fd0` |
| accent-danger | `#ff4545` | `#f87171` | `#fb7185` |
| accent-warning | `#fcd34d` | `#eab308` | `#fbbf24` |
| border-color | `#33141a` | `#12242e` | `#2a1d44` |
| on-accent | `#1a0803` | `#03201c` | `#170b28` |
| ambient-a | `rgba(255,106,43,0.08)` | `rgba(47,214,200,0.06)` | `rgba(176,124,255,0.08)` |
| ambient-b | `rgba(255,60,60,0.05)` | `rgba(30,100,120,0.05)` | `rgba(124,88,200,0.06)` |
| tex-speck-a | `rgba(255,122,82,0.08)` | `rgba(47,214,200,0.10)` | `rgba(198,156,255,0.10)` |
| tex-speck-b | `rgba(22,5,7,0.26)` | `rgba(0,0,0,0.28)` | `rgba(10,5,20,0.24)` |
| tex-speck-c | `rgba(120,32,28,0.10)` | `rgba(16,60,64,0.12)` | `rgba(96,62,150,0.10)` |
| glass-bg | `rgba(20,6,8,0.55)` | `rgba(5,9,14,0.55)` | `rgba(15,10,24,0.55)` |
| hero-scrim rgba-база | `20,6,8` | `5,9,14` | `15,10,24` |

- [ ] **Step 3: Shared «Glasscraft»-блок**

После четырёх блоков:

```css
/* ── Glasscraft biome themes: glass cards, ambient rail/header, glowing CTAs ── */
:is(
  [data-theme="overworld"],
  [data-theme="nether"],
  [data-theme="deepdark"],
  [data-theme="amethyst"]
) {
  --accent-gradient: linear-gradient(145deg, var(--accent-primary) 0%, var(--accent-secondary) 100%);
  --brand-mark-gradient: var(--accent-gradient);
  --brand-mark-fg: var(--on-accent);
  --brand-mark-shadow: 0 4px 12px color-mix(in srgb, var(--accent-primary) 18%, transparent);
  --rail-bg: linear-gradient(
    180deg,
    color-mix(in srgb, var(--accent-primary) 6%, var(--bg-secondary)) 0%,
    color-mix(in srgb, var(--bg-secondary) 88%, transparent) 42%,
    color-mix(in srgb, var(--bg-primary) 78%, transparent) 100%
  );
  --rail-border: color-mix(in srgb, var(--accent-primary) 8%, var(--border-color));
  --rail-backdrop: saturate(140%) blur(22px);
  --header-bg: color-mix(in srgb, var(--bg-secondary) 72%, transparent);
  --header-border: color-mix(in srgb, var(--accent-primary) 6%, var(--border-color));
  --header-backdrop: saturate(140%) blur(18px);
}

/* Quiet glass cards. */
:is(
  [data-theme="overworld"],
  [data-theme="nether"],
  [data-theme="deepdark"],
  [data-theme="amethyst"]
)
  :where(.card, .panel, .tb-card, .projects-section, .settings-grid > .card) {
  background: color-mix(in srgb, var(--bg-secondary) 82%, transparent);
  border-color: var(--border-color);
  box-shadow: var(--shadow-sm);
  -webkit-backdrop-filter: blur(12px) saturate(130%);
  backdrop-filter: blur(12px) saturate(130%);
}

/* Primary CTAs: solid accent with soft biome glow. */
:is(
  [data-theme="overworld"],
  [data-theme="nether"],
  [data-theme="deepdark"],
  [data-theme="amethyst"]
)
  :is(button.primary, button.accent, .play-btn:not(.stop), .primary-action, .tb-btn.primary):not(:disabled) {
  background: var(--accent-gradient);
  color: var(--on-accent);
  border-color: transparent;
  box-shadow: 0 6px 22px color-mix(in srgb, var(--accent-primary) 30%, transparent);
}

:is(
  [data-theme="overworld"],
  [data-theme="nether"],
  [data-theme="deepdark"],
  [data-theme="amethyst"]
)
  :is(button.primary, button.accent, .play-btn:not(.stop), .primary-action, .tb-btn.primary):not(:disabled):hover {
  filter: brightness(1.08);
  box-shadow: 0 8px 28px color-mix(in srgb, var(--accent-primary) 42%, transparent);
}
```

- [ ] **Step 4: Проверка**

Run: `npm run lint:tokens; npm run check; npm run build`
Expected: все PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/tuffbox-desktop/src/styles/themes.css apps/tuffbox-desktop/src/lib/themes.ts
git commit -m "feat(themes): Glasscraft biome set — Overworld, Nether, Deep Dark, Amethyst"
```

---

### Task 6: Финальная проверка и визуальный проход

**Files:** — (только проверки)

- [ ] **Step 1: Статические проверки**

Run (cwd `apps/tuffbox-desktop`): `npm run lint:perf; npm run check; npm run build`
Expected: всё зелёное; bundle-budget не упал (CSS прирост ~10KB допустим; если упал — проверить лимит в `scripts/check-bundle-budget.mjs` и при необходимости обоснованно поднять порог в конфиге скрипта, зафиксировав в коммите).

- [ ] **Step 2: Визуальный чеклист** (через `npm run tauri:dev` или отклонить как ручной шаг пользователя)

1. Home на теме `tuffbox`: фон окна с лёгким stone-speckle; hero fallback с текстурой и grass-кромкой (инстанс без обложки); skin-панель и лента — glass.
2. Каждая из 4 биом-тем: Settings → Appearance → переключить — Home читаем, текстура в палитре темы, CTA со свечением.
3. Тема `minecraft`: без изменений (плоская, свой layout).
4. `potato-pc` (Settings → Performance): текстуры и blur отключены.
5. Длинный ник/имя инстанса: обрезка по ellipsis не сломалась.

- [ ] **Step 3: Финальный коммит (если были правки по итогам проверок)**

```bash
git add <только затронутые файлы>
git commit -m "fix(ui): glasscraft review pass"
```

---

## Self-Review выполнен при написании

- Spec coverage: текстуры (Task 2,3,4), glass-апгрейд (Task 1,3,4), Home (Task 3,4), биом-темы (Task 5), гарды potato/reduced-motion (Task 1,2,3), критерии приёмки (Task 6). Расхождение со спекой осознанное: potato глушит `.tex-*` классами и `--app-shell-bg: var(--bg-primary)` вместо `--tex-opacity` (alpha внутри gradient-стека токеном не управляется) — эффект тот же.
- Placeholders: нет — весь CSS/TS приведён полностью, кроме табличной подстановки палитр Task 5 (все значения даны в таблице).
- Type consistency: `--tex-speck-a/b/c`, `--tex-size`, `--glass-saturate`, `.tex-*`, ThemeId — согласованы между задачами.
