//! promo_core — AppPromoVideo の純関数中核。
//!
//! ここは **プロセスも HTTP も持たない**。data_contract の名詞 (LLM が書く物) を型として持ち、
//! schemars で JSON Schema を機械生成し (`claude -p --json-schema` に渡す。手書き禁止)、
//! LLM 出力を Rust が検める (`validate_scene_plan`)。「LLM は書き、Rust は検める」の Rust 側。

pub mod brief;
pub mod export;
pub mod fenced;
pub mod plan;
pub mod prompts;
pub mod style;

pub use style::{apply_palette, extract_hex, style_anchor};

pub use brief::{BriefInputs, RepoBrief, SnapshotMeta, compress};
pub use export::{
    run_id_from, PromoJson, RunStats, package_dir_name, reference_image_name, scenes_markdown, snapshot_file_name,
    snapshot_prefix,
};
pub use prompts::{Language, analysis_prompt, describe_violation, repair_suffix, scene_prompt};

pub use plan::{
    AnalyzedSummary, Aspect, CopyTarget, CutKind, PlanViolation, Scene, ScenePlan, VisualIdentity, clipboard_text,
    schema_for_scene_plan, schema_for_summary, validate_scene_plan, violation_kind,
};
