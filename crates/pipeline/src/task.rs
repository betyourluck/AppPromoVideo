//! 1 タスクの実行を抽象化する (依存性逆転。Kataribe の `DeltaProposer` と同型)。
//!
//! `stages` はこの trait に対して書く。実装は `CliTaskRunner` (cli_runner 経由) と、テストの fake。

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cli_runner::runner::{CliEvent, RunFailed, RunOk, RunOptions, run};
use cli_runner::{CliSpec, TaskSpec, build_invocation};
use serde_json::Value;
use tokio::sync::watch;

/// 1 回の LLM 呼び出し (本文 + schema) → 結果。
///
/// `Send + Sync` と `+ Send` な future: Tauri の async command は future 全体に Send を要求する
/// (Phase D で判明。GUI から呼べない trait は結線の意味が無い)。
pub trait TaskRunner: Send + Sync {
    fn run_task<'a>(
        &'a self,
        prompt: &'a str,
        schema: Option<&'a Value>,
    ) -> Pin<Box<dyn Future<Output = Result<RunOk, RunFailed>> + Send + 'a>>;
}

/// cli_runner で実行する本番実装。
pub struct CliTaskRunner {
    pub spec: CliSpec,
    pub project_path: PathBuf,
    pub scratch_dir: PathBuf,
    pub max_turns: u32,
    /// 読ませたい追加フォルダ (rev42)。スナップショットの置き場 — 探索ツールが見つけられないと
    /// agy はシェル (`run_command`) に逃げ、見張りに止められる。
    pub extra_read_dirs: Vec<PathBuf>,
    pub cancel: watch::Receiver<bool>,
    /// 進捗の受け口 (Tauri 層が event に写す。CLI なら stderr へ)。
    pub on_event: Box<dyn Fn(CliEvent) + Send + Sync>,
    /// 子に渡さない環境変数の追加分 (OAuth 優先なら ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN)。
    pub env_remove: Vec<String>,
}

/// スナップショットの**置き場**を重複なく集める (rev42、純粋)。
///
/// ユーザーが入力として選んだファイルの親フォルダ。探索ツールがここを見られないと、
/// agy は `run_command` でシェルに逃げて見張りに止められる (2026-09-12 実機)。
pub fn snapshot_dirs(snapshots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for s in snapshots {
        if let Some(d) = s.parent() {
            if !d.as_os_str().is_empty() && !out.iter().any(|x| x == d) {
                out.push(d.to_path_buf());
            }
        }
    }
    out
}

/// OAuth ログインを使わせるために外す変数 (契約 CliSpec.oauth_only)。
pub const OAUTH_ONLY_REMOVE: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

impl TaskRunner for CliTaskRunner {
    fn run_task<'a>(
        &'a self,
        prompt: &'a str,
        schema: Option<&'a Value>,
    ) -> Pin<Box<dyn Future<Output = Result<RunOk, RunFailed>> + Send + 'a>> {
        Box::pin(async move {
            let task = TaskSpec {
                prompt,
                schema,
                project_path: &self.project_path,
                scratch_dir: &self.scratch_dir,
                max_turns: self.max_turns,
                extra_read_dirs: &self.extra_read_dirs,
            };
            let inv = build_invocation(&self.spec, &task);
            let opts = RunOptions {
                timeout: Duration::from_secs(self.spec.timeout_secs()),
                cancel: self.cancel.clone(),
                env_remove: self.env_remove.clone(),
            };
            run(&inv, opts, |e| (self.on_event)(e)).await
        })
    }
}
