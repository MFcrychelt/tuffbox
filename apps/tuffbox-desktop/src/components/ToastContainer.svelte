<script lang="ts">
  import { toasts } from "../lib/toast";
  import { X, CheckCircle2, AlertTriangle, Info, AlertCircle } from "lucide-svelte";
  function icon(t: string) { if(t==="success")return CheckCircle2; if(t==="error")return AlertCircle; if(t==="warning")return AlertTriangle; return Info; }
  function clr(t: string): string { if(t==="success")return "#1bd96a"; if(t==="error")return "#f87171"; if(t==="warning")return "#fbbf24"; return "#93c5fd"; }
</script>
<div class="tc">
  {#each $toasts as t (t.id)}
    <div class="t {t.type}" style="--tc:{clr(t.type)}">
      <span class="ti"><svelte:component this={icon(t.type)} size={16} color="var(--tc)" /></span>
      <span class="tm">{t.message}</span>
      <button class="tx" on:click={() => toasts.dismiss(t.id)}><X size={12} /></button>
    </div>
  {/each}
</div>
<style>
  .tc{position:fixed;bottom:20px;right:20px;z-index:1000;display:flex;flex-direction:column;gap:8px;max-width:420px;pointer-events:none}
  .t{display:flex;align-items:center;gap:10px;padding:12px 16px;border-radius:12px;background:var(--bg-elevated);border:1px solid var(--tc);box-shadow:0 8px 24px rgba(0,0,0,.4);animation:ti .25s ease-out;pointer-events:all;backdrop-filter:blur(12px)}
  .ti{flex-shrink:0}.tm{flex:1;font-size:13px;color:var(--text-primary);line-height:1.4}
  .tx{flex-shrink:0;width:20px;height:20px;padding:0;background:transparent;border:none;color:var(--text-muted);cursor:pointer;display:flex;align-items:center;justify-content:center;border-radius:4px}
  .tx:hover{background:var(--bg-hover);color:var(--text-primary)}
  @keyframes ti{from{opacity:0;transform:translateY(12px) scale(.96)}to{opacity:1;transform:translateY(0) scale(1)}}
</style>
