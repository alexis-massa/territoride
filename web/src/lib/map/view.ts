import type { FeatureCollection } from 'geojson';
import type { Bbox } from '../types';

/** Layers are loaded for the viewport plus this margin (a fraction of its size), so small pans need no reload. */
export const BBOX_MARGIN = 0.25;
/** Zooming in this many levels past what was loaded reloads (finer routes, more detail). */
export const RELOAD_ZOOM_DELTA = 1;

/** The viewport grown by `margin`, clamped to the world so a zoomed-out view counts as fully loaded. */
export function grow([west, south, east, north]: Bbox, margin: number): Bbox {
  const dx = (east - west) * margin;
  const dy = (north - south) * margin;
  return [
    Math.max(west - dx, -180),
    Math.max(south - dy, -90),
    Math.min(east + dx, 180),
    Math.min(north + dy, 90),
  ];
}

export interface Loaded {
  bbox: Bbox;
  zoom: number;
}

/** Whether what was loaded still covers the viewport at about this zoom. */
export function covers(loaded: Loaded | null, view: Bbox, zoom: number): boolean {
  return (
    loaded !== null &&
    view[0] >= loaded.bbox[0] &&
    view[1] >= loaded.bbox[1] &&
    view[2] <= loaded.bbox[2] &&
    view[3] <= loaded.bbox[3] &&
    zoom - loaded.zoom < RELOAD_ZOOM_DELTA
  );
}

/** The query string of a layer request. */
export function layerQuery(bbox: Bbox, mine: boolean): string {
  return `?bbox=${encodeURIComponent(bbox.join(','))}${mine ? '&mine=1' : ''}`;
}

export const EMPTY: FeatureCollection = { type: 'FeatureCollection', features: [] };
