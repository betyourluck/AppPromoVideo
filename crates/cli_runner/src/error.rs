//! 契約 `CliError`。UI はこの列挙で文言を出す (沈黙させない)。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CliError {
    /// spawn 前の存在検査 or ENOENT。
    #[error("CLI が見つかりません: {executable}")]
    NotFound { executable: String },
    /// kill 済み。
    #[error("CLI が {secs} 秒以内に終わらなかったため中断しました")]
    Timeout { secs: u64 },
    /// ユーザー中断 (kill 済み)。
    #[error("中断しました")]
    Cancelled,
    #[error("CLI が異常終了しました (code={code:?}): {stderr_tail}")]
    ExitStatus { code: Option<i32>, stderr_tail: String },
    /// claude stream-json の `authentication_failed` (fixtures/claude_auth_failed.jsonl)。
    #[error("CLI の認証に失敗しました: {message}")]
    Auth { message: String },
    /// agy の見張り (契約 `IsolationGuarantee.watchdog`)。許可外のツールを見て木ごと止めた。
    /// **検出であって予防ではない** — ACTIVE を見た時点でその書き込みは始まっている。
    #[error("CLI が許可されていないツールを使いました ({tool}: {target})。書き換えが起きた可能性があります")]
    ToolNotAllowed { tool: String, target: String },
    /// agy が成功を名乗ったが本文も構造化出力も空。理由は JSON でない行にしか書かれない。
    #[error("CLI は成功を返しましたが出力が空でした: {note}")]
    EmptySuccess { note: String },
    /// stream-json / JSON が読めない。raw を保持する (再生成の燃料)。
    #[error("CLI の出力の形が想定と違います ({detail}): {raw}")]
    Shape { detail: String, raw: String },
}
