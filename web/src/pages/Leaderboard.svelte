<script lang="ts">
  import CaptureList from '../components/CaptureList.svelte';
  import Link from '../components/Link.svelte';
  import Page from '../components/Page.svelte';
  import ScoreBar from '../components/ScoreBar.svelte';
  import { api } from '../lib/api';
  import { plural, podium } from '../lib/format';
  import type { LeaderboardRow } from '../lib/types';

  let rows = $state<LeaderboardRow[] | null>(null);
  api.get<LeaderboardRow[]>('/leaderboard').then((r) => (rows = r));
</script>

<Page title="Leaderboard">
  <div class="card">
    {#if rows === null}
      <p>Loading…</p>
    {:else if rows.length === 0}
      <p>No one has captured anything yet.</p>
    {:else}
      {#each rows as row, i (row.username)}
        <details class="rank-row {podium(i + 1)}">
          <summary>
            <span class="chevron">▸</span>
            <div class="number">{i + 1}</div>
            <div class="main">
              <div class="name">
                <span class="dot" style:background={row.color}></span>
                {row.username}
              </div>
              <ScoreBar
                territoryPct={row.territory_pct}
                poiPct={row.poi_pct}
                title="Territory: {row.territory_points} pts ({row.territory_pct}%) · POIs: {row.poi_points} pts ({row.poi_pct}%)"
              />
              <div class="stats">
                {plural(row.cell_count, 'cell')} ({row.territory_points} pts) · {plural(row.poi_count, 'POI')} ({row.poi_points}
                pts)
              </div>
            </div>
            <div class="total">{row.total}</div>
          </summary>
          <div class="detail">
            {#if row.detail}<CaptureList detail={row.detail} />{/if}
          </div>
        </details>
      {/each}
    {/if}
  </div>

  <div class="legend">
    <span class="item"><span class="swatch" style:background="var(--player-territory)"></span> Territory</span
    >
    <span class="item"><span class="swatch" style:background="var(--player-poi)"></span> POIs</span>
    <Link href="/rules#math">How is this calculated?</Link>
  </div>
</Page>

<style>
  .rank-row {
    border-bottom: 1px solid var(--surface);
  }
  .rank-row:last-child {
    border-bottom: none;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.9rem 0;
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .rank-row[open] summary {
    padding-bottom: 0.5rem;
  }
  .chevron {
    color: var(--muted);
    font-size: 0.75rem;
    flex-shrink: 0;
    width: 0.7rem;
    transition: transform 0.15s;
  }
  .rank-row[open] .chevron {
    transform: rotate(90deg);
  }
  .number {
    width: 1.3rem;
    text-align: center;
    font-weight: 700;
    font-size: 1.1rem;
    color: var(--muted);
    flex-shrink: 0;
  }
  .top1 .number {
    color: #d4a017;
  }
  .top2 .number {
    color: #9aa0a6;
  }
  .top3 .number {
    color: #b06a34;
  }
  .main {
    flex-grow: 1;
    min-width: 0;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
  }
  .dot {
    display: inline-block;
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .stats {
    font-size: 0.8rem;
    color: var(--muted);
    margin-top: 0.15rem;
  }
  .total {
    font-size: 1.3rem;
    font-weight: 700;
    flex-shrink: 0;
  }
  .detail {
    padding: 0 0 1rem 3rem;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 1.25rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }
  .swatch {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 2px;
  }
</style>
