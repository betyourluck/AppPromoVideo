//! LLM への指示本文 (純粋)。契約 `RepoBrief` → タスク 1 (解析) / タスク 2 (シーン構成) / 再生成。
//!
//! 規律: 本文は RepoBrief.render() + 指示。`--json-schema` の schema は型から出るので、
//! ここでは**形を繰り返さない** (二重の真実源を作らない)。ただし fenced_json 経路 (aider / custom) では
//! cli_runner が schema を末尾に足す。
//! 画像プロンプトの規律 (Kataribe #85): 「参照 / スクリーンショット / 資料」を指す語を書かせない。

use serde::{Deserialize, Serialize};

use crate::brief::SnapshotMeta;
use crate::plan::{AnalyzedSummary, Aspect, PlanViolation};

/// コピー・ナレーションの言語 (契約 `PromoProject.language`)。video_prompt / image_prompt は常に英語。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Ja,
    En,
}

impl Language {
    fn name(self) -> &'static str {
        match self {
            Language::Ja => "Japanese",
            Language::En => "English",
        }
    }
}

/// タスク 1: リポジトリ解析。`brief` は `RepoBrief::render()`。
pub fn analysis_prompt(brief: &str, concept: &str, language: Language) -> String {
    format!(
        "You are analyzing a software repository to prepare a promotional video.\n\
         The repository brief below is a compressed overview; if you have file tools, read further \
         (README, docs, main entry points, UI code) before answering. Do not modify any file.\n\n\
         {brief}\n\
         ## Video concept from the user\n{concept}\n\n\
         ## Task\n\
         Produce the product analysis as the requested JSON object. Write `one_liner`, `core_value`, \
         `target_audience`, `differentiators` and `hook_copy` in {lang}. Keep `one_liner` at most 30 characters. \
         Give 3 to 5 differentiators. Write `visual_identity` (`mood`, `ui_traits`, `palette`) in English regardless of \
         the language above, because it is fed to image models; describe only what the repository evidences \
         (theme files, CSS, README screenshots); leave `palette` empty if you cannot tell.\n",
        lang = language.name(),
    )
}

/// タスク 2: シーン構成 (rev3: 画像ファースト・i2v 前提・product カットは実スクショの合成)。
pub fn scene_prompt(
    summary: &AnalyzedSummary,
    concept: &str,
    total_seconds: u32,
    aspect: Aspect,
    language: Language,
    snapshots: &[SnapshotMeta],
) -> String {
    let summary_json = serde_json::to_string_pretty(summary).unwrap_or_default();
    let snap_list = if snapshots.is_empty() {
        "(none — use only `mood` cuts)".to_string()
    } else {
        snapshots
            .iter()
            .enumerate()
            .map(|(i, s)| format!("- index {i}: {} ({}x{})", s.path.rsplit(['/', '\\']).next().unwrap_or(&s.path), s.width, s.height))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "You are designing the cut list for a {total_seconds}-second promotional video ({ar}). The workflow is \
         image-first: each cut becomes ONE still image (also usable as a store screenshot), and the video is made \
         from that image with an image-to-video model.\n\n\
         ## Product analysis\n{summary_json}\n\n\
         ## Video concept from the user\n{concept}\n\n\
         ## Available UI snapshots (real screens of the product)\n{snap_list}\n\n\
         ## Rules\n\
         - 3 to 8 scenes, `scene_id` sequential from 1, each 3 to 10 seconds, durations summing to about {total_seconds}.\n\
         - `cut_kind`: use `product` for cuts that show the real product screen and set `snapshot_index` to one of the \
         indices above. The app composites the actual screenshot pixels onto the backdrop, so for `product` cuts \
         `image_prompt` must describe ONLY the backdrop: surface, environment, lighting, with clear empty space in the \
         center, and NO devices, screens, monitors, phones, UI or text. Most cuts of a product promo should be `product`; \
         use `mood` only for the opening or a transition, and set `snapshot_index` to null for `mood`.\n\
         - `image_prompt` is English. Never mention references, screenshots, sheets or attachments.\n\
         - `motion_prompt` is English, one or two sentences, for image-to-video: ONLY camera movement and motion \
         (push-in, parallax, light flicker, subtle drift). Do not restate the picture.\n\
         - `video_prompt` is English, a full text-to-video description as a fallback. Never include aspect flags \
         such as `--ar`; the aspect is fixed to {ar} elsewhere.\n\
         - `copy_text` (caption or narration) is in {lang}.\n\
         - Leave `reference_image` null.\n",
        ar = aspect.as_ar(),
        lang = language.name(),
    )
}

/// 検査違反を LLM に返す文 (再生成の燃料)。英語 (本文と同じ言語)。
pub fn describe_violation(v: &PlanViolation) -> String {
    match v {
        PlanViolation::SceneCount { got } => format!("scene count must be 3..=8, got {got}"),
        PlanViolation::TotalSeconds { got } => format!("total_seconds must be 15, 30 or 60, got {got}"),
        PlanViolation::SceneIdNotSequential { index, got } => {
            format!("scene at index {index} must have scene_id {}, got {got}", index + 1)
        }
        PlanViolation::DurationOutOfRange { scene_id, got } => {
            format!("scene {scene_id}: duration_seconds must be 3..=10, got {got}")
        }
        PlanViolation::EmptyField { scene_id, field } => format!("scene {scene_id}: `{field}` is empty"),
        PlanViolation::VideoPromptNotEnglish { scene_id } => format!("scene {scene_id}: video_prompt must be English"),
        PlanViolation::VideoPromptHasAspectFlag { scene_id } => {
            format!("scene {scene_id}: remove `--ar` from video_prompt")
        }
        PlanViolation::ImagePromptMentionsInput { scene_id, word } => format!(
            "scene {scene_id}: image_prompt mentions the input (`{word}`); describe the picture only"
        ),
        PlanViolation::NoProductCut => {
            "snapshots are available but no scene has cut_kind `product`; the promo must show the real product screen".into()
        }
        PlanViolation::SnapshotIndexInvalid { scene_id, got, count } => format!(
            "scene {scene_id}: cut_kind `product` needs snapshot_index in 0..{count}, got {got:?}"
        ),
        PlanViolation::ProductBackdropDrawsScreen { scene_id, word } => format!(
            "scene {scene_id}: for a `product` cut, image_prompt must describe only the backdrop (no `{word}`); the app places the real screenshot"
        ),
        PlanViolation::MotionPromptEmpty { scene_id } => format!("scene {scene_id}: `motion_prompt` is empty"),
        PlanViolation::MotionPromptNotEnglish { scene_id } => format!("scene {scene_id}: motion_prompt must be English"),
    }
}

/// 再生成時に本文の末尾へ足す (前回の出力と違反一覧)。
pub fn repair_suffix(previous_json: &str, violations: &[PlanViolation]) -> String {
    let mut s = String::from("\n\n## Your previous answer was rejected\n");
    for v in violations {
        s.push_str("- ");
        s.push_str(&describe_violation(v));
        s.push('\n');
    }
    s.push_str("\nPrevious answer (fix only what is listed, keep the rest):\n```json\n");
    s.push_str(previous_json);
    s.push_str("\n```\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::VisualIdentity;

    fn summary() -> AnalyzedSummary {
        AnalyzedSummary {
            app_name: "TaskFlow".into(),
            one_liner: "摩擦のないタスク記録".into(),
            core_value: "認知負荷の削減".into(),
            target_audience: "30代ビジネスパーソン".into(),
            differentiators: vec!["a".into(), "b".into(), "c".into()],
            hook_copy: "整理するほど、時間は増える。".into(),
            visual_identity: VisualIdentity { palette: vec!["#112233".into()], mood: "calm".into(), ui_traits: vec![] },
        }
    }

    #[test]
    fn analysis_prompt_embeds_brief_concept_and_language() {
        let p = analysis_prompt("# Repository brief: x\nTREE", "minimal, calm", Language::Ja);
        assert!(p.contains("# Repository brief: x"));
        assert!(p.contains("minimal, calm"));
        assert!(p.contains("in Japanese"));
        assert!(p.contains("Do not modify any file"));
        // 2026-09-08 live: mood / ui_traits が日本語で返り、そのまま画像モデルへ渡った → 英語に固定。
        assert!(p.contains("`visual_identity` (`mood`, `ui_traits`, `palette`) in English"));
    }

    #[test]
    fn scene_prompt_lists_snapshots_and_explains_product_cuts() {
        let snaps = vec![SnapshotMeta { path: "D:\\x\\dashboard.png".into(), width: 1920, height: 1080 }];
        let p = scene_prompt(&summary(), "cinematic", 30, Aspect::Portrait, Language::En, &snaps);
        assert!(p.contains("30-second"));
        assert!(p.contains("9:16"));
        assert!(p.contains("Never include aspect flags"));
        assert!(p.contains("Never mention"));
        assert!(p.contains("\"app_name\": \"TaskFlow\""));
        assert!(p.contains("- index 0: dashboard.png (1920x1080)"));
        assert!(p.contains("composites the actual screenshot pixels"));
        assert!(p.contains("image-to-video"));
        let none = scene_prompt(&summary(), "c", 15, Aspect::Landscape, Language::Ja, &[]);
        assert!(none.contains("(none — use only `mood` cuts)"));
        assert!(none.contains("in Japanese"));
    }

    #[test]
    fn repair_suffix_lists_every_violation_and_previous_json() {
        let v = vec![
            PlanViolation::SceneCount { got: 2 },
            PlanViolation::ImagePromptMentionsInput { scene_id: 1, word: "screenshot" },
        ];
        let s = repair_suffix("{\"scenes\":[]}", &v);
        assert!(s.contains("scene count must be 3..=8, got 2"));
        assert!(s.contains("mentions the input (`screenshot`)"));
        assert!(s.contains("{\"scenes\":[]}"));
    }
}
