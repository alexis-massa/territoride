/** Shapes of the JSON the server sends (see src/routes.rs and src/scoring.rs). */

export type Bbox = [west: number, south: number, east: number, north: number];

export interface Player {
  username: string;
  color: string;
  csrf_token: string;
  strava_connected: boolean;
}

export interface MeResponse {
  user: Player | null;
  /** Where the player's holdings are; the map opens there on a first visit. */
  default_bbox: Bbox | null;
}

export interface Score {
  territory_points: number;
  poi_points: number;
  total: number;
}

export interface TerritoryDetail {
  activity_name: string;
  sport_type: string;
  multiplier: number;
  captured_at: string;
  age_days: number;
  cell_count: number;
  points: number;
}

export interface PoiDetail {
  name: string;
  poi_type_label: string;
  altitude_m: number | null;
  sport_type: string;
  claimed_at: string;
  age_days: number;
  points: number;
}

export interface CaptureDetail {
  territory: TerritoryDetail[];
  pois: PoiDetail[];
}

export interface LeaderboardRow extends Score {
  username: string;
  color: string;
  cell_count: number;
  poi_count: number;
  territory_pct: number;
  poi_pct: number;
  detail?: CaptureDetail;
}

export interface Profile {
  username: string;
  color: string;
  member_since: string;
  score: Score;
  territory_pct: number;
  poi_pct: number;
  rank: number | null;
  player_count: number;
  detail: CaptureDetail;
  strava_connected: boolean;
}

export interface Rules {
  loop_tolerance_m: number;
  poi_radius_m: number;
  decay_days: number;
  cell_value: number;
  default_multiplier: number;
  sport_multipliers: [label: string, multiplier: number][];
}

/** What a sync or an upload tells the player. */
export interface Outcome {
  ok: boolean;
  message: string;
}
