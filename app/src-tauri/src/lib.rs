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
use pipeline::export::{existing_run_ids, write_package, write_run_files};
use pipeline::reference::{CaptionSpec, RefJob, generate_references, load_refs};
use pipeline::task::CliTaskRunner;
use pipeline::{analyze, plan_scenes};
use promo_core::brief::{SnapshotMeta, compress};
use promo_core::export::PromoJson;
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
    /// `--version` の名乗りが設定の「種類」と食い違っている (契約 `CliKindCheck`)。
    /// 2026-09-12: 種類が claude のまま実行ファイルだけ agy にでき、claude の argv が別系統の CLI に飛んでいた。
    #[serde(default)]
    kind_mismatch: bool,
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

/// run 開始時に進捗へ出す認証の行 (純粋、rev52)。
///
/// Anthropic の鍵と OAuth を語るのは claude の時だけ。agy には鍵を常に渡さず (`env_remove_for`)、
/// 「API キー あり」「OAuth 優先」と書くと agy に鍵を渡しているように読める (ユーザー 2026-09-14)。
/// aider / custom も同様に語らない (rev53) — スイッチを持つのは claude だけ。
fn auth_log_lines(kind: cli_runner::CliKind, oauth_only: bool, a: &AuthView) -> Vec<String> {
    // claude 以外では 1 行も出さない — claude でない CLI に Anthropic の話を出すのは不自然 (ユーザー 2026-09-14、rev52〜53)。
    if kind != cli_runner::CliKind::Claude {
        return vec![];
    }
    let mut lines = vec![format!(
        "認証: API キー {} / ANTHROPIC_AUTH_TOKEN {} / base_url {} / 子に渡さない変数 {} 個",
        if a.api_key_present { format!("あり (len {}, fp {})", a.api_key_len, a.api_key_fingerprint) } else { "なし (OAuth ログインを使用)".into() },
        if a.auth_token_present { "あり" } else { "なし" },
        if a.base_url.is_empty() { "既定".to_string() } else { a.base_url.clone() },
        a.scrubbed.len()
    )];
    if oauth_only {
        lines.push("OAuth 優先: ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN は子に渡しません".into());
    }
    lines
}

/// `--version` の 1 行目 (契約 `CliKindCheck`)。取れなければ None — **取れないことを食い違いの証拠にしない**。
async fn cli_version(executable: &str) -> Option<String> {
    let fut = cli_runner::no_window::tokio_command(executable).arg("--version").output();
    match tokio::time::timeout(std::time::Duration::from_secs(20), fut).await {
        Ok(Ok(out)) => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            text.lines().next().map(str::to_string).filter(|l| !l.trim().is_empty())
        }
        _ => None,
    }
}

/// 費用の表示 (純粋)。**記録が無ければ 0 と書かない** — 費用を返さない CLI がある (rev42、agy 実測)。
fn cost_text(cost: Option<f64>) -> String {
    match cost {
        Some(c) => format!("{c:.3} USD"),
        None => "費用の記録なし".to_string(),
    }
}

/// 種類と実体が食い違っている時に出す 1 行 (純粋。PoC あり)。
fn kind_mismatch_message(kind: cli_runner::CliKind, executable: &str, version: &str) -> String {
    format!(
        "種類は {kind:?} ですが、{executable} は \"{version}\" と名乗りました — 別系統の引数が飛びます。設定の「CLI の種類」を実体に合わせてください"
    )
}

#[tauri::command]
async fn check_cli(executable: String, kind: Option<cli_runner::CliKind>) -> CliCheck {
    let mut auth = auth_view_env();
    // rev56: 配布ビルドの Windows でコンソールの窓を出さない (起動は必ず cli_runner::no_window を通す)。
    let fut = cli_runner::no_window::tokio_command(&executable).arg("--version").output();
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
        let mut cmd = cli_runner::no_window::tokio_command(&executable);
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
    if let Some(k) = kind {
        check.kind_mismatch = cli_runner::kind_mismatches_version(k, &check.version);
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
    /// 面の貼り方 (rev5)。省略時は frontal (rev22)。
    #[serde(default)]
    plate_mode: PlateMode,
}

#[derive(Serialize)]
struct StageInfo {
    attempts: usize,
    /// **None = 記録なし** (費用を返さない CLI がある。rev42)。0 で埋めない。
    #[serde(default)]
    cost_usd: Option<f64>,
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
    for line in auth_log_lines(req.cli.kind, req.cli.oauth_only, &auth_view_env()) {
        emit(app, "cli", line);
    }
    // 種類と実体の食い違いは**走らせる前に止める** (契約 `CliKindCheck`)。
    // rev40 は設定画面にだけ警告を出したが、実行はメイン画面からするので誰も見なかった
    // (同じ事故が 3 回続いた)。名乗りが取れない時は黙って進む — 取れないことは食い違いの証拠ではない。
    if let Some(version) = cli_version(&req.cli.executable).await {
        if cli_runner::kind_mismatches_version(req.cli.kind, &version) {
            let msg = kind_mismatch_message(req.cli.kind, &req.cli.executable, &version);
            emit(app, "cli", msg.clone());
            return Err(msg);
        }
    }

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
        // スナップショットの置き場も読ませる (rev42)。見つけられないと agy はシェルに逃げる。
        extra_read_dirs: pipeline::task::snapshot_dirs(&snaps),
        cancel,
        on_event: Box::new(move |e| match e {
            CliEvent::Started { pid } => emit(&h, "cli", format!("CLI 起動 (pid {pid})")),
            CliEvent::Stdout { text } => emit(&h, "cli", text),
            CliEvent::Stderr { text } => emit(&h, "cli-stderr", text),
            CliEvent::Progress { text } => emit(&h, "cli", text),
            CliEvent::Structured { .. } => emit(&h, "cli", "構造化出力を受信"),
        }),
        env_remove: pipeline::task::env_remove_for(req.cli.kind, req.cli.oauth_only),
    };

    emit(app, "analyze", "解析中 (タスク 1/2)…");
    let (summary, r1) = analyze(&runner, &brief_text, &req.concept, req.language).await.map_err(|e| e.to_string())?;
    emit(app, "analyze", format!("解析 完了: {} / {:.1} s", cost_text(r1.cost_usd), r1.duration_ms as f64 / 1000.0));

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
        format!("シーン構成 完了: {} 回目で通過 / {} / {:.1} s", r2.attempts, cost_text(r2.cost_usd), r2.duration_ms as f64 / 1000.0),
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
        // rev25: 組み立ては pipeline::stages::run_stats 1 箇所 (CLI の `promo run` と同じ関数)。
        run_stats: Some(pipeline::stages::run_stats(&r1, &r2)),
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
            cost_usd: pipeline::stages::add_cost(r1.cost_usd, r2.cost_usd),
            image_provider: None,
            image_count: 0,
            plan_attempts: r2.attempts,
            violation_kinds: promo
                .run_stats
                .as_ref()
                .map(|s| s.violation_kinds.clone())
                .unwrap_or_default(),
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
    /// 面の貼り方 (rev5)。省略時は frontal (rev22)。
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
    write_run_files(&pkg_dir, &promo)?;
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

/// シーンのプロンプトを書き換える (rev37、契約 `ScenePromptEdit`)。
///
/// 書き換えは `promo_core::plan::set_scene_prompts` だけが行う (空の拒否もそこ)。
/// **promo.json と scenes.md の両方を書き直し**、書き換えた後の `promo` を返す —
/// frontend が手元で真似ると食い違う (reburn_caption と同じ作法)。
#[tauri::command]
fn update_scene_prompts(run_dir: String, scene_id: u32, motion_prompt: String, video_prompt: String) -> Result<PromoJson, String> {
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let mut promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
    promo_core::plan::set_scene_prompts(&mut promo.plan, scene_id, &motion_prompt, &video_prompt).map_err(|e| match e {
        promo_core::plan::PromptEditError::SceneNotFound(id) => format!("シーン {id} が見つかりません"),
        promo_core::plan::PromptEditError::EmptyMotion => "motion prompt が空です".to_string(),
        promo_core::plan::PromptEditError::EmptyVideo => "video prompt が空です".to_string(),
    })?;
    write_run_files(&dir, &promo)?;
    Ok(promo)
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

) -> Result<Reburned, String> {
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
    let tilt = pipeline::reference::tilt_of(promo.plate_mode, scene);
    // 面の判定に要るぶんだけ写す (promo を可変で借りたままにしないため)。
    let scene_for_plate = scene.clone();

    let name = promo_core::export::reference_image_name(scene_id, 1);
    let base = pipeline::reference::base_image_path(&dir, &name);
    let png = std::fs::read(&base).map_err(|e| {
        format!("素材がありません ({}): {e}。rev10 より前に作った run は焼き直せません", base.display())
    })?;

    // rev10: base/ は**素材** (product は背景 / mood は絵)。合成からやり直すので、見出しの位置を
    // 変えても帯が正しく取り直され、傾きもそのまま乗る。
    // rev24: 貼るかどうかは `plate_snapshot_index` だけが決める (mood も人が足せば貼る)。
    let canvas = pipeline::reference::image_dims(&png)?;
    let composed = match pipeline::reference::plate_snapshot_index(&scene_for_plate, plate.as_ref()) {
        None => png,
        Some(idx) => {
            let shot = read_run_snapshot(&dir, idx)?;
            let l = pipeline::reference::layout_for(canvas, spec.as_ref().map(|s| s.effective_position()), tilt, plate.as_ref());
            image_gen::composite_product_cut(&png, &shot, &l)?
        }
    };
    let out = match &spec {
        None => composed,
        Some(sp) => {
            let font = std::fs::read(&sp.font_path).map_err(|e| format!("フォントを読めません {}: {e}", sp.font_path))?;
            // rev17: 組み立ては CaptionSpec::to_caption 1 箇所。手で組むと足したフィールドが片方に落ちる
            // (rev14 の y_ratio がまさにそれで、縦位置スライダーが焼き直しに届いていなかった)。
            image_gen::burn_caption(&composed, &sp.to_caption(&copy_text, &font))?
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
                    y_ratio: sp.y_ratio,
                },
            );
        }
    }
    // rev54: scenes.md も**毎回**揃える。以前はコピー文を変えた時だけで、見出し・はめ込みだけ触った run では
    // scenes.md が作った時のまま残っていた (はめ込みで選び直した番号は表にも出る)。
    write_run_files(&dir, &promo)?;
    Ok(Reburned { url: image_gen::provider::data_url("image/png", &out), promo })
}

/// 焼き直しの戻り (rev21)。**書き換えた後の `promo` も返す** — 上書きを書くのは backend なので、
/// frontend が手元で真似ると食い違う。開き直したときにつまみが実効値を指すのはこの値が根拠。
#[derive(Serialize)]
struct Reburned {
    url: String,
    promo: PromoJson,
}

#[derive(Serialize)]
struct PlatePreview {
    canvas: [u32; 2],
    /// 面の予定位置。左上・右上・右下・左下 (canvas 座標)。傾けると台形になる。
    /// **面が無いシーンでは `None`** (判定は `plate_snapshot_index` — rev24 から mood にも面が乗りうる)。
    quad: Option<[[f32; 2]; 4]>,
    /// 見出しの予定位置 (rev18)。1 行 1 個の `[x, y, w, h]` (canvas 座標)。
    /// 焼き込みと同じ `caption_layout` から出すので、数式の写しを TS 側に持たない。
    caption_lines: Vec<[f32; 4]>,
    /// **いま効いている傾き** `[yaw, pitch]` (rev21)。合成が使う `tilt_of` がそのまま出す。
    /// `PlateMode::Frontal` や `plate_tilt` 無しでは `[0, 0]` = 本当に正面。
    /// スライダーの基準はここ — 0 に置くと、絵が傾いているのにつまみが 0° を指す嘘になる。
    tilt: [f32; 2],
}

/// フォントは 1 ファイルが数 MB〜数十 MB ある。プレビューは打鍵・スライダーのたびに走るので、
/// **直前に読んだものだけ**持ち回す (同じフォントを触り続ける操作が大半)。
type CachedFont = Mutex<Option<(String, std::sync::Arc<Vec<u8>>)>>;
static LAST_FONT: std::sync::OnceLock<CachedFont> = std::sync::OnceLock::new();

async fn font_bytes(path: &str) -> Result<std::sync::Arc<Vec<u8>>, String> {
    let cell = LAST_FONT.get_or_init(|| Mutex::new(None));
    {
        let g = cell.lock().await;
        if let Some((p, bytes)) = g.as_ref() {
            if p == path {
                return Ok(bytes.clone());
            }
        }
    }
    let bytes = std::sync::Arc::new(std::fs::read(path).map_err(|e| format!("フォントを読めません {path}: {e}"))?);
    *cell.lock().await = Some((path.to_string(), bytes.clone()));
    Ok(bytes)
}

/// 見出しと面の**予定位置**を返す (rev14 → rev18)。**画像は作らないので速い** —
/// 素材とスクショのヘッダから寸法を読み、フォントの幅送りを測るだけ。
///
/// **合成と同じ `layout_for` + `plate_quad` + `caption_layout` を通す。**
/// TS 側に射影や版組みの数式を写すと必ず食い違う。
#[tauri::command]
async fn plate_preview(
    run_dir: String,
    scene_id: u32,
    spec: Option<CaptionSpec>,
    plate: Option<promo_core::export::PlateOverride>,
    // copy: 編集中のコピー文 (rev18)。**適用前の textarea の中身**なので promo.json ではなくこちらを見る。
    copy: Option<String>,
) -> Result<PlatePreview, String> {
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
    let scene = promo
        .plan
        .scenes
        .iter()
        .find(|s| s.scene_id == scene_id)
        .ok_or_else(|| format!("scene {scene_id} がありません"))?;

    let name = promo_core::export::reference_image_name(scene_id, 1);
    let base = std::fs::read(pipeline::reference::base_image_path(&dir, &name)).map_err(|e| format!("素材がありません: {e}"))?;
    let canvas = pipeline::reference::image_dims(&base)?;

    // 面があるかは `plate_snapshot_index` が決める — 焼き込みと同じ関数を通す (rev24)。
    // 枠が合成と違う式を持つと枠が嘘をつく (#17 / rev18 と同じ作法)。
    let tilt = pipeline::reference::tilt_of(promo.plate_mode, scene);
    let quad = match pipeline::reference::plate_snapshot_index(scene, plate.as_ref()) {
        None => None,
        Some(idx) => {
            let shot_dims = pipeline::reference::image_dims(&read_run_snapshot(&dir, idx)?)?;
            let l = pipeline::reference::layout_for(canvas, spec.as_ref().map(|s| s.effective_position()), tilt, plate.as_ref());
            let q = image_gen::compose::plate_quad(&l, shot_dims);
            Some([[q[0].0, q[0].1], [q[1].0, q[1].1], [q[2].0, q[2].1], [q[3].0, q[3].1]])
        }
    };

    // 見出しの予定位置。フォントが読めないだけなら枠を出さずに続ける (操作は妨げない)。
    let copy_text = copy.unwrap_or_else(|| scene.copy_text.clone());
    let mut caption_lines = vec![];
    if let Some(sp) = &spec {
        if !copy_text.trim().is_empty() {
            if let Ok(font) = font_bytes(&sp.font_path).await {
                let cap = sp.to_caption(&copy_text, &font);
                if let Ok(Some(l)) = image_gen::caption_layout(canvas.0, canvas.1, &cap) {
                    caption_lines = l.lines.iter().map(|ln| [ln.x, ln.top, ln.width, l.line_h]).collect();
                }
            }
        }
    }

    Ok(PlatePreview {
        canvas: [canvas.0, canvas.1],
        quad,
        caption_lines,
        tilt: tilt.map(|t| [t.yaw_degrees, t.pitch_degrees]).unwrap_or([0.0, 0.0]),
    })
}

#[derive(Serialize)]
struct AddedSnapshot {
    /// 足した画像の番号 (`PlateOverride.snapshot_index` にそのまま渡せる)。
    index: usize,
    /// 更新後の一覧 (promo.json と同じ並び)。
    snapshot_paths: Vec<String>,
}

/// **既存の run にスナップショットを足す** (rev19)。
///
/// 動機: コピー文に合う画面が run の中に無いことがある。そのとき「撮り直して貼る」で済ませたい
/// (ユーザー判断 2026-09-09)。生成はやり直さない — 足した画像は焼き直しの材料になるだけ。
///
/// **契約**: `snapshot_paths[i]` ↔ `snapshots/snapshot_{i+1:02}.*`。新しい番号は**一覧の長さ**で決める
/// (ファイルを数えない — 数えると欠番や孤児で番号がずれ、`snapshot_index` が別の画像を指す)。
/// 同じ番号の孤児ファイルが居たら**拡張子を問わず先に消す**。残すと `read_run_snapshot` の
/// 前方一致がどちらを拾うか決まらなくなる。
#[tauri::command]
fn add_run_snapshot(run_dir: String, path: String) -> Result<AddedSnapshot, String> {
    let src = PathBuf::from(&path);
    // 壊れた画像を run に入れない (入力ペインと同じヘッダ検証)。
    snapshot_meta(&src)?;

    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let mut promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;

    let index = promo.snapshot_paths.len();
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let shots = dir.join("snapshots");
    std::fs::create_dir_all(&shots).map_err(|e| format!("snapshots フォルダを作れません: {e}"))?;

    // 同じ番号の孤児を掃除してから置く。
    let prefix = promo_core::snapshot_prefix(index);
    if let Ok(entries) = std::fs::read_dir(&shots) {
        for e in entries.filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let dst = shots.join(promo_core::snapshot_file_name(index, &ext));
    std::fs::copy(&src, &dst).map_err(|e| format!("スナップショットを写せません {}: {e}", src.display()))?;

    // 一覧に足すのは**元のパス**。run の中の写しが正本で、これは由来の記録
    // (元が動いても run は壊れない — rev10 で read_run_snapshot が写しを読むようにしてある)。
    promo.snapshot_paths.push(path);
    // rev54: scenes.md も揃える (以前は promo.json だけを書いていた)。
    write_run_files(&dir, &promo)?;
    Ok(AddedSnapshot { index, snapshot_paths: promo.snapshot_paths })
}

/// **撮り直しを見つける** (rev23)。返すのは `promo.snapshot_paths` のうち、写した後で元の
/// ファイルの中身が変わったもの。一覧の重複排除から外す材料で、run 自体は書き換えない。
///
/// 呼ぶのは編集ダイアログを開いた時 — 撮り直しはアプリの外で起きるので、見る直前に数えるしかない。
#[tauri::command]
fn stale_snapshots(run_dir: String) -> Result<Vec<String>, String> {
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;
    Ok(stale_run_snapshots(&dir, &promo.snapshot_paths))
}

/// run に写した後で**元のファイルの中身が変わった**元のパスを返す (rev23、撮り直し)。
///
/// 判定は「今の元ファイルのバイト列が、**同じパスで写したどの写しとも一致しない**」。
/// パスは 2 度足せる (data_contract `RunSnapshots.add.no_dedup`) ので、照合は index 単位ではなく
/// 同じパスの写し全部に対して行う — 撮り直しを足した後は一致するものが現れて済んだ状態になる。
///
/// **黙るのは 2 つの場合**: 元ファイルが読めない (消された / 移された) 時と、写しが無い時。
/// どちらも run は自己完結していて焼き直せる (rev10)、または別のエラーで既に報せている。
/// ここで報せるのは「新しい画像が選べないまま埋もれている」という一点だけ。
fn stale_run_snapshots(run_dir: &Path, snapshot_paths: &[String]) -> Vec<String> {
    let mut seen: Vec<&str> = Vec::new();
    let mut stale = Vec::new();
    for path in snapshot_paths {
        if seen.contains(&path.as_str()) {
            continue;
        }
        seen.push(path.as_str());
        let Ok(current) = std::fs::read(path) else { continue };
        let copies = snapshot_paths.iter().enumerate().filter(|(_, p)| p.as_str() == path.as_str());
        let fresh = copies.clone().any(|(i, _)| {
            // 先に大きさで弾く (撮り直しはたいてい長さが違う)。同じ長さの時だけ読んで照合する。
            run_snapshot_file(run_dir, i)
                .and_then(|f| std::fs::metadata(&f).ok().map(|m| (f, m.len())))
                .is_some_and(|(f, len)| len == current.len() as u64 && std::fs::read(&f).is_ok_and(|c| c == current))
        });
        // 写しが 1 つも見つからない run では黙る (焼き直せない旨は read_run_snapshot が別に出す)。
        let has_copy = copies.clone().any(|(i, _)| run_snapshot_file(run_dir, i).is_some());
        if has_copy && !fresh {
            stale.push(path.clone());
        }
    }
    stale
}

/// `snapshots/snapshot_{idx+1:02}.*` の実体。**前方一致はここだけ** (拡張子は元のファイル次第)。
fn run_snapshot_file(run_dir: &Path, idx: usize) -> Option<PathBuf> {
    let want = promo_core::snapshot_prefix(idx);
    std::fs::read_dir(run_dir.join("snapshots"))
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| e.file_name().to_string_lossy().starts_with(&want))
        .map(|e| e.path())
}

/// run に写したスナップショット (rev10)。**元のパスではなく run の写しを読む** — 元は移動されうるし、
/// run は自己完結しているべきなので。
fn read_run_snapshot(run_dir: &Path, idx: usize) -> Result<Vec<u8>, String> {
    let want = promo_core::snapshot_prefix(idx);
    let file = run_snapshot_file(run_dir, idx)
        .ok_or_else(|| format!("snapshots/{want}* がありません (この run は焼き直せません)"))?;
    std::fs::read(file).map_err(|e| format!("スナップショットを読めません: {e}"))
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
    let r = cli_runner::no_window::std_command("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let r = cli_runner::no_window::std_command("open").arg(p).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = cli_runner::no_window::std_command("xdg-open").arg(p).spawn();
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
            update_scene_prompts,
            edit_scenes,
            forget_run,
            reburn_caption,
            plate_preview,
            add_run_snapshot,
            stale_snapshots,
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
mod auth_log_tests {
    use super::{AuthView, auth_log_lines};
    use cli_runner::CliKind;

    fn view() -> AuthView {
        AuthView {
            api_key_present: true,
            api_key_len: 108,
            api_key_fingerprint: "0123456789ab".into(),
            auth_token_present: false,
            base_url: String::new(),
            oauth_logged_in: None,
            oauth_method: String::new(),
            scrubbed: vec![],
        }
    }

    /// rev52 (ユーザー 2026-09-14): agy では鍵を常に外すので、「API キー あり」「OAuth 優先」は嘘になる。
    /// 「agy には … を渡しません」も出さない — agy なのに Anthropic の話を出すのは不自然 (同日ユーザー判断)。
    #[test]
    fn agy_log_does_not_talk_about_anthropic_auth() {
        for oauth_only in [false, true] {
            assert!(auth_log_lines(CliKind::Agy, oauth_only, &view()).is_empty());
        }
    }

    /// rev53 (ユーザー決定 2026-09-14): aider / custom でも Anthropic の話を出さない。鍵は引き継ぐが、スイッチを持つのは claude だけ。
    #[test]
    fn aider_and_custom_log_nothing_about_anthropic_auth() {
        for k in [CliKind::Aider, CliKind::Custom] {
            for oauth_only in [false, true] {
                assert!(auth_log_lines(k, oauth_only, &view()).is_empty(), "{k:?}");
            }
        }
    }

    #[test]
    fn claude_log_keeps_key_and_oauth_lines() {
        let lines = auth_log_lines(CliKind::Claude, true, &view());
        assert!(lines[0].contains("API キー あり (len 108, fp 0123456789ab)"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("OAuth 優先")), "{lines:?}");
        assert_eq!(auth_log_lines(CliKind::Claude, false, &view()).len(), 1);
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

#[cfg(test)]
mod run_files_tests {
    use super::{PromoJson, add_run_snapshot, reburn_caption};
    use promo_core::export::scenes_markdown;
    use promo_core::plan::{AnalyzedSummary, Aspect, CutKind, Scene, ScenePlan, VisualIdentity};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// mood 1 シーンの run。素材 (base/) だけ本物の PNG を置き、scenes.md はまだ無い。
    fn run_dir() -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("apppromo_run_files_{}_{}", std::process::id(), n));
        fs::create_dir_all(d.join("base")).unwrap();
        let promo = PromoJson {
            project_path: "D:/proj".into(),
            snapshot_paths: vec![],
            video_concept: "calm".into(),
            caption_overrides: Default::default(),
            plate_overrides: Default::default(),
            original_copy: Default::default(),
            plate_mode: Default::default(),
            run_stats: None,
            summary: AnalyzedSummary {
                app_name: "Task Flow".into(),
                one_liner: "o".into(),
                core_value: "c".into(),
                target_audience: "t".into(),
                differentiators: vec![],
                hook_copy: "h".into(),
                visual_identity: VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] },
            },
            plan: ScenePlan {
                total_seconds: 5,
                aspect: Aspect::Landscape,
                scenes: vec![Scene {
                    scene_id: 1,
                    cut_kind: CutKind::Mood,
                    snapshot_index: None,
                    plate_tilt: None,
                    motion_prompt: "m".into(),
                    duration_seconds: 5,
                    shot_type: "Wide".into(),
                    video_prompt: "v".into(),
                    copy_text: "コピー".into(),
                    image_prompt: "i".into(),
                    reference_image: Some("scene_01_ref_01.png".into()),
                }],
            },
        };
        fs::write(d.join("promo.json"), serde_json::to_string_pretty(&promo).unwrap()).unwrap();
        fs::write(d.join("base").join("scene_01_ref_01.png"), image_gen::solid_backdrop(8, 8, [10, 20, 30])).unwrap();
        d
    }

    /// scenes.md が書かれていて、保存された promo.json から作ったものと一致する。
    fn assert_scenes_md_matches(dir: &Path) {
        let saved: PromoJson = serde_json::from_str(&fs::read_to_string(dir.join("promo.json")).unwrap()).unwrap();
        let md = fs::read_to_string(dir.join("scenes.md")).expect("scenes.md が書かれていない");
        assert_eq!(md, scenes_markdown(&saved));
    }

    /// rev54 (2026-09-14 実データ): 焼き直しはコピー文を変えた時しか scenes.md を書かず、
    /// 見出し・はめ込みだけ触った run では scenes.md が作った時のまま残っていた。
    #[test]
    fn reburn_without_copy_change_still_writes_scenes_md() {
        let dir = run_dir();
        reburn_caption(dir.to_string_lossy().to_string(), 1, None, None, None).unwrap();
        assert_scenes_md_matches(&dir);
    }

    /// rev54: スナップショットの追加も promo.json だけを書いていた。
    #[test]
    fn add_run_snapshot_writes_scenes_md() {
        let dir = run_dir();
        let shot = dir.join("shot.png");
        fs::write(&shot, image_gen::solid_backdrop(8, 8, [1, 2, 3])).unwrap();
        add_run_snapshot(dir.to_string_lossy().to_string(), shot.to_string_lossy().to_string()).unwrap();
        assert_scenes_md_matches(&dir);
    }
}

#[cfg(test)]
mod prompt_edit_tests {
    use super::{update_scene_prompts, PromoJson};
    use promo_core::plan::{AnalyzedSummary, Aspect, CutKind, Scene, ScenePlan, VisualIdentity};
    use std::fs;
    use std::path::PathBuf;

    fn scene(id: u32) -> Scene {
        Scene {
            scene_id: id,
            cut_kind: CutKind::Mood,
            snapshot_index: None,
            plate_tilt: None,
            motion_prompt: format!("Slow push-in {id}."),
            duration_seconds: 5,
            shot_type: "Wide".into(),
            video_prompt: format!("Video prompt {id}."),
            copy_text: "コピー".into(),
            image_prompt: "i".into(),
            reference_image: None,
        }
    }

    /// promo.json だけを置いた run (scenes.md はまだ無い)。
    fn run_dir() -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("apppromo_prompt_{}_{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        let promo = PromoJson {
            project_path: "D:/proj".into(),
            snapshot_paths: vec![],
            video_concept: "calm".into(),
            caption_overrides: Default::default(),
            plate_overrides: Default::default(),
            original_copy: Default::default(),
            plate_mode: Default::default(),
            run_stats: None,
            summary: AnalyzedSummary {
                app_name: "Task Flow".into(),
                one_liner: "o".into(),
                core_value: "c".into(),
                target_audience: "t".into(),
                differentiators: vec![],
                hook_copy: "h".into(),
                visual_identity: VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] },
            },
            plan: ScenePlan { total_seconds: 10, aspect: Aspect::Landscape, scenes: vec![scene(1), scene(2)] },
        };
        fs::write(d.join("promo.json"), serde_json::to_string_pretty(&promo).unwrap()).unwrap();
        d
    }

    /// 契約 `ScenePromptEdit.files`: promo.json を書き直したら **scenes.md も揃える**。
    #[test]
    fn saving_rewrites_promo_json_and_scenes_md() {
        let dir = run_dir();
        let got = update_scene_prompts(dir.to_string_lossy().to_string(), 2, "  New motion.  ".into(), "New video prompt.".into()).unwrap();
        assert_eq!(got.plan.scenes[1].motion_prompt, "New motion.");
        let saved: PromoJson = serde_json::from_str(&fs::read_to_string(dir.join("promo.json")).unwrap()).unwrap();
        assert_eq!(saved.plan.scenes[1].video_prompt, "New video prompt.");
        let md = fs::read_to_string(dir.join("scenes.md")).expect("scenes.md が書かれていない");
        assert!(md.contains("New video prompt."), "scenes.md に新しい文が無い:\n{md}");
    }

    /// 空は拒む。**拒んだ時はファイルを触らない** (契約 `ScenePromptEdit.empty`)。
    #[test]
    fn an_empty_prompt_is_rejected_and_nothing_is_written() {
        let dir = run_dir();
        let before = fs::read_to_string(dir.join("promo.json")).unwrap();
        let err = update_scene_prompts(dir.to_string_lossy().to_string(), 1, "   ".into(), "v".into()).unwrap_err();
        assert!(err.contains("motion"), "{err}");
        assert_eq!(fs::read_to_string(dir.join("promo.json")).unwrap(), before);
        assert!(!dir.join("scenes.md").exists(), "拒んだのに scenes.md を書いている");
    }
}

#[cfg(test)]
mod kind_preflight_tests {
    use super::kind_mismatch_message;
    use cli_runner::{CliKind, kind_mismatches_version};

    /// 走らせる前に止める判定 (契約 `CliKindCheck`)。実測の名乗りに接地する。
    #[test]
    fn the_preflight_stops_only_on_a_real_mismatch() {
        // 実測: claude 2.1.263 / agy 1.2.2。
        assert!(kind_mismatches_version(CliKind::Claude, "1.2.2"), "今日の事故の形");
        assert!(!kind_mismatches_version(CliKind::Claude, "2.1.263 (Claude Code)"));
        assert!(!kind_mismatches_version(CliKind::Agy, "1.2.2"));
    }

    /// 文言は**何をどう直せばよいか**まで書く (ログに 1 行しか出ないため)。
    #[test]
    fn the_message_names_the_kind_the_executable_and_the_version() {
        let m = kind_mismatch_message(CliKind::Claude, "agy", "1.2.2");
        assert!(m.contains("Claude"), "{m}");
        assert!(m.contains("agy"), "{m}");
        assert!(m.contains("1.2.2"), "{m}");
        assert!(m.contains("CLI の種類"), "直し方が書かれていない: {m}");
    }
}

#[cfg(test)]
mod stale_snapshot_tests {
    use super::stale_run_snapshots;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tmp() -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("apppromo_stale_{}_{}", std::process::id(), n));
        fs::create_dir_all(d.join("snapshots")).unwrap();
        d
    }

    /// `snapshot_paths[i]` ↔ `snapshots/snapshot_{i+1:02}.*` の写しを置く。
    fn put_copy(run: &Path, index: usize, bytes: &[u8]) {
        let name = promo_core::snapshot_file_name(index, "png");
        fs::write(run.join("snapshots").join(name), bytes).unwrap();
    }

    #[test]
    fn a_reshoot_at_the_same_path_is_stale() {
        // data_contract RunSnapshots.add.no_dedup: 撮り直しは同じパスで中身が変わる。
        let run = tmp();
        let a = run.join("a.png");
        let b = run.join("b.png");
        fs::write(&a, b"AAA").unwrap();
        fs::write(&b, b"BBB").unwrap();
        put_copy(&run, 0, b"AAA");
        put_copy(&run, 1, b"BBB");
        let paths = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
        // まだ誰も撮り直していない。
        assert!(stale_run_snapshots(&run, &paths).is_empty());
        // b.png だけ撮り直した (パスは同じ、中身が変わった)。
        fs::write(&b, b"ZZZZ").unwrap();
        assert_eq!(stale_run_snapshots(&run, &paths), vec![b.to_string_lossy().to_string()]);
    }

    #[test]
    fn a_reshoot_of_the_same_length_is_still_stale() {
        // 長さで弾く近道だけでは落ちない場合。ここが**バイト照合そのもの**を固定する 1 本。
        let run = tmp();
        let a = run.join("a.png");
        fs::write(&a, b"AAA").unwrap();
        put_copy(&run, 0, b"BBB");
        let paths = vec![a.to_string_lossy().to_string()];
        assert_eq!(stale_run_snapshots(&run, &paths), paths);
    }

    #[test]
    fn a_missing_source_is_not_stale() {
        // run は自己完結する (rev10)。元が消えても焼き直せるので、撮り直し扱いにはしない。
        let run = tmp();
        let a = run.join("gone.png");
        put_copy(&run, 0, b"AAA");
        let paths = vec![a.to_string_lossy().to_string()];
        assert!(stale_run_snapshots(&run, &paths).is_empty());
    }

    #[test]
    fn the_same_path_added_twice_is_fresh_once_the_new_bytes_are_in() {
        // 撮り直しを足すと同じパスが 2 度並ぶ (no_dedup)。今の中身がどれかの写しと一致すれば済んでいる。
        let run = tmp();
        let a = run.join("a.png");
        fs::write(&a, b"NEW").unwrap();
        put_copy(&run, 0, b"OLD");
        put_copy(&run, 1, b"NEW");
        let p = a.to_string_lossy().to_string();
        assert!(stale_run_snapshots(&run, &[p.clone(), p]).is_empty());
    }

    #[test]
    fn a_run_without_the_copy_is_not_reported() {
        // 写しが無い run は焼き直せない (read_run_snapshot が別に Err を出す)。ここでは黙る。
        let run = tmp();
        let a = run.join("a.png");
        fs::write(&a, b"AAA").unwrap();
        let paths = vec![a.to_string_lossy().to_string()];
        assert!(stale_run_snapshots(&run, &paths).is_empty());
    }
}

/// シーンの並び替え / 複製 / 削除 (rev43、契約 `SceneEdit`)。
///
/// 書き換えは `promo_core::scene_edit::apply_scene_edit` だけが行う (拒否もそこ)。
/// **ファイルの付け替えは `SceneRemap` の手順だけを見て行う** — 一時名を必ず経由するので
/// 1↔2 の入れ替えでも潰れない。promo.json と scenes.md を書き直し、書き換え後の promo を返す。
#[tauri::command]
fn edit_scenes(run_dir: String, op: String, scene_id: u32) -> Result<PromoJson, String> {
    use promo_core::scene_edit::{SceneEditError, SceneOp, apply_scene_edit};

    let sop = match op.as_str() {
        "move_up" => SceneOp::MoveUp,
        "move_down" => SceneOp::MoveDown,
        "duplicate" => SceneOp::Duplicate,
        "remove" => SceneOp::Remove,
        other => return Err(format!("知らない操作です: {other}")),
    };
    let dir = PathBuf::from(&run_dir);
    let text = std::fs::read_to_string(dir.join("promo.json")).map_err(|e| format!("promo.json を読めません: {e}"))?;
    let mut promo: PromoJson = serde_json::from_str(&text).map_err(|e| format!("promo.json の形が違います: {e}"))?;

    let remap = apply_scene_edit(&mut promo, sop, scene_id).map_err(|e| match e {
        SceneEditError::UnknownScene { scene_id } => format!("シーン {scene_id} が見つかりません"),
        other => other.to_string(),
    })?;

    // --- ファイルの付け替え。**run 直下と base/ の両方**、参照画像の番号ぶん。 ---
    for root in [dir.clone(), dir.join("base")] {
        if !root.is_dir() {
            continue;
        }
        if let Some(gone) = remap.removed {
            for i in 1..=MAX_REF_PER_SCENE {
                let p = root.join(promo_core::export::reference_image_name(gone, i));
                if p.is_file() {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        for i in 1..=MAX_REF_PER_SCENE {
            for (from, to) in remap.rename_steps_for(i) {
                let (a, b) = (root.join(&from), root.join(&to));
                if a.is_file() {
                    let _ = std::fs::rename(&a, &b);
                }
            }
        }
    }

    write_run_files(&dir, &promo)?;
    Ok(promo)
}

/// 1 シーンあたりの参照画像の最大枚数 (付け替えで走査する範囲)。
const MAX_REF_PER_SCENE: u32 = 8;

#[cfg(test)]
mod scene_edit_io_tests {
    use super::edit_scenes;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn tmp() -> PathBuf {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("apppromo_scene_edit_{}_{}", std::process::id(), n));
        fs::create_dir_all(d.join("base")).unwrap();
        d
    }

    /// promo.json と 1 枚ずつの参照画像 (run 直下と base/) を置く。中身はシーン番号そのもの。
    fn seed(dir: &Path, promo: &promo_core::PromoJson) {
        fs::write(dir.join("promo.json"), serde_json::to_string_pretty(promo).unwrap()).unwrap();
        for s in &promo.plan.scenes {
            let name = promo_core::export::reference_image_name(s.scene_id, 1);
            fs::write(dir.join(&name), format!("image of {}", s.scene_id)).unwrap();
            fs::write(dir.join("base").join(&name), format!("base of {}", s.scene_id)).unwrap();
        }
    }

    fn read(dir: &Path, scene_id: u32) -> Option<String> {
        fs::read_to_string(dir.join(promo_core::export::reference_image_name(scene_id, 1))).ok()
    }

    fn promo_of(n: u32) -> promo_core::PromoJson {
        let text = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/promo_4.json"))
            .unwrap_or_default();
        let mut p: promo_core::PromoJson = serde_json::from_str(&text).expect("fixture promo_4.json");
        p.plan.scenes.truncate(n as usize);
        p
    }

    /// **入れ替えても画像が潰れない** (一時名を経由しているか)。run 直下と base/ の両方。
    #[test]
    fn swapping_two_scenes_swaps_their_images_without_losing_one() {
        let dir = tmp();
        let p = promo_of(4);
        seed(&dir, &p);

        edit_scenes(dir.to_string_lossy().to_string(), "move_up".into(), 2).unwrap();

        assert_eq!(read(&dir, 1).as_deref(), Some("image of 2"), "2 の画像が 1 番へ");
        assert_eq!(read(&dir, 2).as_deref(), Some("image of 1"), "1 の画像が 2 番へ — どちらも潰れていない");
        assert_eq!(read(&dir, 3).as_deref(), Some("image of 3"), "動かないシーンはそのまま");
        let base = fs::read_to_string(dir.join("base").join("scene_01_ref_01.png")).unwrap();
        assert_eq!(base, "base of 2", "base/ も同じ付け替え");
        assert!(!dir.join("scene_01_ref_01.png.tmp").exists(), "一時ファイルを残さない");
    }

    /// 削除は消えたシーンの画像を消し、後ろを詰める。
    #[test]
    fn removing_a_scene_deletes_its_image_and_shifts_the_rest() {
        let dir = tmp();
        seed(&dir, &promo_of(4));

        let promo = edit_scenes(dir.to_string_lossy().to_string(), "remove".into(), 2).unwrap();

        assert_eq!(promo.plan.scenes.len(), 3);
        assert_eq!(read(&dir, 2).as_deref(), Some("image of 3"), "3 の画像が 2 番へ");
        assert_eq!(read(&dir, 3).as_deref(), Some("image of 4"));
        assert!(read(&dir, 4).is_none(), "余りを残さない");
    }

    /// 複製したシーンには**画像が無い** (生成し直すまで出ない)。
    #[test]
    fn a_duplicated_scene_has_no_image_yet() {
        let dir = tmp();
        seed(&dir, &promo_of(3));

        edit_scenes(dir.to_string_lossy().to_string(), "duplicate".into(), 1).unwrap();

        assert_eq!(read(&dir, 1).as_deref(), Some("image of 1"));
        assert!(read(&dir, 2).is_none(), "複製したシーンの画像はまだ無い");
        assert_eq!(read(&dir, 3).as_deref(), Some("image of 2"), "元の 2 は 3 番へ");
    }

    /// 拒まれた時は **promo.json もファイルも触らない**。
    #[test]
    fn a_refused_edit_changes_nothing_on_disk() {
        let dir = tmp();
        seed(&dir, &promo_of(3));
        let before = fs::read_to_string(dir.join("promo.json")).unwrap();

        let err = edit_scenes(dir.to_string_lossy().to_string(), "remove".into(), 1).unwrap_err();

        assert!(err.contains("減らせません"), "{err}");
        assert_eq!(fs::read_to_string(dir.join("promo.json")).unwrap(), before);
        assert_eq!(read(&dir, 1).as_deref(), Some("image of 1"));
    }

    /// scenes.md も書き直す (plan からの導出なので追従不要 = 作り直す)。
    #[test]
    fn scenes_markdown_is_rewritten() {
        let dir = tmp();
        seed(&dir, &promo_of(4));
        edit_scenes(dir.to_string_lossy().to_string(), "remove".into(), 2).unwrap();
        let md = fs::read_to_string(dir.join("scenes.md")).unwrap();
        assert!(!md.is_empty());
        assert_eq!(md.matches("## Scene").count(), 3, "3 シーンぶん: {md}");
    }

    /// **末尾のシーンを消したときだけ孤児が出る。**
    ///
    /// 手前のシーンを消す場合は後ろの rename が上書きしていくので気づかないが、末尾を消すと
    /// 番号の付け替えが 1 つも起きず、そのファイルだけが残る。`base/` も同じ。
    #[test]
    fn removing_the_last_scene_deletes_its_image_even_though_nothing_is_renamed() {
        let dir = tmp();
        seed(&dir, &promo_of(4));

        edit_scenes(dir.to_string_lossy().to_string(), "remove".into(), 4).unwrap();

        assert_eq!(read(&dir, 3).as_deref(), Some("image of 3"), "残るシーンはそのまま");
        assert!(read(&dir, 4).is_none(), "消したシーンの画像が残っている (孤児)");
        assert!(!dir.join("base").join("scene_04_ref_01.png").exists(), "base/ にも孤児を残さない");
    }
}
