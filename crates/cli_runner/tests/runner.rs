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

/// ログを数えるテスト用の隔離された cwd (共有 scratch では他のテストの刈り取りと干渉する)。
fn scratch_named(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cli_runner_test_{}_{name}", std::process::id()));
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

/// 2026-09-12 の live 障害の形: claude が result 行を出さずに終了した。
/// **落ちた run から手掛かりが残ること**を固定する — 生ログのパスと、子の終了コード。
#[tokio::test]
async fn truncated_stream_keeps_the_raw_log_and_reports_exit_code() {
    let mut i = inv(CliKind::Claude, &["stream-truncated", "1"], "", PromptTransport::Stdin, Structured::JsonSchema);
    i.cwd = scratch_named("truncated");
    let (o, _tx) = opts(10);
    let err = run(&i, o, |_| {}).await.unwrap_err();
    let CliError::Shape { detail, raw } = &err.error else { panic!("Shape であるべき: {:?}", err.error) };
    assert!(detail.contains("result 行が無い"), "{detail}");
    assert!(detail.contains("終了コード 1"), "終了コードが detail に無い: {detail}");
    assert!(raw.contains("出力しました"), "JSON でない行が raw に残っていない: {raw}");

    let path = err.log_path.as_ref().expect("生ログのパスが RunFailed に無い");
    let body = std::fs::read_to_string(path).unwrap();
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines.len(), 3, "stdout の 3 行が揃っていない: {lines:?}");
    assert!(lines[0].contains(r#""subtype":"init""#));
    assert!(lines[2].contains("出力しました"), "JSON でない行がログから落ちている");
    assert!(err.to_string().contains(&path.display().to_string()), "文言にログのパスが無い: {err}");
}

/// 成功した run の封筒も fixture の原料になるので残す。
#[tokio::test]
async fn successful_run_also_leaves_the_raw_log() {
    let mut i = inv(CliKind::Claude, &["stream-echo"], "body", PromptTransport::Stdin, Structured::JsonSchema);
    i.cwd = scratch_named("success_log");
    let (o, _tx) = opts(10);
    let before = log_files(&i.cwd);
    run(&i, o, |_| {}).await.unwrap();
    let after = log_files(&i.cwd);
    let fresh: Vec<_> = after.iter().filter(|p| !before.contains(p)).collect();
    assert_eq!(fresh.len(), 1, "成功 run のログが 1 本増えていない");
    assert_eq!(std::fs::read_to_string(fresh[0]).unwrap().lines().count(), 3);
}

/// ログは無限に溜めない。
#[tokio::test]
async fn old_raw_logs_are_pruned() {
    let mut i = inv(CliKind::Claude, &["stream-echo"], "body", PromptTransport::Stdin, Structured::JsonSchema);
    i.cwd = scratch_named("prune");
    let dir = i.cwd.join("cli-logs");
    std::fs::create_dir_all(&dir).unwrap();
    for n in 0..(cli_runner::runner::CLI_LOG_KEEP + 12) {
        std::fs::write(dir.join(format!("19700101-0000{n:02}-0.jsonl")), "x
").unwrap();
        std::fs::write(dir.join(format!("19700101-0000{n:02}-0.invocation.json")), "{}").unwrap();
    }
    let (o, _tx) = opts(10);
    run(&i, o, |_| {}).await.unwrap();
    assert!(log_files(&i.cwd).len() <= cli_runner::runner::CLI_LOG_KEEP, "古いログが刈られていない");
    let orphans = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
            name.ends_with(".invocation.json") && !dir.join(name.replace(".invocation.json", ".jsonl")).exists()
        })
        .count();
    assert_eq!(orphans, 0, "起動の記録だけが孤児として残っている");
}

/// 何を起動したかも残す (argv が分からないと再現できない。2026-09-12)。
/// **本文は argv に載らない契約**なので、ここにも出てはいけない。
#[tokio::test]
async fn the_invocation_is_recorded_next_to_the_raw_log() {
    let mut i = inv(CliKind::Claude, &["stream-truncated", "1"], "秘密の指示本文", PromptTransport::Stdin, Structured::JsonSchema);
    i.cwd = scratch_named("invocation");
    let (o, _tx) = opts(10);
    let mut progress = Vec::new();
    let err = run(&i, o, |e| {
        if let CliEvent::Progress { text } = &e {
            progress.push(text.clone());
        }
    })
    .await
    .unwrap_err();

    let side = err.log_path.as_ref().unwrap().with_extension("invocation.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&side).unwrap()).unwrap();
    assert_eq!(v["args"][0], "stream-truncated");
    assert!(!side.display().to_string().is_empty());
    assert!(!std::fs::read_to_string(&side).unwrap().contains("秘密の指示本文"), "本文が argv の記録に漏れている");

    let started = progress.iter().find(|t| t.starts_with("起動: ")).expect("起動の 1 行が進捗に無い");
    assert!(started.contains("stream-truncated"), "{started}");
    assert!(!started.contains("秘密の指示本文"), "本文が進捗ログに漏れている");
}

/// `--json-schema` の値は長さだけにする (数千字がログを埋める)。
#[test]
fn invocation_summary_abbreviates_the_schema() {
    let args: Vec<String> = ["-p", "--json-schema", r#"{"type":"object"}"#, "--add-dir", "D:/x"].iter().map(|s| s.to_string()).collect();
    let line = cli_runner::runner::invocation_summary("claude", &args);
    assert_eq!(line, "起動: claude -p --json-schema <schema 17 字> --add-dir D:/x");
}

fn log_files(cwd: &Path) -> Vec<PathBuf> {
    let dir = cwd.join("cli-logs");
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    // 起動の記録 (.invocation.json) は数えない — 数えたいのはログの本数。
    let mut v: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    v.sort();
    v
}

#[test]
fn scratch_dir_is_under_temp_not_the_repo() {
    assert!(!scratch().starts_with(Path::new(env!("CARGO_MANIFEST_DIR"))));
}
