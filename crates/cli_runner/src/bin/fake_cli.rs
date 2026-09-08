//! PoC 用の偽 CLI (Phase A)。第 1 引数がモード。実 LLM を呼ばない。
//!
//! - `stream-echo`            : stdin を全部読み、claude 風 stream-json (init / assistant text / result +
//!   structured_output {"echo": <stdin>}) を出す
//! - `stream-auth-then-hang`  : init + assistant(error=authentication_failed) を出して 60 秒眠る
//! - `hang <secs>`            : 眠る (stdout に "hanging" を 1 行)
//! - `grandchild`             : 自分を `hang 120` で spawn し、"grandchild_pid=<pid>" を出して 120 秒眠る
//! - `fail <code>`            : stderr に "boom" を出して code で終了
//! - `message-file`           : `--message-file <path>` を読んで内容をそのまま stdout へ (aider 風)
//! - `plain-json`             : 散文 + ```json フェンス (custom 風)
//! - `env <NAME>`             : 環境変数 NAME が見えるか (`NAME=present|absent`) — runner の env_scrub 検査用

use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let mut out = std::io::stdout().lock();
    match mode {
        "stream-echo" => {
            let mut body = String::new();
            std::io::stdin().read_to_string(&mut body).unwrap();
            writeln!(out, r#"{{"type":"system","subtype":"init","session_id":"fake"}}"#).unwrap();
            let text = serde_json::to_string(&format!("echo: {}", body.chars().take(40).collect::<String>())).unwrap();
            writeln!(out, r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":{text}}}]}}}}"#).unwrap();
            let structured = serde_json::json!({"echo": body});
            let result = serde_json::to_string(&structured.to_string()).unwrap();
            writeln!(
                out,
                r#"{{"type":"result","is_error":false,"result":{result},"total_cost_usd":0.001,"duration_ms":5,"structured_output":{structured}}}"#
            )
            .unwrap();
        }
        "stream-auth-then-hang" => {
            writeln!(out, r#"{{"type":"system","subtype":"init","session_id":"fake"}}"#).unwrap();
            writeln!(
                out,
                r#"{{"type":"assistant","error":"authentication_failed","message":{{"content":[{{"type":"text","text":"Failed to authenticate"}}]}}}}"#
            )
            .unwrap();
            out.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        "hang" => {
            let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(60);
            writeln!(out, "hanging").unwrap();
            out.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(secs));
        }
        "grandchild" => {
            let me = std::env::current_exe().unwrap();
            // intent: 孫を wait しないのが目的 (親が kill された後に生き残るかを検査する被験体)。
            #[allow(clippy::zombie_processes)]
            let child = std::process::Command::new(me)
                .args(["hang", "120"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            writeln!(out, "grandchild_pid={}", child.id()).unwrap();
            out.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(120));
        }
        "fail" => {
            let code: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);
            eprintln!("boom");
            std::process::exit(code);
        }
        "message-file" => {
            let i = args.iter().position(|a| a == "--message-file").expect("--message-file");
            let body = std::fs::read_to_string(&args[i + 1]).unwrap();
            write!(out, "{body}").unwrap();
        }
        "plain-json" => {
            writeln!(out, "Here is the plan.").unwrap();
            writeln!(out, "```json").unwrap();
            writeln!(out, r#"{{"ok": true, "n": 2}}"#).unwrap();
            writeln!(out, "```").unwrap();
        }
        "env" => {
            let name = args.get(2).expect("env <NAME>");
            let state = if std::env::var_os(name).is_some() { "present" } else { "absent" };
            writeln!(out, "{name}={state}").unwrap();
        }
        other => {
            eprintln!("unknown mode: {other}");
            std::process::exit(2);
        }
    }
}
