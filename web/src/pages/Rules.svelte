<script lang="ts">
  import Link from '../components/Link.svelte';
  import Page from '../components/Page.svelte';
  import { times } from '../lib/format';
  import { rules } from '../lib/rules.svelte';

  const r = $derived(rules.value);

  // `/rules#math`: the content only exists once the rules are loaded.
  $effect(() => {
    if (r && window.location.hash) document.getElementById(window.location.hash.slice(1))?.scrollIntoView();
  });
</script>

<Page title="How it works">
  {#if r}
    <div class="card">
      <h2>The rules</h2>
      <ul>
        <li>
          The world is divided into small hexagonal tiles. Ride through one and it's yours ({r.cell_value} points).
        </li>
        <li>
          If your ride is a loop (start and end within {r.loop_tolerance_m}m of each other), you also claim
          every tile enclosed inside it - not just the ones you rode through.
        </li>
        <li>
          Points of interest (mountain passes and summits) are separate landmarks scattered worldwide. Ride
          within
          {r.poi_radius_m}m of one to claim it - it's worth its altitude in points, so bigger ones are worth
          more.
        </li>
        <li>
          Whoever rode it most recently owns it - always. There's no permanent ownership, so ride back through
          contested ground to take it.
        </li>
        <li>
          Everything decays: a tile or POI loses value evenly over {r.decay_days} days, and after {r.decay_days}
          days it's released back to unclaimed automatically. Ride it again to keep it.
        </li>
        <li>
          Your score is the sum of everything you currently hold, decay included - check the
          <Link href="/leaderboard">leaderboard</Link> anytime.
        </li>
      </ul>
    </div>

    <div class="card" id="math">
      <h2>The math</h2>
      <ul>
        <li>
          Each tile is worth {r.cell_value} points at the moment you capture it. Each POI is worth its altitude
          in meters, flat - sport doesn't change a POI's value, only a tile's.
        </li>
        <li>
          A tile's base value is multiplied by how you got there - cycling covers far more ground per hour
          than walking or swimming, so slower sports are weighted up to stay fair:
          {r.sport_multipliers.map(([label, m]) => `${label} ${times(m)}`).join(', ')}. Anything else (or a
          manually-uploaded GPX file, which carries no sport data) defaults to {times(r.default_multiplier)}.
          POIs always score at 1× regardless of sport - otherwise driving most of the way up and hiking the
          last stretch to a high pass would be wildly overpowered.
        </li>
        <li>
          Swimming is the one exception: a pool is much smaller than a tile, so a flat per-tile multiplier
          would score a quick dip the same as a long swim. Swim tiles are valued by distance instead -
          <code>distance_km × {r.cell_value} × 8</code> total, split evenly if a swim ever spans more than one tile.
        </li>
        <li>
          From there, value decays linearly: <code>value × (1 − days_held / {r.decay_days})</code> - so a tile
          captured 15 days ago is worth half what it was on day one, and 0 by day {r.decay_days}.
        </li>
        <li>
          Your total score is just the sum of every tile and POI you currently hold, decay already applied -
          nothing else factors in. The <Link href="/leaderboard">leaderboard</Link> shows exactly this breakdown
          for every player.
        </li>
        <li>
          The multiplier only affects tile value, not ownership - anyone can still steal any tile regardless
          of sport, including a high-multiplier one. That's a deliberate tradeoff, not an oversight.
        </li>
      </ul>
    </div>

    <div class="card">
      <h2>Is it safe to connect Strava?</h2>
      <ul>
        <li>
          Login goes through Strava's own sign-in page - TerritoRide never sees or stores your Strava
          password.
        </li>
        <li>
          We only ever request read access to your activities. We can't post, edit, or delete anything on your
          Strava account.
        </li>
        <li>From each activity we only read its name, start time, and GPS route - nothing else.</li>
        <li>Your Strava connection is stored encrypted, not in plain text.</li>
        <li>You can delete your account at any time from the <Link href="/profile">profile</Link> page.</li>
        <li>
          You can disconnect at any time from Strava's own "My Apps" settings - we're notified immediately and
          stop storing your access.
        </li>
        <li>
          This project is open source - see the <Link href="/changelog">changelog</Link> or
          <a href="https://github.com/alexis-massa/territoride" target="_blank" rel="noopener"
            >view the code on GitHub</a
          >.
        </li>
      </ul>
    </div>
  {/if}
</Page>

<style>
  h2 {
    margin-top: 0;
  }
  ul {
    margin: 0;
    padding-left: 1.2rem;
  }
  li {
    margin-bottom: 0.75rem;
    line-height: 1.5;
  }
  li:last-child {
    margin-bottom: 0;
  }
</style>
