//! cli_runner — ローカル LLM CLI をサブプロセスで叩く (契約 `CliSpec` / `CliInvocation` / `CliEvent`)。
//!
//! **LLM の HTTP は禁止** (data_contract meta.scope_out)。ここが LLM への唯一の口。
//! Phase 0 は純関数だけ: argv の組み立て ([`invocation`]) と stream-json の解析 ([`stream`])。
//! spawn / timeout / cancel は Phase A (`tokio::process`)。

pub mod env_scrub;
pub mod error;
pub mod invocation;
pub mod raw_log;
pub mod runner;
pub mod stream;
pub mod tree_kill;

pub use error::CliError;
pub use invocation::{
    AIDER_MESSAGE_FILE_FLAG, CliInvocation, CliKind, CliSpec, PromptTransport, Structured, TaskSpec, build_invocation,
};
pub use stream::{ParsedLine, StreamFold, early_abort, fold_lines, parse_line, retry_notice};
