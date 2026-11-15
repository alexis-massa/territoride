<script lang="ts">
  import Page from '../components/Page.svelte';
  import { api } from '../lib/api';

  let html = $state('');
  api.get<{ html: string }>('/changelog').then((c) => (html = c.html));
</script>

<Page title="Changelog">
  <p class="byline">
    Open source -
    <a href="https://github.com/alexis-massa/territoride" target="_blank" rel="noopener"
      >view the code on GitHub</a
    >
  </p>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- our own CHANGELOG.md, rendered by the server -->
  <div class="card body">{@html html}</div>
</Page>

<style>
  .byline {
    color: var(--muted);
    font-size: 0.9rem;
    margin-bottom: 1.5rem;
  }
  .body :global(h2) {
    margin-top: 1.5rem;
  }
  .body :global(h2:first-child) {
    margin-top: 0;
  }
  .body :global(h3) {
    margin-bottom: 0.5rem;
  }
  .body :global(ul) {
    padding-left: 1.2rem;
  }
  .body :global(li) {
    margin-bottom: 0.4rem;
    line-height: 1.5;
  }
</style>
