/** The layers menu's choices. They are remembered per browser, under the same keys as before the rewrite. */
export const POI_TYPES: [value: string, label: string][] = [
  ['mountain_pass', 'Mountain pass'],
  ['summit', 'Summit'],
];

function read(key: string, fallback: boolean): boolean {
  try {
    const stored = localStorage.getItem(key);
    return stored === null ? fallback : stored === 'true';
  } catch {
    return fallback;
  }
}

function write(key: string, value: boolean) {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // private mode: the choice just isn't remembered
  }
}

class LayerMenu {
  mine = $state(read('toggle-mine', false));
  territory = $state(read('toggle-territory', true));
  routes = $state(read('toggle-routes', true));
  poiTypes = $state(Object.fromEntries(POI_TYPES.map(([v]) => [v, read(`poi-type-${v}`, true)])));

  /** The POI types switched on. */
  get shownPoiTypes(): string[] {
    return POI_TYPES.map(([v]) => v).filter((v) => this.poiTypes[v]);
  }

  /** Remember the choices; call from an effect. */
  persist() {
    write('toggle-mine', this.mine);
    write('toggle-territory', this.territory);
    write('toggle-routes', this.routes);
    for (const [v] of POI_TYPES) write(`poi-type-${v}`, this.poiTypes[v]);
  }
}

export const layers = new LayerMenu();
