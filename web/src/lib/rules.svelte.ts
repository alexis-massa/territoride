import { api } from './api';
import type { Rules } from './types';

/** The game's numbers (decay days, multipliers…), from the server so the pages never disagree with it. */
class RulesStore {
  value = $state<Rules | null>(null);

  async load() {
    this.value = await api.get<Rules>('/rules');
  }
}

export const rules = new RulesStore();
