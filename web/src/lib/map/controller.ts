import type { Feature, FeatureCollection } from 'geojson';
import {
  type ExpressionSpecification,
  type GeoJSONSource,
  Map as MapLibre,
  type Map as MapLibreMap,
  NavigationControl,
  Popup,
  setWorkerUrl,
} from 'maplibre-gl';
import workerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url';
import 'maplibre-gl/dist/maplibre-gl.css';

import type { Bbox } from '../types';
import { LayerLoader } from './loader';
import { EMPTY } from './view';

// The renderer's worker, bundled by Vite so it is served from our own origin (the CSP allows no other).
setWorkerUrl(workerUrl);

const STYLE_URL = 'https://tiles.openfreemap.org/styles/liberty';
const RHONE_ALPES: [number, number] = [5.8, 45.4];
export const VIEW_KEY = 'map-view';

/** What the layers menu has chosen. */
export interface LayerChoice {
  /** Only the player's own activity, coloured by how recently they visited. */
  mine: boolean;
  territory: boolean;
  routes: boolean;
  /** The POI types shown (`mountain_pass`, `summit`). */
  poiTypes: string[];
}

type Props = Record<string, string | number | null | undefined>;

const OWNER_COLOR: ExpressionSpecification = ['get', 'color'];
const UNCLAIMED: ExpressionSpecification = ['==', ['get', 'owner'], null];
const TERRITORY_OPACITY: ExpressionSpecification = [
  'interpolate',
  ['linear'],
  ['get', 'remaining'],
  0,
  0.25,
  1,
  0.55,
];

/** Colour by age: green today, red when about to expire. */
export function recencyColor(decayDays: number): ExpressionSpecification {
  return [
    'interpolate',
    ['linear'],
    ['get', 'age_days'],
    0,
    '#2ecc71',
    decayDays * 0.33,
    '#f1c40f',
    decayDays * 0.66,
    '#e67e22',
    decayDays,
    '#e74c3c',
  ];
}

function readView(): { center: [number, number]; zoom: number } | null {
  try {
    return JSON.parse(localStorage.getItem(VIEW_KEY) ?? 'null');
  } catch {
    return null;
  }
}

/** `<dot> text` for a tooltip; built from nodes, never HTML, since names come from other players. */
function chip(color: string, text: string): HTMLElement {
  const el = document.createElement('span');
  el.className = 'chip';
  const dot = document.createElement('span');
  dot.className = 'owner-tooltip-dot';
  dot.style.background = color;
  el.append(dot, text);
  return el;
}

/** SVG icons are drawn through <img> and a canvas: `map.loadImage` cannot reliably decode SVG. */
function loadIcon(url: string): Promise<ImageData> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => {
      const canvas = document.createElement('canvas');
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      const ctx = canvas.getContext('2d')!;
      ctx.drawImage(img, 0, 0);
      resolve(ctx.getImageData(0, 0, canvas.width, canvas.height));
    };
    img.onerror = reject;
    img.src = url;
  });
}

/** The MapLibre map and its three data layers; the Svelte side only tells it what the menu chose. */
export class MapController {
  readonly map: MapLibreMap;
  #choice: LayerChoice;
  #ready = false;
  #allPois = EMPTY;
  readonly #territory = new LayerLoader('/api/territory.geojson', (d) => this.#set('territory', d));
  readonly #routes = new LayerLoader('/api/routes.geojson', (d) => this.#set('routes', d));
  readonly #pois = new LayerLoader('/api/pois.geojson', (d) => {
    this.#allPois = d;
    this.#applyPoiTypes();
  });

  constructor(
    container: HTMLElement,
    private readonly decayDays: number,
    defaultBbox: Bbox | null,
    choice: LayerChoice,
  ) {
    this.#choice = choice;
    const saved = readView();
    this.map = new MapLibre({
      container,
      style: STYLE_URL,
      center: saved?.center ?? RHONE_ALPES,
      zoom: saved?.zoom ?? 8,
    });
    this.map.addControl(new NavigationControl({ showZoom: false, showCompass: true }), 'top-left');
    if (!saved && defaultBbox) {
      this.map.fitBounds(
        [
          [defaultBbox[0], defaultBbox[1]],
          [defaultBbox[2], defaultBbox[3]],
        ],
        { padding: 40, duration: 0 },
      );
    }
    this.map.on('moveend', () => {
      localStorage.setItem(
        VIEW_KEY,
        JSON.stringify({ center: this.map.getCenter().toArray(), zoom: this.map.getZoom() }),
      );
      if (this.#ready) this.#loadVisible(false);
    });
    this.map.on('load', () => void this.#onLoad());
  }

  destroy() {
    this.map.remove();
  }

  /** The menu changed. */
  update(next: LayerChoice) {
    const prev = this.#choice;
    this.#choice = next;
    if (!this.#ready) return;
    if (next.mine !== prev.mine) {
      this.#applyMineStyle();
      this.#loadVisible(true);
      return;
    }
    if (next.territory !== prev.territory) this.#toggle(this.#territory, next.territory);
    if (next.routes !== prev.routes) this.#toggle(this.#routes, next.routes);
    if (next.poiTypes.join() !== prev.poiTypes.join()) this.#applyPoiTypes();
  }

  /** Fetch everything shown again, e.g. after a sync. */
  reload() {
    if (this.#ready) this.#loadVisible(true);
  }

  #view(): Bbox {
    const b = this.map.getBounds();
    return [b.getWest(), b.getSouth(), b.getEast(), b.getNorth()];
  }

  #loadVisible(force: boolean) {
    const [view, zoom, mine] = [this.#view(), this.map.getZoom(), this.#choice.mine];
    if (this.#choice.territory) void this.#territory.load(view, zoom, mine, force);
    if (this.#choice.routes) void this.#routes.load(view, zoom, mine, force);
    void this.#pois.load(view, zoom, mine, force);
  }

  #toggle(loader: LayerLoader, on: boolean) {
    if (on) void loader.load(this.#view(), this.map.getZoom(), this.#choice.mine, true);
    else loader.clear(EMPTY);
  }

  #set(source: string, data: FeatureCollection) {
    (this.map.getSource(source) as GeoJSONSource | undefined)?.setData(data);
  }

  /** The type filter is applied to the data itself, not a layer filter, so cluster counts only count shown types. */
  #applyPoiTypes() {
    const shown = new Set(this.#choice.poiTypes);
    this.#set('pois', {
      type: 'FeatureCollection',
      features: this.#allPois.features.filter((f: Feature) => shown.has(String(f.properties?.poi_type))),
    });
  }

  /** "Only mine" colours by age instead of by player and makes everything equally strong. */
  #applyMineStyle() {
    const { map } = this;
    const mine = this.#choice.mine;
    const color = mine ? recencyColor(this.decayDays) : OWNER_COLOR;
    if (map.getLayer('territory-fill')) {
      map.setPaintProperty('territory-fill', 'fill-color', color);
      map.setPaintProperty('territory-fill', 'fill-opacity', mine ? 0.55 : TERRITORY_OPACITY);
    }
    if (map.getLayer('routes-line')) map.setPaintProperty('routes-line', 'line-color', color);
    if (map.getLayer('pois-circle')) {
      map.setPaintProperty('pois-circle', 'circle-color', ['case', UNCLAIMED, '#000000', color]);
      map.setPaintProperty(
        'pois-circle',
        'circle-opacity',
        mine ? 1 : ['case', UNCLAIMED, 1, ['get', 'remaining']],
      );
    }
  }

  /** ` · 12/30 days · captured 3 Nov 2026`. */
  #decaySuffix(p: Props): string {
    const verb = this.#choice.mine ? 'last visited' : 'captured';
    return ` · ${p.age_days}/${this.decayDays} days · ${verb} ${new Date(String(p.captured_at)).toLocaleDateString()}`;
  }

  async #onLoad() {
    const { map } = this;
    map.addSource('territory', { type: 'geojson', data: EMPTY });
    map.addLayer({
      id: 'territory-fill',
      type: 'fill',
      source: 'territory',
      paint: {
        'fill-color': OWNER_COLOR,
        'fill-opacity': TERRITORY_OPACITY,
        'fill-outline-color': '#000000',
      },
    });
    this.#tooltip('territory-fill', (p) => chip(String(p.color), `${p.owner}${this.#decaySuffix(p)}`));

    map.addSource('routes', { type: 'geojson', data: EMPTY });
    map.addLayer({
      id: 'routes-line',
      type: 'line',
      source: 'routes',
      paint: { 'line-color': OWNER_COLOR, 'line-width': 2 },
    });

    const [pass, peak] = await Promise.all([loadIcon('/icons/pass.svg'), loadIcon('/icons/peak.svg')]);
    map.addImage('pass', pass);
    map.addImage('peak', peak);
    this.#addPois();
    this.#applyMineStyle();
    this.#ready = true;
    this.#loadVisible(true);
  }

  #addPois() {
    const { map } = this;
    const single: ExpressionSpecification = ['!', ['has', 'point_count']];
    map.addSource('pois', {
      type: 'geojson',
      data: EMPTY,
      cluster: true,
      clusterMaxZoom: 13,
      clusterRadius: 50,
    });
    map.addLayer({
      id: 'pois-cluster-circle',
      type: 'circle',
      source: 'pois',
      filter: ['has', 'point_count'],
      paint: {
        'circle-radius': ['interpolate', ['linear'], ['get', 'point_count'], 1, 10, 500, 24],
        'circle-color': '#3b6fd6',
        'circle-stroke-width': 2,
        'circle-stroke-color': '#ffffff',
      },
    });
    map.addLayer({
      id: 'pois-cluster-count',
      type: 'symbol',
      source: 'pois',
      filter: ['has', 'point_count'],
      layout: { 'text-field': ['get', 'point_count_abbreviated'], 'text-size': 12 },
      paint: { 'text-color': '#ffffff' },
    });
    map.addLayer({
      id: 'pois-circle',
      type: 'circle',
      source: 'pois',
      filter: single,
      paint: {
        'circle-radius': ['interpolate', ['linear'], ['zoom'], 6, 2, 10, 5, 14, 13],
        'circle-color': ['case', UNCLAIMED, '#000000', OWNER_COLOR],
        'circle-opacity': ['case', UNCLAIMED, 1, ['get', 'remaining']],
        'circle-stroke-width': 2,
        'circle-stroke-color': '#ffffff',
      },
    });
    map.addLayer({
      id: 'pois-icon',
      type: 'symbol',
      source: 'pois',
      filter: single,
      layout: {
        'icon-image': ['match', ['get', 'poi_type'], 'summit', 'peak', 'pass'],
        'icon-size': ['interpolate', ['linear'], ['zoom'], 6, 0.15, 10, 0.4, 14, 1],
        'icon-allow-overlap': true,
      },
    });
    map.on('click', 'pois-cluster-circle', async (e) => {
      const cluster = e.features?.[0];
      if (!cluster || cluster.geometry.type !== 'Point') return;
      const source = map.getSource('pois') as GeoJSONSource;
      const zoom = await source.getClusterExpansionZoom(Number(cluster.properties.cluster_id));
      map.easeTo({ center: cluster.geometry.coordinates as [number, number], zoom });
    });
    map.on('mouseenter', 'pois-cluster-circle', () => (map.getCanvas().style.cursor = 'pointer'));
    map.on('mouseleave', 'pois-cluster-circle', () => (map.getCanvas().style.cursor = ''));
    const poiChip = (p: Props) => {
      const label = `${p.name} (${p.poi_type_label})${p.altitude_m ? ` · ${p.altitude_m}m` : ''}`;
      const owner = p.owner ? `${p.owner}${this.#decaySuffix(p)}` : 'unclaimed';
      return chip(p.owner ? String(p.color) : '#000000', `${label} · ${owner}`);
    };
    this.#tooltip('pois-circle', poiChip);
    this.#tooltip('pois-icon', poiChip);
  }

  /** A pill that follows the pointer over a layer. */
  #tooltip(layer: string, content: (props: Props) => HTMLElement) {
    const { map } = this;
    const popup = new Popup({
      closeButton: false,
      closeOnClick: false,
      className: 'owner-tooltip',
      offset: 10,
    });
    map.on('mousemove', layer, (e) => {
      map.getCanvas().style.cursor = 'pointer';
      const feature = e.features?.[0];
      if (feature)
        popup
          .setLngLat(e.lngLat)
          .setDOMContent(content(feature.properties as Props))
          .addTo(map);
    });
    map.on('mouseleave', layer, () => {
      map.getCanvas().style.cursor = '';
      popup.remove();
    });
  }
}
