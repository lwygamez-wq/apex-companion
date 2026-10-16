// Miroir TypeScript des types sérialisés par apex-core (serde). Toute évolution
// côté Rust doit être répercutée ici : ce fichier est le contrat IPC.

export type SimId = "iracing" | "lmu" | "assetto_corsa" | "ac_evo" | "acc" | "demo";
export type SessionKind = "practice" | "qualifying" | "race" | "hotlap" | "private_testing" | "unknown";
export type LapKind = "out_lap" | "flying" | "in_lap" | "incomplete";
export type Validity = "valid" | "invalid" | "unknown";
export type Support = "implemented" | "unverified" | "planned" | "unavailable";
export type Priority = "critical" | "important" | "improvement" | "info";
export type Confidence = "low" | "medium" | "high";

export interface Weather {
  air_temp_c: number | null;
  track_temp_c: number | null;
  description: string | null;
  wet: boolean | null;
}

export interface SessionMeta {
  sim: SimId;
  track: string;
  track_config: string | null;
  track_length_m: number | null;
  car: string;
  kind: SessionKind;
  started_at_unix: number;
  weather: Weather;
  source_fingerprint: string;
  source_path: string | null;
}

export interface SessionSummary {
  id: number;
  sim: SimId;
  sim_name: string;
  track: string;
  track_config: string | null;
  car: string;
  kind: SessionKind;
  started_at_unix: number;
  weather: Weather;
  lap_count: number;
  clean_lap_count: number;
  best_lap_s: number | null;
  mean_clean_s: number | null;
  stddev_clean_s: number | null;
  incident_laps: number;
  fuel_per_lap_l: number | null;
}

export interface LapFlags {
  off_track: boolean;
  incident: boolean;
  pit: boolean;
  yellow_flag: boolean;
  discontinuity: boolean;
}

export interface LapRow {
  id: number;
  session_id: number;
  is_reference: boolean;
  clean: boolean;
  number: number;
  kind: LapKind;
  lap_time_s: number | null;
  sector_times_s: number[];
  flags: LapFlags;
  validity: Validity;
  validity_source: "sim_reported" | "derived";
  fuel_start_l: number | null;
  fuel_used_l: number | null;
}

export interface Consistency {
  clean_laps: number;
  mean_s: number;
  stddev_s: number;
  cv_percent: number;
}

export interface SessionDetail {
  id: number;
  meta: SessionMeta;
  laps: LapRow[];
  best_lap_id: number | null;
  theoretical_best_s: number | null;
  consistency: Consistency | null;
  personal_reference_lap_id: number | null;
}

export interface Resampled {
  step_m: number;
  dist: number[];
  time: number[];
  speed_kmh: number[];
  throttle: number[];
  brake: number[];
  gear: number[] | null;
  steer_deg: number[] | null;
  x: number[] | null;
  y: number[] | null;
  coverage: number;
  source_density: number;
}

export interface CornerDef {
  id: number;
  name: string;
  search_start_m: number;
  entry_m: number;
  apex_m: number;
  exit_m: number;
}

export interface CornerMetrics {
  corner_id: number;
  time_s: number;
  brake_on_m: number | null;
  brake_peak: number;
  min_speed_kmh: number;
  min_speed_at_m: number;
  throttle_on_m: number | null;
  exit_speed_kmh: number;
  throttle_ramp_s: number | null;
  steer_reversals: number | null;
}

export interface CornerComparison {
  corner: CornerDef;
  reference: CornerMetrics;
  compared: CornerMetrics;
  time_delta_s: number;
}

export interface Evidence {
  label: string;
  reference: string;
  compared: string;
  delta: string;
}

export interface Insight {
  id: string;
  title: string;
  priority: Priority;
  est_gain_s: [number, number] | null;
  segment: string | null;
  segment_range_m: [number, number] | null;
  explanation: string;
  evidence: Evidence[];
  confidence: Confidence;
  confidence_notes: string[];
  recommendation: string;
}

export interface LapSummary {
  lap_id: number;
  number: number;
  lap_time_s: number | null;
  clean: boolean;
  coverage: number;
}

export interface Comparison {
  step_m: number;
  length_m: number;
  reference: LapSummary;
  compared: LapSummary;
  ref_trace: Resampled;
  cmp_trace: Resampled;
  delta_s: number[];
  corners: CornerComparison[];
  insights: Insight[];
  warnings: string[];
}

export type InsertOutcome = { status: "inserted" | "duplicate"; session_id: number };

export interface ImportReport {
  path: string;
  outcome: InsertOutcome | null;
  laps: number;
  error: string | null;
}

export type MonitorState =
  | { state: "no_simulator" }
  | { state: "game_detected"; sim: SimId }
  | { state: "waiting_for_session"; sim: SimId }
  | { state: "session_active"; sim: SimId }
  | { state: "session_ended"; sim: SimId }
  | { state: "importing"; detail: string };

export interface LiveSnapshot {
  connected: boolean;
  on_track: boolean;
  session_time_s: number;
  lap: number;
  lap_dist_pct: number;
  speed_kmh: number;
  gear: number;
  throttle: number;
  brake: number;
  tick: number;
}

export interface StatusPayload {
  state: MonitorState;
  label: string;
  running: SimId[];
  live: LiveSnapshot | null;
  importing: string | null;
  last_auto_import: string | null;
}

export interface Capabilities {
  live_telemetry: Support;
  post_session_import: Support;
  position_xy: Support;
  tyres: Support;
  damage: Support;
  weather: Support;
  replay: Support;
  data_source: string;
  notes: string[];
}

export interface ConnectorInfo {
  id: SimId;
  name: string;
  process_names: string[];
  capabilities: Capabilities;
  install: { installed: boolean | null; detail: string };
  import_extensions: string[];
  default_import_dir: string | null;
}

export interface ProgressZone {
  segment: string;
  range_m: [number, number];
  occurrences: number;
  laps_compared: number;
  avg_loss_s: number;
  max_loss_s: number;
  typical_issue: string;
  example: Insight;
}

export interface Dashboard {
  last_session: SessionSummary | null;
  theoretical_best_s: number | null;
  progress_zones: ProgressZone[];
  consistency_trend: [number, number][];
  next_goal: string | null;
  total_sessions: number;
}

export interface AppSettings {
  iracing_telemetry_dir: string | null;
  units: "metric" | "imperial";
  live_enabled: boolean;
  sample_rate_hz: number;
  auto_import: boolean;
  coach_in_race: boolean;
  coach_audio: boolean;
  coach_min_interval_s: number;
  log_level: string;
  cloud_enabled: boolean;
  beginner_mode: boolean;
  check_updates: boolean;
}

export interface SessionFilter {
  sim?: SimId;
  track?: string;
  car?: string;
  kind?: SessionKind;
  from_unix?: number;
  to_unix?: number;
}
