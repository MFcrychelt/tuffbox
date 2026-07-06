<script lang="ts">
  import { Palette, Info, Moon, Sun, Command } from "lucide-svelte";

  let theme = localStorage.getItem("tuffbox-theme") || "dark";
  let shortcuts: any[] = [];
  let shortcutsOpen = false;
  let appVersion = "";
  let updateCheck: any = null;

  async function checkUpdate() {
    try { updateCheck = await invoke("check_for_app_update"); } catch { updateCheck = null; }
  }

  import { onMount } from "svelte";
  onMount(async () => {
    document.documentElement.setAttribute("data-theme", theme);
    try { shortcuts = await invoke("get_keyboard_shortcuts"); } catch {}
  });

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("tuffbox-theme", theme);
    document.documentElement.setAttribute("data-theme", theme);
  }

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("tuffbox-theme", theme);
    document.documentElement.setAttribute("data-theme", theme);
  }

  import { onMount } from "svelte";
  onMount(() => {
    document.documentElement.setAttribute("data-theme", theme);
  });
</script>

<div class="settings">
  <div class="settings-grid">
    <section class="card">
      <div class="card-title">
        <Palette size={18} />
        <h3>Appearance</h3>
      </div>
      <div class="field">
        <label for="theme-select">
          <Moon size={14} />
          Theme
        </label>
        <button class="theme-toggle" on:click={toggleTheme}>
          {#if theme === "dark"}
            <Moon size={16} /> Dark theme
          {:else}
            <Sun size={16} /> Light theme
          {/if}
        </button>
      </div>
    </section>

    <section class="card">
      <div class="card-title">
        <Command size={18} />
        <h3>Keyboard shortcuts</h3>
      </div>
      <button class="ghost" on:click={() => (shortcutsOpen = !shortcutsOpen)}>
        {shortcutsOpen ? "Hide" : "Show"} shortcuts ({shortcuts.length})
      </button>
      {#if shortcutsOpen}
        <div class="shortcut-list">
          {#each shortcuts as s}
            <div class="shortcut-row">
              <kbd>{s.key}</kbd>
              <span>{s.action}</span>
              <small>{s.context}</small>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section class="card">
      <div class="card-title">
        <Info size={18} />
        <h3>About</h3>
      </div>
      <button class="ghost" on:click={async () => { appVersion = await invoke("get_app_version"); await checkUpdate(); }}>
        Check for updates
      </button>
      {#if updateCheck}
        <div class="update-info">
          {#if updateCheck.updateAvailable}<span class="update-avail">Update available: {updateCheck.latestVersion}</span>
          {:else}<span class="update-ok">Up to date ({updateCheck.currentVersion})</span>{/if}
        </div>
      {/if}
      <div class="about">
        <div class="logo-big">T</div>
        <div>
          <h4>TuffBox IDE</h4>
          <p>Developer harness for Minecraft modpacks.</p>
          <span class="version">Version {appVersion || "0.1.0-alpha"}</span>
        </div>
      </div>
    </section>
  </div>
</div>

<style>
  .settings {
    max-width: 900px;
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 20px;
  }

  .card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 24px;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 20px;
    color: var(--text-secondary);
  }

  .card-title h3 {
    font-size: 16px;
    color: var(--text-primary);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
    font-weight: 600;
  }

  select {
    max-width: 200px;
  }

  .about {
    display: flex;
    align-items: center;
    gap: 18px;
  }

  .logo-big {
    width: 64px;
    height: 64px;
    border-radius: var(--border-radius-lg);
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 900;
    font-size: 32px;
    color: #000;
    box-shadow: 0 8px 24px rgba(27, 217, 106, 0.25);
  }

  .about h4 {
    font-size: 18px;
    margin-bottom: 4px;
  }

  .about p {
    color: var(--text-secondary);
    font-size: 13px;
    margin-bottom: 8px;
  }

  .theme-toggle { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: var(--border-radius-md); color: var(--text-secondary); cursor: pointer; font-size: 13px; font-weight: 600; }
  .theme-toggle:hover { border-color: var(--accent-primary); color: var(--text-primary); }

  .shortcut-list { display: grid; gap: 4px; margin-top: 8px; }
  .shortcut-row { display: flex; align-items: center; gap: 12px; padding: 6px 10px; border-radius: 6px; background: var(--bg-tertiary); }
  .shortcut-row kbd { font-family: ui-monospace,monospace; font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--bg-elevated); border: 1px solid var(--border-color); color: var(--text-primary); min-width: 60px; text-align: center; }
  .shortcut-row span { flex: 1; color: var(--text-secondary); font-size: 12px; }
  .shortcut-row small { color: var(--text-muted); font-size: 10px; }

  .update-info { padding: 8px 10px; border-radius: 8px; background: var(--bg-tertiary); border: 1px solid var(--border-color); margin-bottom: 10px; font-size: 12px; }
  .update-avail { color: var(--accent-primary); font-weight: 700; }
  .update-ok { color: var(--text-muted); }

  .version {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 4px;
  }
</style>
