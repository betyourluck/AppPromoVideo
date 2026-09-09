//! 参照画像の生成と保存 (契約 `ExportPackage.layout` の `scene_NN_ref_MM.png`)。
//!
//! 1 シーン = 1 枚 (v1)。参照 = スナップショットのバイト列 (契約 `reference_limits` で切り詰め)。
//! プロンプト = style anchor (ImageGenConfig.user_prefix) + scene.image_prompt (+ 参照ありなら 1 場面規律)。
//! 失敗は**シーン単位**で返し、他のシーンは続ける (1 枚の 429 で全体を止めない)。

use std::fs;
use std::path::{Path, PathBuf};

use image_gen::provider::SizeMap;
use image_gen::{
    Caption, CaptionPosition, ImageGenConfig, ImageGenError, ImageGenerator, Layout, RefImage, burn_caption,
    CANVAS_MAX_LONG_EDGE, Tilt, canvas_for_snapshot, compose_reference_prompt, composite_product_cut, fit_to_canvas,
    plate_scale, select_refs, solid_backdrop,
};
use serde::{Deserialize, Serialize};

/// 見出し (copy_text) の焼き込み指定 (契約 `caption`、opt-in・既定 OFF。アプリで焼くか手で焼くかはユーザー判断待ち)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptionSpec {
    pub font_path: String,
    #[serde(default)]
    pub font_index: u32,
    /// 文字の高さ / canvas の高さ。既定 0.055。
    #[serde(default = "default_size_ratio")]
    pub size_ratio: f32,
    #[serde(default)]
    pub position: CaptionPosition,
    /// 文字色 `#RRGGBB` (rev9)。読めない / 省略で白。
    #[serde(default)]
    pub color: Option<String>,
}

impl CaptionSpec {
    /// scene ごとの上書きを重ねた spec を返す (純粋、契約 `caption.per_scene.resolution`)。
    /// **省略されたフィールドは自分の値のまま** — 「位置だけ変えて他は既定」が成り立つ。
    pub fn with_override(&self, o: &CaptionOverride) -> CaptionSpec {
        CaptionSpec {
            font_path: o.font_path.clone().unwrap_or_else(|| self.font_path.clone()),
            font_index: o.font_index.unwrap_or(self.font_index),
            size_ratio: o.size_ratio.unwrap_or(self.size_ratio),
            position: match o.position.as_deref() {
                Some("top") => CaptionPosition::Top,
                Some("bottom") => CaptionPosition::Bottom,
                _ => self.position,
            },
            color: o.color.clone().or_else(|| self.color.clone()),
        }
    }

    /// 焼くときの色 (読めない指定は白に落ちる)。
    pub fn rgba(&self) -> [u8; 4] {
        self.color.as_deref().and_then(promo_core::export::parse_hex_rgba).unwrap_or([255, 255, 255, 255])
    }
}

fn default_size_ratio() -> f32 {
    0.055
}
use promo_core::export::{CaptionOverride, reference_image_name};
use promo_core::plan::{CutKind, PlateMode, ScenePlan};
use promo_core::style::extract_hex;

/// product カットの背景プロンプトに足す規律 (rev3): 画面・端末・文字を描かせず中央を空ける。
pub const BACKDROP_CLAUSE: &str =
    "Backdrop only: no devices, no screens, no monitors, no phones, no UI, no text. Keep the center of the frame clear and uncluttered.";

/// fallback の背景色 (palette の先頭。無ければ濃い灰)。
fn fallback_rgb(prefix_or_palette: &[String]) -> [u8; 3] {
    prefix_or_palette
        .iter()
        .filter_map(|p| extract_hex(p))
        .next()
        .and_then(|h| u32::from_str_radix(&h[1..], 16).ok())
        .map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .unwrap_or([28, 25, 38])
}

/// 1 シーンの結果。
#[derive(Debug)]
pub struct SceneImage {
    pub scene_id: u32,
    pub result: Result<PathBuf, ImageGenError>,
    /// 参照を切り詰めた時の報告 (UI に出す)。
    pub truncated: Option<image_gen::Truncated>,
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

/// スナップショットを参照バイト列に読む。読めないものは Err (契約: バリデーション)。
pub fn load_refs(snapshots: &[PathBuf]) -> Result<Vec<RefImage>, String> {
    snapshots
        .iter()
        .map(|p| {
            let bytes = fs::read(p).map_err(|e| format!("スナップショットを読めません {}: {e}", p.display()))?;
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "snapshot.png".into());
            Ok(RefImage { name, mime: mime_for(p).into(), bytes })
        })
        .collect()
}

/// 1 回の参照画像ジョブの設定 (シーンをまたいで不変のもの)。
pub struct RefJob<'a> {
    pub cfg: &'a ImageGenConfig,
    pub api_key: &'a str,
    pub out_dir: &'a Path,
    /// product カットの fallback 背景色に使う palette (hex 列)。
    pub palette: &'a [String],
    /// 見出しの焼き込み (None = 焼かない)。
    pub caption: Option<&'a CaptionSpec>,
    /// 先頭から何シーン分作るか (None なら全部)。
    pub max_scenes: Option<usize>,
    /// ComfyUI の seed の起点 (シーンごとに +1)。lock_seed なら cfg.seed。
    pub seed_base: u64,
    /// 送る参照枚数の希望 (None なら既定)。
    pub requested_refs: Option<usize>,
    /// 面の貼り方 (rev5)。Frontal なら scene.plate_tilt を無視して正対で貼る。
    pub plate_mode: PlateMode,
    /// scene ごとの見出しの上書き (rev9)。無ければ `caption` がそのまま既定として効く。
    pub caption_overrides: &'a std::collections::BTreeMap<u32, CaptionOverride>,
}

/// 画像の寸法をヘッダだけで読む (デコードしない)。焼き直しで canvas を素材から取るのに使う。
pub fn image_dims(bytes: &[u8]) -> Result<(u32, u32), String> {
    imagesize::blob_size(bytes).map(|d| (d.width as u32, d.height as u32)).map_err(|e| format!("寸法を読めません: {e}"))
}

/// 素材を base/ に残す。失敗しても本流は止めない (画像は出す) が、焼き直せなくなるので理由は出す。
fn save_base(out_dir: &Path, name: &str, bytes: &[u8], scene_id: u32, progress: &mut (dyn FnMut(String) + Send)) {
    let bp = base_image_path(out_dir, name);
    let saved = fs::create_dir_all(bp.parent().unwrap_or(out_dir)).and_then(|_| fs::write(&bp, bytes));
    if let Err(e) = saved {
        progress(format!("scene {scene_id}: 素材を残せません ({e}) → あとから見出しだけ変えられません"));
    }
}

/// perspective の時だけ scene の傾きを効かせる (純粋)。
pub fn tilt_of(mode: PlateMode, scene: &promo_core::plan::Scene) -> Option<Tilt> {
    match (mode, scene.plate_tilt) {
        (PlateMode::Perspective, Some(t)) => Some(Tilt { yaw_degrees: t.yaw_degrees, pitch_degrees: t.pitch_degrees }),
        _ => None,
    }
}

/// その scene に効く見出し指定 (純粋)。既定に上書きを重ねる。既定が無ければ焼かない。
fn spec_for_scene(
    base: Option<&CaptionSpec>,
    overrides: &std::collections::BTreeMap<u32, CaptionOverride>,
    scene_id: u32,
) -> Option<CaptionSpec> {
    let base = base?;
    Some(match overrides.get(&scene_id) {
        Some(o) => base.with_override(o),
        None => base.clone(),
    })
}

/// 合成の配置を決める**唯一の場所** (純粋、契約 `caption.per_scene.layout_for`、rev10)。
///
/// 帯 (`with_caption_band`) は**そのカットで実際に効いている見出しの位置**から決める。
/// rev9 はここをジョブ既定の位置で合成時に焼き込んでいたので、あとから位置を下→上に変えると
/// 上に空きが無いところへ焼くことになりプレートに重なった。生成と焼き直しでこの関数を共有する。
pub fn layout_for(canvas: (u32, u32), caption: Option<CaptionPosition>, tilt: Option<Tilt>) -> Layout {
    let base = match caption {
        Some(pos) => Layout::for_canvas(canvas.0, canvas.1).with_caption_band(pos == CaptionPosition::Top),
        None => Layout::for_canvas(canvas.0, canvas.1),
    };
    match tilt {
        Some(t) => base.with_tilt(t),
        None => base,
    }
}

/// 見出しを焼く**前**の合成の置き場 (契約 `ExportPackage.layout`、rev9)。
/// `<run>/base/<同じ名前>`。ここから何度でも焼き直せるので、位置や色を変えるのに背景を作り直さずに済む。
pub fn base_image_path(run_dir: &Path, name: &str) -> PathBuf {
    run_dir.join("base").join(name)
}

/// シーンごとに参照画像を作り、`job.out_dir` に保存して `plan.scenes[i].reference_image` を埋める。
pub async fn generate_references(
    generator: &dyn ImageGenerator,
    job: &RefJob<'_>,
    plan: &mut ScenePlan,
    refs: &[RefImage],
    progress: &mut (dyn FnMut(String) + Send),
) -> Vec<SceneImage> {
    let RefJob {
        cfg,
        api_key,
        out_dir,
        palette,
        caption,
        max_scenes,
        seed_base,
        requested_refs,
        plate_mode,
        caption_overrides: overrides,
    } = *job;
    // フォントは 1 回だけ読む。読めなければ焼かずに進む (進捗に理由を出す)。
    let font_data: Option<(Vec<u8>, &CaptionSpec)> = match caption {
        Some(c) => match fs::read(&c.font_path) {
            Ok(b) => Some((b, c)),
            Err(e) => {
                progress(format!("見出しのフォントを読めません ({}: {e}) → 焼き込みなしで続行", c.font_path));
                None
            }
        },
        None => None,
    };
    let (sent, truncated) = select_refs(cfg.provider, refs, requested_refs);
    if let Some(t) = &truncated {
        progress(format!("参照 {} 枚のうち先頭 {} 枚を送ります ({})", t.total, t.sent, t.limit_by));
    }
    // rev6: canvas は「スクショを縮めない」大きさにする。縮めると i2v が読めない文字を作り変える
    // (ユーザー実測 2026-09-08、MiniMax)。背景は柔らかいので拡大が効く。パッケージ内は全カット同寸。
    let base = SizeMap::comfy_dims(cfg.shape);
    let ratio = if caption.is_some() { 0.68 } else { 0.78 };
    let biggest = refs.iter().filter_map(|r| imagesize::blob_size(&r.bytes).ok()).fold((0u32, 0u32), |a, d| {
        (a.0.max(d.width as u32), a.1.max(d.height as u32))
    });
    let (cw, ch) = if biggest.0 > 0 { canvas_for_snapshot(base, biggest, ratio, CANVAS_MAX_LONG_EDGE) } else { base };
    if (cw, ch) != base {
        progress(format!("canvas を {}x{} → {cw}x{ch} に拡げます (スクショ {}x{} を等倍で置くため)", base.0, base.1, biggest.0, biggest.1));
    }
    if biggest.0 > 0 {
        let sc = plate_scale((cw, ch), biggest, ratio);
        if sc < 0.999 {
            progress(format!(
                "警告: スクショを {:.0}% に縮めます (canvas 上限 {CANVAS_MAX_LONG_EDGE}px)。小さい文字は動画化で作り変えられます",
                sc * 100.0
            ));
        }
    }
    let n = max_scenes.unwrap_or(plan.scenes.len()).min(plan.scenes.len());
    let mut out = Vec::new();
    for (i, scene) in plan.scenes.iter_mut().take(n).enumerate() {
        let seed = if cfg.lock_seed { cfg.seed } else { seed_base.wrapping_add(i as u64) };
        // 帯の有無と向きは、そのカットで効く見出し指定から決める (rev10)。
        let spec = spec_for_scene(caption, overrides, scene.scene_id);
        let bytes: Result<Vec<u8>, ImageGenError> = match scene.cut_kind {
            // product: 背景だけモデルに描かせ (参照は送らない = 画面を発明させない)、実スクショを Rust が貼る。
            CutKind::Product => {
                let idx = scene.snapshot_index.map(|i| i as usize).unwrap_or(0);
                match refs.get(idx) {
                    None => Err(ImageGenError::Config(format!(
                        "scene {}: snapshot_index {idx} に対応するスナップショットが無い ({} 枚)",
                        scene.scene_id,
                        refs.len()
                    ))),
                    Some(shot) => {
                        let prompt = format!("{} {BACKDROP_CLAUSE}", compose_reference_prompt(&cfg.user_prefix, &scene.image_prompt, false));
                        progress(format!("scene {}: 背景を生成 → スクショ {idx} を合成", scene.scene_id));
                        let backdrop = match generator.generate(cfg, api_key, &prompt, seed, &[], progress).await {
                            Ok(g) => g.bytes,
                            Err(e) => {
                                progress(format!("scene {}: 背景の生成に失敗 ({e}) → 単色背景で合成", scene.scene_id));
                                solid_backdrop(cw, ch, fallback_rgb(palette))
                            }
                        };
                        // rev10: 背景を canvas 同寸で base/ に残す。スクショは snapshots/ にあるので、
                        // あとから**合成からやり直せる** (帯の取り直し・傾きの変更が無料でできる)。
                        let backdrop = fit_to_canvas(&backdrop, cw, ch).unwrap_or(backdrop);
                        save_base(out_dir, &reference_image_name(scene.scene_id, 1), &backdrop, scene.scene_id, progress);
                        let l = layout_for((cw, ch), spec.as_ref().map(|s| s.position), tilt_of(plate_mode, scene));
                        composite_product_cut(&backdrop, &shot.bytes, &l).map_err(ImageGenError::Config)
                    }
                }
            }
            CutKind::Mood => {
                let prompt = compose_reference_prompt(&cfg.user_prefix, &scene.image_prompt, !sent.is_empty());
                progress(format!("scene {}: 生成中 ({})", scene.scene_id, prompt.chars().take(60).collect::<String>()));
                // rev6: プロバイダの出力は寸法も形式もまちまち (Gemini は JPEG 1376x768) なので canvas に揃える。
                generator.generate(cfg, api_key, &prompt, seed, &sent, progress).await.map(|g| {
                    // 揃えられない時は素のまま通す (生成には金がかかっている)。ただし黙らない。
                    let b = match fit_to_canvas(&g.bytes, cw, ch) {
                        Ok(b) => b,
                        Err(e) => {
                            progress(format!("scene {}: 生成画像を canvas に揃えられません ({e}) → そのまま保存", scene.scene_id));
                            g.bytes
                        }
                    };
                    // mood は合成が無いので、絵そのものが素材 (rev10)。
                    save_base(out_dir, &reference_image_name(scene.scene_id, 1), &b, scene.scene_id, progress);
                    b
                })
            }
        };
        // 見出し (opt-in): 生成・合成の後に copy_text を焼く。失敗は焼かずに続行 (画像は残す)。
        let bytes = match (&bytes, &font_data) {
            (Ok(png), Some((font, _))) if !scene.copy_text.trim().is_empty() => {
                let spec = spec.as_ref().expect("font_data があるなら spec もある");
                let mut cap = Caption::new(&scene.copy_text, font, spec.font_index);
                cap.size_ratio = spec.size_ratio;
                cap.position = spec.position;
                cap.color = spec.rgba();
                match burn_caption(png, &cap) {
                    Ok(b) => Ok(b),
                    Err(e) => {
                        progress(format!("scene {}: 見出しを焼けません ({e}) → 画像はそのまま", scene.scene_id));
                        bytes
                    }
                }
            }
            _ => bytes,
        };
        let result = match bytes {
            Ok(bytes) => {
                let name = reference_image_name(scene.scene_id, 1);
                let path = out_dir.join(&name);
                match fs::write(&path, &bytes) {
                    Ok(()) => {
                        scene.reference_image = Some(name);
                        Ok(path)
                    }
                    Err(e) => Err(ImageGenError::Config(format!("保存できません {}: {e}", path.display()))),
                }
            }
            Err(e) => Err(e),
        };
        match &result {
            Ok(p) => progress(format!("scene {}: 保存 {}", scene.scene_id, p.display())),
            Err(e) => progress(format!("scene {}: 失敗 {e}", scene.scene_id)),
        }
        out.push(SceneImage { scene_id: scene.scene_id, result, truncated: truncated.clone() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_gen::provider::{Detail, Generated, PromptStyle, Provider, Shape};
    use promo_core::plan::{Aspect, Scene};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    /// 呼ばれた prompt / 参照枚数を記録し、n 回目に失敗する fake (Sync のため Mutex)。
    struct Fake {
        calls: Mutex<Vec<(String, usize)>>,
        fail_on: Option<usize>,
    }

    impl ImageGenerator for Fake {
        fn generate<'a>(
            &'a self,
            _cfg: &'a ImageGenConfig,
            _api_key: &'a str,
            prompt: &'a str,
            _seed: u64,
            refs: &'a [RefImage],
            _progress: &'a mut (dyn FnMut(String) + Send),
        ) -> Pin<Box<dyn Future<Output = Result<Generated, ImageGenError>> + Send + 'a>> {
            self.calls.lock().unwrap().push((prompt.to_string(), refs.len()));
            let n = self.calls.lock().unwrap().len();
            Box::pin(async move {
                if Some(n) == self.fail_on {
                    Err(ImageGenError::RateLimited { detail: "quota".into() })
                } else {
                    // 実物の PNG を返す (rev6 の canvas 正規化を経路ごとテストするため)。
                    Ok(Generated { mime: "image/png".into(), bytes: image_gen::solid_backdrop(64, 48, [n as u8, 20, 30]) })
                }
            })
        }

        fn probe<'a>(
            &'a self,
            _cfg: &'a ImageGenConfig,
            _api_key: &'a str,
        ) -> Pin<Box<dyn Future<Output = Result<String, ImageGenError>> + Send + 'a>> {
            Box::pin(async { Ok("ok".into()) })
        }
    }

    fn cfg(provider: Provider) -> ImageGenConfig {
        ImageGenConfig {
            provider,
            base_url: "http://x".into(),
            model: String::new(),
            shape: Shape::Landscape,
            detail: Detail::Standard,
            style: Some(PromptStyle::Prose),
            user_prefix: "Color palette: #111111. Mood: calm".into(),
            negative: String::new(),
            workflow_json: None,
            timeout_secs: None,
            lock_seed: false,
            seed: 0,
        }
    }

    fn plan() -> ScenePlan {
        ScenePlan {
            total_seconds: 15,
            aspect: Aspect::Landscape,
            scenes: (1..=3)
                .map(|i| Scene {
                    scene_id: i,
                    cut_kind: CutKind::Mood,
                    snapshot_index: None,
                    plate_tilt: None,
                    motion_prompt: "Slow push-in.".into(),
                    duration_seconds: 5,
                    shot_type: "Wide".into(),
                    video_prompt: "v".into(),
                    copy_text: "c".into(),
                    image_prompt: format!("A desk number {i}."),
                    reference_image: None,
                })
                .collect(),
        }
    }

    fn refs(n: usize) -> Vec<RefImage> {
        (0..n).map(|i| RefImage { name: format!("s{i}.png"), mime: "image/png".into(), bytes: vec![1] }).collect()
    }

    fn out_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("pipeline_ref_{}_{}", std::process::id(), rand()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[tokio::test]
    async fn saves_named_files_fills_plan_and_composes_prompt_with_anchor_and_refs() {
        let fake = Fake { calls: Mutex::new(vec![]), fail_on: None };
        let mut p = plan();
        let dir = out_dir();
        let mut log = Vec::new();
        let c = cfg(Provider::Gemini);
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &[], caption: None, max_scenes: None, seed_base: 7, requested_refs: None, plate_mode: PlateMode::default(), caption_overrides: &Default::default() };
        let out = generate_references(&fake, &job, &mut p, &refs(2), &mut |s| log.push(s)).await;
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|s| s.result.is_ok()));
        assert_eq!(p.scenes[0].reference_image.as_deref(), Some("scene_01_ref_01.png"));
        // rev6: mood もプロバイダの寸法・形式でなく canvas に揃う。
        let m = image::open(dir.join("scene_02_ref_01.png")).unwrap().to_rgba8();
        assert_eq!(m.dimensions(), (1344, 768), "スナップショットが無い run では base のまま");
        let calls = fake.calls.lock().unwrap();
        assert!(calls[0].0.starts_with("Color palette: #111111. Mood: calm. A desk number 1."));
        assert!(calls[0].0.ends_with(image_gen::SINGLE_FRAME_CLAUSE));
        assert_eq!(calls[0].1, 2, "gemini は 2 枚とも送る");
        assert!(out[0].truncated.is_none());
    }

    #[tokio::test]
    async fn openai_default_sends_one_ref_and_reports_truncation() {
        let fake = Fake { calls: Mutex::new(vec![]), fail_on: None };
        let mut p = plan();
        let mut log = Vec::new();
        let c = cfg(Provider::Openai);
        let dir = out_dir();
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &[], caption: None, max_scenes: Some(1), seed_base: 0, requested_refs: None, plate_mode: PlateMode::default(), caption_overrides: &Default::default() };
        let out = generate_references(&fake, &job, &mut p, &refs(3), &mut |s| log.push(s)).await;
        assert_eq!(out.len(), 1, "max_scenes で先頭 1 シーンだけ");
        assert_eq!(fake.calls.lock().unwrap()[0].1, 1);
        let t = out[0].truncated.as_ref().unwrap();
        assert_eq!((t.total, t.sent), (3, 1));
        assert!(log.iter().any(|l| l.contains("3 枚のうち先頭 1 枚")));
        assert!(p.scenes[1].reference_image.is_none());
    }

    /// rev3: product カットは参照を送らず背景だけ生成し、実スクショの画素を中央に貼る。
    /// 背景生成が失敗しても単色背景で合成する (製品カットは provider の都合で落ちない)。
    /// rev10: 帯は**効いている見出しの位置**から決める。rev9 は合成時のジョブ既定で焼き込んでいたので、
    /// あとから位置を下→上に変えるとプレートに重なった。
    #[test]
    fn the_caption_band_follows_the_position_that_is_actually_used() {
        let canvas = (1000, 1000);
        let none = layout_for(canvas, None, None);
        assert_eq!(none.screen_ratio, 0.78, "見出し無しなら帯を取らない");
        assert_eq!(none.y_offset_ratio, 0.0);

        let bottom = layout_for(canvas, Some(CaptionPosition::Bottom), None);
        assert_eq!(bottom.screen_ratio, 0.68);
        assert!(bottom.y_offset_ratio < 0.0, "下に帯 → プレートは上へ寄る");

        let top = layout_for(canvas, Some(CaptionPosition::Top), None);
        assert!(top.y_offset_ratio > 0.0, "上に帯 → プレートは下へ寄る");
        assert_ne!(top.y_offset_ratio, bottom.y_offset_ratio, "位置で向きが変わる");

        // 傾きはそのまま乗る (焼き直しで合成をやり直しても tilt を失わない)。
        let tilted = layout_for(canvas, Some(CaptionPosition::Top), Some(Tilt { yaw_degrees: 12.0, pitch_degrees: -8.0 }));
        assert!(tilted.tilt.is_some());
        assert_eq!(tilted.y_offset_ratio, top.y_offset_ratio);
    }

    /// rev9: 見出しを焼く**前**を base/ に残す。あとから位置や色を変えるのに背景を作り直さないため。
    #[test]
    fn base_path_sits_under_the_run_and_keeps_the_same_name() {
        let run = Path::new("D:/out/App_Promo_Package/runs/20260908-120000");
        let p = base_image_path(run, "scene_02_ref_01.png");
        assert!(p.ends_with("base/scene_02_ref_01.png"), "{}", p.display());
        assert_eq!(p.parent().unwrap().parent().unwrap(), run, "run 直下の base/");
    }

    /// 上書きは**フィールド単位で**既定に落ちる (位置だけ変えて他は既定、が成り立つ)。
    #[test]
    fn overrides_fall_back_field_by_field() {
        let base = CaptionSpec {
            font_path: "D:/f.ttc".into(),
            font_index: 1,
            size_ratio: 0.055,
            position: CaptionPosition::Bottom,
            color: Some("#FFFFFF".into()),
        };
        let only_pos = base.with_override(&CaptionOverride { position: Some("top".into()), ..Default::default() });
        assert_eq!(only_pos.position, CaptionPosition::Top);
        assert_eq!(only_pos.font_path, "D:/f.ttc", "触っていないものは既定のまま");
        assert_eq!(only_pos.size_ratio, 0.055);
        assert_eq!(only_pos.rgba(), [255, 255, 255, 255]);

        let colored = base.with_override(&CaptionOverride { color: Some("#FFCC00".into()), ..Default::default() });
        assert_eq!(colored.rgba(), [255, 204, 0, 255]);
        assert_eq!(colored.position, CaptionPosition::Bottom);

        // 読めない色は白に落ちる (焼けないより焼ける方がよい)。
        let bad = base.with_override(&CaptionOverride { color: Some("not-a-color".into()), ..Default::default() });
        assert_eq!(bad.rgba(), [255, 255, 255, 255]);
    }

    #[tokio::test]
    async fn product_cut_composites_real_screenshot_and_survives_backdrop_failure() {
        // fake は 1 回目 (scene 1 の背景) に有効な PNG、2 回目 (scene 2) で失敗。
        struct Bg;
        impl ImageGenerator for Bg {
            fn generate<'a>(
                &'a self,
                _cfg: &'a ImageGenConfig,
                _k: &'a str,
                prompt: &'a str,
                _seed: u64,
                refs: &'a [RefImage],
                _p: &'a mut (dyn FnMut(String) + Send),
            ) -> Pin<Box<dyn Future<Output = Result<Generated, ImageGenError>> + Send + 'a>> {
                assert!(refs.is_empty(), "product の背景に参照を送らない (画面を発明させない)");
                assert!(prompt.contains("no screens"), "背景規律が本文に入る: {prompt}");
                let first = prompt.contains("desk one");
                Box::pin(async move {
                    if first {
                        Ok(Generated { mime: "image/png".into(), bytes: image_gen::solid_backdrop(64, 36, [40, 40, 40]) })
                    } else {
                        Err(ImageGenError::RateLimited { detail: "quota".into() })
                    }
                })
            }
            fn probe<'a>(&'a self, _c: &'a ImageGenConfig, _k: &'a str) -> Pin<Box<dyn Future<Output = Result<String, ImageGenError>> + Send + 'a>> {
                Box::pin(async { Ok("ok".into()) })
            }
        }
        let mut p = plan();
        p.scenes.truncate(2);
        for (s, bd) in p.scenes.iter_mut().zip(["A wooden desk one, warm light.", "A stone floor two."]) {
            s.cut_kind = CutKind::Product;
            s.snapshot_index = Some(0);
            s.image_prompt = bd.into();
        }
        let shot = RefImage { name: "ui.png".into(), mime: "image/png".into(), bytes: image_gen::solid_backdrop(1920, 1080, [0, 200, 255]) };
        let c = cfg(Provider::Gemini);
        let dir = out_dir();
        let palette = vec!["#112233".to_string()];
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &palette, caption: None, max_scenes: None, seed_base: 0, requested_refs: None, plate_mode: PlateMode::default(), caption_overrides: &Default::default() };
        let out = generate_references(&Bg, &job, &mut p, &[shot], &mut |_| {}).await;
        assert!(out.iter().all(|r| r.result.is_ok()), "{:?}", out.iter().map(|r| r.result.as_ref().err().map(|e| e.to_string())).collect::<Vec<_>>());
        // rev6: 1920x1080 のスクショを 0.78 で等倍に置ける canvas まで拡がる (1344x768 では 0.711 倍に縮んでいた)。
        let (cw, ch) = image_gen::canvas_for_snapshot((1344, 768), (1920, 1080), 0.78, image_gen::CANVAS_MAX_LONG_EDGE);
        assert!(image_gen::plate_scale((cw, ch), (1920, 1080), 0.78) >= 0.999, "縮小しない canvas: {cw}x{ch}");
        for (i, expected_bg) in [(1u32, [40u8, 40, 40]), (2, [0x11, 0x22, 0x33])] {
            let img = image::open(dir.join(format!("scene_0{i}_ref_01.png"))).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (cw, ch));
            let c = img.get_pixel(cw / 2, ch / 2);
            assert_eq!([c[0], c[1], c[2]], [0, 200, 255], "中央は実スクショの画素");
            let e = img.get_pixel(10, 10);
            assert_eq!([e[0], e[1], e[2]], expected_bg, "scene {i} の背景 (2 は fallback の palette 色)");
        }
    }

    #[tokio::test]
    async fn one_failure_does_not_stop_the_others() {
        let fake = Fake { calls: Mutex::new(vec![]), fail_on: Some(2) };
        let mut p = plan();
        let c = cfg(Provider::Comfy);
        let dir = out_dir();
        let job = RefJob { cfg: &c, api_key: "", out_dir: &dir, palette: &[], caption: None, max_scenes: None, seed_base: 0, requested_refs: None, plate_mode: PlateMode::default(), caption_overrides: &Default::default() };
        let out = generate_references(&fake, &job, &mut p, &[], &mut |_| {}).await;
        assert!(out[0].result.is_ok());
        assert!(matches!(out[1].result, Err(ImageGenError::RateLimited { .. })));
        assert!(out[2].result.is_ok());
        assert!(p.scenes[1].reference_image.is_none() && p.scenes[2].reference_image.is_some());
        // 参照なしなら 1 場面規律は付かない。
        assert!(!fake.calls.lock().unwrap()[0].0.contains("single continuous scene"));
    }
}
