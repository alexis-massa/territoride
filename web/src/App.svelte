<script lang="ts">
  import { Toasts } from 'pi-ui';

  import { api } from './lib/api';
  import { router } from './lib/router.svelte';
  import { rules } from './lib/rules.svelte';
  import { session } from './lib/session.svelte';
  import Changelog from './pages/Changelog.svelte';
  import Leaderboard from './pages/Leaderboard.svelte';
  import MapPage from './pages/MapPage.svelte';
  import OAuthComplete from './pages/OAuthComplete.svelte';
  import Profile from './pages/Profile.svelte';
  import Rules from './pages/Rules.svelte';

  void session.load();
  void rules.load();
  // A session that ended elsewhere: back to the map as a visitor.
  api.onUnauthorized(() => {
    session.set({ user: null, default_bbox: null });
    router.navigate('/');
  });
</script>

{#if router.path === '/leaderboard'}
  <Leaderboard />
{:else if router.path === '/rules'}
  <Rules />
{:else if router.path === '/changelog'}
  <Changelog />
{:else if router.path === '/profile'}
  <Profile />
{:else if router.path === '/oauth/complete'}
  <OAuthComplete />
{:else}
  <MapPage />
{/if}
<Toasts />
