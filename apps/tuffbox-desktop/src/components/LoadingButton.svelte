<script lang="ts">
  import { Loader2 } from "lucide-svelte";

  export let loading = false;
  export let disabled = false;
  export let variant: "primary" | "secondary" | "ghost" | "danger" = "primary";
  export let type: "button" | "submit" = "button";

  $: isDisabled = disabled || loading;
</script>

<button
  {type}
  class={variant}
  disabled={isDisabled}
  class:loading
  on:click
>
  {#if loading}
    <Loader2 size={16} class="spin-icon" />
  {/if}
  <slot />
</button>

<style>
  button {
    position: relative;
    min-width: 80px;
  }

  button.loading {
    pointer-events: none;
  }

  button :global(.spin-icon) {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button:disabled:hover {
    transform: none;
  }
</style>
