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
    /// 生成時の再生成ループの記録 (rev15)。**`None` = 記録が無い** (rev14 以前の run)。
    /// 索引 (`app_data/runs.json`) は消えうるキャッシュなので、正本である promo.json 自身が持つ
    /// (`plate_mode` と同じ理由)。CLI (`promo run`) と GUI の両方が書く。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_stats: Option<RunStats>,
}

/// 再生成ループが何回で通り、途中でどの検査に引っかかったか (契約 `RunStats`、rev15)。
///
/// **なぜ残すか**: 2026-09-08 の実測で frontal の run だけ費用が約 1.5 倍 (1.422 USD 対 0.919 / 0.944)
/// だったが、attempts も違反種別も永続化していなかったため
/// 「`ProductBackdropAngled` で再生成が発火した」という推測を確認できなかった。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStats {
    /// シーン構成が検査を通るまでにかかった回数。1 = 一発で通った。
    pub plan_attempts: usize,
    /// 途中で出た違反の**種別** (`violation_kind`) を重複なく整列したもの。
    /// 人が読む文ではなく種別を持つのは、scene_id や語が混ざると数えられないため。
    #[serde(default)]
    pub violation_kinds: Vec<String>,
    /// analyze + plan の合計 (USD)。再生成が費用に効いたかを promo.json だけで見るために添える。
    ///
    /// **None = 記録なし。** 費用を返さない CLI (agy) があるので 0 で埋めない — 0 と書くと
    /// 「無料だった」という存在しない事実になる (rev36 と同じ規律)。
    /// 片方の段でも不明なら合計は不明にする (分かっている分だけ足すと部分的な総額という別の嘘になる)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// analyze + plan の全試行で CLI が init 行に書いた**実際の**モデル名。重複なく整列 (rev25)。
    /// 入力した文字列 (`sonnet` / 空欄) ではなく解決後の名前。
    /// **None = 記録なし** (rev24 以前の run)。**Some([]) = CLI が名乗らなかった** (stream-json を出さない CLI)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
}

impl RunStats {
    pub fn new(
        plan_attempts: usize,
        violations_per_attempt: &[Vec<crate::plan::PlanViolation>],
        cost_usd: Option<f64>,
        models: &[String],
    ) -> Self {
        let mut kinds: Vec<String> = violations_per_attempt
            .iter()
            .flatten()
            .map(|v| crate::plan::violation_kind(v).to_string())
            .collect();
        kinds.sort();
        kinds.dedup();
        // rev25: 名乗ったモデルは違反の種別と同じく重複なく整列する。空でも Some — None は「記録なし」専用。
        let mut models: Vec<String> = models.to_vec();
        models.sort();
        models.dedup();
        RunStats { plan_attempts, violation_kinds: kinds, cost_usd, models: Some(models) }
    }
}

/// run に写したスナップショットの名前の**前半** (契約: `snapshot_paths[i]` ↔ `snapshots/snapshot_{i+1:02}.*`)。
/// 拡張子は元のファイル次第なので、読む側はこの前方一致で探す。
pub fn snapshot_prefix(index: usize) -> String {
    format!("snapshot_{:02}.", index + 1)
}

/// 同上のファイル名。**書き出し・読み出し・追加はすべてこの 2 つを通す** —
/// 同じ式を 3 箇所に写していると、片方だけ直して別の画像を貼ることになる (failures #18)。
pub fn snapshot_file_name(index: usize, ext: &str) -> String {
    format!("{}{}", snapshot_prefix(index), ext)
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
    /// 縦位置 (canvas 高さ比、rev14)。指定すると `position` + 余白より優先。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_ratio: Option<f32>,
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
///
/// `run_id` と `CliRawLog` の名前が同じ書式・同じ暦を使う (cli_runner から参照)。
pub fn format_utc_compact(unix_ms: i64) -> String {
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

    pub(super) fn promo_with_overrides() -> PromoJson {
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
            run_stats: None,
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

#[cfg(test)]
mod run_stats_tests {
    use super::tests::promo_with_overrides;
    use super::*;
    use crate::plan::PlanViolation;

    #[test]
    fn kinds_are_deduped_and_sorted_across_attempts() {
        let per_attempt = vec![
            vec![
                PlanViolation::ProductBackdropAngled { scene_id: 1, word: "three-quarter" },
                PlanViolation::ProductBackdropAngled { scene_id: 4, word: "low angle" },
                PlanViolation::MotionPromptEmpty { scene_id: 2 },
            ],
            vec![PlanViolation::ProductBackdropAngled { scene_id: 4, word: "tilted" }],
            vec![],
        ];
        let s = RunStats::new(3, &per_attempt, Some(1.42), &[]);
        assert_eq!(s.plan_attempts, 3);
        assert_eq!(s.violation_kinds, vec!["motion_prompt_empty", "product_backdrop_angled"], "重複なし・整列");
        assert!((s.cost_usd.unwrap() - 1.42).abs() < 1e-9);
    }

    #[test]
    fn a_run_that_passed_first_try_records_no_kinds() {
        let s = RunStats::new(1, &[vec![]], Some(0.92), &[]);
        assert_eq!(s.plan_attempts, 1);
        assert!(s.violation_kinds.is_empty(), "一発で通った run は違反ゼロ");
    }

    /// rev25: 解析と構成の全試行で名乗ったモデルを、重複なく整列して残す。
    #[test]
    fn models_are_deduped_and_sorted_and_kept_even_when_empty() {
        let seen: Vec<String> = vec!["claude-sonnet-5".into(), "claude-opus-5".into(), "claude-sonnet-5".into()];
        let s = RunStats::new(2, &[vec![], vec![]], Some(0.5), &seen);
        assert_eq!(s.models, Some(vec!["claude-opus-5".to_string(), "claude-sonnet-5".to_string()]));
        // CLI が名乗らなかった (stream-json を出さない CLI) は Some([]) — None (記録なし) と区別する。
        assert_eq!(RunStats::new(1, &[vec![]], Some(0.1), &[]).models, Some(vec![]));
    }

    /// rev24 以前の `run_stats` には `models` が無い。**None (記録なし)** として読めること —
    /// `Some([])` に落とすと「CLI が名乗らなかった」と嘘をつく。
    #[test]
    fn an_older_run_stats_without_models_reads_as_not_recorded() {
        let s: RunStats = serde_json::from_str(r#"{"plan_attempts":1,"violation_kinds":[],"cost_usd":0.9}"#).unwrap();
        assert_eq!(s.models, None);
    }

    /// rev14 以前の promo.json には `run_stats` が無い。**読めること**と、
    /// **無いことが `None` (記録なし) として残ること**の両方を要求する。
    /// ここを `Default` (= attempts 1) に落とすと、過去の run が「一発で通った」と嘘をつく。
    #[test]
    fn an_older_promo_json_loads_with_no_stats_and_does_not_claim_one_attempt() {
        let mut p = promo_with_overrides();
        p.run_stats = None;
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("run_stats"), "None なら書き出さない (既存の promo.json を汚さない)");
        let back: PromoJson = serde_json::from_str(&json).unwrap();
        assert!(back.run_stats.is_none(), "記録が無いことは 1 回ではない");
    }

    #[test]
    fn run_stats_round_trips() {
        let mut p = promo_with_overrides();
        p.run_stats = Some(RunStats {
            plan_attempts: 2,
            violation_kinds: vec!["product_backdrop_angled".into()],
            cost_usd: Some(1.422),
            models: Some(vec!["claude-sonnet-5".into()]),
        });
        let back: PromoJson = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.run_stats, p.run_stats);
    }
}

#[cfg(test)]
mod snapshot_name_tests {
    use super::*;

    /// **契約**: `PromoJson.snapshot_paths[i]` ↔ `<run>/snapshots/snapshot_{i+1:02}.*`。
    /// ここが崩れると `PlateOverride.snapshot_index` が**別の画像**を指す。
    /// 書き出し (write_package) / 読み出し (read_run_snapshot) / 追加 (add_run_snapshot) の
    /// 3 箇所が同じ式を持っていたので 1 箇所にした (failures #18 の処方の一般形)。
    #[test]
    fn the_index_and_the_file_name_agree() {
        assert_eq!(snapshot_file_name(0, "png"), "snapshot_01.png");
        assert_eq!(snapshot_file_name(1, "jpg"), "snapshot_02.jpg");
        assert_eq!(snapshot_prefix(0), "snapshot_01.");
        assert!(snapshot_file_name(0, "png").starts_with(&snapshot_prefix(0)));
    }

    /// 100 枚目以降も桁が伸びるだけで、前方一致は保たれる (`{:02}` は切り捨てない)。
    #[test]
    fn beyond_two_digits_the_prefix_still_matches() {
        assert_eq!(snapshot_file_name(99, "png"), "snapshot_100.png");
        assert!(snapshot_file_name(99, "png").starts_with(&snapshot_prefix(99)));
        // 09 の前方一致が 090 を巻き込まない (点で切れている)。
        assert!(!snapshot_file_name(89, "png").starts_with(&snapshot_prefix(8)));
    }
}
