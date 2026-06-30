<script lang="ts">
  import { Palette, Info, Moon } from "lucide-svelte";

  let theme = "dark";
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
        <select id="theme-select" bind:value={theme}>
          <option value="dark">Dark</option>
          <option value="light" disabled>Light (soon)</option>
        </select>
      </div>
    </section>

    <section class="card">
      <div class="card-title">
        <Info size={18} />
        <h3>About</h3>
      </div>
      <div class="about">
        <div class="logo-big">T</div>
        <div>
          <h4>TuffBox IDE</h4>
          <p>Developer harness for Minecraft modpacks.</p>
          <span class="version">Version 0.1.0 alpha</span>
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

  .version {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 4px;
  }
</style>
