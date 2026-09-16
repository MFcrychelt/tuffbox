/**
 * Minimal RU/EN localization for the app shell and library surfaces.
 *
 * The backend ships a small L10N map (`localize` / `list_localizations`
 * commands), but a synchronous front-end dictionary is the better transport:
 * no IPC latency on every label, trivially extensible, and testable in node.
 * The backend commands stay available but are no longer the plan of record.
 *
 * Coverage grows incrementally — `t()` falls back to the English string and
 * then to the key itself, so untranslated surfaces keep working unchanged.
 */
import { derived, writable } from "svelte/store";

export type Locale = "ru" | "en";

const LOCALE_KEY = "tuffbox.locale";

export const LOCALES: { id: Locale; label: string }[] = [
  { id: "en", label: "EN" },
  { id: "ru", label: "RU" },
];

function readStoredLocale(): Locale {
  try {
    const raw = localStorage.getItem(LOCALE_KEY);
    if (raw === "ru" || raw === "en") return raw;
  } catch {
    /* unavailable outside the webview */
  }
  return "en";
}

/** Active UI language; persisted to localStorage. */
export const locale = writable<Locale>(readStoredLocale());

locale.subscribe((l) => {
  try {
    localStorage.setItem(LOCALE_KEY, l);
  } catch {
    /* ignore */
  }
});

type Entry = { en: string; ru: string };

/** Dictionary — dotted keys, `{token}` placeholders filled by `t()`. */
const dict: Record<string, Entry> = {
  // ── Common verbs / nouns ──
  "common.play": { en: "Play", ru: "Играть" },
  "common.stop": { en: "Stop", ru: "Стоп" },
  "common.settings": { en: "Settings", ru: "Настройки" },
  "common.help": { en: "Help", ru: "Помощь" },
  "common.refresh": { en: "Refresh", ru: "Обновить" },
  "common.export": { en: "Export", ru: "Экспорт" },
  "common.more": { en: "More", ru: "Ещё" },
  "common.apply": { en: "Apply", ru: "Применить" },
  "common.cancel": { en: "Cancel", ru: "Отмена" },
  "common.save": { en: "Save", ru: "Сохранить" },
  "common.close": { en: "Close", ru: "Закрыть" },
  "common.delete": { en: "Delete", ru: "Удалить" },
  "common.done": { en: "Done", ru: "Готово" },

  // ── Sidebar rail ──
  "nav.home": { en: "Java Edition", ru: "Java Edition" },
  "nav.library": { en: "Library", ru: "Библиотека" },
  "nav.ide": { en: "IDE", ru: "IDE" },
  "nav.addInstance": { en: "Add instance", ru: "Добавить сборку" },
  "nav.logs": { en: "Logs", ru: "Логи" },
  "nav.settings": { en: "Settings", ru: "Настройки" },
  "nav.profile": { en: "Profile", ru: "Профиль" },
  "nav.app": { en: "App", ru: "Приложение" },

  // ── Library toolbar ──
  "library.title": { en: "Library", ru: "Библиотека" },
  "library.addInstance": { en: "Add Instance", ru: "Добавить сборку" },
  "library.addInstanceTitle": {
    en: "Add an instance to the library",
    ru: "Добавить сборку в библиотеку",
  },
  "library.folders": { en: "Folders", ru: "Папки" },
  "library.update": { en: "Update", ru: "Обновить" },
  "library.account": { en: "Account", ru: "Аккаунт" },
  "library.filterPlaceholder": { en: "Filter instances…", ru: "Фильтр сборок…" },
  "library.filterAria": { en: "Filter instances", ru: "Фильтр сборок" },
  "library.clearFilter": { en: "Clear filter", ru: "Сбросить фильтр" },
  "library.sort": { en: "Sort instances", ru: "Сортировка сборок" },
  "library.sortRecent": { en: "Last played", ru: "Недавние" },
  "library.sortName": { en: "Name", ru: "По имени" },
  "library.sortPlaytime": { en: "Most played", ru: "По времени игры" },
  "library.layout": { en: "Layout", ru: "Вид" },
  "library.gridView": { en: "Grid view", ru: "Плитка" },
  "library.listView": { en: "List view", ru: "Список" },

  // ── Library empty states ──
  "library.emptyTitle": { en: "No instances yet", ru: "Пока нет сборок" },
  "library.emptyBody": {
    en: "Create or import a pack to build your library.",
    ru: "Создайте или импортируйте сборку, чтобы наполнить библиотеку.",
  },
  "library.noMatchesTitle": { en: "No matches", ru: "Ничего не найдено" },
  "library.noMatchesBody": {
    en: "Nothing matches “{filter}”. Try another name, version or loader.",
    ru: "По запросу «{filter}» ничего нет. Попробуйте другое имя, версию или загрузчик.",
  },

  // ── Tile / row actions ──
  "library.playAria": { en: "Play {name}", ru: "Играть: {name}" },
  "library.stopAria": { en: "Stop {name}", ru: "Остановить {name}" },
  "library.openInIde": { en: "Open in IDE", ru: "Открыть в IDE" },
  "library.openFolder": { en: "Open folder", ru: "Открыть папку" },
  "library.lastPlayed": { en: "Last played", ru: "Последний запуск" },
  "library.playtime": { en: "Total playtime", ru: "Всего в игре" },
  "library.playStats": { en: "Play statistics", ru: "Статистика игры" },
  "library.changeGroup": { en: "Change group", ru: "Сменить группу" },

  // ── Update center ──
  "library.updatesTitle": {
    en: "Update every mod with a newer release",
    ru: "Обновить все моды до свежих версий",
  },
  "library.modsHaveUpdates": {
    en: "{n} mods have updates",
    ru: "Модов с обновлениями: {n}",
  },
  "library.modHasUpdate": { en: "1 mod has updates", ru: "1 мод с обновлением" },
  "library.updateAll": { en: "Update all", ru: "Обновить всё" },
  "library.updatesBadgeTitle": {
    en: "{n} mods have updates",
    ru: "Модов с обновлениями: {n}",
  },

  // ── Instance side panel ──
  "library.manageInstance": { en: "Manage instance", ru: "Управление сборкой" },
  "library.manageEllipsis": { en: "Manage…", ru: "Управлять…" },
  "library.folder": { en: "Folder", ru: "Папка" },
  "library.manage": { en: "Manage", ru: "Управлять" },
  "library.moreActions": { en: "More actions", ru: "Ещё действия" },
  "library.sideMeta": { en: "Instance actions", ru: "Действия сборки" },
  "library.playTime": { en: "Play time", ru: "Время в игре" },
  "library.java": { en: "Java", ru: "Java" },
  "library.memory": { en: "Memory", ru: "Память" },
  "library.notes": { en: "Notes", ru: "Заметки" },
  "library.notesPlaceholder": {
    en: "Reminders, TODOs, server IPs… (saved automatically)",
    ru: "Напоминания, планы, адреса серверов… (сохраняется автоматически)",
  },
  "library.content": { en: "Content", ru: "Содержимое" },
  "library.selectInstance": {
    en: "Select an instance to see its details",
    ru: "Выберите сборку, чтобы увидеть подробности",
  },
  "library.modsBackupsHealth": {
    en: "Mods, backups and health for this pack",
    ru: "Моды, бэкапы и состояние сборки",
  },
  "library.openIdeAria": { en: "Open in IDE", ru: "Открыть в IDE" },
  "library.openFolderAria": { en: "Open the instance folder", ru: "Открыть папку сборки" },

  // ── Group dialog ──
  "library.groupDialogTitle": { en: "Change Group", ru: "Смена группы" },
  "library.groupDialogBody": {
    en: "Move “{name}” into a group.",
    ru: "Переместить «{name}» в группу.",
  },
  "library.groupOrNew": { en: "Or type a new name", ru: "Или введите новое имя" },

  // ── Header action buttons ──
  "library.renameInstance": { en: "Rename instance", ru: "Переименовать сборку" },
  "library.copyInstance": { en: "Copy instance", ru: "Копировать сборку" },
  "library.importGithub": { en: "Import from GitHub", ru: "Импорт с GitHub" },
  "library.installGithubPack": { en: "Install GitHub pack", ru: "Установить пак с GitHub" },

  // ── Language switcher ──
  "nav.language": { en: "Language", ru: "Язык" },
};

/** Translate `key` for the active locale, filling `{token}` placeholders. */
export const t = derived(locale, (l) => {
  return (key: string, params?: Record<string, string | number>): string => {
    const entry = dict[key];
    const raw = entry ? entry[l] : key;
    if (!params) return raw;
    return raw.replace(/\{(\w+)\}/g, (m, name: string) =>
      name in params ? String(params[name]) : m,
    );
  };
});

export type Translate = typeof t extends import("svelte/store").Readable<infer F> ? F : never;
