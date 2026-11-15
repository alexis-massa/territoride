<!-- What a player holds, ride by ride and pass by pass. -->
<script lang="ts">
  import { plural, times } from '../lib/format';
  import type { CaptureDetail } from '../lib/types';

  let { detail }: { detail: CaptureDetail } = $props();
</script>

{#if detail.territory.length}
  <h3>Territory captures</h3>
  {#each detail.territory as g (g.activity_name + g.captured_at)}
    <div class="row">
      <span>
        {g.activity_name} - {plural(g.cell_count, 'cell')}{g.multiplier !== 1
          ? ` (${times(g.multiplier)})`
          : ''}
      </span>
      <span class="meta">{g.age_days}d ago · {g.points} pts</span>
    </div>
  {/each}
{/if}
{#if detail.pois.length}
  <h3>Points of interest</h3>
  {#each detail.pois as p (p.name + p.claimed_at)}
    <div class="row">
      <span>{p.name} ({p.altitude_m ?? '?'}m) · {p.poi_type_label}</span>
      <span class="meta">{p.age_days}d ago · {p.points} pts</span>
    </div>
  {/each}
{/if}

<style>
  h3 {
    margin: 0.75rem 0 0.4rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--muted);
  }
  h3:first-child {
    margin-top: 0;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    font-size: 0.85rem;
    padding: 0.3rem 0;
    border-bottom: 1px solid var(--surface);
  }
  .row:last-child {
    border-bottom: none;
  }
  .meta {
    color: var(--muted);
    flex-shrink: 0;
  }
</style>
