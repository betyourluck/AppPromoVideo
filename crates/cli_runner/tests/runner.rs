//! Phase A PoC: 実プロセス (fake_cli) で spawn / 本文運搬 / 行ストリーム / timeout / cancel / NotFound /
//! 終端エラーでの打ち切り / **孫プロセスが timeout 後に消えている** を固定する。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cli_runner::runner::{CliEvent, RunOptions, run};
use cli_runner::tree_kill::process_alive;
use cli_runner::{CliError, CliInvocation, CliKind, PromptTransport, Structured};
use tokio::sync::watch;

const FAKE: &str = env!("CARGO_BIN_EXE_fake_cli");

fn scratch() -> PathBuf {
    let d = std::env::temp_dir().join(format!("cli_runner_test_{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn inv(kind: CliKind, args: &[&str], prompt: &str, transport: PromptTransport, structured: Structured) -> CliInvocation {
    CliInvocation {
        program: FAKE.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        prompt: prompt.to_string(),
        transport,
        cwd: scratch(),
        structured,
        kind,
    }
}

fn opts(secs: u64) -> (RunOptions, watch::Sender<bool>) {
    let (tx, rx) = watch::channel(false);
    (RunOptions { timeout: Duration::from_secs(secs), cancel: rx, env_remove: vec![] }, tx)
}

#[tokio::test]
async fn stdin_body_roundtrips_and_structured_output_is_extracted() {
    let i = inv(CliKind::Claude, &["stream-echo"], "hello from stdin\nline 2", PromptTransport::Stdin, Structured::JsonSchema);
    let (o, _tx) = opts(10);
    let mut events = Vec::new();
    let ok = run(&i, o, |e| events.push(e)).await.unwrap();
    assert_eq!(ok.structured, Some(serde_json::json!({"echo": "hello from stdin\nline 2"})));
    assert_eq!(ok.cost_usd, Some(0.001));
    assert!(events.iter().any(|e| matches!(e, CliEvent::Started { pid } if *pid > 0)));
    assert!(events.iter().any(|e| matches!(e, CliEvent::Stdout { text } if text.starts_with("echo: hello"))));
    assert!(events.iter().any(|e| matches!(e, CliEvent::Structured { .. })));
}

#[tokio::test]
async fn missing_executable_is_not_found_before_anything_runs() {
    let mut i = inv(CliKind::Claude, &[], "x", PromptTransport::Stdin, Structured::None);
    i.program = "definitely-not-a-real-cli-xyz".into();
    let (o, _tx) = opts(10);
    let err = run(&i, o, |_| {}).await.unwrap_err();
    assert!(matches!(err.error, CliError::NotFound { ref executable } if executable.contains("definitely-not")));
}

#[tokio::test]
async fn timeout_kills_the_whole_tree_including_grandchild() {
    // kind=Custom は生 stdout が Stdout event に乗るので、孫の pid をそこから拾う。
    let i = inv(CliKind::Custom, &["grandchild"], "", PromptTransport::Stdin, Structured::None);
    let (o, _tx) = opts(2);
    let mut pid: Option<u32> = None;
    let t0 = Instant::now();
    let err = run(&i, o, |e| {
        if let CliEvent::Stdout { text } = &e {
            if let Some(p) = text.strip_prefix("grandchild_pid=") {
                pid = p.trim().parse().ok();
            }
        }
    })
    .await
    .unwrap_err();
    assert!(matches!(err.error, CliError::Timeout { secs: 2 }));
    assert!(t0.elapsed() < Duration::from_secs(10), "kill 後に wait が戻らない");
    let pid = pid.expect("孫の pid が stdout に出ているはず");
    // 木ごと止めた直後。少しだけ猶予 (OS がハンドルを畳む時間)。
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!process_alive(pid), "孫プロセス {pid} が生き残っている (子だけ kill している)");
}

#[tokio::test]
async fn cancel_stops_a_hanging_process() {
    let i = inv(CliKind::Custom, &["hang", "60"], "", PromptTransport::Stdin, Structured::None);
    let (o, tx) = opts(60);
    let t0 = Instant::now();
    let fut = run(&i, o, |_| {});
    let canceller = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        tx.send(true).unwrap();
    };
    let (res, _) = tokio::join!(fut, canceller);
    assert!(matches!(res.unwrap_err().error, CliError::Cancelled));
    assert!(t0.elapsed() < Duration::from_secs(10));
}

#[tokio::test]
async fn nonzero_exit_reports_status_and_stderr_tail() {
    let i = inv(CliKind::Custom, &["fail", "7"], "", PromptTransport::Stdin, Structured::None);
    let (o, _tx) = opts(10);
    let mut saw_stderr = false;
    let err = run(&i, o, |e| {
        if let CliEvent::Stderr { text } = &e {
            saw_stderr = text.contains("boom");
        }
    })
    .await
    .unwrap_err();
    assert!(matches!(err.error, CliError::ExitStatus { code: Some(7), .. }));
    assert!(err.stderr_tail.contains("boom"));
    assert!(saw_stderr);
}

#[tokio::test]
async fn message_file_transport_writes_reads_and_deletes_the_file() {
    let i = inv(CliKind::Aider, &["message-file"], "body via file\nsecond line", PromptTransport::MessageFile, Structured::None);
    let cwd = i.cwd.clone();
    let (o, _tx) = opts(10);
    let ok = run(&i, o, |_| {}).await.unwrap();
    assert_eq!(ok.text, "body via file\nsecond line");
    let leftovers: Vec<_> = std::fs::read_dir(&cwd)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("prompt_"))
        .collect();
    assert!(leftovers.is_empty(), "一時ファイルが残っている: {leftovers:?}");
}

#[tokio::test]
async fn custom_fenced_json_is_extracted_from_prose() {
    let i = inv(CliKind::Custom, &["plain-json"], "", PromptTransport::Stdin, Structured::FencedJson);
    let (o, _tx) = opts(10);
    let ok = run(&i, o, |_| {}).await.unwrap();
    assert_eq!(ok.structured, Some(serde_json::json!({"ok": true, "n": 2})));
    assert!(ok.text.starts_with("Here is the plan."));
}

#[tokio::test]
async fn terminal_auth_error_aborts_without_waiting_for_eof() {
    let i = inv(CliKind::Claude, &["stream-auth-then-hang"], "", PromptTransport::Stdin, Structured::None);
    let (o, _tx) = opts(30);
    let t0 = Instant::now();
    let err = run(&i, o, |_| {}).await.unwrap_err();
    assert!(matches!(err.error, CliError::Auth { .. }), "{:?}", err.error);
    assert!(t0.elapsed() < Duration::from_secs(10), "終端エラー後に 60 秒の hang を待ってしまった");
}

/// ホストと結びつく環境変数 (CLAUDE_CODE_* 等) は子に渡らない。ANTHROPIC_* は渡る。
#[tokio::test]
async fn host_coupling_env_is_scrubbed_but_user_config_passes() {
    // SAFETY: このテストプロセスの環境を書く。並走テストは読まない名前を使う。
    unsafe {
        std::env::set_var("CLAUDE_CODE_MESSAGING_SOCKET", "dead.sock");
        std::env::set_var("CLAUDECODE", "1");
        std::env::set_var("ANTHROPIC_BASE_URL_TESTPROBE", "keep");
    }
    for (name, expect) in [("CLAUDE_CODE_MESSAGING_SOCKET", "absent"), ("CLAUDECODE", "absent"), ("ANTHROPIC_BASE_URL_TESTPROBE", "present")] {
        let i = inv(CliKind::Custom, &["env", name], "", PromptTransport::Stdin, Structured::None);
        let (o, _tx) = opts(10);
        let ok = run(&i, o, |_| {}).await.unwrap();
        assert_eq!(ok.text.trim(), format!("{name}={expect}"));
    }
    // 追加の env_remove (OAuth を使わせるための鍵外し) も効く。
    let i = inv(CliKind::Custom, &["env", "ANTHROPIC_BASE_URL_TESTPROBE"], "", PromptTransport::Stdin, Structured::None);
    let (mut o, _tx) = opts(10);
    o.env_remove = vec!["ANTHROPIC_BASE_URL_TESTPROBE".into()];
    let ok = run(&i, o, |_| {}).await.unwrap();
    assert_eq!(ok.text.trim(), "ANTHROPIC_BASE_URL_TESTPROBE=absent");
}

/// 孫 kill の検査は `process_alive` に依存する。常に false を返す実装でも通ってしまうので、
/// 生きているプロセス (自分) で true、確実に終わったプロセスで false を先に固定する。
#[test]
fn process_alive_distinguishes_live_and_dead() {
    assert!(process_alive(std::process::id()), "自分自身は生きている");
    let child = std::process::Command::new(FAKE).args(["fail", "0"]).stderr(std::process::Stdio::null()).spawn().unwrap();
    let pid = child.id();
    let _ = child.wait_with_output().unwrap();
    assert!(!process_alive(pid), "終了済みのプロセス {pid} が生きていると判定された");
}

#[test]
fn scratch_dir_is_under_temp_not_the_repo() {
    assert!(!scratch().starts_with(Path::new(env!("CARGO_MANIFEST_DIR"))));
}
