<!-- An in-app link: navigates without reloading, but still opens in a new tab with a modifier key. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAnchorAttributes } from 'svelte/elements';

  import { router } from '../lib/router.svelte';

  interface Props extends HTMLAnchorAttributes {
    href: string;
    children: Snippet;
  }

  let { href, children, ...rest }: Props = $props();

  function go(e: MouseEvent) {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    router.navigate(href);
  }
</script>

<a {href} onclick={go} {...rest}>{@render children()}</a>
