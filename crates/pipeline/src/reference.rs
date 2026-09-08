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
    compose_reference_prompt, composite_product_cut, select_refs, solid_backdrop,
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
}

fn default_size_ratio() -> f32 {
    0.055
}
use promo_core::export::reference_image_name;
use promo_core::plan::{CutKind, ScenePlan};
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
}

/// シーンごとに参照画像を作り、`job.out_dir` に保存して `plan.scenes[i].reference_image` を埋める。
pub async fn generate_references(
    generator: &dyn ImageGenerator,
    job: &RefJob<'_>,
    plan: &mut ScenePlan,
    refs: &[RefImage],
    progress: &mut (dyn FnMut(String) + Send),
) -> Vec<SceneImage> {
    let RefJob { cfg, api_key, out_dir, palette, caption, max_scenes, seed_base, requested_refs } = *job;
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
    let (cw, ch) = SizeMap::comfy_dims(cfg.shape);
    let layout = match caption {
        Some(c) => Layout::for_canvas(cw, ch).with_caption_band(c.position == CaptionPosition::Top),
        None => Layout::for_canvas(cw, ch),
    };
    let n = max_scenes.unwrap_or(plan.scenes.len()).min(plan.scenes.len());
    let mut out = Vec::new();
    for (i, scene) in plan.scenes.iter_mut().take(n).enumerate() {
        let seed = if cfg.lock_seed { cfg.seed } else { seed_base.wrapping_add(i as u64) };
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
                        composite_product_cut(&backdrop, &shot.bytes, &layout).map_err(ImageGenError::Config)
                    }
                }
            }
            CutKind::Mood => {
                let prompt = compose_reference_prompt(&cfg.user_prefix, &scene.image_prompt, !sent.is_empty());
                progress(format!("scene {}: 生成中 ({})", scene.scene_id, prompt.chars().take(60).collect::<String>()));
                generator.generate(cfg, api_key, &prompt, seed, &sent, progress).await.map(|g| g.bytes)
            }
        };
        // 見出し (opt-in): 生成・合成の後に copy_text を焼く。失敗は焼かずに続行 (画像は残す)。
        let bytes = match (&bytes, &font_data) {
            (Ok(png), Some((font, spec))) if !scene.copy_text.trim().is_empty() => {
                let mut cap = Caption::new(&scene.copy_text, font, spec.font_index);
                cap.size_ratio = spec.size_ratio;
                cap.position = spec.position;
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
                    Ok(Generated { mime: "image/png".into(), bytes: format!("png{n}").into_bytes() })
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
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &[], caption: None, max_scenes: None, seed_base: 7, requested_refs: None };
        let out = generate_references(&fake, &job, &mut p, &refs(2), &mut |s| log.push(s)).await;
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|s| s.result.is_ok()));
        assert_eq!(p.scenes[0].reference_image.as_deref(), Some("scene_01_ref_01.png"));
        assert_eq!(fs::read(dir.join("scene_02_ref_01.png")).unwrap(), b"png2");
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
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &[], caption: None, max_scenes: Some(1), seed_base: 0, requested_refs: None };
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
        let job = RefJob { cfg: &c, api_key: "k", out_dir: &dir, palette: &palette, caption: None, max_scenes: None, seed_base: 0, requested_refs: None };
        let out = generate_references(&Bg, &job, &mut p, &[shot], &mut |_| {}).await;
        assert!(out.iter().all(|r| r.result.is_ok()), "{:?}", out.iter().map(|r| r.result.as_ref().err().map(|e| e.to_string())).collect::<Vec<_>>());
        for (i, expected_bg) in [(1u32, [40u8, 40, 40]), (2, [0x11, 0x22, 0x33])] {
            let img = image::open(dir.join(format!("scene_0{i}_ref_01.png"))).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (1344, 768));
            let c = img.get_pixel(672, 384);
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
        let job = RefJob { cfg: &c, api_key: "", out_dir: &dir, palette: &[], caption: None, max_scenes: None, seed_base: 0, requested_refs: None };
        let out = generate_references(&fake, &job, &mut p, &[], &mut |_| {}).await;
        assert!(out[0].result.is_ok());
        assert!(matches!(out[1].result, Err(ImageGenError::RateLimited { .. })));
        assert!(out[2].result.is_ok());
        assert!(p.scenes[1].reference_image.is_none() && p.scenes[2].reference_image.is_some());
        // 参照なしなら 1 場面規律は付かない。
        assert!(!fake.calls.lock().unwrap()[0].0.contains("single continuous scene"));
    }
}
