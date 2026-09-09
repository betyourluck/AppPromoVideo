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
    /// scene_id → 見出しの上書き (rev9)。**LLM の schema には足さない** — 埋めるのは人。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub caption_overrides: std::collections::BTreeMap<u32, CaptionOverride>,
    /// scene_id → はめ込みの上書き (rev11)。product カットのみ。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub plate_overrides: std::collections::BTreeMap<u32, PlateOverride>,
    /// scene_id → **LLM が最初に書いたコピー文** (rev12)。人が書き換えても元へ戻せるように、
    /// 生成時に 1 度だけ入れて以後は触らない。`scene.copy_text` の方が「今の文」。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub original_copy: std::collections::BTreeMap<u32, String>,
    /// この run がどちらのモードで合成されたか (rev10)。焼き直しで合成をやり直すとき、
    /// 傾きを効かせるかがこれで決まる — 索引 (app_data) に頼らず promo.json 自身が持つ。
    #[serde(default)]
    pub plate_mode: crate::plan::PlateMode,
}

/// 1 シーンぶんの見出しの上書き (契約 `caption.per_scene`)。**省略したフィールドは既定に落ちる。**
/// 「位置だけ変えて他は設定のまま」が成り立つように、全フィールドが `Option`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CaptionOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_ratio: Option<f32>,
    /// "top" | "bottom"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// "#RRGGBB"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// 生成時のコピー文を控える (rev12、純粋)。空文字のシーンは入れない。
pub fn capture_original_copy(plan: &ScenePlan) -> std::collections::BTreeMap<u32, String> {
    plan.scenes
        .iter()
        .filter(|s| !s.copy_text.trim().is_empty())
        .map(|s| (s.scene_id, s.copy_text.clone()))
        .collect()
}

/// プレート (実スクショの面) の置き方の上書き (契約 `PlateOverride`、rev11)。
/// 全フィールド `Option` — 省略したものは既定 (帯と `scene.plate_tilt`) に落ちる。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlateOverride {
    /// 使うスナップショット (rev13)。LLM の `scene.snapshot_index` を人が選び直せる —
    /// コピー文に合わない画面が選ばれることがあるため。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw_degrees: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pitch_degrees: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_offset_ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_offset_ratio: Option<f32>,
}

impl PlateOverride {
    /// UI の入力は信用せず範囲に丸める (契約 `PlateOverride.clamp`)。
    pub fn clamped(&self) -> PlateOverride {
        let c = |v: Option<f32>, lo: f32, hi: f32| v.map(|x| x.clamp(lo, hi));
        PlateOverride {
            snapshot_index: self.snapshot_index,
            yaw_degrees: c(self.yaw_degrees, -35.0, 35.0),
            pitch_degrees: c(self.pitch_degrees, -35.0, 35.0),
            screen_ratio: c(self.screen_ratio, 0.2, 0.95),
            x_offset_ratio: c(self.x_offset_ratio, -0.4, 0.4),
            y_offset_ratio: c(self.y_offset_ratio, -0.4, 0.4),
        }
    }
}

/// `#RRGGBB` → RGBA (純粋)。読めなければ `None` — 呼び出し側が既定 (白) に落とす。
pub fn parse_hex_rgba(hex: &str) -> Option<[u8; 4]> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
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

/// run の識別子 (契約 `ExportPackage.run_isolation.run_id`、rev7)。
///
/// `YYYYMMDD-HHMMSS` (UTC)。同じ秒に複数走った時は `-2`, `-3` … を付す。
/// **文字列比較がそのまま時系列順**になるので、一覧の並べ替えに日付解析が要らない。
/// UTC に固定するのは、ローカル時刻だと夏時間の切り替わりで順序が壊れるため。
pub fn run_id_from(unix_ms: i64, existing: &[String]) -> String {
    let base = format_utc_compact(unix_ms);
    if !existing.iter().any(|e| e == &base) {
        return base;
    }
    for n in 2..1000 {
        let cand = format!("{base}-{n}");
        if !existing.iter().any(|e| e == &cand) {
            return cand;
        }
    }
    format!("{base}-{}", unix_ms)
}

/// unix ms → `YYYYMMDD-HHMMSS` (UTC、純粋)。暦は Howard Hinnant の civil_from_days。
fn format_utc_compact(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (h, mi, sec) = (sod / 3600, (sod % 3600) / 60, sod % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{sec:02}")
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

    fn scene_for_test(id: u32) -> Scene {
        Scene {
            scene_id: id,
            cut_kind: crate::plan::CutKind::Mood,
            snapshot_index: None,
            plate_tilt: None,
            motion_prompt: "m".into(),
            duration_seconds: 5,
            shot_type: "Wide".into(),
            video_prompt: "v".into(),
            copy_text: String::new(),
            image_prompt: "i".into(),
            reference_image: None,
        }
    }

    fn promo_with_overrides() -> PromoJson {
        PromoJson {
            project_path: "D:/p".into(),
            snapshot_paths: vec![],
            video_concept: "c".into(),
            summary: AnalyzedSummary {
                app_name: "T".into(),
                one_liner: "x".into(),
                core_value: "y".into(),
                target_audience: "z".into(),
                differentiators: vec![],
                hook_copy: "h".into(),
                visual_identity: VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] },
            },
            plan: ScenePlan { total_seconds: 15, aspect: Aspect::Square, scenes: vec![] },
            caption_overrides: Default::default(),
            plate_overrides: Default::default(),
            original_copy: Default::default(),
            plate_mode: Default::default(),
        }
    }
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
                plate_tilt: None,
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

    /// rev7: run を隔離しないと、同じアプリの 2 回目が 1 回目を上書きして消す。
    #[test]
    fn run_id_is_sortable_and_avoids_collisions_in_the_same_second() {
        let a = run_id_from(1_757_000_000_000, &[]);
        assert_eq!(a, "20250904-153320", "UTC 固定 (再現可能にするため): {a}");
        // 同じ秒に 2 本目が来たら -2、3 本目は -3。
        let b = run_id_from(1_757_000_000_000, std::slice::from_ref(&a));
        assert_eq!(b, format!("{a}-2"));
        let c = run_id_from(1_757_000_000_000, &[a.clone(), b.clone()]);
        assert_eq!(c, format!("{a}-3"));
        // 新しい方が文字列比較で必ず後ろに来る (一覧の並べ替えを日付解析に頼らない)。
        let later = run_id_from(1_757_000_060_000, &[]);
        assert!(later > a, "{later} > {a}");
        // ファイル名に使えない文字を含まない。
        assert!(a.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'));
    }

    /// rev12: コピー文は**その場で書き換える** (scenes.md もクリップボードも plan を読むので自動で揃う)。
    /// 元の文は `original_copy` に控えて、戻せるようにする。
    #[test]
    fn original_copy_keeps_only_non_empty_scenes() {
        let mut plan = ScenePlan { total_seconds: 15, aspect: Aspect::Square, scenes: vec![] };
        for (id, text) in [(1u32, "一行目"), (2, "   "), (3, "三行目")] {
            let mut sc = scene_for_test(id);
            sc.copy_text = text.into();
            plan.scenes.push(sc);
        }
        let orig = capture_original_copy(&plan);
        assert_eq!(orig.len(), 2, "空白だけのシーンは控えない");
        assert_eq!(orig[&1], "一行目");
        assert_eq!(orig[&3], "三行目");
        assert!(!orig.contains_key(&2));
    }

    #[test]
    fn hex_color_parses_or_falls_back() {
        assert_eq!(parse_hex_rgba("#FFCC00"), Some([255, 204, 0, 255]));
        assert_eq!(parse_hex_rgba("ffcc00"), Some([255, 204, 0, 255]));
        assert_eq!(parse_hex_rgba(" #000000 "), Some([0, 0, 0, 255]));
        // 読めないものは None にして、呼び出し側が既定 (白) に落とす。
        for bad in ["#FFF", "#GGGGGG", "", "rgb(1,2,3)", "#1234567"] {
            assert_eq!(parse_hex_rgba(bad), None, "{bad}");
        }
    }

    /// rev9: 上書きは**フィールド単位で**既定に落ちる (位置だけ変えて他は既定、が成り立つ)。
    #[test]
    fn overrides_round_trip_and_omit_empty_fields() {
        let mut p = promo_with_overrides();
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("caption_overrides"), "空なら書き出さない (既存の promo.json を汚さない)");

        p.caption_overrides.insert(2, CaptionOverride { position: Some("top".into()), ..Default::default() });
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("caption_overrides"));
        assert!(!json.contains("font_path"), "省略したフィールドは書かない: {json}");
        let back: PromoJson = serde_json::from_str(&json).unwrap();
        assert_eq!(back.caption_overrides[&2].position.as_deref(), Some("top"));
        assert_eq!(back.caption_overrides[&2].size_ratio, None);
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
