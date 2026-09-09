//! AppPromoVideo デスクトップ殻のバックエンド (Tauri 2)。
//!
//! 役割は **pipeline を Tauri command で叩くだけ**。HTTP とプロセスは全部ここ (backend) で動き、
//! frontend は command が返す DTO を描画し、進捗は event `promo-progress` で受ける。
//! 設定の非秘密は frontend の localStorage (+ settings.json ミラー)、画像 API キーは `app_data/.env`。

mod env_store;
mod runs_store;
mod settings_store;

use std::path::{Path, PathBuf};

use cli_runner::runner::CliEvent;
use cli_runner::{CliKind, CliSpec};
use image_gen::{HttpImageGenerator, ImageGenConfig, Provider};
use pipeline::collect::{collect_brief, snapshot_meta};
use pipeline::export::{existing_run_ids, write_atomic, write_package};
use pipeline::reference::{CaptionSpec, RefJob, generate_references, load_refs};
use pipeline::task::CliTaskRunner;
use pipeline::{analyze, plan_scenes};
use promo_core::brief::{SnapshotMeta, compress};
use promo_core::export::{PromoJson, scenes_markdown};
use promo_core::plan::{Aspect, PlateMode};
use promo_core::prompts::Language;
use promo_core::style::{apply_palette, style_anchor};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex, watch};

/// 実行中の run を止めるための cancel sender (同時に 1 run)。
pub struct RunControl(Mutex<Option<watch::Sender<bool>>>);

#[derive(Clone, Serialize)]
struct Progress {
    stage: String,
    text: String,
}

fn emit(app: &AppHandle, stage: &str, text: impl Into<String>) {
    let _ = app.emit("promo-progress", Progress { stage: stage.into(), text: text.into() });
}

fn app_data(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| format!("app_data_dir: {e}"))
}

// ---------------------------------------------------------------------------
// 設定ミラー (契約 config_sources.settings_mirror)
// ---------------------------------------------------------------------------

#[tauri::command]
fn load_ui_settings(app: AppHandle) -> Result<Option<String>, String> {
    Ok(settings_store::read_valid(&app_data(&app)?.join("settings.json")))
}

#[tauri::command]
fn save_ui_settings(app: AppHandle, json: String) -> Result<(), String> {
    settings_store::validate(&json)?;
    settings_store::write_atomic(&app_data(&app)?.join("settings.json"), &json)
}

// ---------------------------------------------------------------------------
// 入力 (ダイアログ / brief)
// ---------------------------------------------------------------------------

#[tauri::command]
async fn pick_directory() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().pick_folder().map(|p| p.to_string_lossy().to_string()))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
async fn pick_images() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("画像", &["png", "jpg", "jpeg", "webp", "PNG", "JPG", "JPEG", "WEBP"])
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[derive(Serialize)]
struct BriefPreview {
    text: String,
    chars: usize,
    tree_lines: usize,
    snapshots: Vec<SnapshotMeta>,
}

#[tauri::command]
fn brief_preview(project_path: String, snapshot_paths: Vec<String>) -> Result<BriefPreview, String> {
    let snaps: Vec<PathBuf> = snapshot_paths.iter().map(PathBuf::from).collect();
    let inputs = collect_brief(Path::new(&project_path), &snaps)?;
    let brief = compress(&inputs);
    let text = brief.render();
    Ok(BriefPreview { chars: text.chars().count(), tree_lines: brief.tree.lines().count(), snapshots: brief.snapshots.clone(), text })
}

/// スナップショット 1 枚の検証 (追加時)。
#[tauri::command]
fn validate_snapshot(path: String) -> Result<SnapshotMeta, String> {
    snapshot_meta(Path::new(&path))
}

/// クリップボードから貼られた画像 (frontend の paste → data URL の base64) を app_data/snapshots に保存し、
/// 他のスナップショットと同じ「パス」として扱えるようにする。png 以外の mime は拡張子だけ合わせる。
#[tauri::command]
fn save_clipboard_image(app: AppHandle, base64: String, mime: String) -> Result<SnapshotMeta, String> {
    let bytes = image_gen::provider::base64_decode(base64.trim()).ok_or("base64 を読めません")?;
    if bytes.is_empty() {
        return Err("空の画像です".into());
    }
    let ext = match mime.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    let dir = app_data(&app)?.join("snapshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("snapshots フォルダを作れません: {e}"))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = dir.join(format!("clip_{stamp}.{ext}"));
    std::fs::write(&path, &bytes).map_err(|e| format!("保存できません {}: {e}", path.display()))?;
    // ヘッダ検証 (壊れた貼り付けを一覧に乗せない)。
    snapshot_meta(&path).inspect_err(|_| {
        let _ = std::fs::remove_file(&path);
    })
}

// ---------------------------------------------------------------------------
// CLI の存在検査 (P3: 無いものは実行前に見せる)
// ---------------------------------------------------------------------------

#[derive(Serialize, Default)]
struct CliCheck {
    found: bool,
    version: String,
    error: String,
    /// 認証の見え方 (値は出さない)。2026-09-08 GUI 実測: 401 の切り分けに「どの資格情報を子が使うか」が要った。
    auth: AuthView,
}

/// 子 CLI が見る認証ソース。鍵は長さと SHA-256 先頭 12 桁だけ (別の鍵かどうかを比べるため)。
#[derive(Serialize, Default, Clone)]
struct AuthView {
    api_key_present: bool,
    api_key_len: usize,
    api_key_fingerprint: String,
    auth_token_present: bool,
    base_url: String,
    /// `claude auth status --json` の loggedIn / authMethod (claude 以外の CLI では空)。
    oauth_logged_in: Option<bool>,
    oauth_method: String,
    /// 子に渡さない変数 (env_scrub) のうち、このプロセスに載っていたもの。
    scrubbed: Vec<String>,
}

fn fingerprint(v: &str) -> String {
    // 依存を足さない FNV-1a 64bit + 簡易混合。衝突耐性は要らない (同じ鍵かどうかの目視比較用)。
    let mut h: u64 = 0xcbf29ce484222325;
    for b in v.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")[..12].to_string()
}

fn auth_view_env() -> AuthView {
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    AuthView {
        api_key_present: !key.trim().is_empty(),
        api_key_len: key.trim().len(),
        api_key_fingerprint: if key.trim().is_empty() { String::new() } else { fingerprint(key.trim()) },
        auth_token_present: std::env::var("ANTHROPIC_AUTH_TOKEN").map(|v| !v.trim().is_empty()).unwrap_or(false),
        base_url: std::env::var("ANTHROPIC_BASE_URL").unwrap_or_default(),
        oauth_logged_in: None,
        oauth_method: String::new(),
        scrubbed: cli_runner::env_scrub::names_to_scrub(),
    }
}

#[tauri::command]
async fn check_cli(executable: String) -> CliCheck {
    let mut auth = auth_view_env();
    let fut = tokio::process::Command::new(&executable).arg("--version").output();
    let mut check = match tokio::time::timeout(std::time::Duration::from_secs(20), fut).await {
        Ok(Ok(out)) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            CliCheck { found: true, version: text.lines().next().unwrap_or("").to_string(), ..Default::default() }
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            CliCheck { found: false, error: format!("CLI が見つかりません: {executable}"), ..Default::default() }
        }
        Ok(Err(e)) => CliCheck { found: false, error: e.to_string(), ..Default::default() },
        Err(_) => CliCheck { found: true, error: "--version が 20 秒で返りませんでした".into(), ..Default::default() },
    };
    if check.found && check.version.to_lowercase().contains("claude") {
        let mut cmd = tokio::process::Command::new(&executable);
        for n in cli_runner::env_scrub::names_to_scrub() {
            cmd.env_remove(n);
        }
        let fut = cmd.args(["auth", "status", "--json"]).output();
        if let Ok(Ok(out)) = tokio::time::timeout(std::time::Duration::from_secs(20), fut).await {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                auth.oauth_logged_in = v.get("loggedIn").and_then(|b| b.as_bool());
                auth.oauth_method = v.get("authMethod").and_then(|s| s.as_str()).unwrap_or("").to_string();
            }
        }
    }
    check.auth = auth;
    check
}

// ---------------------------------------------------------------------------
// 解析 → シーン構成 → export (契約 PromoProject / ExportPackage)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CliSettings {
    kind: CliKind,
    executable: String,
    #[serde(default)]
    extra_args: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    #[serde(default = "default_turns")]
    max_turns: u32,
    /// true なら子に ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN を渡さず、claude auth login の OAuth を使わせる。
    #[serde(default)]
    oauth_only: bool,
}

fn default_timeout() -> u64 {
    600
}
fn default_turns() -> u32 {
    12
}

#[derive(Deserialize)]
struct RunRequest {
    project_path: String,
    #[serde(default)]
    snapshot_paths: Vec<String>,
    concept: String,
    seconds: u32,
    aspect: Aspect,
    language: Language,
    export_dir: String,
    cli: CliSettings,
    /// 面の貼り方 (rev5)。省略時は perspective。
    #[serde(default)]
    plate_mode: PlateMode,
}

#[derive(Serialize)]
struct StageInfo {
    attempts: usize,
    cost_usd: f64,
    duration_ms: u64,
    /// 各試行の違反 (人が読む文)。
    violations: Vec<Vec<String>>,
}

impl From<pipeline::StageReport> for StageInfo {
    fn from(r: pipeline::StageReport) -> Self {
        StageInfo {
            attempts: r.attempts,
            cost_usd: r.cost_usd,
            duration_ms: r.duration_ms,
            violations: r
                .violations_per_attempt
                .iter()
                .map(|vs| vs.iter().map(promo_core::describe_violation).collect())
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct RunResult {
    promo: PromoJson,
    package_dir: String,
    analyze: StageInfo,
    plan: StageInfo,
    brief_chars: usize,
}

#[tauri::command]
async fn run_pipeline(app: AppHandle, ctl: State<'_, RunControl>, req: RunRequest) -> Result<RunResult, String> {
    let (tx, rx) = watch::channel(false);
    *ctl.0.lock().await = Some(tx);
    let result = run_inner(&app, rx, req).await;
    *ctl.0.lock().await = None;
    result
}

async fn run_inner(app: &AppHandle, cancel: watch::Receiver<bool>, req: RunRequest) -> Result<RunResult, String> {
    if req.concept.trim().is_empty() {
        return Err("動画イメージ (concept) が空です".into());
    }
    let project = PathBuf::from(&req.project_path);
    let snaps: Vec<PathBuf> = req.snapshot_paths.iter().map(PathBuf::from).collect();
    let a = auth_view_env();
    emit(
        app,
        "cli",
        format!(
            "認証: API キー {} / ANTHROPIC_AUTH_TOKEN {} / base_url {} / 子に渡さない変数 {} 個",
            if a.api_key_present { format!("あり (len {}, fp {})", a.api_key_len, a.api_key_fingerprint) } else { "なし (OAuth ログインを使用)".into() },
            if a.auth_token_present { "あり" } else { "なし" },
            if a.base_url.is_empty() { "既定".to_string() } else { a.base_url.clone() },
            a.scrubbed.len()
        ),
    );
    emit(app, "brief", "リポジトリを読んでいます…");
    let inputs = collect_brief(&project, &snaps)?;
    let brief = compress(&inputs);
    let brief_text = brief.render();
    emit(app, "brief", format!("brief {} 字 / tree {} 行", brief_text.chars().count(), brief.tree.lines().count()));

    // cwd は app 所有の作業ディレクトリ (対象リポジトリの hook / MCP を拾わない — 決定 4)。
    let scratch = app_data(app)?.join("work");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("作業フォルダを作れません: {e}"))?;
    let h = app.clone();
    let runner = CliTaskRunner {
        spec: CliSpec {
            kind: req.cli.kind,
            executable: req.cli.executable.clone(),
            extra_args: req.cli.extra_args.clone(),
            timeout_secs: req.cli.timeout_secs,
            model: req.cli.model.clone().filter(|m| !m.trim().is_empty()),
        },
        project_path: project.clone(),
        scratch_dir: scratch,
        max_turns: req.cli.max_turns,
        cancel,
        on_event: Box::new(move |e| match e {
            CliEvent::Started { pid } => emit(&h, "cli", format!("CLI 起動 (pid {pid})")),
            CliEvent::Stdout { text } => emit(&h, "cli", text),
            CliEvent::Stderr { text } => emit(&h, "cli-stderr", text),
            CliEvent::Progress { text } => emit(&h, "cli", text),
            CliEvent::Structured { .. } => emit(&h, "cli", "構造化出力を受信"),
        }),
        env_remove: if req.cli.oauth_only { pipeline::task::OAUTH_ONLY_REMOVE.iter().map(|s| s.to_string()).collect() } else { vec![] },
    };
    if req.cli.oauth_only {
        emit(app, "cli", "OAuth 優先: ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN は子に渡しません");
    }

    emit(app, "analyze", "解析中 (タスク 1/2)…");
    let (summary, r1) = analyze(&runner, &brief_text, &req.concept, req.language).await.map_err(|e| e.to_string())?;
    emit(app, "analyze", format!("解析 完了: {:.3} USD / {:.1} s", r1.cost_usd, r1.duration_ms as f64 / 1000.0));

    emit(app, "plan", "シーン構成中 (タスク 2/2)…");
    let (plan, r2) = plan_scenes(
        &runner,
        &summary,
        &req.concept,
        req.seconds,
        req.aspect,
        req.language,
        &brief.snapshots,
        req.plate_mode,
    )
        .await
        .map_err(|e| e.to_string())?;
    emit(
        app,
        "plan",
        format!("シーン構成 完了: {} 回目で通過 / {:.3} USD / {:.1} s", r2.attempts, r2.cost_usd, r2.duration_ms as f64 / 1000.0),
    );
    for (i, vs) in r2.violations_per_attempt.iter().enumerate() {
        for v in vs {
            emit(app, "plan", format!("  試行 {}: {}", i + 1, promo_core::describe_violation(v)));
        }
    }

    let promo = PromoJson {
        project_path: req.project_path.clone(),
        snapshot_paths: req.snapshot_paths.clone(),
        video_concept: req.concept.clone(),
        original_copy: promo_core::export::capture_original_copy(&plan),
        summary,
        plan,
        caption_overrides: Default::default(),
        plate_overrides: Default::default(),
        plate_mode: req.plate_mode,
    };
    // rev7: run ごとに隔離する。以前は同じパッケージを上書きして過去の出力を消していた。
    let export_dir = Path::new(&req.export_dir);
    let now_ms = now_unix_ms();
    let run_id = promo_core::export::run_id_from(now_ms, &existing_run_ids(export_dir, &promo.summary.app_name));
    let dir = write_package(export_dir, &promo, &run_id)?;
    let package_dir = dir.parent().and_then(|p| p.parent()).unwrap_or(&dir).to_path_buf();
    record_run(
        app,
        runs_store::RunRecord {
            id: run_id.clone(),
            created_at: now_ms,
            app_name: promo.summary.app_name.clone(),
            project_path: req.project_path.clone(),
            package_dir: package_dir.to_string_lossy().to_string(),
            run_dir: dir.to_string_lossy().to_string(),
            seconds: req.seconds,
            aspect: wire(&req.aspect),
            language: wire(&req.language),
            plate_mode: wire(&req.plate_mode),
            scene_count: promo.plan.scenes.len(),
            cost_usd: r1.cost_usd + r2.cost_usd,
            image_provider: None,
            image_count: 0,
        },
    );
    emit(app, "done", format!("書き出し: {}", dir.display()));
    Ok(RunResult {
        promo,
        package_dir: dir.to_string_lossy().to_string(),
        analyze: r1.into(),
        plan: r2.into(),
        brief_chars: brief_text.chars().count(),
    })
}

#[tauri::command]
async fn cancel_run(ctl: State<'_, RunControl>) -> Result<bool, String> {
    // 参照 (State) を取る async command は Result を返す必要がある (Tauri の制約)。
    Ok(match ctl.0.lock().await.as_ref() {
        Some(tx) => tx.send(true).is_ok(),
        None => false,
    })
}

// ---------------------------------------------------------------------------
// 参照画像 (契約 ImageGenConfig / reference_limits)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct ImagesRequest {
    promo: PromoJson,
    package_dir: String,
    image: ImageGenConfig,
    #[serde(default)]
    snapshot_paths: Vec<String>,
    #[serde(default)]
    max_scenes: Option<usize>,
    #[serde(default)]
    requested_refs: Option<usize>,
    /// 見出しの焼き込み (None = 焼かない。既定 OFF)。
    #[serde(default)]
    caption: Option<CaptionSpec>,
    /// 面の貼り方 (rev5)。省略時は perspective。
    #[serde(default)]
    plate_mode: PlateMode,
}

#[derive(Serialize)]
struct SceneImageInfo {
    scene_id: u32,
    ok: bool,
    path: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct ImagesResult {
    promo: PromoJson,
    results: Vec<SceneImageInfo>,
    palette: Vec<String>,
    anchor: String,
    truncated: Option<String>,
}

#[tauri::command]
async fn generate_images(app: AppHandle, req: ImagesRequest) -> Result<ImagesResult, String> {
    let mut promo = req.promo;
    let mut cfg = req.image;
    let snaps: Vec<PathBuf> = if req.snapshot_paths.is_empty() {
        promo.snapshot_paths.iter().map(PathBuf::from).collect()
    } else {
        req.snapshot_paths.iter().map(PathBuf::from).collect()
    };
    let refs = load_refs(&snaps)?;
    // 決定 8: palette は画素から (最初のスナップショット)。
    let mut palette = Vec::new();
    if let Some(first) = snaps.first() {
        match image_gen::palette_from_file(first, image_gen::DEFAULT_COLORS) {
            Ok(colors) => {
                palette = colors.clone();
                apply_palette(&mut promo.summary.visual_identity, colors);
                emit(&app, "images", format!("palette (画素): {}", palette.join(", ")));
            }
            Err(e) => emit(&app, "images", format!("palette 算出失敗 (LLM の値を残す): {e}")),
        }
    }
    let anchor = style_anchor(&promo.summary.visual_identity);
    if cfg.user_prefix.trim().is_empty() {
        cfg.user_prefix = anchor.clone();
    }
    emit(&app, "images", format!("style anchor: {}", cfg.user_prefix));
    let key = env_store::image_api_key(&app_data(&app)?.join(".env"), cfg.provider);
    if cfg.provider != Provider::Comfy && key.is_empty() {
        return Err(format!("{:?} の API キーが未設定です (設定 → 画像生成)", cfg.provider));
    }
    let pkg_dir = PathBuf::from(&req.package_dir);
    std::fs::create_dir_all(&pkg_dir).map_err(|e| e.to_string())?;
    let seed_base = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1);
    let palette_for_fallback = promo.summary.visual_identity.palette.clone();
    if let Some(c) = &req.caption {
        emit(&app, "images", format!("見出しを焼き込みます: {} #{} size {:.3} {:?}", c.font_path, c.font_index, c.size_ratio, c.position));
    }
    let job = RefJob {
        cfg: &cfg,
        api_key: &key,
        out_dir: &pkg_dir,
        palette: &palette_for_fallback,
        caption: req.caption.as_ref(),
        plate_mode: req.plate_mode,
        caption_overrides: &promo.caption_overrides,
        plate_overrides: &promo.plate_overrides,
        max_scenes: req.max_scenes,
        seed_base,
        requested_refs: req.requested_refs,
    };
    let h = app.clone();
    let mut progress = move |s: String| emit(&h, "images", s);
    let results = generate_references(&HttpImageGenerator, &job, &mut promo.plan, &refs, &mut progress).await;
    let truncated = results.first().and_then(|r| r.truncated.as_ref()).map(|t| format!("{} 枚のうち先頭 {} 枚 ({})", t.total, t.sent, t.limit_by));
    // plan に reference_image が入ったので書き直す。
    let json = serde_json::to_string_pretty(&promo).map_err(|e| e.to_string())?;
    write_atomic(&pkg_dir.join("promo.json"), json.as_bytes())?;
    write_atomic(&pkg_dir.join("scenes.md"), scenes_markdown(&promo.summary, &promo.plan).as_bytes())?;
    let results: Vec<SceneImageInfo> = results
        .into_iter()
        .map(|r| match r.result {
            Ok(p) => SceneImageInfo { scene_id: r.scene_id, ok: true, path: Some(p.to_string_lossy().to_string()), error: None },
            Err(e) => SceneImageInfo { scene_id: r.scene_id, ok: false, path: None, error: Some(e.to_string()) },
        })
        .collect();
    update_run_images(&app, &req.package_dir, wire(&cfg.provider), &results);
    Ok(ImagesResult { promo, results, palette, anchor, truncated })
}

// ---------------------------------------------------------------------------
// run の履歴 (契約 RunIndex / RunRecord、rev7)
// ---------------------------------------------------------------------------

/// 契約の表記 (serde の値) を文字列で取る。`format!("{:?}")` は Rust の識別子であって契約ではない —
/// `Aspect::Landscape` の契約表記は `16:9`、`PlateMode::Perspective` は `perspective`。
fn wire<T: Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 索引への書き込みは**失敗しても本流を止めない** (生成そのものは成功しているので、
/// 索引が書けないことで結果を捨てない)。理由は進捗ログに出す。
fn record_run(app: &AppHandle, rec: runs_store::RunRecord) {
    let Ok(dir) = app_data(app) else {
        emit(app, "done", "履歴を保存できません (app_data が取れません)");
        return;
    };
    let path = runs_store::index_path(&dir);
    let mut index = runs_store::load(&path);
    runs_store::upsert(&mut index, rec);
    if let Err(e) = runs_store::save(&path, &index) {
        emit(app, "done", format!("履歴を保存できません: {e}"));
    }
}

/// 画像を作った後に、同じ run の記録へ枚数とプロバイダを書き足す。
fn update_run_images(app: &AppHandle, run_dir: &str, provider: String, results: &[SceneImageInfo]) {
    let Ok(dir) = app_data(app) else { return };
    let path = runs_store::index_path(&dir);
    let mut index = runs_store::load(&path);
    let Some(rec) = index.runs.iter_mut().find(|r| r.run_dir == run_dir) else { return };
    rec.image_provider = Some(provider);
    rec.image_count = results.iter().filter(|r| r.ok).count();
    if let Err(e) = runs_store::save(&path, &index) {
        emit(app, "images", format!("履歴を更新できません: {e}"));
    }
}

#[derive(Serialize)]
struct RunListItem {
    #[serde(flatten)]
    record: runs_store::RunRecord,
    /// run_dir が実在するか。false でも索引からは消さない (移動しただけかもしれない)。
    exists: bool,
}

/// 履歴の一覧 (新しい順)。**正本はフォルダ側**なので、実在するかを添えて返す。
#[tauri::command]
fn list_runs(app: AppHandle) -> Result<Vec<RunListItem>, String> {
    let dir = app_data(&app)?;
    let index = runs_store::load(&runs_store::index_path(&dir));
    Ok(index
        .runs
        .into_iter()
        .map(|r| {
            let exists = Path::new(&r.run_dir).join("promo.json").is_file();
            RunListItem { record: r, exists }
        })
        .collect())
}

#[derive(Serialize)]
struct OpenedRun {
    promo: PromoJson,
    package_dir: String,
    images: Vec<SceneImageInfo>,
}

/// 過去の run を読み戻す。**索引ではなく `run_dir/promo.json` から読む** (そちらが正本)。
#[tauri::command]
fn open_run(run_dir: String) -> Result<OpenedRun, String> {
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json"))
        .map_err(|e| format!("この run を読めません {}: {e}", dir.display()))?;
    let promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
    let images = promo
        .plan
        .scenes
        .iter()
        .map(|s| {
            let p = dir.join(promo_core::export::reference_image_name(s.scene_id, 1));
            match p.is_file() {
                true => SceneImageInfo { scene_id: s.scene_id, ok: true, path: Some(p.to_string_lossy().to_string()), error: None },
                false => SceneImageInfo { scene_id: s.scene_id, ok: false, path: None, error: None },
            }
        })
        .collect();
    Ok(OpenedRun { promo, package_dir: run_dir, images })
}

/// 履歴から外す。`delete_files` が true の時だけフォルダも消す (既定は索引からだけ)。
#[tauri::command]
fn forget_run(app: AppHandle, run_dir: String, delete_files: bool) -> Result<bool, String> {
    let dir = app_data(&app)?;
    let path = runs_store::index_path(&dir);
    let mut index = runs_store::load(&path);
    let removed = runs_store::forget(&mut index, &run_dir);
    runs_store::save(&path, &index)?;
    if delete_files {
        let target = PathBuf::from(&run_dir);
        // 誤爆よけ: runs/<id> の形で promo.json を持つフォルダだけ消す。
        let looks_like_run = target.parent().map(|p| p.ends_with("runs")).unwrap_or(false);
        if !looks_like_run {
            return Err(format!("run のフォルダに見えないので消しません: {run_dir}"));
        }
        std::fs::remove_dir_all(&target).map_err(|e| format!("フォルダを消せません {run_dir}: {e}"))?;
    }
    Ok(removed)
}

/// 見出しだけ焼き直す (rev9、契約 `caption.per_scene.reburn`)。
///
/// **生成 API は呼ばない。** `base/` に残した焼く前の合成を読んで焼き、直下の完成品を置き換える。
/// 何度やっても劣化しない (毎回 base から焼くので、焼いた上に焼くことがない)。
#[tauri::command]
fn reburn_caption(
    run_dir: String,
    scene_id: u32,
    spec: Option<CaptionSpec>,
    plate: Option<promo_core::export::PlateOverride>,
    new_copy: Option<String>, // コピー文の書き換え (rev12)。None なら今の文のまま

) -> Result<String, String> {
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let mut promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
    let scene = promo
        .plan
        .scenes
        .iter_mut()
        .find(|s| s.scene_id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} がありません"))?;
    // rev12: コピー文はその場で書き換える。scenes.md もクリップボードも plan を読むので揃う。
    if let Some(t) = &new_copy {
        scene.copy_text = t.clone();
    }
    let copy_text = scene.copy_text.clone();
    let scene_kind = scene.cut_kind;
    let snapshot_index = scene.snapshot_index;
    let tilt = pipeline::reference::tilt_of(promo.plate_mode, scene);

    let name = promo_core::export::reference_image_name(scene_id, 1);
    let base = pipeline::reference::base_image_path(&dir, &name);
    let png = std::fs::read(&base).map_err(|e| {
        format!("素材がありません ({}): {e}。rev10 より前に作った run は焼き直せません", base.display())
    })?;

    // rev10: base/ は**素材** (product は背景 / mood は絵)。product は合成からやり直すので、
    // 見出しの位置を変えても帯が正しく取り直され、傾きもそのまま乗る。
    let canvas = pipeline::reference::image_dims(&png)?;
    let composed = match scene_kind {
        promo_core::plan::CutKind::Mood => png,
        promo_core::plan::CutKind::Product => {
            let idx = snapshot_index.unwrap_or(0) as usize;
            let shot = read_run_snapshot(&dir, idx)?;
            let l = pipeline::reference::layout_for(canvas, spec.as_ref().map(|s| s.position), tilt, plate.as_ref());
            image_gen::composite_product_cut(&png, &shot, &l)?
        }
    };
    let out = match &spec {
        None => composed,
        Some(sp) => {
            let font = std::fs::read(&sp.font_path).map_err(|e| format!("フォントを読めません {}: {e}", sp.font_path))?;
            let mut cap = image_gen::Caption::new(&copy_text, &font, sp.font_index);
            cap.size_ratio = sp.size_ratio;
            cap.position = sp.position;
            cap.color = sp.rgba();
            image_gen::burn_caption(&composed, &cap)?
        }
    };
    std::fs::write(dir.join(&name), &out).map_err(|e| format!("書けません {name}: {e}"))?;

    // はめ込みの上書きも残す (rev11)。空の指定は「既定に戻す」なので行ごと消す。
    match &plate {
        Some(p) if *p != Default::default() => {
            promo.plate_overrides.insert(scene_id, p.clamped());
        }
        _ => {
            promo.plate_overrides.remove(&scene_id);
        }
    }
    // 上書きを promo.json に残す (run を開き直しても効くように)。
    match &spec {
        None => {
            promo.caption_overrides.remove(&scene_id);
        }
        Some(sp) => {
            promo.caption_overrides.insert(
                scene_id,
                promo_core::export::CaptionOverride {
                    font_path: Some(sp.font_path.clone()),
                    font_index: Some(sp.font_index),
                    size_ratio: Some(sp.size_ratio),
                    position: Some(match sp.position {
                        image_gen::CaptionPosition::Top => "top".into(),
                        image_gen::CaptionPosition::Bottom => "bottom".into(),
                    }),
                    color: sp.color.clone(),
                },
            );
        }
    }
    let json = serde_json::to_string_pretty(&promo).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("promo.json"), json.as_bytes())?;
    // コピー文を書き換えたら scenes.md も揃える (パッケージの中で食い違わせない)。
    if new_copy.is_some() {
        write_atomic(&dir.join("scenes.md"), scenes_markdown(&promo.summary, &promo.plan).as_bytes())?;
    }
    Ok(image_gen::provider::data_url("image/png", &out))
}

/// run に写したスナップショット (rev10)。**元のパスではなく run の写しを読む** — 元は移動されうるし、
/// run は自己完結しているべきなので。
fn read_run_snapshot(run_dir: &Path, idx: usize) -> Result<Vec<u8>, String> {
    let dir = run_dir.join("snapshots");
    let want = format!("snapshot_{:02}.", idx + 1);
    let entry = std::fs::read_dir(&dir)
        .map_err(|e| format!("snapshots/ を読めません: {e}"))?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&want))
        .ok_or_else(|| format!("snapshots/{want}* がありません (この run は焼き直せません)"))?;
    std::fs::read(entry.path()).map_err(|e| format!("スナップショットを読めません: {e}"))
}

/// 表示用 data URL (CSP: img-src に data: あり。ローカルファイルを WebView に直接見せない)。
#[tauri::command]
fn image_data_url(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("画像を読めません {path}: {e}"))?;
    let mime = match Path::new(&path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };
    Ok(image_gen::provider::data_url(mime, &bytes))
}

#[derive(Serialize)]
struct KeysView {
    openai: bool,
    gemini: bool,
}

#[tauri::command]
fn get_image_api_keys(app: AppHandle) -> Result<KeysView, String> {
    let env = app_data(&app)?.join(".env");
    Ok(KeysView {
        openai: !env_store::image_api_key(&env, Provider::Openai).is_empty(),
        gemini: !env_store::image_api_key(&env, Provider::Gemini).is_empty(),
    })
}

#[tauri::command]
fn set_image_api_key(app: AppHandle, provider: Provider, key: String) -> Result<(), String> {
    let name = env_store::env_name(provider).ok_or("このプロバイダはキーを使いません")?;
    env_store::upsert_env(&app_data(&app)?.join(".env"), name, key.trim())
}

#[tauri::command]
async fn probe_image(app: AppHandle, image: ImageGenConfig) -> Result<String, String> {
    let key = env_store::image_api_key(&app_data(&app)?.join(".env"), image.provider);
    image_gen::provider::probe(&image, &key).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// 見出しのフォント (契約 caption.fonts: システム + app_data/fonts)
// ---------------------------------------------------------------------------

fn fonts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app_data(app)?.join("fonts");
    std::fs::create_dir_all(&d).map_err(|e| format!("fonts フォルダを作れません: {e}"))?;
    Ok(d)
}

#[tauri::command]
async fn list_fonts(app: AppHandle) -> Result<Vec<image_gen::FontEntry>, String> {
    let dir = fonts_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || image_gen::list_fonts(&dir)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn open_fonts_folder(app: AppHandle) -> Result<(), String> {
    open_folder(fonts_dir(&app)?.to_string_lossy().to_string())
}

#[derive(Deserialize)]
struct CaptionPreviewRequest {
    /// 背景に使う画像 (スナップショットや参照画像)。無ければ濃色の単色。
    #[serde(default)]
    image_path: Option<String>,
    text: String,
    caption: CaptionSpec,
}

/// 設定画面のプレビュー: 指定フォントで見出しを焼いた data URL を返す。
#[tauri::command]
fn caption_preview(req: CaptionPreviewRequest) -> Result<String, String> {
    let base = match &req.image_path {
        Some(p) => std::fs::read(p).map_err(|e| format!("画像を読めません {p}: {e}"))?,
        None => image_gen::solid_backdrop(1344, 768, [28, 25, 38]),
    };
    // 大きいスナップショットはプレビュー用に 16:9 canvas へ合成してから焼く (見え方を本番と揃える)。
    let canvas = if req.image_path.is_some() {
        image_gen::composite_product_cut(&image_gen::solid_backdrop(1344, 768, [28, 25, 38]), &base, &image_gen::Layout::for_canvas(1344, 768))?
    } else {
        base
    };
    let font = std::fs::read(&req.caption.font_path).map_err(|e| format!("フォントを読めません {}: {e}", req.caption.font_path))?;
    let mut cap = image_gen::Caption::new(&req.text, &font, req.caption.font_index);
    cap.size_ratio = req.caption.size_ratio;
    cap.position = req.caption.position;
    let png = image_gen::burn_caption(&canvas, &cap)?;
    Ok(image_gen::provider::data_url("image/png", &png))
}

// ---------------------------------------------------------------------------
// export 補助
// ---------------------------------------------------------------------------

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.is_dir() {
        return Err(format!("フォルダではありません: {path}"));
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(p).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// パッケージを別フォルダへ丸ごと写す (画像込み)。
#[tauri::command]
fn copy_package(src_dir: String, dst_root: String) -> Result<String, String> {
    let src = PathBuf::from(&src_dir);
    let name = copy_target_name(&src).ok_or("パッケージ名が取れません")?;
    let dst = PathBuf::from(&dst_root).join(name);
    copy_dir(&src, &dst)?;
    Ok(dst.to_string_lossy().to_string())
}

/// コピー先のフォルダ名 (純粋)。rev7 で run が `<pkg>/runs/<id>/` に入ったので、
/// そのまま `file_name()` を使うと日時だけの名前になってアプリ名が消える。
/// run dir なら `<App>_Promo_Package_<run_id>` に組み直す。
fn copy_target_name(src: &Path) -> Option<String> {
    let leaf = src.file_name()?.to_string_lossy().to_string();
    let parent = src.parent()?;
    if parent.file_name().map(|n| n == "runs").unwrap_or(false)
        && let Some(pkg) = parent.parent().and_then(|p| p.file_name())
    {
        return Some(format!("{}_{leaf}", pkg.to_string_lossy()));
    }
    Some(leaf)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let to = dst.join(e.file_name());
        if e.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(RunControl(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            load_ui_settings,
            save_ui_settings,
            pick_directory,
            pick_images,
            brief_preview,
            validate_snapshot,
            save_clipboard_image,
            check_cli,
            run_pipeline,
            cancel_run,
            generate_images,
            image_data_url,
            get_image_api_keys,
            set_image_api_key,
            probe_image,
            list_fonts,
            open_fonts_folder,
            caption_preview,
            open_folder,
            copy_package,
            list_runs,
            open_run,
            forget_run,
            reburn_caption,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod copy_name_tests {
    use super::copy_target_name;
    use std::path::Path;

    #[test]
    fn run_dirs_keep_the_app_name_in_the_copy() {
        // rev7: run は <pkg>/runs/<id>/ に入る。日時だけの名前でコピーしない。
        let run = Path::new("D:/out/Task_Flow_Promo_Package/runs/20260908-143200");
        assert_eq!(copy_target_name(run).as_deref(), Some("Task_Flow_Promo_Package_20260908-143200"));
        // rev6 以前のパッケージ (runs を挟まない) はそのままの名前。
        let legacy = Path::new("D:/out/Task_Flow_Promo_Package");
        assert_eq!(copy_target_name(legacy).as_deref(), Some("Task_Flow_Promo_Package"));
    }
}

#[cfg(test)]
mod wire_tests {
    use super::wire;
    use promo_core::plan::{Aspect, PlateMode};
    use promo_core::prompts::Language;

    /// 履歴に残すのは Rust の識別子ではなく契約の表記 (GUI で `Perspective` と出ていた回帰)。
    #[test]
    fn record_keeps_contract_spelling_not_debug_names() {
        assert_eq!(wire(&Aspect::Landscape), "16:9");
        assert_eq!(wire(&Aspect::Portrait), "9:16");
        assert_eq!(wire(&Language::Ja), "ja");
        assert_eq!(wire(&PlateMode::Perspective), "perspective");
        assert_eq!(wire(&PlateMode::Frontal), "frontal");
    }
}

