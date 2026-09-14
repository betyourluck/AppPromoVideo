//! 1 タスクの実行を抽象化する (依存性逆転。Kataribe の `DeltaProposer` と同型)。
//!
//! `stages` はこの trait に対して書く。実装は `CliTaskRunner` (cli_runner 経由) と、テストの fake。

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use cli_runner::runner::{CliEvent, RunFailed, RunOk, RunOptions, run};
use cli_runner::{CliKind, CliSpec, TaskSpec, build_invocation};
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

/// 子に渡さない環境変数の追加分 (契約 `CliInvocation.env_scrub`、rev52、純粋)。
///
/// - claude: 「OAuth ログインを使う」が ON の時だけ Anthropic の鍵 2 つを外す (スイッチを持つのは claude だけ)
/// - **agy: 設定に関係なく常に外す** (ユーザー決定 2026-09-14)。agy に Anthropic の鍵を持たせる理由が無く、
///   隔離が弱い (書き込み系ツールが通る。契約 `IsolationGuarantee`)。スイッチは画面にも出さない
/// - **aider / custom: 設定に関係なく外さない** (rev53、同日ユーザー決定)。スイッチを画面に出さないので、
///   保存された値を効かせない (見えない設定で挙動を変えない)。aider は ANTHROPIC_API_KEY を環境変数から読む (公式文書)
pub fn env_remove_for(kind: CliKind, oauth_only: bool) -> Vec<String> {
    let remove = match kind {
        CliKind::Claude => oauth_only,
        CliKind::Agy => true,
        CliKind::Aider | CliKind::Custom => false,
    };
    if remove { OAUTH_ONLY_REMOVE.iter().map(|s| s.to_string()).collect() } else { vec![] }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [&str; 2] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

    /// rev52 (ユーザー決定 2026-09-14): agy には「OAuth ログインを使う」の設定に関係なく Anthropic の鍵を渡さない。
    /// それまでは種類を問わずスイッチだけで決まり、切ると agy も鍵を環境変数として引き継いでいた。
    #[test]
    fn agy_never_receives_anthropic_keys() {
        assert_eq!(env_remove_for(CliKind::Agy, false), KEYS);
        assert_eq!(env_remove_for(CliKind::Agy, true), KEYS);
    }

    #[test]
    fn only_claude_follows_the_oauth_switch() {
        assert!(env_remove_for(CliKind::Claude, false).is_empty());
        assert_eq!(env_remove_for(CliKind::Claude, true), KEYS);
    }

    /// rev53 (ユーザー決定 2026-09-14): aider / custom はスイッチを出さないので、保存された値を効かせない
    /// (見えない設定で挙動を変えない)。鍵は外さず普通に引き継ぐ — aider は ANTHROPIC_API_KEY を環境変数から読む (公式文書)。
    #[test]
    fn aider_and_custom_inherit_keys_regardless_of_the_switch() {
        for k in [CliKind::Aider, CliKind::Custom] {
            assert!(env_remove_for(k, false).is_empty(), "{k:?}");
            assert!(env_remove_for(k, true).is_empty(), "{k:?}");
        }
    }
}
