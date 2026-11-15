<script lang="ts">
  import { Button, Dialog, toasts } from 'pi-ui';

  import CaptureList from '../components/CaptureList.svelte';
  import Page from '../components/Page.svelte';
  import ScoreBar from '../components/ScoreBar.svelte';
  import { api } from '../lib/api';
  import { monthYear } from '../lib/format';
  import { router } from '../lib/router.svelte';
  import { session } from '../lib/session.svelte';
  import type { Profile } from '../lib/types';

  let profile = $state<Profile | null>(null);
  let confirming = $state(false);
  api.get<Profile>('/profile').then((p) => (profile = p));

  async function deleteAccount() {
    const username = profile?.username ?? '';
    await api.post('/account/delete');
    confirming = false;
    session.set({ user: null, default_bbox: null });
    router.navigate('/');
    toasts.show(`Account ${JSON.stringify(username)} deleted.`, 'success');
  }
</script>

<Page title={profile?.username ?? 'Profile'}>
  {#if profile}
    <p class="meta">
      <span class="dot" style:background={profile.color}></span>
      Member since {monthYear(profile.member_since)}
      {#if profile.rank}· Ranked #{profile.rank} of {profile.player_count}{/if}
    </p>

    <div class="card">
      <div class="total">{profile.score.total}</div>
      <div class="label">total points</div>
      <ScoreBar territoryPct={profile.territory_pct} poiPct={profile.poi_pct} />
      <div class="legend">
        <span class="item">
          <span class="swatch" style:background="var(--player-territory)"></span>
          Territory — {profile.score.territory_points}
        </span>
        <span class="item">
          <span class="swatch" style:background="var(--player-poi)"></span>
          POIs — {profile.score.poi_points}
        </span>
      </div>
    </div>

    <div class="card">
      {#if profile.detail.territory.length || profile.detail.pois.length}
        <CaptureList detail={profile.detail} />
      {:else}
        <p>Nothing captured yet.</p>
      {/if}
    </div>

    <div class="danger">
      <button id="delete-account" onclick={() => (confirming = true)}>Delete my account</button>
    </div>

    <Dialog bind:open={confirming} title="Delete your account?">
      <p>
        This revokes Strava access, deletes your activities, and releases your territory permanently. This
        cannot be undone.
      </p>
      {#snippet actions()}
        <Button variant="ghost" onclick={() => (confirming = false)}>Cancel</Button>
        <Button variant="danger" onclick={deleteAccount}>Delete my account</Button>
      {/snippet}
    </Dialog>
  {/if}
</Page>

<style>
  .meta {
    color: var(--muted);
    font-size: 0.9rem;
    margin: -0.5rem 0 1.5rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .dot {
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .total {
    font-size: 2.25rem;
    font-weight: 700;
  }
  .label {
    color: var(--muted);
    font-size: 0.9rem;
  }
  .legend {
    display: flex;
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
  .danger {
    text-align: center;
  }
  #delete-account {
    background: none;
    border: none;
    color: var(--muted);
    font-size: 0.85rem;
    text-decoration: underline;
    cursor: pointer;
  }
  #delete-account:hover {
    color: var(--danger);
  }
</style>
