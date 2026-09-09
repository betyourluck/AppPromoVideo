/** backend DTO (snake_case は Rust の serde と一致させる)。契約は data_contract.yaml。 */

export type Aspect = "16:9" | "9:16" | "1:1";
export type Language = "ja" | "en";

export interface VisualIdentity {
  palette: string[];
  mood: string;
  ui_traits: string[];
}

export interface AnalyzedSummary {
  app_name: string;
  one_liner: string;
  core_value: string;
  target_audience: string;
  differentiators: string[];
  hook_copy: string;
  visual_identity: VisualIdentity;
}

export type CutKind = "product" | "mood";

export interface Scene {
  scene_id: number;
  cut_kind: CutKind;
  snapshot_index: number | null;
  /** image-to-video 用 (動きとカメラだけ)。 */
  motion_prompt: string;
  duration_seconds: number;
  shot_type: string;
  video_prompt: string;
  copy_text: string;
  image_prompt: string;
  reference_image: string | null;
}

export interface ScenePlan {
  total_seconds: number;
  aspect: Aspect;
  scenes: Scene[];
}

export interface PromoJson {
  project_path: string;
  snapshot_paths: string[];
  video_concept: string;
  summary: AnalyzedSummary;
  plan: ScenePlan;
  /** scene_id → LLM が最初に書いたコピー文 (rev12)。書き換えても戻せるように。 */
  original_copy?: Record<number, string>;
  /** scene_id → 見出しの上書き (rev9)。**開き直した時につまみへ戻すため** frontend も読む (rev21)。 */
  caption_overrides?: Record<number, CaptionOverride>;
  /** scene_id → はめ込みの上書き (rev11)。同上。 */
  plate_overrides?: Record<number, PlateOverride>;
}

/** 契約 `CaptionOverride`。省略したフィールドは既定に落ちる。 */
export interface CaptionOverride {
  font_path?: string | null;
  font_index?: number | null;
  size_ratio?: number | null;
  position?: "top" | "bottom" | null;
  color?: string | null;
  y_ratio?: number | null;
}

/** 契約 `PlateOverride`。product カットのみ。 */
export interface PlateOverride {
  yaw_degrees?: number | null;
  pitch_degrees?: number | null;
  screen_ratio?: number | null;
  x_offset_ratio?: number | null;
  y_offset_ratio?: number | null;
  snapshot_index?: number | null;
}

export interface StageInfo {
  attempts: number;
  cost_usd: number;
  duration_ms: number;
  violations: string[][];
}

export interface RunResult {
  promo: PromoJson;
  package_dir: string;
  analyze: StageInfo;
  plan: StageInfo;
  brief_chars: number;
}

export interface SceneImageInfo {
  scene_id: number;
  ok: boolean;
  path: string | null;
  error: string | null;
}

export interface ImagesResult {
  promo: PromoJson;
  results: SceneImageInfo[];
  palette: string[];
  anchor: string;
  truncated: string | null;
}

export interface SnapshotMeta {
  path: string;
  width: number;
  height: number;
}

export interface BriefPreview {
  text: string;
  chars: number;
  tree_lines: number;
  snapshots: SnapshotMeta[];
}

export interface AuthView {
  api_key_present: boolean;
  api_key_len: number;
  api_key_fingerprint: string;
  auth_token_present: boolean;
  base_url: string;
  oauth_logged_in: boolean | null;
  oauth_method: string;
  scrubbed: string[];
}

export interface CliCheck {
  found: boolean;
  version: string;
  error: string;
  auth: AuthView;
}

export interface Progress {
  stage: string;
  text: string;
}

export interface FontEntry {
  path: string;
  index: number;
  family: string;
  source: "system" | "user";
  has_japanese: boolean;
}

/** backend `CaptionSpec` (snake_case)。 */
export interface CaptionSpec {
  color?: string | null;
  font_path: string;
  font_index: number;
  size_ratio: number;
  position: "top" | "bottom";
  /** 縦位置 (canvas 高さ比、rev14)。null = position + 余白の既定どおり。 */
  y_ratio?: number | null;
}

/** 履歴の 1 行 (契約 RunRecord + 実在フラグ、rev7)。 */
export interface RunListItem {
  id: string;
  created_at: number;
  app_name: string;
  project_path: string;
  package_dir: string;
  run_dir: string;
  seconds: number;
  aspect: string;
  language: string;
  plate_mode: string;
  scene_count: number;
  cost_usd: number;
  image_provider: string | null;
  image_count: number;
  /** シーン構成が通るまでの回数 (rev15)。**0 = 記録なし** (rev14 以前の行)。 */
  plan_attempts: number;
  /** 途中で出た違反の種別 (重複なく整列済み)。 */
  violation_kinds: string[];
  /** run_dir/promo.json が実在するか。false でも索引からは消さない。 */
  exists: boolean;
}

export interface OpenedRun {
  promo: PromoJson;
  package_dir: string;
  images: SceneImageInfo[];
}

