//! pipeline — IO と結線 (spec 01 Phase B)。Kataribe の harness に相当。
//!
//! - [`collect`] — FS を読んで `BriefInputs` を作る (刈り込みは promo_core)。
//! - [`task`] — 1 タスクの実行を `TaskRunner` trait に抽象化 (実 CLI とテスト用 fake の差し替え)。
//! - [`stages`] — 解析 → シーン構成。LLM 出力を Rust が検め、違反を戻して再生成する。
//! - [`export`] — パッケージの書き出し。
//! - [`reference`] — シーンごとの参照画像の生成と保存 (image_gen 経由)。

pub mod collect;
pub mod export;
pub mod reference;
pub mod stages;
pub mod task;

pub use stages::{PipelineError, StageReport, analyze, plan_scenes};
pub use task::{CliTaskRunner, TaskRunner};
