//! パイプラインの名詞 (data_contract `AnalyzedSummary` / `ScenePlan` / `Scene`) と、その検査。
//!
//! `JsonSchema` 派生が **規格の正本**: `--json-schema` に渡す schema はここから機械生成する。
//! doc comment は schema の `description` に写るので、LLM への指示として書く。

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// スタイルアンカー (画像プロンプトの接頭辞素材)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VisualIdentity {
    /// Dominant colors of the app UI, 3 to 5 entries. Color names or hex codes.
    pub palette: Vec<String>,
    /// Mood in a few English words, e.g. "calm, minimal, warm light".
    pub mood: String,
    /// Distinctive UI traits, e.g. "dark sidebar", "rounded cards".
    pub ui_traits: Vec<String>,
}

/// タスク 1 (リポジトリ解析) の出力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnalyzedSummary {
    /// Product name as users would see it.
    pub app_name: String,
    /// One line, at most 30 characters, in the requested language.
    pub one_liner: String,
    /// The core value: what pain it removes and for whom.
    pub core_value: String,
    /// Target audience in one sentence.
    pub target_audience: String,
    /// 3 to 5 differentiators, each a short phrase.
    pub differentiators: Vec<String>,
    /// One hook copy line for the video opening.
    pub hook_copy: String,
    /// Visual identity inferred from README, theme settings and UI descriptions.
    pub visual_identity: VisualIdentity,
}

/// 動画のアスペクト比 (ScenePlan が 1 箇所で持つ。video_prompt には書かせない)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[serde(rename = "16:9")]
    Landscape,
    #[serde(rename = "9:16")]
    Portrait,
    #[serde(rename = "1:1")]
    Square,
}

impl Aspect {
    /// コピー / export に付ける `--ar` 表記。
    pub fn as_ar(self) -> &'static str {
        match self {
            Aspect::Landscape => "16:9",
            Aspect::Portrait => "9:16",
            Aspect::Square => "1:1",
        }
    }
}

/// カットの種別 (rev3、2026-09-08 ユーザー FB「スクショに全く従わない」起点)。
/// `product` = 実スクショの画素を Rust が合成する (モデルは背景だけ描く) / `mood` = 情景 (モデルが全部描く)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum CutKind {
    /// Shows the real product screen. The app composites the actual screenshot pixels onto the
    /// backdrop; `image_prompt` must describe ONLY the backdrop.
    Product,
    /// Atmospheric scene with no product screen.
    #[default]
    Mood,
}

/// 1 カット。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Scene {
    /// Sequential id starting at 1.
    pub scene_id: u32,
    /// `product` when the real app screen is shown (the screenshot is composited by the app),
    /// `mood` for atmospheric shots. Most scenes of a product promo should be `product`.
    #[serde(default)]
    pub cut_kind: CutKind,
    /// For `product` cuts: 0-based index into the provided snapshot list. Null for `mood`.
    #[serde(default)]
    pub snapshot_index: Option<u32>,
    /// English, one or two sentences, for image-to-video: describe ONLY camera movement and motion
    /// (e.g. "slow push-in, soft light flicker, subtle parallax"). The still image supplies the content.
    #[serde(default)]
    pub motion_prompt: String,
    /// 3 to 10 seconds.
    pub duration_seconds: u32,
    /// Shot type, e.g. "Close-up", "Medium shot", "Wide", "Screen recording".
    pub shot_type: String,
    /// English prompt for a video generation model (Veo / Sora). Describe camera, subject, light,
    /// motion. Do NOT append aspect flags such as "--ar".
    pub video_prompt: String,
    /// Caption or narration in the requested language.
    pub copy_text: String,
    /// English prompt for ONE still frame, for an image model. For `product` cuts describe ONLY the
    /// backdrop (surface, lighting, environment) with clear empty space in the center and NO devices,
    /// screens, UI or text — the app places the real screenshot there. For `mood` cuts describe the
    /// whole picture. Never mention references, screenshots, sheets or attachments.
    pub image_prompt: String,
    /// For `product` cuts only: how far the app should tilt the pasted screen plate so it matches the
    /// perspective you describe in `image_prompt`. Both values in degrees, -35 to 35. Omit (null) for a
    /// straight-on backdrop. `yaw_degrees` negative tilts the LEFT edge away, positive the RIGHT edge away;
    /// `pitch_degrees` negative looks DOWN at the plate (backdrop seen from above), positive looks UP at it.
    #[serde(default)]
    pub plate_tilt: Option<PlateTilt>,
    /// Filled in after reference image generation. Leave null.
    #[serde(default)]
    pub reference_image: Option<String>,
}

/// タスク 2 (シーン構成) の出力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScenePlan {
    /// Total length: 15, 30 or 60 seconds.
    pub total_seconds: u32,
    /// Aspect ratio of the video.
    pub aspect: Aspect,
    /// 3 to 8 scenes.
    pub scenes: Vec<Scene>,
}

/// `--json-schema` に渡す schema (機械生成)。
pub fn schema_for_scene_plan() -> Value {
    serde_json::to_value(schema_for!(ScenePlan)).expect("schemars の出力は常に JSON 化できる")
}

/// `--json-schema` に渡す schema (機械生成)。
pub fn schema_for_summary() -> Value {
    serde_json::to_value(schema_for!(AnalyzedSummary)).expect("schemars の出力は常に JSON 化できる")
}

/// LLM 出力の検査結果。構造化データで返し、文面は提示層が作る (Kataribe RejectReason と同じ流儀)。
#[derive(Debug, Clone, PartialEq)]
pub enum PlanViolation {
    SceneCount {
        got: usize,
    },
    TotalSeconds {
        got: u32,
    },
    SceneIdNotSequential {
        index: usize,
        got: u32,
    },
    DurationOutOfRange {
        scene_id: u32,
        got: u32,
    },
    EmptyField {
        scene_id: u32,
        field: &'static str,
    },
    VideoPromptNotEnglish {
        scene_id: u32,
    },
    VideoPromptHasAspectFlag {
        scene_id: u32,
    },
    /// image_prompt に「入力を指す語」がある (Kataribe #85: 被写体として描かれる)。
    ImagePromptMentionsInput {
        scene_id: u32,
        word: &'static str,
    },
    /// スナップショットがあるのに product カットが 1 つも無い (製品が映らない宣伝になる)。
    NoProductCut,
    /// product カットの snapshot_index が範囲外 / 無い。
    SnapshotIndexInvalid {
        scene_id: u32,
        got: Option<u32>,
        count: usize,
    },
    /// product カットの image_prompt に画面・端末・文字を描く語がある (合成と二重になる)。
    ProductBackdropDrawsScreen {
        scene_id: u32,
        word: &'static str,
    },
    MotionPromptEmpty {
        scene_id: u32,
    },
    MotionPromptNotEnglish {
        scene_id: u32,
    },
    /// plate_tilt が ±35 度の外 (rev5)。両モードで検める。
    PlateTiltOutOfRange {
        scene_id: u32,
        yaw: f32,
        pitch: f32,
    },
    /// PlateMode::Frontal なのに背景がアングル指定を含む (面は正対のままなので噛み合わない)。
    ProductBackdropAngled {
        scene_id: u32,
        word: &'static str,
    },
}

/// 違反の**種別**だけを返す (契約 `RunStats.violation_kinds`、rev15)。
///
/// `describe_violation` は LLM と人に返す文なので scene_id や語が埋まっており、同じ種類の違反でも
/// 文字列が変わる。「このモードは再生成が起きやすいか」を数えるには畳めるキーが要るので別に持つ。
/// **この文字列は promo.json に残る = 契約**。変えたら data_contract と過去の run の両方に響く。
pub fn violation_kind(v: &PlanViolation) -> &'static str {
    match v {
        PlanViolation::SceneCount { .. } => "scene_count",
        PlanViolation::TotalSeconds { .. } => "total_seconds",
        PlanViolation::SceneIdNotSequential { .. } => "scene_id_not_sequential",
        PlanViolation::DurationOutOfRange { .. } => "duration_out_of_range",
        PlanViolation::EmptyField { .. } => "empty_field",
        PlanViolation::VideoPromptNotEnglish { .. } => "video_prompt_not_english",
        PlanViolation::VideoPromptHasAspectFlag { .. } => "video_prompt_has_aspect_flag",
        PlanViolation::ImagePromptMentionsInput { .. } => "image_prompt_mentions_input",
        PlanViolation::NoProductCut => "no_product_cut",
        PlanViolation::SnapshotIndexInvalid { .. } => "snapshot_index_invalid",
        PlanViolation::ProductBackdropDrawsScreen { .. } => "product_backdrop_draws_screen",
        PlanViolation::MotionPromptEmpty { .. } => "motion_prompt_empty",
        PlanViolation::MotionPromptNotEnglish { .. } => "motion_prompt_not_english",
        PlanViolation::PlateTiltOutOfRange { .. } => "plate_tilt_out_of_range",
        PlanViolation::ProductBackdropAngled { .. } => "product_backdrop_angled",
    }
}

/// product カットのスクショ面の貼り方 (契約 `PlateMode`、rev5)。設定で切り替える。
///
/// **既定は rev22 で `Frontal` に変えた** — MiniMax の image-to-video に通した実測で、
/// 面を傾けると**動きが過剰になった** (ユーザー観測 2026-09-09、rev5 から開いていた判断の決着)。
/// 傾ける経路は残す (静止画としての見栄えと i2v での挙動は別の話なので、捨てる根拠は無い)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlateMode {
    /// `Scene::plate_tilt` に従い、面を射影変換で傾けて背景のパースに合わせる。
    Perspective,
    /// **既定** (rev22)。正面固定。`plate_tilt` を無視し、背景側もアングル語を弾いて正対に保つ。
    #[default]
    Frontal,
}

/// スクショ面の傾き (契約 `Scene.plate_tilt`)。省略 = 正面。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlateTilt {
    /// Left/right swing in degrees, -35..=35. Negative tilts the LEFT edge away, positive the RIGHT edge away.
    pub yaw_degrees: f32,
    /// Up/down swing in degrees, -35..=35. Negative looks DOWN at the plate, positive looks UP at it.
    pub pitch_degrees: f32,
}

impl PlateTilt {
    pub const LIMIT_DEGREES: f32 = 35.0;

    pub fn is_zero(&self) -> bool {
        self.yaw_degrees == 0.0 && self.pitch_degrees == 0.0
    }

    fn out_of_range(&self) -> bool {
        let l = Self::LIMIT_DEGREES;
        !(-l..=l).contains(&self.yaw_degrees) || !(-l..=l).contains(&self.pitch_degrees)
    }
}

const SCENES_MIN: usize = 3;
const SCENES_MAX: usize = 8;
const DURATION_MIN: u32 = 3;
const DURATION_MAX: u32 = 10;
const TOTALS: [u32; 3] = [15, 30, 60];
/// image_prompt に混ざると構図の指示として読まれる語 (小文字比較)。
const INPUT_WORDS: [&str; 5] = ["reference", "screenshot", "sheet", "attachment", "attached"];
/// product の背景に混ざると合成と二重になる語 (画面や文字をモデルが描いてしまう)。
const SCREEN_WORDS: [&str; 8] = [
    "screen",
    "monitor",
    "laptop",
    "phone",
    "display",
    "ui",
    "interface",
    "text",
];
/// Frontal モードで背景に混ざると面と噛み合わない語 (背景だけ傾く)。語順のある句なので部分一致で見る。
const ANGLE_PHRASES: [&str; 10] = [
    "low angle",
    "high angle",
    "top-down",
    "top down",
    "overhead",
    "bird's-eye",
    "three-quarter",
    "isometric",
    "from above",
    "from below",
];

/// 英語判定: アルファベット文字のうち ASCII が 90% 以上。空は呼び出し側が先に弾く。
fn looks_english(s: &str) -> bool {
    let alpha: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    if alpha.is_empty() {
        return false;
    }
    let ascii = alpha.iter().filter(|c| c.is_ascii_alphabetic()).count();
    ascii * 10 >= alpha.len() * 9
}

/// ScenePlan を検める (純粋)。違反は全件返す (最初の 1 件で止めない — 再生成の燃料にする)。
/// `snapshot_count` = 入力スナップショットの枚数 (0 なら product カットは要求しない)。
pub fn validate_scene_plan(
    plan: &ScenePlan,
    snapshot_count: usize,
    mode: PlateMode,
) -> Vec<PlanViolation> {
    let mut v = Vec::new();
    if snapshot_count > 0 && !plan.scenes.iter().any(|s| s.cut_kind == CutKind::Product) {
        v.push(PlanViolation::NoProductCut);
    }
    if !TOTALS.contains(&plan.total_seconds) {
        v.push(PlanViolation::TotalSeconds {
            got: plan.total_seconds,
        });
    }
    let n = plan.scenes.len();
    if !(SCENES_MIN..=SCENES_MAX).contains(&n) {
        v.push(PlanViolation::SceneCount { got: n });
    }
    for (i, s) in plan.scenes.iter().enumerate() {
        let expect = i as u32 + 1;
        if s.scene_id != expect {
            v.push(PlanViolation::SceneIdNotSequential {
                index: i,
                got: s.scene_id,
            });
        }
        if !(DURATION_MIN..=DURATION_MAX).contains(&s.duration_seconds) {
            v.push(PlanViolation::DurationOutOfRange {
                scene_id: s.scene_id,
                got: s.duration_seconds,
            });
        }
        for (field, val) in [
            ("shot_type", &s.shot_type),
            ("video_prompt", &s.video_prompt),
            ("copy_text", &s.copy_text),
            ("image_prompt", &s.image_prompt),
        ] {
            if val.trim().is_empty() {
                v.push(PlanViolation::EmptyField {
                    scene_id: s.scene_id,
                    field,
                });
            }
        }
        if !s.video_prompt.trim().is_empty() && !looks_english(&s.video_prompt) {
            v.push(PlanViolation::VideoPromptNotEnglish {
                scene_id: s.scene_id,
            });
        }
        if s.video_prompt.contains("--ar") {
            v.push(PlanViolation::VideoPromptHasAspectFlag {
                scene_id: s.scene_id,
            });
        }
        let lower = s.image_prompt.to_lowercase();
        if let Some(word) = INPUT_WORDS.iter().find(|w| lower.contains(*w)) {
            v.push(PlanViolation::ImagePromptMentionsInput {
                scene_id: s.scene_id,
                word,
            });
        }
        if s.cut_kind == CutKind::Product {
            match s.snapshot_index {
                Some(i) if (i as usize) < snapshot_count => {}
                other => v.push(PlanViolation::SnapshotIndexInvalid {
                    scene_id: s.scene_id,
                    got: other,
                    count: snapshot_count,
                }),
            }
            // 単語境界で見る ("ui" が "building" に当たらないように)。
            let words: Vec<String> = lower
                .split(|c: char| !c.is_alphanumeric())
                .map(|w| w.to_string())
                .collect();
            if let Some(word) = SCREEN_WORDS
                .iter()
                .find(|w| words.iter().any(|x| x == *w || x == &format!("{w}s")))
            {
                v.push(PlanViolation::ProductBackdropDrawsScreen {
                    scene_id: s.scene_id,
                    word,
                });
            }
            if mode == PlateMode::Frontal
                && let Some(word) = ANGLE_PHRASES.iter().find(|w| lower.contains(**w))
            {
                v.push(PlanViolation::ProductBackdropAngled {
                    scene_id: s.scene_id,
                    word,
                });
            }
            if let Some(t) = s.plate_tilt
                && t.out_of_range()
            {
                v.push(PlanViolation::PlateTiltOutOfRange {
                    scene_id: s.scene_id,
                    yaw: t.yaw_degrees,
                    pitch: t.pitch_degrees,
                });
            }
        }
        if s.motion_prompt.trim().is_empty() {
            v.push(PlanViolation::MotionPromptEmpty {
                scene_id: s.scene_id,
            });
        } else if !looks_english(&s.motion_prompt) {
            v.push(PlanViolation::MotionPromptNotEnglish {
                scene_id: s.scene_id,
            });
        }
    }
    v
}

/// コピー先 (契約 `ExportPackage.clipboard`、rev3)。
/// MiniMax (画像→動画) が主。Veo / Sora は「指示に従わない」(ユーザー実測 2026-09-08) ので選択肢から外し、
/// text-to-video は汎用の保険として 1 つだけ残す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CopyTarget {
    /// 画像→動画 (MiniMax 等): 動きの短文だけ。画像はファイルで渡す。
    Minimax,
    /// text-to-video の保険: 全文 + 別行にメタ。
    Generic,
}

/// クリップボードに載せる 1 シーン分 (契約 `ExportPackage.clipboard`)。`--ar` はどこにも出さない。
pub fn clipboard_text(scene: &Scene, aspect: Aspect, target: CopyTarget) -> String {
    match target {
        CopyTarget::Minimax => {
            let m = scene.motion_prompt.trim();
            if m.is_empty() {
                scene.video_prompt.trim().to_string()
            } else {
                m.to_string()
            }
        }
        CopyTarget::Generic => format!(
            "{}\n[aspect {} | {}s]",
            scene.video_prompt.trim(),
            aspect.as_ar(),
            scene.duration_seconds
        ),
    }
}


/// プロンプトの書き換えで起きうること (契約 `ScenePromptEdit`、rev37)。
#[derive(Debug, PartialEq, Eq)]
pub enum PromptEditError {
    /// その `scene_id` が plan に無い。
    SceneNotFound(u32),
    /// `motion_prompt` が空 (前後の空白を落として空)。
    EmptyMotion,
    /// `video_prompt` が空。
    EmptyVideo,
}

/// シーンの `motion_prompt` / `video_prompt` を書き換える**唯一の関数** (rev37、契約 `ScenePromptEdit`)。
///
/// 空は拒む — どちらも検査の対象 (`motion_prompt_empty` 等) で、空のまま保存すると次の検査と食い違う。
/// **拒んだ時は plan を触らない** (片方だけ書き換わると、画面と promo.json が食い違う)。
pub fn set_scene_prompts(
    plan: &mut ScenePlan,
    scene_id: u32,
    motion: &str,
    video: &str,
) -> Result<(), PromptEditError> {
    let motion = motion.trim();
    let video = video.trim();
    if motion.is_empty() {
        return Err(PromptEditError::EmptyMotion);
    }
    if video.is_empty() {
        return Err(PromptEditError::EmptyVideo);
    }
    let scene = plan
        .scenes
        .iter_mut()
        .find(|s| s.scene_id == scene_id)
        .ok_or(PromptEditError::SceneNotFound(scene_id))?;
    scene.motion_prompt = motion.to_string();
    scene.video_prompt = video.to_string();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(id: u32) -> Scene {
        Scene {
            scene_id: id,
            cut_kind: CutKind::Mood,
            snapshot_index: None,
            plate_tilt: None,
            motion_prompt: "Slow push-in with a soft light flicker.".into(),
            duration_seconds: 5,
            shot_type: "Close-up".into(),
            video_prompt: "Cinematic close-up of a laptop on a sunlit desk, slow dolly-in.".into(),
            copy_text: "整理するほど、時間は増える。".into(),
            image_prompt: "A laptop on a sunlit wooden desk, minimalist dashboard on screen."
                .into(),
            reference_image: None,
        }
    }

    fn plan() -> ScenePlan {
        ScenePlan {
            total_seconds: 15,
            aspect: Aspect::Landscape,
            scenes: (1..=3).map(scene).collect(),
        }
    }

    fn product(id: u32, idx: Option<u32>, backdrop: &str) -> Scene {
        let mut s = scene(id);
        s.cut_kind = CutKind::Product;
        s.snapshot_index = idx;
        s.image_prompt = backdrop.into();
        s
    }

    // rev37: プロンプトの書き換え (契約 ScenePromptEdit)。ユーザー「鉛筆で編集モードにしてから書き換えたい」。
    #[test]
    fn set_scene_prompts_writes_both_and_trims() {
        let mut p = plan();
        assert_eq!(set_scene_prompts(&mut p, 2, "  Slow pan right.  ", "
A calm desk, slow pan right.
"), Ok(()));
        let s = p.scenes.iter().find(|s| s.scene_id == 2).unwrap();
        assert_eq!(s.motion_prompt, "Slow pan right.");
        assert_eq!(s.video_prompt, "A calm desk, slow pan right.");
    }

    #[test]
    fn set_scene_prompts_rejects_empty_and_does_not_touch_the_plan() {
        // 空は検査の違反 (motion_prompt_empty)。**拒んだ時は片方だけ書き換えない**。
        let mut p = plan();
        let (motion, video) = (p.scenes[0].motion_prompt.clone(), p.scenes[0].video_prompt.clone());
        assert_eq!(set_scene_prompts(&mut p, 1, "   ", "ok video"), Err(PromptEditError::EmptyMotion));
        assert_eq!(set_scene_prompts(&mut p, 1, "ok motion", "	
"), Err(PromptEditError::EmptyVideo));
        assert_eq!(p.scenes[0].motion_prompt, motion);
        assert_eq!(p.scenes[0].video_prompt, video);
    }

    #[test]
    fn set_scene_prompts_rejects_unknown_scene() {
        let mut p = plan();
        let motion = p.scenes[0].motion_prompt.clone();
        assert_eq!(set_scene_prompts(&mut p, 99, "m", "v"), Err(PromptEditError::SceneNotFound(99)));
        assert_eq!(p.scenes[0].motion_prompt, motion);
    }

    #[test]
    fn schema_has_required_scene_fields_and_descriptions() {
        let s = schema_for_scene_plan();
        let scene_def = &s["definitions"]["Scene"];
        let required: Vec<&str> = scene_def["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for f in [
            "scene_id",
            "duration_seconds",
            "shot_type",
            "video_prompt",
            "copy_text",
            "image_prompt",
        ] {
            assert!(required.contains(&f), "required に {f} が無い");
        }
        // reference_image は default 付きなので required に入らない (LLM に埋めさせない)。
        assert!(!required.contains(&"reference_image"));
        // rev3: cut_kind は product | mood の文字列 enum、product の意味が description に出る。
        assert_eq!(
            &s["definitions"]["CutKind"]["oneOf"][0]["enum"],
            &serde_json::json!(["product"])
        );
        assert!(
            s["definitions"]["CutKind"]["oneOf"][0]["description"]
                .as_str()
                .unwrap()
                .contains("composites the actual screenshot")
        );
        assert!(
            scene_def["properties"]["motion_prompt"]["description"]
                .as_str()
                .unwrap()
                .contains("image-to-video")
        );
        // doc comment が description に写る = LLM への指示が型から出る。
        let desc = scene_def["properties"]["image_prompt"]["description"]
            .as_str()
            .unwrap();
        assert!(desc.contains("Never mention references"));
        // aspect は "16:9" 等の文字列 enum。
        let aspect = &s["definitions"]["Aspect"]["enum"];
        assert_eq!(aspect, &serde_json::json!(["16:9", "9:16", "1:1"]));
    }

    #[test]
    fn valid_plan_has_no_violations() {
        assert!(validate_scene_plan(&plan(), 0, PlateMode::Perspective).is_empty());
    }

    /// rev3: スナップショットがあるなら product カットが要り、index は範囲内、背景に画面や文字を描かせない。
    #[test]
    fn product_cut_rules() {
        let mut p = plan();
        assert!(
            validate_scene_plan(&p, 2, PlateMode::Perspective)
                .contains(&PlanViolation::NoProductCut)
        );
        assert!(
            !validate_scene_plan(&p, 0, PlateMode::Perspective)
                .contains(&PlanViolation::NoProductCut),
            "スナップショット 0 枚なら要求しない"
        );
        p.scenes[0] = product(
            1,
            Some(5),
            "A warm wooden desk surface, soft morning light, empty center.",
        );
        let v = validate_scene_plan(&p, 2, PlateMode::Perspective);
        assert!(v.contains(&PlanViolation::SnapshotIndexInvalid {
            scene_id: 1,
            got: Some(5),
            count: 2
        }));
        assert!(!v.contains(&PlanViolation::NoProductCut));
        p.scenes[0] = product(1, None, "A desk.");
        assert!(validate_scene_plan(&p, 2, PlateMode::Perspective).contains(
            &PlanViolation::SnapshotIndexInvalid {
                scene_id: 1,
                got: None,
                count: 2
            }
        ));
        p.scenes[0] = product(1, Some(1), "A laptop on a desk showing the dashboard UI.");
        let v = validate_scene_plan(&p, 2, PlateMode::Perspective);
        assert!(
            v.iter().any(|x| matches!(
                x,
                PlanViolation::ProductBackdropDrawsScreen { scene_id: 1, .. }
            )),
            "{v:?}"
        );
        p.scenes[0] = product(1, Some(1), "A modern building lobby at dusk, empty center.");
        assert!(
            validate_scene_plan(&p, 2, PlateMode::Perspective).is_empty(),
            "building の ui は誤検知しない"
        );
    }

    // --- rev5: 面の傾き (Phase E で背景と面のパースが噛み合わなかった) ---

    #[test]
    fn plate_tilt_out_of_range_is_a_violation_in_both_modes() {
        let mut p = plan();
        p.scenes[0] = product(
            1,
            Some(0),
            "A warm wooden desk surface, soft morning light, empty center.",
        );
        p.scenes[0].plate_tilt = Some(PlateTilt {
            yaw_degrees: 18.0,
            pitch_degrees: -12.0,
        });
        assert!(
            validate_scene_plan(&p, 1, PlateMode::Perspective).is_empty(),
            "範囲内は通る"
        );
        p.scenes[0].plate_tilt = Some(PlateTilt {
            yaw_degrees: 41.0,
            pitch_degrees: 0.0,
        });
        for mode in [PlateMode::Perspective, PlateMode::Frontal] {
            let v = validate_scene_plan(&p, 1, mode);
            assert!(
                v.iter()
                    .any(|x| matches!(x, PlanViolation::PlateTiltOutOfRange { scene_id: 1, .. })),
                "{mode:?}: {v:?}"
            );
        }
    }

    #[test]
    fn frontal_mode_rejects_an_angled_backdrop_but_perspective_allows_it() {
        let mut p = plan();
        p.scenes[0] = product(
            1,
            Some(0),
            "A softly lit dark desk surface photographed at a low angle, empty center.",
        );
        let v = validate_scene_plan(&p, 1, PlateMode::Frontal);
        assert!(
            v.iter().any(|x| matches!(
                x,
                PlanViolation::ProductBackdropAngled {
                    scene_id: 1,
                    word: "low angle"
                }
            )),
            "正面固定では背景のアングル指定を弾く: {v:?}"
        );
        assert!(
            validate_scene_plan(&p, 1, PlateMode::Perspective).is_empty(),
            "射影変換モードでは許す (面を傾けて合わせるため)"
        );
    }

    #[test]
    fn motion_prompt_is_required_and_english() {
        let mut p = plan();
        p.scenes[1].motion_prompt = "".into();
        p.scenes[2].motion_prompt = "ゆっくり寄る".into();
        let v = validate_scene_plan(&p, 0, PlateMode::Perspective);
        assert!(v.contains(&PlanViolation::MotionPromptEmpty { scene_id: 2 }));
        assert!(v.contains(&PlanViolation::MotionPromptNotEnglish { scene_id: 3 }));
    }

    #[test]
    fn violations_are_collected_not_short_circuited() {
        let mut p = plan();
        p.total_seconds = 20;
        p.scenes[1].scene_id = 7;
        p.scenes[2].duration_seconds = 30;
        p.scenes[0].video_prompt = "夕日の中のノートパソコン、ゆっくり寄る --ar 16:9".into();
        p.scenes[0].image_prompt = "Same as the Screenshot, but at dusk".into();
        let v = validate_scene_plan(&p, 0, PlateMode::Perspective);
        assert!(v.contains(&PlanViolation::TotalSeconds { got: 20 }));
        assert!(v.contains(&PlanViolation::SceneIdNotSequential { index: 1, got: 7 }));
        assert!(v.contains(&PlanViolation::DurationOutOfRange {
            scene_id: 3,
            got: 30
        }));
        assert!(v.contains(&PlanViolation::VideoPromptNotEnglish { scene_id: 1 }));
        assert!(v.contains(&PlanViolation::VideoPromptHasAspectFlag { scene_id: 1 }));
        assert!(v.contains(&PlanViolation::ImagePromptMentionsInput {
            scene_id: 1,
            word: "screenshot"
        }));
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn scene_count_bounds() {
        let mut p = plan();
        p.scenes.truncate(2);
        assert!(
            validate_scene_plan(&p, 0, PlateMode::Perspective)
                .contains(&PlanViolation::SceneCount { got: 2 })
        );
        let mut p = plan();
        p.scenes = (1..=9).map(scene).collect();
        assert!(
            validate_scene_plan(&p, 0, PlateMode::Perspective)
                .contains(&PlanViolation::SceneCount { got: 9 })
        );
    }

    /// rev3: MiniMax (i2v) は動きの短文だけ。motion が空なら video_prompt に倒す。`--ar` はどこにも出ない。
    #[test]
    fn clipboard_is_minimax_first_and_never_uses_ar_flag() {
        let s = scene(1);
        assert_eq!(
            clipboard_text(&s, Aspect::Portrait, CopyTarget::Minimax),
            "Slow push-in with a soft light flicker."
        );
        let mut no_motion = scene(1);
        no_motion.motion_prompt = String::new();
        assert_eq!(
            clipboard_text(&no_motion, Aspect::Portrait, CopyTarget::Minimax),
            no_motion.video_prompt
        );
        let generic = clipboard_text(&s, Aspect::Portrait, CopyTarget::Generic);
        assert!(generic.starts_with("Cinematic close-up"));
        assert!(
            generic.contains("9:16") && generic.contains("5s"),
            "{generic}"
        );
        for t in [CopyTarget::Minimax, CopyTarget::Generic] {
            assert!(!clipboard_text(&s, Aspect::Portrait, t).contains("--ar"));
        }
    }

    #[test]
    fn plan_roundtrips_through_json_with_aspect_strings() {
        let text = serde_json::to_string(&plan()).unwrap();
        assert!(text.contains("\"aspect\":\"16:9\""));
        let back: ScenePlan = serde_json::from_str(&text).unwrap();
        assert_eq!(back, plan());
    }
}

#[cfg(test)]
mod violation_kind_tests {
    use super::*;
    use std::collections::BTreeSet;

    /// 全 variant の見本 (新しい variant を足したらここにも足す — 網羅の番人)。
    fn every_variant() -> Vec<PlanViolation> {
        vec![
            PlanViolation::SceneCount { got: 2 },
            PlanViolation::TotalSeconds { got: 20 },
            PlanViolation::SceneIdNotSequential { index: 1, got: 5 },
            PlanViolation::DurationOutOfRange { scene_id: 1, got: 99 },
            PlanViolation::EmptyField { scene_id: 1, field: "copy_text" },
            PlanViolation::VideoPromptNotEnglish { scene_id: 1 },
            PlanViolation::VideoPromptHasAspectFlag { scene_id: 1 },
            PlanViolation::ImagePromptMentionsInput { scene_id: 1, word: "screenshot" },
            PlanViolation::NoProductCut,
            PlanViolation::SnapshotIndexInvalid { scene_id: 1, got: None, count: 2 },
            PlanViolation::ProductBackdropDrawsScreen { scene_id: 1, word: "screen" },
            PlanViolation::MotionPromptEmpty { scene_id: 1 },
            PlanViolation::MotionPromptNotEnglish { scene_id: 1 },
            PlanViolation::PlateTiltOutOfRange { scene_id: 1, yaw: 40.0, pitch: 0.0 },
            PlanViolation::ProductBackdropAngled { scene_id: 1, word: "three-quarter" },
        ]
    }

    #[test]
    fn kind_is_snake_case_and_unique_per_variant() {
        let kinds: Vec<&str> = every_variant().iter().map(violation_kind).collect();
        let uniq: BTreeSet<&&str> = kinds.iter().collect();
        assert_eq!(uniq.len(), kinds.len(), "種別は variant ごとに別の文字列でなければ数えられない");
        for k in &kinds {
            assert!(
                k.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "契約の表記は snake_case: {k}"
            );
        }
        assert_eq!(violation_kind(&PlanViolation::ProductBackdropAngled { scene_id: 1, word: "x" }), "product_backdrop_angled");
        assert_eq!(violation_kind(&PlanViolation::NoProductCut), "no_product_cut");
    }

    /// 種別は**中身に依らない**。scene_id や語が混ざると集計できなくなる
    /// (`describe_violation` は人が読む文なので混ざる — 用途が違う)。
    #[test]
    fn kind_ignores_the_payload_but_the_prose_does_not() {
        let a = PlanViolation::ProductBackdropAngled { scene_id: 1, word: "three-quarter" };
        let b = PlanViolation::ProductBackdropAngled { scene_id: 4, word: "low angle" };
        assert_eq!(violation_kind(&a), violation_kind(&b), "同じ種別は同じキーに畳む");
        assert_ne!(
            crate::describe_violation(&a),
            crate::describe_violation(&b),
            "人が読む文は畳まない (この差が集計を壊すので種別を別に持つ)"
        );
    }
}

#[cfg(test)]
mod plate_mode_default_tests {
    use super::*;

    /// rev22: 既定は **frontal** (正面固定)。MiniMax の image-to-video に等倍で通した実測で、
    /// 面を傾けると**動きが過剰になった** (ユーザー観測 2026-09-09、開いている判断 1 の決着)。
    /// 傾ける経路は残す — 設定と `--plate perspective` で選べる。
    #[test]
    fn the_default_is_frontal() {
        assert_eq!(PlateMode::default(), PlateMode::Frontal);
        // 契約の表記は変えない (既存の run の promo.json / runs.json が読めなくなる)。
        assert_eq!(serde_json::to_value(PlateMode::Frontal).unwrap(), "frontal");
        assert_eq!(serde_json::to_value(PlateMode::Perspective).unwrap(), "perspective");
    }

    /// **既存の run は影響を受けない** — rev10 で promo.json 自身が plate_mode を持つ。
    /// 既定に頼っていないので、焼き直しは作られた時のモードのまま動く。
    #[test]
    fn an_existing_run_keeps_the_mode_it_was_made_with() {
        let v = serde_json::json!("perspective");
        let mode: PlateMode = serde_json::from_value(v).unwrap();
        assert_eq!(mode, PlateMode::Perspective, "既定が変わっても保存済みの値が勝つ");
    }
}
