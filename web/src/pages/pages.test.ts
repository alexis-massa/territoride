import { cleanup, render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { rules } from '../lib/rules.svelte';
import Leaderboard from './Leaderboard.svelte';
import Rules from './Rules.svelte';

describe('pages', () => {
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it('says so when nobody has captured anything', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Response.json([])),
    );
    render(Leaderboard);
    expect(await screen.findByText('No one has captured anything yet.')).toBeTruthy();
  });

  it('quotes the numbers the server sends on the rules page', () => {
    rules.value = {
      loop_tolerance_m: 500,
      poi_radius_m: 100,
      decay_days: 30,
      cell_value: 10,
      default_multiplier: 1,
      sport_multipliers: [
        ['Cycling', 1],
        ['Hike', 5],
      ],
    };
    render(Rules);
    expect(screen.getByText(/within 500m of each other/)).toBeTruthy();
    expect(screen.getByText(/Cycling 1×, Hike 5×/)).toBeTruthy();
  });
});
