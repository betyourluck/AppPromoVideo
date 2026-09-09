//! image_gen — 参照画像の生成 (契約 `ImageGenConfig` / `ImageGenerator` / `ImageGenError`)。
//!
//! [`provider`] は Kataribe `app/src-tauri/src/image_gen.rs` (2026-09-08 時点) の**ファイル単位の写し**。
//! 移植の正しさは同梱 PoC がこの workspace で green になることで示す (encode/decode の入出力同型)。
//! 契約差分は別モジュールに載せ、provider.rs への手入れはしない (Kataribe 側との diff を追える状態を保つ):
//!
//! - [`generator`] — `ImageGenerator` trait と HTTP 実装。ComfyUI だけ `/queue` 併読の待機ループを持つ。
//! - [`refs`] — 参照枚数の上限 (API 上限と既定送信枚数を別に持ち、切り詰めを報告する)。
//! - [`comfy_wait`] — バックオフと `/queue` の解釈 (純粋)。
//! - [`palette`] — スナップショットの画素から支配色を出す (LLM 不要)。
//! - [`compose`] — 製品カット: モデルが描いた背景に**実スクショの画素**を貼る (忠実さを構造で保証)。
//! - [`fonts`] / [`caption`] — 見出しの焼き込み (opt-in)。システム + app_data/fonts のフォントを列挙し、copy を描く。
//!
//! provider.rs の `image_file_name` (Kataribe の `{stamp}_{slug}_T{turn}` 命名) はここでは使わない —
//! 保存名は `promo_core::reference_image_name` (`scene_NN_ref_MM.png`)。

pub mod caption;
pub mod comfy_wait;
pub mod compose;
pub mod fonts;
pub mod generator;
pub mod palette;
pub mod provider;
pub mod refs;

pub use generator::{HttpImageGenerator, ImageGenerator, SINGLE_FRAME_CLAUSE, compose_reference_prompt};
pub use caption::{Caption, CaptionLayout, CaptionPosition, LaidOutLine, burn_caption, caption_layout};
pub use compose::{CANVAS_MAX_LONG_EDGE, Layout, Tilt, canvas_for_snapshot, composite_product_cut, fit_to_canvas, plate_scale, solid_backdrop};
pub use fonts::{FontEntry, FontSource, describe_font_file, list_fonts, system_font_dirs};
pub use palette::{DEFAULT_COLORS, dominant_colors, palette_from_file};
pub use provider::{Detail, Generated, ImageGenConfig, ImageGenError, PromptStyle, Provider, RefImage, Shape};
pub use refs::{Truncated, default_send, select_refs};
