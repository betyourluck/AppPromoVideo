//! 子 CLI に**渡さない**環境変数 (契約 `CliInvocation.env_scrub`、2026-09-08 GUI 実測起点)。
//!
//! Claude Code は自分が起こした子プロセスに `CLAUDE_CODE_*` / `CLAUDECODE` 等を注入し、子は
//! 認証の更新をホストのメッセージングソケット (`CLAUDE_CODE_MESSAGING_SOCKET`) 経由でホストに頼む
//! (`CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH=1`)。本アプリが Claude デスクトップの端末から起動されると
//! この変数群を継承し、アプリの中で spawn した `claude -p` は**居ないホスト**に更新を頼んで
//! `authentication_failed` (401) を繰り返す。本アプリの子 CLI は独立したプロセスであるべきなので、
//! ホストとの結びつきを表す変数は全部落とす。`ANTHROPIC_*` (ユーザーの鍵・base URL) は残す。

/// 落とす変数か (純粋)。
pub fn should_scrub(name: &str) -> bool {
    name.starts_with("CLAUDE_CODE_")
        || name.starts_with("CLAUDE_PREVIEW_")
        || matches!(name, "CLAUDECODE" | "CLAUDE_PID" | "CLAUDE_EFFORT" | "CLAUDE_AGENT_SDK_VERSION")
}

/// 現在のプロセス環境から落とすべき名前を列挙する。
pub fn names_to_scrub() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .filter(|k| should_scrub(k))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrubs_host_coupling_vars_and_keeps_user_config() {
        for n in [
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_CODE_MESSAGING_SOCKET",
            "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
            "CLAUDECODE",
            "CLAUDE_PID",
            "CLAUDE_EFFORT",
            "CLAUDE_AGENT_SDK_VERSION",
            "CLAUDE_PREVIEW_CLASSIFIER_FLOOR",
        ] {
            assert!(should_scrub(n), "{n}");
        }
        for n in ["ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN", "PATH", "HOME", "CLAUDE_CONFIG_DIR"] {
            assert!(!should_scrub(n), "{n} は残す");
        }
    }
}
