<script lang="ts">
  import { onDestroy, untrack } from 'svelte';

  import LayersMenu from '../components/LayersMenu.svelte';
  import TopBar from '../components/TopBar.svelte';
  import WelcomeDialog from '../components/WelcomeDialog.svelte';
  import { layers } from '../lib/layers.svelte';
  import { MapController, type LayerChoice } from '../lib/map/controller';
  import { rules } from '../lib/rules.svelte';
  import { session } from '../lib/session.svelte';

  let container: HTMLDivElement;
  let controller = $state.raw<MapController | null>(null);

  // "Only my activity" means nothing (and the server refuses it) without a login.
  const choice: LayerChoice = $derived({
    mine: layers.mine && !!session.user,
    territory: layers.territory,
    routes: layers.routes,
    poiTypes: layers.shownPoiTypes,
  });

  // The map needs to know who is looking and how long things last before it starts.
  $effect(() => {
    if (controller || session.user === undefined || !rules.value) return;
    const decayDays = rules.value.decay_days;
    controller = untrack(() => new MapController(container, decayDays, session.defaultBbox, choice));
  });
  $effect(() => controller?.update(choice));
  $effect(() => layers.persist());
  onDestroy(() => controller?.destroy());
</script>

<div bind:this={container} class="map"></div>
<TopBar onchange={() => controller?.reload()} />
{#if rules.value}
  <LayersMenu decayDays={rules.value.decay_days} />
  <WelcomeDialog decayDays={rules.value.decay_days} />
{/if}

<style>
  .map {
    position: fixed;
    inset: 0;
  }
</style>
