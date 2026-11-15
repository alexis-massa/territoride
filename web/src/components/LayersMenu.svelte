<!-- What the map shows: territory, routes, points of interest, and "only my activity". -->
<script lang="ts">
  import { POI_TYPES, layers } from '../lib/layers.svelte';
  import { session } from '../lib/session.svelte';

  let { decayDays }: { decayDays: number } = $props();
</script>

<div class="menu">
  {#if session.user}
    <label title="Hide other players and colour your own tiles, rides and passes by how recently you visited">
      <input type="checkbox" bind:checked={layers.mine} /> Only my activity
    </label>
    {#if layers.mine}
      <div class="legend">
        <div class="bar"></div>
        <div class="labels"><span>today</span><span>{decayDays} days ago</span></div>
      </div>
    {/if}
    <hr />
  {/if}
  <label><input type="checkbox" bind:checked={layers.territory} /> Territory</label>
  <label><input type="checkbox" bind:checked={layers.routes} /> Routes</label>
  <details>
    <summary>Points of interest</summary>
    {#each POI_TYPES as [value, label] (value)}
      <label class="poi"><input type="checkbox" bind:checked={layers.poiTypes[value]} /> {label}</label>
    {/each}
  </details>
</div>

<style>
  .menu {
    position: absolute;
    bottom: 1rem;
    left: 1rem;
    z-index: 1;
    background: var(--bg);
    color: var(--ink);
    border-radius: 4px;
    padding: 0.5rem 0.75rem;
    font-size: 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  hr {
    width: 100%;
    border: 0;
    border-top: 1px solid var(--border);
    margin: 0.15rem 0;
  }
  .legend {
    padding-left: 1.4rem;
  }
  .bar {
    height: 0.5rem;
    border-radius: 3px;
    background: linear-gradient(to right, #2ecc71, #f1c40f, #e67e22, #e74c3c);
  }
  .labels {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    font-size: 0.75rem;
    color: var(--muted);
  }
  summary {
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary::before {
    content: '\25b8\00a0';
  }
  details[open] summary::before {
    content: '\25be\00a0';
  }
  .poi {
    display: block;
    padding-left: 1.1rem;
    margin-top: 0.25rem;
  }
</style>
