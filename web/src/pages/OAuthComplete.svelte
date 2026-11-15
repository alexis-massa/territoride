<!-- Where the Strava popup ends up: tells the page that opened it how it went, then closes itself. -->
<script lang="ts">
  const params = new URLSearchParams(window.location.search);
  const status = params.get('status') ?? 'error';
  const message = params.get('message') ?? '';

  window.opener?.postMessage({ source: 'strava-oauth', status, message }, window.location.origin);
  // Deferred: an immediate close() can race the delivery of the message in some browsers.
  setTimeout(() => window.close(), 0);
</script>

<p class="message">{message || 'Connecting to Strava…'}</p>

<style>
  .message {
    padding: 2rem;
    text-align: center;
  }
</style>
