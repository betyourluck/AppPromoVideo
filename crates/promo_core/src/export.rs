//! export の整形 (純粋)。契約 `ExportPackage`。書き出し (IO) は pipeline。

use serde::{Deserialize, Serialize};

use crate::plan::{AnalyzedSummary, ScenePlan};

/// `promo.json` の形。`project` はパスだけ (秘密や設定を混ぜない)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromoJson {
    pub project_path: String,
    pub snapshot_paths: Vec<String>,
    pub video_concept: String,
    pub summary: AnalyzedSummary,
    pub plan: ScenePlan,
}

/// export フォルダ名 (`<app_name>_Promo_Package`)。パス要素禁止 — 英数と `-_` 以外は `_`。
pub fn package_dir_name(app_name: &str) -> String {
    let slug: String = app_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let slug = slug.trim_matches('_');
    let slug = if slug.is_empty() { "App" } else { slug };
    format!("{slug}_Promo_Package")
}

/// 参照画像の保存名 (契約: `scene_{NN}_ref_{MM}.png`)。
pub fn reference_image_name(scene_id: u32, index: u32) -> String {
    format!("scene_{scene_id:02}_ref_{index:02}.png")
}

/// `scenes.md`: 構成表 + 各シーンの全文 (Veo / Sora に貼る単位)。
pub fn scenes_markdown(summary: &AnalyzedSummary, plan: &ScenePlan) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {} — promo shot list\n\n", summary.app_name));
    s.push_str(&format!("**Hook**: {}\n\n", summary.hook_copy));
    s.push_str(&format!("**Length**: {}s · **Aspect**: {}\n\n", plan.total_seconds, plan.aspect.as_ar()));
    s.push_str("Workflow: each cut is ONE still image → image-to-video (MiniMax). Paste the image and the motion prompt.\n\n");
    s.push_str("| # | sec | kind | shot | copy | image |\n|---|---|---|---|---|---|\n");
    for sc in &plan.scenes {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            sc.scene_id,
            sc.duration_seconds,
            match sc.cut_kind {
                crate::plan::CutKind::Product => format!("product (snapshot {})", sc.snapshot_index.map(|i| i.to_string()).unwrap_or("?".into())),
                crate::plan::CutKind::Mood => "mood".into(),
            },
            cell(&sc.shot_type),
            cell(&sc.copy_text),
            sc.reference_image.as_deref().unwrap_or("-")
        ));
    }
    for sc in &plan.scenes {
        s.push_str(&format!("\n## Scene {} ({}s, {})\n\n", sc.scene_id, sc.duration_seconds, sc.shot_type));
        if let Some(img) = &sc.reference_image {
            s.push_str(&format!("Image: `{img}`\n\n"));
        }
        s.push_str("### Motion prompt (image-to-video)\n\n```\n");
        s.push_str(sc.motion_prompt.trim());
        s.push_str("\n```\n\n### Video prompt (text-to-video fallback)\n\n```\n");
        s.push_str(sc.video_prompt.trim());
        s.push_str("\n```\n\n### Backdrop / image prompt\n\n```\n");
        s.push_str(sc.image_prompt.trim());
        s.push_str("\n```\n\n### Copy\n\n");
        s.push_str(sc.copy_text.trim());
        s.push('\n');
    }
    s
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Aspect, Scene, VisualIdentity};

    fn fixture() -> (AnalyzedSummary, ScenePlan) {
        let summary = AnalyzedSummary {
            app_name: "Task Flow/Minimal".into(),
            one_liner: "x".into(),
            core_value: "y".into(),
            target_audience: "z".into(),
            differentiators: vec![],
            hook_copy: "整理するほど、時間は増える。".into(),
            visual_identity: VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] },
        };
        let plan = ScenePlan {
            total_seconds: 15,
            aspect: Aspect::Landscape,
            scenes: vec![Scene {
                scene_id: 1,
                cut_kind: crate::plan::CutKind::Product,
                snapshot_index: Some(0),
                motion_prompt: "Slow push-in.".into(),
                duration_seconds: 5,
                shot_type: "Close-up".into(),
                video_prompt: "Cinematic desk shot.".into(),
                copy_text: "a | b".into(),
                image_prompt: "A desk.".into(),
                reference_image: Some("scene_01_ref_01.png".into()),
            }],
        };
        (summary, plan)
    }

    #[test]
    fn package_dir_name_has_no_path_elements() {
        assert_eq!(package_dir_name("Task Flow/Minimal"), "Task_Flow_Minimal_Promo_Package");
        assert_eq!(package_dir_name("../../etc"), "etc_Promo_Package");
        assert_eq!(package_dir_name("日本語"), "App_Promo_Package");
    }

    #[test]
    fn reference_image_name_is_zero_padded() {
        assert_eq!(reference_image_name(2, 1), "scene_02_ref_01.png");
    }

    #[test]
    fn scenes_markdown_has_table_and_fenced_prompts() {
        let (s, p) = fixture();
        let md = scenes_markdown(&s, &p);
        assert!(md.contains("| 1 | 5 | product (snapshot 0) | Close-up | a \\| b | scene_01_ref_01.png |"));
        assert!(md.contains("### Motion prompt (image-to-video)\n\n```\nSlow push-in.\n```"));
        assert!(md.contains("```\nCinematic desk shot.\n```"));
        assert!(md.contains("**Aspect**: 16:9"));
        assert!(!md.contains("--ar"));
    }
}
