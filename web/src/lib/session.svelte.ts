import { api, setCsrf } from './api';
import type { Bbox, MeResponse, Player } from './types';

/** The logged-in player: `undefined` until known, `null` when nobody is logged in. */
class Session {
  user = $state<Player | null | undefined>(undefined);
  defaultBbox = $state<Bbox | null>(null);

  async load() {
    try {
      this.set(await api.get<MeResponse>('/me'));
    } catch {
      this.set({ user: null, default_bbox: null });
    }
  }

  set({ user, default_bbox }: MeResponse) {
    setCsrf(user?.csrf_token ?? null);
    this.user = user;
    this.defaultBbox = default_bbox;
  }

  async logout() {
    await api.post('/logout');
    this.set({ user: null, default_bbox: null });
  }
}

export const session = new Session();
