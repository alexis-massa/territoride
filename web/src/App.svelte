<script lang="ts">
  import { AppShell, Button, LoginForm, toasts } from 'pi-ui';

  import { api, type Me } from './lib/api';

  let me = $state<Me | null | undefined>(undefined);

  function signedIn(user: Me | null) {
    api.setCsrf(user?.csrf_token ?? null);
    me = user;
  }

  api.onUnauthorized(() => signedIn(null));
  api.get<Me>('/me').then(signedIn, () => signedIn(null));

  async function login(username: string, password: string) {
    signedIn(await api.post<Me>('/login', { username, password }));
  }

  async function logout() {
    await api.post('/logout');
    signedIn(null);
    toasts.show('Logged out');
  }
</script>

{#if me === null}
  <LoginForm title="TerritoRide" onlogin={login} />
{:else if me}
  <AppShell title="TerritoRide">
    {#snippet actions()}
      <Button variant="ghost" onclick={logout}>Log out</Button>
    {/snippet}
    <h1>Hello, {me.username}</h1>
    <p>This is a new pi-kit app. Start in <code>src/routes.rs</code> and <code>web/src/App.svelte</code>.</p>
  </AppShell>
{/if}
