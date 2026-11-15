<!-- An information page: back link, title, and cards. -->
<script lang="ts">
  import type { Snippet } from 'svelte';

  import Link from './Link.svelte';

  interface Props {
    title: string;
    children: Snippet;
  }

  let { title, children }: Props = $props();
  $effect(() => {
    document.title = `${title} — TerritoRide`;
    return () => (document.title = 'TerritoRide');
  });
</script>

<div class="page">
  <Link href="/" class="back-link">← Back to map</Link>
  <h1>{title}</h1>
  {@render children()}
</div>

<style>
  .page {
    max-width: 40rem;
    margin: 0 auto;
    padding: 2.5rem 1.25rem 4rem;
  }
  .page :global(.back-link) {
    display: inline-block;
    margin-bottom: 1.5rem;
    color: var(--muted);
    text-decoration: none;
    font-size: 0.9rem;
  }
  .page :global(.back-link:hover) {
    color: var(--ink);
  }
  .page :global(.card) {
    background: var(--bg);
    border-radius: 14px;
    box-shadow: 0 1px 10px rgb(0 0 0 / 0.07);
    padding: 1.5rem 1.75rem;
    margin-bottom: 1.25rem;
  }
</style>
