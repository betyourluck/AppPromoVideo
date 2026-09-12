//! argv の組み立て (純粋。プロセスを起動しない)。契約 `CliSpec` / `CliInvocation`。
//!
//! 凍結した既定引数 (data_contract `CliInvocation`、spec 01 rev2):
//! - claude: `-p --output-format stream-json --verbose --max-turns N [--model M] [--json-schema S]
//!   --add-dir <project> --allowedTools Read Glob Grep` + extra_args。本文は **stdin**。
//!   **cwd は project ではなく app 所有の作業ディレクトリ** — `-p` は cwd の `.claude/settings.json`
//!   の hook と `.mcp.json` を信頼ダイアログなしで実行する (公式 headless 文書)。解析対象は他人の
//!   リポジトリになり得るので、cwd に置かず `--add-dir` で読み取りだけ許す。
//! - aider: `--message-file <path>` (実行時に runner が本文を一時ファイルへ書く) `--no-git --yes-always
//!   --no-auto-commits [--model M]`。`--message` は採らない (査読 1: 引数長・ログ漏れ)。
//! - agy: `--input-format stream-json --output-format stream-json --print-timeout <timeout>s
//!   --disable-slash-commands [--model M] [--json-schema S] --add-dir <project>`。本文は **stdin の NDJSON**
//!   (`{"event":"user","message":{"content":…}}`)。**`-p` / `--print` は使わない** — あれは値 (prompt) を
//!   取るので本文が argv に載る (2026-09-12 の障害の正体)。`--max-turns` / `--verbose` / `--allowedTools`
//!   は agy に**存在しない**。許可リストが無いので隔離は検出のみ (契約 `IsolationGuarantee`)。
//! - custom: 既定引数なし。本文は stdin。
//!
//! 不変条件 (テストで固定): `Read` を許す時は必ず `--add-dir <project>` が付き、cwd は project でない。
//! `--dangerously-skip-permissions` は**既定で付けない** (ユーザーが extra_args に書いた時だけ)。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliKind {
    Claude,
    Aider,
    /// 別系統の CLI (2026-09-12)。封筒も argv も claude と別 (契約 `AgyStreamLine`)。
    /// **隔離の保証が弱い** — 契約 `IsolationGuarantee` を読むこと。
    Agy,
    Custom,
}

/// 契約 `CliSpec` (frontend の localStorage から渡る)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliSpec {
    pub kind: CliKind,
    pub executable: String,
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// 既定 600。最小 30 (それ未満は切り上げ)。
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub model: Option<String>,
}

fn default_timeout() -> u64 {
    600
}

pub const TIMEOUT_MIN_SECS: u64 = 30;

impl CliSpec {
    /// 実効タイムアウト (契約: 最小 30)。
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs.max(TIMEOUT_MIN_SECS)
    }
}

/// 構造化出力の取り方 (契約 `CliInvocation.structured`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Structured {
    JsonSchema,
    FencedJson,
    None,
}

/// 本文の運び方 (契約 `CliInvocation.transport`)。どちらも本文は argv に載らない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptTransport {
    /// stdin に流す (claude / custom)。
    Stdin,
    /// runner が一時ファイルに書き、`--message-file <path>` を argv 末尾に足す (aider)。
    MessageFile,
}

/// 1 回の実行の依頼。
#[derive(Debug, Clone)]
pub struct TaskSpec<'a> {
    /// 指示本文 (RepoBrief.render() + 指示)。
    pub prompt: &'a str,
    /// 構造化したい時の schema (promo_core が機械生成)。None なら散文。
    pub schema: Option<&'a Value>,
    /// 解析対象リポジトリ。`--add-dir` に渡す。**cwd にはしない**。
    pub project_path: &'a Path,
    /// app 所有の作業ディレクトリ (cwd)。対象リポジトリの設定・hook を拾わないための隔離。
    pub scratch_dir: &'a Path,
    /// claude の --max-turns (走査の深掘り回数の上限)。
    pub max_turns: u32,
}

/// 契約 `CliInvocation`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInvocation {
    pub program: String,
    pub args: Vec<String>,
    /// 本文。`transport` に従って runner が stdin か一時ファイルへ運ぶ。
    pub prompt: String,
    pub transport: PromptTransport,
    pub cwd: PathBuf,
    pub structured: Structured,
    /// 畳み方 (claude = stream-json / 他 = 生 stdout) を runner が選ぶために持つ。
    pub kind: CliKind,
}

/// claude に許すツール (契約: Read / Glob / Grep のみ。Write / Edit / Bash は許可しない)。
pub const CLAUDE_ALLOWED_TOOLS: [&str; 3] = ["Read", "Glob", "Grep"];

/// aider の本文ファイル引数 (runner が path を続けて足す)。
pub const AIDER_MESSAGE_FILE_FLAG: &str = "--message-file";

/// fenced_json 経路で本文の末尾に足す指示。
const FENCED_JSON_SUFFIX: &str =
    "\n\nRespond with a single JSON object only, matching this JSON Schema. No prose before or after.\n";

pub fn build_invocation(spec: &CliSpec, task: &TaskSpec<'_>) -> CliInvocation {
    let project = task.project_path.to_string_lossy().to_string();
    let model = spec.model.as_deref().map(str::trim).filter(|m| !m.is_empty());
    match spec.kind {
        CliKind::Claude => {
            let mut args = vec![
                "-p".to_string(),
                "--output-format".into(),
                "stream-json".into(),
                "--verbose".into(),
                "--max-turns".into(),
                task.max_turns.to_string(),
            ];
            if let Some(m) = model {
                args.push("--model".into());
                args.push(m.to_string());
            }
            let structured = if let Some(schema) = task.schema {
                args.push("--json-schema".into());
                args.push(schema.to_string());
                Structured::JsonSchema
            } else {
                Structured::None
            };
            args.push("--add-dir".into());
            args.push(project.clone());
            args.push("--allowedTools".into());
            args.extend(CLAUDE_ALLOWED_TOOLS.iter().map(|s| s.to_string()));
            args.extend(spec.extra_args.iter().cloned());
            CliInvocation {
                program: spec.executable.clone(),
                args,
                prompt: task.prompt.to_string(),
                transport: PromptTransport::Stdin,
                cwd: task.scratch_dir.to_path_buf(),
                structured,
                kind: spec.kind,
            }
        }
        CliKind::Agy => {
            let mut args = vec![
                "--input-format".to_string(),
                "stream-json".into(),
                "--output-format".into(),
                "stream-json".into(),
                // 既定 5 分。解析は実測 4 分 22 秒かかったことがあるので必ず渡す (契約 AgyStreamLine.argv)。
                "--print-timeout".into(),
                format!("{}s", spec.timeout_secs()),
                // 本文がスキル / スラッシュコマンドに展開されないように塞ぐ (cwd 隔離と同じ趣旨)。
                "--disable-slash-commands".into(),
            ];
            if let Some(m) = model {
                args.push("--model".into());
                args.push(m.to_string());
            }
            let structured = if let Some(schema) = task.schema {
                args.push("--json-schema".into());
                args.push(schema.to_string());
                Structured::JsonSchema
            } else {
                Structured::None
            };
            args.push("--add-dir".into());
            args.push(project.clone());
            args.extend(spec.extra_args.iter().cloned());
            CliInvocation {
                program: spec.executable.clone(),
                args,
                // **本文は argv に載せず stdin の NDJSON で運ぶ。**
                prompt: crate::agy::user_message_line(task.prompt),
                transport: PromptTransport::Stdin,
                cwd: task.scratch_dir.to_path_buf(),
                structured,
                kind: spec.kind,
            }
        }
        CliKind::Aider => {
            let (body, structured) = fenced_body(task);
            let mut args = vec!["--no-git".to_string(), "--yes-always".into(), "--no-auto-commits".into()];
            if let Some(m) = model {
                args.push("--model".into());
                args.push(m.to_string());
            }
            args.extend(spec.extra_args.iter().cloned());
            // --message-file <path> は runner が末尾に足す (path は spawn 時にしか決まらない)。
            CliInvocation {
                program: spec.executable.clone(),
                args,
                prompt: body,
                transport: PromptTransport::MessageFile,
                cwd: task.scratch_dir.to_path_buf(),
                structured,
                kind: spec.kind,
            }
        }
        CliKind::Custom => {
            let (body, structured) = fenced_body(task);
            CliInvocation {
                program: spec.executable.clone(),
                args: spec.extra_args.clone(),
                prompt: body,
                transport: PromptTransport::Stdin,
                cwd: task.scratch_dir.to_path_buf(),
                structured,
                kind: spec.kind,
            }
        }
    }
}

/// schema があれば本文に「JSON だけ出せ」+ schema を足す (fenced_json 経路)。
fn fenced_body(task: &TaskSpec<'_>) -> (String, Structured) {
    match task.schema {
        Some(schema) => {
            let mut body = task.prompt.to_string();
            body.push_str(FENCED_JSON_SUFFIX);
            body.push_str(&schema.to_string());
            body.push('\n');
            (body, Structured::FencedJson)
        }
        None => (task.prompt.to_string(), Structured::None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(kind: CliKind) -> CliSpec {
        CliSpec { kind, executable: "claude".into(), extra_args: vec![], timeout_secs: 600, model: None }
    }

    fn task<'a>(schema: Option<&'a Value>) -> TaskSpec<'a> {
        TaskSpec {
            prompt: "SECRET PROMPT BODY",
            schema,
            project_path: Path::new("D:/proj"),
            scratch_dir: Path::new("C:/app_data/work"),
            max_turns: 12,
        }
    }

    /// 全 kind: 本文は argv に載らない (査読 1 で aider の例外を撤去)。
    #[test]
    fn prompt_body_never_appears_in_argv_for_any_kind() {
        let schema = json!({"type": "object"});
        for kind in [CliKind::Claude, CliKind::Aider, CliKind::Agy, CliKind::Custom] {
            for sch in [None, Some(&schema)] {
                let inv = build_invocation(&spec(kind), &task(sch));
                assert!(inv.args.iter().all(|a| !a.contains("SECRET")), "{kind:?}: {:?}", inv.args);
                // agy は本文を NDJSON の封筒に包んで stdin に流すので前方一致にならない。
                assert!(inv.prompt.contains("SECRET PROMPT BODY"), "{kind:?}");
            }
        }
    }

    /// 不変条件: Read を許す時は --add-dir <project> が必ず付き、cwd は project ではない (hook 隔離)。
    #[test]
    fn read_permission_implies_add_dir_and_cwd_is_not_the_repo() {
        for sch in [None, Some(&json!({"type": "object"}))] {
            let inv = build_invocation(&spec(CliKind::Claude), &task(sch));
            let allows_read = inv.args.iter().any(|a| a == "Read");
            assert!(allows_read);
            let i = inv.args.iter().position(|x| x == "--add-dir").expect("--add-dir が無い");
            assert_eq!(inv.args[i + 1], "D:/proj");
            assert_ne!(inv.cwd, PathBuf::from("D:/proj"));
            assert_eq!(inv.cwd, PathBuf::from("C:/app_data/work"));
        }
    }

    /// agy の凍結引数 (契約 `AgyStreamLine.argv`)。**`-p` を付けない**のが要点 —
    /// agy の `-p` は値 (prompt) を取るので、付けると次のフラグを本文として飲み込む
    /// (2026-09-12 の障害: `-p took "--output-format" as its prompt`)。
    #[test]
    fn agy_frozen_defaults_have_no_print_flag_and_set_the_timeout() {
        let schema = json!({"type": "object"});
        let inv = build_invocation(&spec(CliKind::Agy), &task(Some(&schema)));
        let a = &inv.args;
        assert_eq!(&a[..4], &["--input-format", "stream-json", "--output-format", "stream-json"]);
        assert!(!a.iter().any(|x| x == "-p" || x == "--print" || x == "--prompt"), "{a:?}");
        // agy に存在しないフラグは渡さない。
        for absent in ["--max-turns", "--verbose", "--allowedTools"] {
            assert!(!a.iter().any(|x| x == absent), "{absent} は agy に存在しない: {a:?}");
        }
        let i = a.iter().position(|x| x == "--print-timeout").expect("--print-timeout が無い (既定 5 分で自ら切れる)");
        assert_eq!(a[i + 1], "600s");
        assert!(a.iter().any(|x| x == "--disable-slash-commands"));
        let i = a.iter().position(|x| x == "--add-dir").unwrap();
        assert_eq!(a[i + 1], "D:/proj");
        assert_ne!(inv.cwd, PathBuf::from("D:/proj"));
        // 本文は stdin の NDJSON。
        let v: Value = serde_json::from_str(inv.prompt.trim_end()).unwrap();
        assert_eq!(v["event"], "user");
        assert_eq!(v["message"]["content"], "SECRET PROMPT BODY");
    }

    #[test]
    fn claude_frozen_defaults_and_tool_allowlist() {
        let schema = json!({"type": "object"});
        let inv = build_invocation(&spec(CliKind::Claude), &task(Some(&schema)));
        let a = &inv.args;
        assert_eq!(&a[..6], &["-p", "--output-format", "stream-json", "--verbose", "--max-turns", "12"]);
        let i = a.iter().position(|x| x == "--json-schema").unwrap();
        assert_eq!(a[i + 1], schema.to_string());
        let i = a.iter().position(|x| x == "--allowedTools").unwrap();
        assert_eq!(&a[i + 1..i + 4], &["Read", "Glob", "Grep"]);
        assert!(!a.iter().any(|x| x.contains("skip-permissions")), "危険フラグは既定で付けない");
        assert!(!a.iter().any(|x| x == "--bare"), "--bare は OAuth を使わない (API キーが要る) ので既定で付けない");
        assert_eq!(inv.structured, Structured::JsonSchema);
        assert_eq!(inv.transport, PromptTransport::Stdin);
    }

    #[test]
    fn claude_without_schema_has_no_json_schema_flag() {
        let inv = build_invocation(&spec(CliKind::Claude), &task(None));
        assert!(!inv.args.iter().any(|a| a == "--json-schema"));
        assert_eq!(inv.structured, Structured::None);
    }

    #[test]
    fn claude_extra_args_and_model_are_appended() {
        let mut s = spec(CliKind::Claude);
        s.extra_args = vec!["--dangerously-skip-permissions".into()];
        s.model = Some(" haiku ".into());
        let inv = build_invocation(&s, &task(None));
        assert_eq!(inv.args.last().unwrap(), "--dangerously-skip-permissions");
        let i = inv.args.iter().position(|x| x == "--model").unwrap();
        assert_eq!(inv.args[i + 1], "haiku");
    }

    /// 査読 1: aider は --message でなく --message-file (path は runner が足す)。--yes は誤りで --yes-always。
    #[test]
    fn aider_uses_message_file_transport_and_official_flags() {
        let schema = json!({"type": "object"});
        let inv = build_invocation(&spec(CliKind::Aider), &task(Some(&schema)));
        assert_eq!(inv.transport, PromptTransport::MessageFile);
        assert!(!inv.args.iter().any(|a| a == "--message" || a == "-m"));
        assert!(!inv.args.iter().any(|a| a == AIDER_MESSAGE_FILE_FLAG), "path は spawn 時に runner が足す");
        assert!(!inv.args.iter().any(|a| a == "--yes"), "aider の公式フラグは --yes-always");
        for f in ["--no-git", "--yes-always", "--no-auto-commits"] {
            assert!(inv.args.contains(&f.to_string()), "{f}");
        }
        assert!(inv.prompt.contains("single JSON object only"));
        assert!(inv.prompt.contains(&schema.to_string()));
        assert_eq!(inv.structured, Structured::FencedJson);
        assert_eq!(inv.cwd, PathBuf::from("C:/app_data/work"));
    }

    #[test]
    fn custom_has_no_default_args_and_stdin_body() {
        let mut s = spec(CliKind::Custom);
        s.extra_args = vec!["--flag".into()];
        let inv = build_invocation(&s, &task(None));
        assert_eq!(inv.args, vec!["--flag"]);
        assert_eq!(inv.prompt, "SECRET PROMPT BODY");
        assert_eq!(inv.transport, PromptTransport::Stdin);
        assert_eq!(inv.structured, Structured::None);
    }

    #[test]
    fn timeout_floor_is_30() {
        let mut s = spec(CliKind::Claude);
        s.timeout_secs = 5;
        assert_eq!(s.timeout_secs(), 30);
        s.timeout_secs = 900;
        assert_eq!(s.timeout_secs(), 900);
    }

    #[test]
    fn spec_deserializes_with_defaults() {
        let s: CliSpec = serde_json::from_str(r#"{"kind":"claude","executable":"claude"}"#).unwrap();
        assert_eq!(s.timeout_secs, 600);
        assert!(s.extra_args.is_empty());
        assert_eq!(s.model, None);
    }
}
