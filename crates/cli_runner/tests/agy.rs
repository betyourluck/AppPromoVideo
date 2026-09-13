//! agy の封筒と見張りの PoC。**実測した stream をそのまま fixture にしている** (2026-09-12、agy 1.2.2)。

use cli_runner::CliError;
use cli_runner::agy::{AgyLine, early_abort, fold_lines, parse_line, tool_is_allowed, tool_notice, user_message_line};

/// `--json-schema` つきで成功した run。
const OK: &str = include_str!("../fixtures/agy_json_schema_ok.jsonl");
/// run_command が headless で自動拒否され、**status=SUCCESS のまま本文が空**だった run。
/// 末尾に JSON でない散文行 (`jetski: …`) が付く。
const EMPTY: &str = include_str!("../fixtures/agy_empty_success.jsonl");
/// **write_to_file が拒否されずに通った** run (probe.txt が実際に作られた)。見張りの被験体。
const WRITE: &str = include_str!("../fixtures/agy_write_allowed.jsonl");
/// 2026-09-13 ユーザー実機 (パスは中立化)。agy 側の PreToolUse hook が壊れていて `view_file` が
/// `state: ERROR` で失敗し、agy がシェル (`run_command`) へ逃げた。見張りは 2 手目で止めたが、
/// **逃げた理由 (1 手目の失敗) が進捗ログに出ていなかった** ので、ユーザーには見張りのエラーしか見えなかった。
const ESCAPE: &str = include_str!("../fixtures/agy_tool_error_escape.jsonl");

#[test]
fn ok_fixture_folds_into_text_and_structured_output() {
    let fold = fold_lines(OK.lines()).unwrap();
    assert_eq!(fold.structured, Some(serde_json::json!({"answer": "answer=ok"})));
    assert!(fold.transcript.contains("answer=ok"), "{}", fold.transcript);
    assert!(fold.conversation_id.is_some());
    assert!(fold.duration_ms.unwrap() >= 4000, "duration_seconds を ms に直せていない: {:?}", fold.duration_ms);
}

/// **費用は返らない。** トークン数から計算して埋めない (捏造しない)。
#[test]
fn the_envelope_carries_no_cost() {
    assert!(!OK.contains("total_cost_usd"), "agy が費用を返すようになったなら契約を見直すこと");
}

/// status=SUCCESS でも中身が空なら成功にしない。**理由は JSON でない行にしか書かれていない。**
#[test]
fn empty_success_is_an_error_and_keeps_the_plain_line() {
    let err = fold_lines(EMPTY.lines()).unwrap_err();
    match err {
        CliError::EmptySuccess { note } => {
            assert!(note.contains("auto-denied"), "拒否の理由が残っていない: {note}");
        }
        other => panic!("EmptySuccess であるべき: {other:?}"),
    }
}

/// 見張り: 読み取りは通し、書き込みは止める。
#[test]
fn the_watchdog_allows_reads_and_stops_writes() {
    for t in ["view_file", "list_dir", "find_by_name", "grep_search"] {
        assert!(tool_is_allowed(t), "{t} は読み取りなので通すべき");
    }
    for t in ["write_to_file", "run_command", "sed_file", "multi_replace_file_content", "notebook_edit", "browser_click_element", "generate_image"] {
        assert!(!tool_is_allowed(t), "{t} を通してはいけない");
    }
}

/// 実測した「書き込みが通った run」から、見張りが止める行を拾えること。
#[test]
fn the_write_run_is_aborted_at_the_tool_line() {
    let abort = WRITE.lines().map(parse_line).find_map(|l| early_abort(&l));
    match abort {
        Some(CliError::ToolNotAllowed { tool, target }) => {
            assert_eq!(tool, "write_to_file");
            assert!(target.contains("probe.txt"), "対象が運ばれていない: {target}");
        }
        other => panic!("ToolNotAllowed で止まるべき: {other:?}"),
    }
}

/// 読み取りだけの run では見張りは何も止めない (誤検出しない)。
#[test]
fn reads_do_not_trip_the_watchdog() {
    let tools: Vec<String> = EMPTY
        .lines()
        .map(parse_line)
        .filter_map(|l| match l {
            AgyLine::Tool { name, .. } => Some(name),
            _ => None,
        })
        .collect();
    assert!(tools.iter().any(|t| t == "view_file"), "読み取りの tool event が fixture に無い: {tools:?}");
    let stopped: Vec<String> = EMPTY
        .lines()
        .map(parse_line)
        .filter_map(|l| early_abort(&l))
        .map(|e| match e {
            CliError::ToolNotAllowed { tool, .. } => tool,
            other => panic!("ToolNotAllowed 以外で止まった: {other:?}"),
        })
        .collect();
    // 止まるのは run_command **だけ**。空で通ってしまわないよう、止まった件数も固定する
    // (見張りを無効化すると stopped が空になり、all() は真のまま通っていた)。
    assert!(!stopped.is_empty(), "許可外の run_command で止まっていない");
    assert!(stopped.iter().all(|t| t == "run_command"), "読み取りで誤検出している: {stopped:?}");
}

/// 本文は **stdin の NDJSON** で運ぶ (argv に載せない)。
#[test]
fn the_user_message_is_ndjson_with_an_event_tag() {
    let line = user_message_line("解析してください\n2 行目");
    assert!(line.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
    assert_eq!(v["event"], "user");
    assert_eq!(v["message"]["content"], "解析してください\n2 行目");
}

#[test]
fn a_failed_tool_step_carries_its_error_message() {
    let err = ESCAPE
        .lines()
        .map(parse_line)
        .find_map(|l| match l {
            AgyLine::Tool { name, state, error, .. } if state == "ERROR" => Some((name, error)),
            _ => None,
        })
        .expect("ERROR の tool 行がある");
    assert_eq!(err.0, "view_file");
    let msg = err.1.expect("error.message が運ばれる");
    // 1 行目だけ (stderr のスタックトレースは生ログにある)。
    assert!(msg.starts_with("JSON hook \"jsonhook__vendor.telemetry_PreToolUse_0_0\" failed"), "{msg}");
    assert!(!msg.contains('\n') && !msg.contains('\r'), "複数行を進捗に流さない: {msg:?}");
}

#[test]
fn the_progress_log_names_the_failure_before_the_watchdog_fires() {
    // 進捗に出る行を順に集め、見張りが止めた位置も取る。
    let mut notices = Vec::new();
    let mut abort = None;
    for line in ESCAPE.lines() {
        let parsed = parse_line(line);
        if let Some(n) = tool_notice(&parsed) {
            notices.push(n);
        }
        if abort.is_none() {
            abort = early_abort(&parsed);
        }
    }
    assert_eq!(
        notices,
        vec![
            "ツール: view_file D:/repo/README_jp.md".to_string(),
            "ツール失敗: view_file D:/repo/README_jp.md — JSON hook \"jsonhook__vendor.telemetry_PreToolUse_0_0\" failed: command failed: exit status 1, stderr: node:internal/modules/cjs/loader:1479".to_string(),
            "ツール: run_command Get-Content D:\\repo\\README_jp.md -TotalCount 100".to_string(),
        ]
    );
    assert!(matches!(abort, Some(CliError::ToolNotAllowed { ref tool, .. }) if tool == "run_command"));
}

#[test]
fn a_successful_tool_step_produces_no_failure_notice() {
    let failures: Vec<String> = OK
        .lines()
        .map(parse_line)
        .filter_map(|l| tool_notice(&l))
        .filter(|n| n.starts_with("ツール失敗"))
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
}
