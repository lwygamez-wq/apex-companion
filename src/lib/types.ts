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
  /** Pilote d'un tour de référence externe (Garage 61…) ; null pour tes sessions. */
  external_driver: string | null;
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
  /** Tour lancé complet le plus rapide, même invalide. */
  fastest_lap_id: number | null;
  /** Référence par défaut : perso, sinon meilleur tour propre, sinon le plus rapide. */
  analysis_reference_lap_id: number | null;
  off_tracks: OffTrackZone[];
  /** Libellé de la référence perso quand elle vient d'un autre pilote ou d'une autre session. */
  reference_label: string | null;
  risk: CornerRisk[];
  risk_headline: string | null;
}

export type RiskVerdict = "secure" | "cuts" | "push" | "fine" | "insufficient";

export interface CornerRisk {
  name: string;
  apex_m: number;
  entry_m: number;
  exit_m: number;
  passes: number;
  offs: number;
  off_rate: number;
  mean_clean_s: number | null;
  best_clean_s: number | null;
  mean_off_s: number | null;
  off_cost_s: number | null;
  gain_available_s: number | null;
  expected_cost_s: number;
  verdict: RiskVerdict;
  sessions: number;
  message: string;
}

export interface OffTrackZone {
  at_m: number;
  corner: string | null;
  laps: number[];
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
  /** Écart de trajectoire à l'apex (m) : positif = plus à l'intérieur que la référence. */
  apex_offset_m: number | null;
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
  goal: GoalSuggestion | null;
}

export type GoalMetric =
  | { kind: "brake_point"; apex_m: number; target_m: number }
  | { kind: "min_speed"; apex_m: number; target_kmh: number }
  | { kind: "exit_speed"; apex_m: number; target_kmh: number }
  | { kind: "throttle_on"; apex_m: number; target_m: number }
  | { kind: "consistency"; target_sd_s: number }
  | { kind: "clean_laps"; target: number }
  | { kind: "corner_clean"; apex_m: number; target_rate: number };

export interface GoalSuggestion {
  title: string;
  metric: GoalMetric;
}

export interface GoalResult {
  laps_evaluated: number;
  laps_ok: number;
  achieved: boolean;
  summary: string;
}

export interface GoalView {
  id: number;
  title: string;
  track_key: string;
  created_at_unix: number;
  result: GoalResult | null;
  evaluated_session_id: number | null;
}

export interface AutoLogStatus {
  enabled: boolean | null;
  path: string;
}

export interface ExportSummary {
  path: string;
  bytes: number;
  laps: number;
}

export interface LapSummary {
  lap_id: number;
  number: number;
  lap_time_s: number | null;
  clean: boolean;
  coverage: number;
  driver?: string | null;
}

export interface Comparison {
  step_m: number;
  length_m: number;
  reference: LapSummary;
  compared: LapSummary;
  ref_trace: Resampled;
  cmp_trace: Resampled;
  delta_s: number[];
  /** Écart latéral à distance égale (m), positif = à gauche de la référence. */
  line_offset_m: number[] | null;
  ref_off_track_m: [number, number][];
  cmp_off_track_m: [number, number][];
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
  disk_logging: boolean | null;
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
  active_goal: GoalView | null;
  next_goal_suggestion: GoalSuggestion | null;
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
  mobile_export_dir: string | null;
  garage61_token: string | null;
  garage61_auto: boolean;
  watch_downloads: boolean;
  garage61_drivers: string[];
  downloads_dir: string | null;
}

export interface SessionFilter {
  sim?: SimId;
  track?: string;
  car?: string;
  kind?: SessionKind;
  from_unix?: number;
  to_unix?: number;
  /** true : uniquement les références externes. */
  external?: boolean;
}

export interface CircuitOverview {
  sim: SimId;
  track: string;
  track_config: string | null;
  car: string;
  track_length_m: number | null;
  sessions: number;
  laps: number;
  timed_laps: number;
  clean_laps: number;
  best_clean_s: number | null;
  best_any_s: number | null;
  first_unix: number;
  last_unix: number;
  corner_names: "exact" | "aligned" | "none";
  external_reference: [string, number] | null;
}

export interface HistoryLap {
  lap_id: number;
  session_id: number;
  session_kind: SessionKind;
  started_at_unix: number;
  number: number;
  kind: LapKind;
  lap_time_s: number | null;
  clean: boolean;
  off_track: boolean;
  incident: boolean;
  is_reference: boolean;
  delta_to_best_s: number | null;
  personal_best_at_time: boolean;
}

/** Récupération de tes sessions depuis Garage 61. */
export interface RecoverReport {
  sessions: number;
  laps: number;
  messages: string[];
}
