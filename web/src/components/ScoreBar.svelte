<!-- The territory/POI split of a score, as a bar. -->
<script lang="ts">
  interface Props {
    territoryPct: number;
    poiPct: number;
    title?: string;
  }

  let { territoryPct, poiPct, title }: Props = $props();
</script>

<div class="score-bar" {title}>
  <div class="territory" style:width="{territoryPct}%"></div>
  <div class="poi" style:width="{poiPct}%"></div>
</div>

<style>
  .score-bar {
    display: flex;
    height: 0.5rem;
    border-radius: 999px;
    overflow: hidden;
    background: var(--border);
    margin: 0.5rem 0;
  }
  .territory {
    background: var(--player-territory);
  }
  .poi {
    background: var(--player-poi);
  }
</style>
