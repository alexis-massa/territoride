<!-- Shown once per browser. -->
<script lang="ts">
  import { Button, Dialog } from 'pi-ui';

  import Link from './Link.svelte';

  const SEEN_KEY = 'welcome-seen';

  let { decayDays }: { decayDays: number } = $props();

  function seen(): boolean {
    try {
      return localStorage.getItem(SEEN_KEY) !== null;
    } catch {
      return true;
    }
  }

  let open = $state(!seen());

  function dismiss() {
    try {
      localStorage.setItem(SEEN_KEY, 'true');
    } catch {
      // asked again next time
    }
    open = false;
  }
</script>

<Dialog bind:open title="Welcome to TerritoRide">
  <p>
    Ride your bike, claim hexagonal tiles and mountain passes, and hold them before someone else rides
    through. Everything fades over {decayDays} days, so keep moving.
  </p>
  <p>
    Connecting Strava uses their own login page - we never see your password, only ever request read access,
    and store your connection encrypted.
  </p>
  <p><Link href="/rules" onclick={dismiss}>Full rules &amp; security details</Link></p>
  {#snippet actions()}
    <Button onclick={dismiss}>Got it</Button>
  {/snippet}
</Dialog>
