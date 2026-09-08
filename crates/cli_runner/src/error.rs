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
    /// stream-json / JSON が読めない。raw を保持する (再生成の燃料)。
    #[error("CLI の出力の形が想定と違います ({detail}): {raw}")]
    Shape { detail: String, raw: String },
}
