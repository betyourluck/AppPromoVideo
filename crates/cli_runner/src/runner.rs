//! 実行 (Phase A)。契約 `CliEvent` / `CliOutcome` / `CliError`。
//!
//! 1 回の実行 = spawn → 本文を運ぶ (stdin | 一時ファイル) → stdout/stderr を行で読む →
//! 終了を待つ → kind 別に畳む。timeout / cancel / 終端エラー (`early_abort`) では **木ごと** 止める
//! ([`crate::tree_kill`])。UI へは `on_event` で push する (Tauri 層が event に写す)。

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;

use crate::error::CliError;
use crate::raw_log::RawLog;
use crate::invocation::{AIDER_MESSAGE_FILE_FLAG, CliInvocation, CliKind, PromptTransport, Structured};
use crate::agy;
use crate::stream::{ParsedLine, early_abort, fold_lines, parse_line, retry_notice};
use crate::tree_kill::Tree;

pub use crate::raw_log::CLI_LOG_KEEP;

/// 契約 `CliEvent` (Finished は戻り値で表す)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CliEvent {
    Started { pid: u32 },
    /// 人が読む断片 (claude なら assistant text、他は生の行)。
    Stdout { text: String },
    Stderr { text: String },
    /// 再試行など、結果に影響しない進捗 (`retry_notice`)。
    Progress { text: String },
    /// 構造化出力が取れた (1 回)。
    Structured { json: Value },
}

/// 契約 `CliOutcome::Ok`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOk {
    pub text: String,
    pub structured: Option<Value>,
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    /// CLI が init 行で名乗った実際のモデル名 (rev25)。stream-json を出さない CLI では None。
    #[serde(default)]
    pub model: Option<String>,
}

/// 契約 `CliOutcome::Failed`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{error}{}", .log_path.as_ref().map(|p| format!(" — 生ログ: {}", p.display())).unwrap_or_default())]
pub struct RunFailed {
    pub error: CliError,
    pub stderr_tail: String,
    /// 生ログ (契約 `CliRawLog`)。書けなかった時は None。**落ちた run の唯一の手掛かり**なので文言にも出す。
    #[serde(default)]
    pub log_path: Option<PathBuf>,
}

pub struct RunOptions {
    pub timeout: Duration,
    /// `true` が送られたら中断。
    pub cancel: watch::Receiver<bool>,
    /// 追加で子に渡さない環境変数 (例: OAuth を使わせるために `ANTHROPIC_API_KEY` を外す)。
    /// `env_scrub` の既定集合に加えて外す。
    pub env_remove: Vec<String>,
}

/// stderr の保持量 (末尾)。
const STDERR_TAIL_BYTES: usize = 4096;

fn push_tail(tail: &mut String, line: &str) {
    tail.push_str(line);
    tail.push('\n');
    if tail.len() > STDERR_TAIL_BYTES {
        let cut = tail.len() - STDERR_TAIL_BYTES;
        // char 境界に丸める。
        let cut = tail.char_indices().map(|(i, _)| i).find(|&i| i >= cut).unwrap_or(0);
        tail.drain(..cut);
    }
}

/// 一時ファイル (aider の `--message-file`)。Drop で消す。
struct PromptFile(PathBuf);

impl Drop for PromptFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

enum LoopEnd {
    Eof,
    Abort(CliError),
    Timeout,
    Cancelled,
}

pub async fn run(
    inv: &CliInvocation,
    mut opts: RunOptions,
    mut on_event: impl FnMut(CliEvent),
) -> Result<RunOk, RunFailed> {
    let started = Instant::now();
    // spawn 前の失敗にはまだ生ログが無い。spawn 後は `failed` を使う。
    let fail = |error: CliError, stderr_tail: String| RunFailed { error, stderr_tail, log_path: None };

    // --- 本文の運搬先を決める ---
    let mut args = inv.args.clone();
    let _prompt_file = match inv.transport {
        PromptTransport::Stdin => None,
        PromptTransport::MessageFile => {
            std::fs::create_dir_all(&inv.cwd).map_err(|e| fail(CliError::Shape { detail: format!("cwd を作れない: {e}"), raw: inv.cwd.display().to_string() }, String::new()))?;
            let path = inv.cwd.join(format!("prompt_{}_{}.txt", std::process::id(), started.elapsed().as_nanos()));
            std::fs::write(&path, &inv.prompt)
                .map_err(|e| fail(CliError::Shape { detail: format!("本文ファイルを書けない: {e}"), raw: path.display().to_string() }, String::new()))?;
            args.push(AIDER_MESSAGE_FILE_FLAG.to_string());
            args.push(path.to_string_lossy().to_string());
            Some(PromptFile(path))
        }
    };

    // --- spawn ---
    let mut cmd = Command::new(&inv.program);
    // ホストと結びつく環境変数を子に渡さない (env_scrub)。GUI 実測: デスクトップの端末から起動した
    // アプリの子 claude が居ないホストに認証更新を頼み 401 を繰り返した。
    for name in crate::env_scrub::names_to_scrub().into_iter().chain(opts.env_remove.iter().cloned()) {
        cmd.env_remove(name);
    }
    cmd.args(&args)
        .current_dir(&inv.cwd)
        .stdin(if inv.transport == PromptTransport::Stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let tree = Tree::prepare(&mut cmd).map_err(|e| fail(CliError::Shape { detail: format!("プロセス木の準備に失敗: {e}"), raw: String::new() }, String::new()))?;
    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(fail(CliError::NotFound { executable: inv.program.clone() }, String::new()));
        }
        Err(e) => return Err(fail(CliError::Shape { detail: format!("spawn 失敗: {e}"), raw: inv.program.clone() }, String::new())),
    };
    if let Err(e) = tree.attach(&child) {
        let _ = child.kill().await;
        return Err(fail(CliError::Shape { detail: format!("プロセス木への登録に失敗: {e}"), raw: String::new() }, String::new()));
    }
    let pid = child.id().unwrap_or(0);
    on_event(CliEvent::Started { pid });

    // --- 生ログ (契約 CliRawLog。開けなくても続ける) ---
    let mut log = RawLog::open(&inv.cwd, pid);
    if let Some(l) = log.as_ref() {
        l.write_invocation(&inv.program, &args, &inv.cwd);
    }
    // 何を起動したかは失敗の半分の情報 (2026-09-12: argv が分からず再現できなかった)。
    on_event(CliEvent::Progress { text: invocation_summary(&inv.program, &args) });
    let log_path = log.as_ref().map(|l| l.path().to_path_buf());
    let failed = |error: CliError, stderr_tail: String| RunFailed { error, stderr_tail, log_path: log_path.clone() };

    // --- stdin (書き切ったら閉じる = EOF を届ける) ---
    if let Some(mut stdin) = child.stdin.take() {
        let body = inv.prompt.clone();
        // 子が stdin を読まずに終了しても壊れないよう、エラーは無視する。
        tokio::spawn(async move {
            let _ = stdin.write_all(body.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }

    // --- stdout / stderr を行で読む ---
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let mut out_lines = BufReader::new(stdout).lines();
    let mut err_lines = BufReader::new(stderr).lines();
    let mut raw_lines: Vec<String> = Vec::new();
    let mut stderr_tail = String::new();
    let deadline = tokio::time::sleep(opts.timeout);
    tokio::pin!(deadline);
    let mut out_open = true;
    let mut err_open = true;

    let end = loop {
        if !out_open && !err_open {
            break LoopEnd::Eof;
        }
        tokio::select! {
            l = out_lines.next_line(), if out_open => match l {
                Ok(Some(line)) => {
                    if let Some(l) = log.as_mut() { l.write_line(&line); }
                    if let Some(err) = observe_line(inv.kind, &line, &mut on_event) {
                        raw_lines.push(line);
                        break LoopEnd::Abort(err);
                    }
                    raw_lines.push(line);
                }
                _ => out_open = false,
            },
            l = err_lines.next_line(), if err_open => match l {
                Ok(Some(line)) => {
                    push_tail(&mut stderr_tail, &line);
                    on_event(CliEvent::Stderr { text: line });
                }
                _ => err_open = false,
            },
            _ = &mut deadline => break LoopEnd::Timeout,
            r = opts.cancel.changed() => {
                if r.is_err() || *opts.cancel.borrow() { break LoopEnd::Cancelled; }
            }
        }
    };

    match end {
        LoopEnd::Timeout => {
            tree.kill(&mut child).await;
            return Err(failed(CliError::Timeout { secs: opts.timeout.as_secs() }, stderr_tail));
        }
        LoopEnd::Cancelled => {
            tree.kill(&mut child).await;
            return Err(failed(CliError::Cancelled, stderr_tail));
        }
        LoopEnd::Abort(err) => {
            tree.kill(&mut child).await;
            return Err(failed(err, stderr_tail));
        }
        LoopEnd::Eof => {}
    }

    // --- 終了を待つ (残り時間内) ---
    let remaining = opts.timeout.saturating_sub(started.elapsed()).max(Duration::from_secs(1));
    let status = match tokio::time::timeout(remaining, child.wait()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Err(failed(CliError::Shape { detail: format!("wait 失敗: {e}"), raw: String::new() }, stderr_tail)),
        Err(_) => {
            tree.kill(&mut child).await;
            return Err(failed(CliError::Timeout { secs: opts.timeout.as_secs() }, stderr_tail));
        }
    };
    let duration_ms = started.elapsed().as_millis() as u64;

    // --- kind 別に畳む ---
    match inv.kind {
        CliKind::Claude => {
            let fold = fold_lines(raw_lines.iter().map(String::as_str))
                .map_err(|e| failed(annotate_exit(e, &status), stderr_tail.clone()))?;
            let structured = match (inv.structured, fold.structured) {
                (Structured::JsonSchema, Some(v)) => Some(v),
                (Structured::JsonSchema, None) => promo_core::fenced::parse_json_object(&fold.result_text).ok(),
                (Structured::FencedJson, _) => promo_core::fenced::parse_json_object(&fold.result_text).ok(),
                (Structured::None, _) => None,
            };
            if let Some(v) = &structured {
                on_event(CliEvent::Structured { json: v.clone() });
            }
            let text = if fold.result_text.is_empty() { fold.transcript } else { fold.result_text };
            Ok(RunOk { text, structured, cost_usd: fold.cost_usd, duration_ms: fold.duration_ms.unwrap_or(duration_ms), model: fold.model })
        }
        CliKind::Agy => {
            let fold = agy::fold_lines(raw_lines.iter().map(String::as_str))
                .map_err(|e| failed(annotate_exit(e, &status), stderr_tail.clone()))?;
            let structured = match (inv.structured, fold.structured) {
                (Structured::JsonSchema, Some(v)) => Some(v),
                (Structured::JsonSchema, None) | (Structured::FencedJson, _) => {
                    promo_core::fenced::parse_json_object(&fold.result_text).ok()
                }
                (Structured::None, _) => None,
            };
            if let Some(v) = &structured {
                on_event(CliEvent::Structured { json: v.clone() });
            }
            let text = if fold.result_text.is_empty() { fold.transcript } else { fold.result_text };
            Ok(RunOk {
                text,
                structured,
                // **費用は返らない (トークン数だけ)。計算して埋めない** — 契約 AgyStreamLine.cost。
                cost_usd: None,
                duration_ms: fold.duration_ms.unwrap_or(duration_ms),
                model: None,
            })
        }
        CliKind::Aider | CliKind::Custom => {
            if !status.success() {
                return Err(failed(CliError::ExitStatus { code: status.code(), stderr_tail: stderr_tail.clone() }, stderr_tail));
            }
            let text = raw_lines.join("\n");
            let structured = match inv.structured {
                Structured::FencedJson | Structured::JsonSchema => promo_core::fenced::parse_json_object(&text).ok(),
                Structured::None => None,
            };
            if let Some(v) = &structured {
                on_event(CliEvent::Structured { json: v.clone() });
            }
            Ok(RunOk { text, structured, cost_usd: None, duration_ms, model: None })
        }
    }
}

/// claude の畳みが失敗した時、子の**終了コード**を detail に足す。
///
/// rev38 以前は `status` を計算しておきながら aider / custom でしか見ておらず、claude が result 行を
/// 出さずに落ちた時に「自分で終わったのか異常終了なのか」が分からなかった (2026-09-12 の live 障害)。
fn annotate_exit(err: CliError, status: &std::process::ExitStatus) -> CliError {
    match err {
        CliError::Shape { detail, raw } => {
            let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "不明 (シグナル)".into());
            CliError::Shape { detail: format!("{detail}, 終了コード {code}"), raw }
        }
        other => other,
    }
}

/// 起動の 1 行 (進捗ログ用、純粋)。`--json-schema` の値は**長さだけ**にする — 数千字が
/// ログを埋めるうえに、読みたいのは「schema を渡したか」であって中身ではない。
/// **本文 (prompt) は argv に載らない契約**なので、ここに現れることはない。
pub fn invocation_summary(program: &str, args: &[String]) -> String {
    let mut out = vec![program.to_string()];
    let mut skip_next = false;
    for (i, a) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            out.push(format!("<schema {} 字>", a.chars().count()));
            continue;
        }
        if a == "--json-schema" && i + 1 < args.len() {
            skip_next = true;
        }
        out.push(a.clone());
    }
    format!("起動: {}", out.join(" "))
}

/// 1 行を kind に応じて解釈し、UI へ流し、**止めるべきならエラーを返す**。
///
/// claude は終端の `assistant.error`、agy は**許可外のツール** (契約 `IsolationGuarantee.watchdog`) で止める。
/// agy の JSON でない行 (`jetski: …`) は人が読む説明なのでそのまま Stdout に流す (捨てない)。
fn observe_line(kind: CliKind, line: &str, on_event: &mut impl FnMut(CliEvent)) -> Option<CliError> {
    match kind {
        CliKind::Claude => {
            let parsed = parse_line(line);
            if let Some(err) = early_abort(&parsed) {
                return Some(err);
            }
            match &parsed {
                ParsedLine::Text(t) if !t.is_empty() => on_event(CliEvent::Stdout { text: t.clone() }),
                ParsedLine::ApiRetry { .. } => {
                    if let Some(n) = retry_notice(&parsed) {
                        on_event(CliEvent::Progress { text: n });
                    }
                }
                _ => {}
            }
            None
        }
        CliKind::Agy => {
            let parsed = agy::parse_line(line);
            if let Some(err) = agy::early_abort(&parsed) {
                return Some(err);
            }
            match &parsed {
                agy::AgyLine::Text(t) if !t.is_empty() => on_event(CliEvent::Stdout { text: t.clone() }),
                // 読み取りツールは進捗として見せる (何を読んだかが分かる)。
                agy::AgyLine::Tool { name, state, target } if state == "ACTIVE" => {
                    on_event(CliEvent::Progress { text: format!("ツール: {name} {target}") })
                }
                agy::AgyLine::Other(raw) if !raw.is_empty() => on_event(CliEvent::Stdout { text: raw.clone() }),
                _ => {}
            }
            None
        }
        CliKind::Aider | CliKind::Custom => {
            if !line.is_empty() {
                on_event(CliEvent::Stdout { text: line.to_string() });
            }
            None
        }
    }
}
