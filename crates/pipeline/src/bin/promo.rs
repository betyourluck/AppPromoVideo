//! live 実測用 CLI (spec 01 Phase B/C)。GUI (Phase D) と同じ結線を端末から回す。
//!
//! ```text
//! promo run <repo> --concept "..." [--out <dir>] [--snapshot <img>]... [--seconds 30] [--aspect 16:9]
//!           [--plate frontal|perspective] [--lang ja|en] [--cli claude|aider|custom] [--exe <path>] [--model <m>] [--max-turns 12] [--timeout 600]
//!           [--images openai|gemini|comfy] [--image-base-url <url>] [--image-model <m>] [--workflow <api.json>]
//!           [--image-scenes N] [--image-refs N] [--image-detail standard|high|highest]
//! promo brief <repo> [--snapshot <img>]...      # RepoBrief.render() を出すだけ (LLM ゼロ)
//! promo images <promo.json> --images <provider> [--snapshot <img>]... [--image-scenes N] ...   # 既存の plan に参照画像だけ
//! ```
//! 画像の API キーは環境変数 `IMAGE_API_KEY_OPENAI` / `IMAGE_API_KEY_GEMINI` (無ければ `OPENAI_API_KEY` /
//! `GEMINI_API_KEY`)。LLM のキーは持たない (CLI の認証に委ねる)。

use std::path::{Path, PathBuf};

use cli_runner::runner::CliEvent;
use cli_runner::{CliKind, CliSpec};
use image_gen::{Detail, HttpImageGenerator, ImageGenConfig, Provider, Shape};
use pipeline::collect::collect_brief;
use pipeline::export::{existing_run_ids, write_package, write_run_files};
use pipeline::reference::{CaptionSpec, RefJob, generate_references, load_refs};
use pipeline::task::CliTaskRunner;
use pipeline::{analyze, plan_scenes};
use promo_core::brief::compress;
use promo_core::export::PromoJson;
use promo_core::plan::{Aspect, PlateMode};
use promo_core::prompts::Language;
use promo_core::style::{apply_palette, style_anchor};

struct Args {
    cmd: String,
    target: PathBuf,
    concept: String,
    out: PathBuf,
    snapshots: Vec<PathBuf>,
    seconds: u32,
    aspect: Aspect,
    lang: Language,
    kind: CliKind,
    exe: String,
    model: Option<String>,
    max_turns: u32,
    timeout: u64,
    images: Option<Provider>,
    image_base_url: Option<String>,
    image_model: String,
    workflow: Option<PathBuf>,
    image_scenes: Option<usize>,
    image_refs: Option<usize>,
    image_detail: Detail,
    caption_font: Option<String>,
    caption_index: u32,
    caption_size: f32,
    caption_pos: image_gen::CaptionPosition,
    caption_color: Option<String>,
    plate_mode: PlateMode,
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("usage: promo run|brief|images <repo|promo.json> [options]")?;
    let target = PathBuf::from(it.next().ok_or("<repo> または <promo.json> が要ります")?);
    let mut a = Args {
        cmd,
        target,
        concept: String::new(),
        out: PathBuf::from("./promo_out"),
        snapshots: vec![],
        seconds: 30,
        aspect: Aspect::Landscape,
        lang: Language::Ja,
        kind: CliKind::Claude,
        exe: "claude".into(),
        model: None,
        max_turns: 12,
        timeout: 600,
        images: None,
        image_base_url: None,
        image_model: String::new(),
        workflow: None,
        image_scenes: None,
        image_refs: None,
        image_detail: Detail::Standard,
        caption_font: None,
        caption_index: 0,
        caption_size: 0.055,
        caption_pos: image_gen::CaptionPosition::Bottom,
        caption_color: None,
        plate_mode: PlateMode::default(),
    };
    while let Some(k) = it.next() {
        let mut val = || it.next().ok_or(format!("{k} に値が要ります"));
        match k.as_str() {
            "--concept" => a.concept = val()?,
            "--out" => a.out = PathBuf::from(val()?),
            "--snapshot" => a.snapshots.push(PathBuf::from(val()?)),
            "--seconds" => a.seconds = val()?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--aspect" => {
                a.aspect = match val()?.as_str() {
                    "16:9" => Aspect::Landscape,
                    "9:16" => Aspect::Portrait,
                    "1:1" => Aspect::Square,
                    o => return Err(format!("--aspect は 16:9|9:16|1:1 ({o})")),
                }
            }
            "--lang" => a.lang = if val()? == "en" { Language::En } else { Language::Ja },
            "--cli" => {
                a.kind = match val()?.as_str() {
                    "claude" => CliKind::Claude,
                    "aider" => CliKind::Aider,
                    "custom" => CliKind::Custom,
                    o => return Err(format!("--cli は claude|aider|custom ({o})")),
                }
            }
            "--exe" => a.exe = val()?,
            "--caption-color" => a.caption_color = Some(val()?),
            "--plate" => {
                a.plate_mode = match val()?.as_str() {
                    "perspective" => PlateMode::Perspective,
                    "frontal" => PlateMode::Frontal,
                    o => return Err(format!("--plate は perspective|frontal ({o})")),
                }
            }
            "--model" => a.model = Some(val()?),
            "--max-turns" => a.max_turns = val()?.parse().map_err(|e| format!("--max-turns: {e}"))?,
            "--timeout" => a.timeout = val()?.parse().map_err(|e| format!("--timeout: {e}"))?,
            "--images" => {
                a.images = Some(match val()?.as_str() {
                    "openai" => Provider::Openai,
                    "gemini" => Provider::Gemini,
                    "comfy" => Provider::Comfy,
                    o => return Err(format!("--images は openai|gemini|comfy ({o})")),
                })
            }
            "--image-base-url" => a.image_base_url = Some(val()?),
            "--image-model" => a.image_model = val()?,
            "--workflow" => a.workflow = Some(PathBuf::from(val()?)),
            "--image-scenes" => a.image_scenes = Some(val()?.parse().map_err(|e| format!("--image-scenes: {e}"))?),
            "--image-refs" => a.image_refs = Some(val()?.parse().map_err(|e| format!("--image-refs: {e}"))?),
            "--image-detail" => {
                a.image_detail = match val()?.as_str() {
                    "standard" => Detail::Standard,
                    "high" => Detail::High,
                    "highest" => Detail::Highest,
                    o => return Err(format!("--image-detail は standard|high|highest ({o})")),
                }
            }
            "--caption-font" => a.caption_font = Some(val()?),
            "--caption-index" => a.caption_index = val()?.parse().map_err(|e| format!("--caption-index: {e}"))?,
            "--caption-size" => a.caption_size = val()?.parse().map_err(|e| format!("--caption-size: {e}"))?,
            "--caption-pos" => {
                a.caption_pos = match val()?.as_str() {
                    "top" => image_gen::CaptionPosition::Top,
                    "bottom" => image_gen::CaptionPosition::Bottom,
                    o => return Err(format!("--caption-pos は top|bottom ({o})")),
                }
            }
            o => return Err(format!("unknown option {o}")),
        }
    }
    Ok(a)
}

/// `promo caption <in.png> <font> <index> <text> <out.png>` — 見出しの焼き込みだけ (LLM ゼロ、目視用)。
/// `promo fonts` — 列挙 (システム + ./fonts)。
fn caption_cmd(args: &[String]) -> Result<(), String> {
    let [input, font, index, text, out, ..] = args else { return Err("caption <in.png> <font> <index> <text> <out.png>".into()) };
    let font_data = std::fs::read(font).map_err(|e| format!("フォントを読めません {font}: {e}"))?;
    let idx: u32 = index.parse().map_err(|e| format!("index: {e}"))?;
    let png = std::fs::read(input).map_err(|e| e.to_string())?;
    let text = text.replace("\\n", "\n");
    let cap = image_gen::Caption::new(&text, &font_data, idx);
    std::fs::write(out, image_gen::burn_caption(&png, &cap)?).map_err(|e| e.to_string())?;
    println!("{out}");
    Ok(())
}

fn image_key(provider: Provider) -> String {
    let names: &[&str] = match provider {
        Provider::Openai => &["IMAGE_API_KEY_OPENAI", "OPENAI_API_KEY"],
        Provider::Gemini => &["IMAGE_API_KEY_GEMINI", "GEMINI_API_KEY"],
        Provider::Comfy => &[],
    };
    names.iter().find_map(|n| std::env::var(n).ok().filter(|v| !v.trim().is_empty())).unwrap_or_default()
}

fn shape_for(aspect: Aspect) -> Shape {
    match aspect {
        Aspect::Landscape => Shape::Landscape,
        Aspect::Portrait => Shape::Portrait,
        Aspect::Square => Shape::Square,
    }
}

fn image_cfg(a: &Args, provider: Provider, aspect: Aspect, prefix: String) -> Result<ImageGenConfig, String> {
    let base_url = a.image_base_url.clone().unwrap_or_else(|| {
        match provider {
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Gemini => "https://generativelanguage.googleapis.com",
            Provider::Comfy => "http://127.0.0.1:8188",
        }
        .to_string()
    });
    let workflow_json = match (&a.workflow, provider) {
        (Some(p), _) => Some(std::fs::read_to_string(p).map_err(|e| format!("--workflow を読めません {}: {e}", p.display()))?),
        (None, Provider::Comfy) => return Err("--images comfy には --workflow <API 形式 JSON> が要ります".into()),
        _ => None,
    };
    Ok(ImageGenConfig {
        provider,
        base_url,
        model: a.image_model.clone(),
        shape: shape_for(aspect),
        detail: a.image_detail,
        style: None,
        user_prefix: prefix,
        negative: String::new(),
        workflow_json,
        timeout_secs: None,
        lock_seed: false,
        seed: 0,
    })
}

/// 参照画像を作って promo.json / scenes.md を更新する (run と images の共通部)。
async fn make_images(a: &Args, provider: Provider, promo: &mut PromoJson, pkg_dir: &Path) -> Result<(), String> {
    let snapshots: Vec<PathBuf> = if a.snapshots.is_empty() {
        promo.snapshot_paths.iter().map(PathBuf::from).collect()
    } else {
        a.snapshots.clone()
    };
    let refs = load_refs(&snapshots)?;
    // 決定 8: palette は画素から。最初のスナップショットで測る (v1)。
    if let Some(first) = snapshots.first() {
        match image_gen::palette_from_file(first, image_gen::DEFAULT_COLORS) {
            Ok(colors) => {
                eprintln!("[img] palette from pixels: {}", colors.join(", "));
                apply_palette(&mut promo.summary.visual_identity, colors);
            }
            Err(e) => eprintln!("[img] palette 算出失敗 (LLM の値を残す): {e}"),
        }
    }
    let prefix = style_anchor(&promo.summary.visual_identity);
    eprintln!("[img] style anchor: {prefix}");
    let cfg = image_cfg(a, provider, promo.plan.aspect, prefix)?;
    let key = image_key(provider);
    if provider != Provider::Comfy && key.is_empty() {
        return Err(format!("{provider:?} の API キーが環境変数にありません (IMAGE_API_KEY_* / *_API_KEY)"));
    }
    let generator = HttpImageGenerator;
    let seed_base = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1);
    let palette = promo.summary.visual_identity.palette.clone();
    let caption = a.caption_font.as_ref().map(|f| CaptionSpec {
        font_path: f.clone(),
        font_index: a.caption_index,
        size_ratio: a.caption_size,
        position: a.caption_pos,
        color: a.caption_color.clone(),
        y_ratio: None,
    });
    let job = RefJob {
        cfg: &cfg,
        api_key: &key,
        out_dir: pkg_dir,
        palette: &palette,
        caption: caption.as_ref(),
        max_scenes: a.image_scenes,
        seed_base,
        requested_refs: a.image_refs,
        plate_mode: a.plate_mode,
        caption_overrides: &promo.caption_overrides,
        plate_overrides: &promo.plate_overrides,
    };
    let results = generate_references(&generator, &job, &mut promo.plan, &refs, &mut |s| eprintln!("[img] {s}")).await;
    let ok = results.iter().filter(|r| r.result.is_ok()).count();
    eprintln!("== images: {ok}/{} ok ==", results.len());
    // plan に reference_image が入ったので書き直す。
    write_run_files(pkg_dir, promo)?;
    if ok < results.len() {
        return Err(format!("{} シーンの参照画像が失敗しました (詳細は上のログ)", results.len() - ok));
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = real_main().await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// `promo compose <backdrop.png> <screenshot.png> <out.png> [16:9|9:16|1:1]` — 合成だけ (LLM ゼロ、目視用)。
fn compose_cmd(args: &[String]) -> Result<(), String> {
    let [bg, shot, out, rest @ ..] = args else { return Err("compose <backdrop> <screenshot> <out> [aspect]".into()) };
    let shape = match rest.first().map(String::as_str) {
        Some("9:16") => Shape::Portrait,
        Some("1:1") => Shape::Square,
        _ => Shape::Landscape,
    };
    let (w, h) = image_gen::provider::SizeMap::comfy_dims(shape);
    let mut layout = image_gen::Layout::for_canvas(w, h);
    if rest.iter().any(|a| a == "band") {
        layout = layout.with_caption_band(false);
    }
    let png = image_gen::composite_product_cut(
        &std::fs::read(bg).map_err(|e| e.to_string())?,
        &std::fs::read(shot).map_err(|e| e.to_string())?,
        &layout,
    )?;
    std::fs::write(out, png).map_err(|e| e.to_string())?;
    println!("{out}");
    Ok(())
}

async fn real_main() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("compose") {
        return compose_cmd(&argv[2..]);
    }
    if argv.get(1).map(String::as_str) == Some("caption") {
        return caption_cmd(&argv[2..]);
    }
    if argv.get(1).map(String::as_str) == Some("fonts") {
        for f in image_gen::list_fonts(Path::new("fonts")) {
            println!("{}\t{:?}\t{}\t#{}\t{}", if f.has_japanese { "JP" } else { "--" }, f.source, f.family, f.index, f.path);
        }
        return Ok(());
    }
    let a = parse()?;
    match a.cmd.as_str() {
        "brief" => {
            let inputs = collect_brief(&a.target, &a.snapshots)?;
            println!("{}", compress(&inputs).render());
            Ok(())
        }
        "images" => {
            let provider = a.images.ok_or("--images <provider> が要ります")?;
            let text = std::fs::read_to_string(&a.target).map_err(|e| format!("promo.json を読めません: {e}"))?;
            let mut promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
            let pkg_dir = a.target.parent().ok_or("promo.json の親フォルダが要ります")?.to_path_buf();
            make_images(&a, provider, &mut promo, &pkg_dir).await?;
            println!("{}", pkg_dir.display());
            Ok(())
        }
        "run" => run(&a).await,
        other => Err(format!("unknown command {other}")),
    }
}

async fn run(a: &Args) -> Result<(), String> {
    if a.concept.trim().is_empty() {
        return Err("--concept が要ります".into());
    }
    let inputs = collect_brief(&a.target, &a.snapshots)?;
    let brief = compress(&inputs);
    let brief_text = brief.render();
    let scratch = std::env::temp_dir().join("apppromo_work");
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let runner = CliTaskRunner {
        spec: CliSpec { kind: a.kind, executable: a.exe.clone(), extra_args: vec![], timeout_secs: a.timeout, model: a.model.clone() },
        project_path: a.target.clone(),
        scratch_dir: scratch,
        max_turns: a.max_turns,
        extra_read_dirs: pipeline::task::snapshot_dirs(&a.snapshots),
        cancel: rx,
        on_event: Box::new(|e| match e {
            CliEvent::Started { pid } => eprintln!("[cli] started pid={pid}"),
            CliEvent::Stdout { text } => eprintln!("[cli] {}", text.chars().take(200).collect::<String>()),
            CliEvent::Stderr { text } => eprintln!("[cli!] {text}"),
            CliEvent::Progress { text } => eprintln!("[cli~] {text}"),
            CliEvent::Structured { .. } => eprintln!("[cli] structured output received"),
        }),
        // agy には Anthropic の鍵を渡さない (rev52)。CLI にスイッチは無いので claude / aider / custom は外さない。
        env_remove: pipeline::task::env_remove_for(a.kind, false),
    };
    eprintln!("== brief: {} chars, tree {} lines ==", brief_text.chars().count(), brief.tree.lines().count());
    let (summary, r1) = analyze(&runner, &brief_text, &a.concept, a.lang).await.map_err(|e| e.to_string())?;
    eprintln!("== analyze: {}, {} ms ==", cost_text(r1.cost_usd), r1.duration_ms);
    eprintln!("{}", serde_json::to_string_pretty(&summary).unwrap());
    let (plan, r2) = plan_scenes(&runner, &summary, &a.concept, a.seconds, a.aspect, a.lang, &brief.snapshots, a.plate_mode)
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("== plan: attempts {}, {}, {} ms ==", r2.attempts, cost_text(r2.cost_usd), r2.duration_ms);
    for (i, vs) in r2.violations_per_attempt.iter().enumerate() {
        if vs.is_empty() {
            eprintln!("   attempt {}: ok", i + 1);
        } else {
            for v in vs {
                eprintln!("   attempt {}: {}", i + 1, promo_core::describe_violation(v));
            }
        }
    }
    let mut promo = PromoJson {
        project_path: a.target.to_string_lossy().to_string(),
        snapshot_paths: a.snapshots.iter().map(|p| p.to_string_lossy().to_string()).collect(),
        video_concept: a.concept.clone(),
        original_copy: promo_core::export::capture_original_copy(&plan),
        summary,
        plan,
        caption_overrides: Default::default(),
        plate_overrides: Default::default(),
        plate_mode: a.plate_mode,
        // rev25: 組み立ては stages::run_stats 1 箇所 (GUI と同じ関数)。
        run_stats: Some(pipeline::stages::run_stats(&r1, &r2)),
    };
    // rev7: run ごとに隔離する。既存の id を見てから採番 (同じ秒に 2 本走っても衝突しない)。
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let run_id = promo_core::export::run_id_from(now_ms, &existing_run_ids(&a.out, &promo.summary.app_name));
    let dir = write_package(&a.out, &promo, &run_id)?;
    if let Some(provider) = a.images {
        make_images(a, provider, &mut promo, &dir).await?;
    }
    println!("{}", dir.display());
    Ok(())
}

/// 費用の 1 行。**記録が無ければ 0 と書かない** (agy は費用を返さない)。
fn cost_text(cost: Option<f64>) -> String {
    match cost {
        Some(c) => format!("{c:.4} USD"),
        None => "費用の記録なし".to_string(),
    }
}
