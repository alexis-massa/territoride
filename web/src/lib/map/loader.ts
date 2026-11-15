import type { FeatureCollection } from 'geojson';
import { type Loaded, BBOX_MARGIN, covers, grow, layerQuery } from './view';
import type { Bbox } from '../types';

/**
 * Loads one GeoJSON layer for the viewport. A request that is superseded is aborted, so a slow old answer can
 * never overwrite a newer one, and nothing is fetched while the loaded area still covers the view.
 */
export class LayerLoader {
  #controller: AbortController | null = null;
  #loaded: Loaded | null = null;

  constructor(
    private readonly url: string,
    private readonly apply: (data: FeatureCollection) => void,
  ) {}

  async load(view: Bbox, zoom: number, mine: boolean, force = false) {
    if (!force && covers(this.#loaded, view, zoom)) return;
    this.#controller?.abort();
    const controller = (this.#controller = new AbortController());
    const bbox = grow(view, BBOX_MARGIN);
    try {
      const res = await fetch(this.url + layerQuery(bbox, mine), {
        signal: controller.signal,
        credentials: 'same-origin',
      });
      if (!res.ok) throw new Error(`${this.url}: ${res.status}`);
      const data = (await res.json()) as FeatureCollection;
      this.#loaded = { bbox, zoom };
      this.#controller = null;
      this.apply(data);
    } catch (err) {
      if ((err as Error).name !== 'AbortError') console.error(err);
    }
  }

  /** Forget everything and empty the layer. */
  clear(empty: FeatureCollection) {
    this.#controller?.abort();
    this.#controller = null;
    this.#loaded = null;
    this.apply(empty);
  }
}
