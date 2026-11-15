<!-- The buttons floating over the map: rules, leaderboard, and the player's account menu or "Connect with Strava". -->
<script lang="ts">
  import { toasts } from 'pi-ui';

  import { api, uploadGpx } from '../lib/api';
  import { session } from '../lib/session.svelte';
  import type { Outcome } from '../lib/types';
  import Link from './Link.svelte';

  let { onchange }: { onchange: () => void } = $props();
  let busy = $state(false);
  let menuOpen = $state(false);

  function report(outcome: Outcome) {
    toasts.show(outcome.message, outcome.ok ? 'success' : 'error', 5000);
    if (outcome.ok) onchange();
  }

  async function run(action: () => Promise<Outcome>) {
    busy = true;
    try {
      report(await action());
    } catch (err) {
      toasts.show(err instanceof Error ? err.message : 'Something went wrong', 'error', 5000);
    } finally {
      busy = false;
    }
  }

  const sync = () => run(() => api.post<Outcome>('/strava/sync'));

  function upload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (file) void run(() => uploadGpx(file));
  }

  /** Strava's own page in a popup; when it closes, reload so the new session shows. */
  function connect() {
    const popup = window.open('/api/strava/authorize', 'strava_oauth', 'width=600,height=700');
    const onMessage = (event: MessageEvent) => {
      if (event.origin !== window.location.origin || event.data?.source !== 'strava-oauth') return;
      window.removeEventListener('message', onMessage);
      if (event.data.status !== 'success')
        toasts.show(event.data.message || 'Strava connection failed.', 'error', 8000);
    };
    window.addEventListener('message', onMessage);
    const poll = setInterval(() => {
      if (!popup || popup.closed) {
        clearInterval(poll);
        window.location.reload();
      }
    }, 500);
  }
</script>

<svelte:window
  onclick={(e) => {
    if (menuOpen && !(e.target as Element).closest('.account')) menuOpen = false;
  }}
  onkeydown={(e) => e.key === 'Escape' && (menuOpen = false)}
/>

<div class="topbar">
  <Link href="/rules" class="chip">Rules</Link>
  <Link href="/leaderboard" class="chip">Leaderboard</Link>
  {#if session.user}
    <div class="account" class:open={menuOpen}>
      <button
        class="chip"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
      >
        {session.user.username}
      </button>
      <div class="dropdown">
        <Link href="/profile" class="chip" onclick={() => (menuOpen = false)}>Profile</Link>
        <button class="chip" disabled={busy} onclick={sync}>Sync from Strava</button>
        <label class="chip">
          Import activity
          <input type="file" accept=".gpx" hidden onchange={upload} />
        </label>
        <button class="chip" onclick={() => session.logout()}>Log out</button>
      </div>
    </div>
  {:else if session.user === null}
    <button class="strava" onclick={connect}>Connect with Strava</button>
  {/if}
</div>

<style>
  .topbar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    justify-content: flex-end;
    padding: 0.75rem 1rem;
    pointer-events: none;
  }
  .topbar > :global(*) {
    pointer-events: auto;
  }
  .topbar :global(.chip),
  .chip {
    display: block;
    width: 100%;
    background: var(--bg);
    color: var(--ink);
    border: none;
    border-radius: 4px;
    padding: 0.6rem 1rem;
    font: inherit;
    font-size: 0.95rem;
    text-align: left;
    text-decoration: none;
    cursor: pointer;
    box-sizing: border-box;
  }
  .topbar > :global(.chip) {
    width: auto;
  }
  .strava {
    background: var(--strava);
    color: white;
    border: none;
    border-radius: 4px;
    padding: 0.6rem 1rem;
    font: inherit;
    font-size: 0.95rem;
    cursor: pointer;
  }
  .account {
    position: relative;
  }
  .dropdown {
    display: none;
    flex-direction: column;
    gap: 0.25rem;
    position: absolute;
    top: 100%;
    right: 0;
    width: max(100%, 11rem);
  }
  .account.open .dropdown {
    display: flex;
  }
  /* A mouse also opens it on hover; a touch screen only by tapping the name (hover sticks there). */
  @media (hover: hover) {
    .account:hover .dropdown {
      display: flex;
    }
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
