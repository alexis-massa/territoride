import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import App from './App.svelte';

describe('App', () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('shows the login form when there is no session', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Response.json({ detail: 'not logged in' }, { status: 401 })),
    );
    render(App);
    expect(await screen.findByRole('button', { name: 'Log in' })).toBeTruthy();
  });

  it('greets the logged-in user', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Response.json({ username: 'Ann', csrf_token: 't' })),
    );
    render(App);
    expect(await screen.findByText('Hello, Ann')).toBeTruthy();
  });
});
