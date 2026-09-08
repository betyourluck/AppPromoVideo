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
  font_path: string;
  font_index: number;
  size_ratio: number;
  position: "top" | "bottom";
}
