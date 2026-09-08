//! claude `-p --output-format stream-json` (NDJSON) の解析 (純粋)。契約 `ClaudeStreamLine`。
//!
//! 既知の `type` だけ読み、未知は `Other` として**捨てずに**流す (Kataribe #75 の教訓:
//! 受信側の allowlist は「誤って落とすコストが高い配送」では既定で通す)。
//! 構造化出力 (`--json-schema`) の置き場は実測待ち (Phase B) — `result.structured_output` と
//! `result` 本文の fenced JSON の両方を試す。

use serde_json::Value;

use crate::error::CliError;

/// 1 行の解釈。
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedLine {
    /// `{"type":"system","subtype":"init",...}`
    Init { session_id: String },
    /// assistant の text 断片 (人が読む)。
    Text(String),
    /// assistant に `error` が付いた (例 `authentication_failed`)。本文は content[0].text。
    AssistantError { error: String, message: String },
    /// `{"type":"system","subtype":"api_retry",...}` (公式: attempt / max_retries / retry_delay_ms /
    /// error_status / error)。2026-09-08 実測: 401 は 10 回・約 2.5 分再試行してから result に落ちる。
    ApiRetry { attempt: u32, max_retries: u32, retry_delay_ms: u64, error_status: Option<u16>, error: String },
    /// 終端。
    Result {
        is_error: bool,
        text: String,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
        structured: Option<Value>,
    },
    /// 既知でない type / JSON でない行 (生で流す)。
    Other(String),
}

/// 1 行を解釈する。JSON でない行は `Other` (claude 以外の CLI の生 stdout もここを通る)。
pub fn parse_line(line: &str) -> ParsedLine {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return ParsedLine::Other(String::new());
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return ParsedLine::Other(line.to_string()),
    };
    match v.get("type").and_then(Value::as_str) {
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("init") => ParsedLine::Init {
            session_id: v.get("session_id").and_then(Value::as_str).unwrap_or("").to_string(),
        },
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("api_retry") => ParsedLine::ApiRetry {
            attempt: v.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32,
            max_retries: v.get("max_retries").and_then(Value::as_u64).unwrap_or(0) as u32,
            retry_delay_ms: v.get("retry_delay_ms").and_then(Value::as_u64).unwrap_or(0),
            error_status: v.get("error_status").and_then(Value::as_u64).map(|s| s as u16),
            error: v.get("error").and_then(Value::as_str).unwrap_or("unknown").to_string(),
        },
        Some("assistant") => {
            let text = assistant_text(&v);
            if let Some(err) = v.get("error").and_then(Value::as_str) {
                return ParsedLine::AssistantError { error: err.to_string(), message: text };
            }
            ParsedLine::Text(text)
        }
        Some("result") => {
            let text = v.get("result").and_then(Value::as_str).unwrap_or("").to_string();
            let structured = v.get("structured_output").filter(|s| !s.is_null()).cloned();
            ParsedLine::Result {
                is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                text,
                cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
                duration_ms: v.get("duration_ms").and_then(Value::as_u64),
                structured,
            }
        }
        _ => ParsedLine::Other(line.to_string()),
    }
}

fn assistant_text(v: &Value) -> String {
    v.pointer("/message/content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// runner がこの行を見た時点で **プロセスを止めて** 返すべきエラー (契約 `CliError.early_abort`)。
///
/// **`api_retry` では止めない。** 初案は「`authentication_failed` の初回再試行で打ち切る」だったが、
/// 2026-09-08 の成功 fixture (`claude_json_schema_ok.jsonl`) は 401 の再試行を **7 回**経てから成功して
/// いる (OAuth 期限切れ + ANTHROPIC_API_KEY あり)。再試行の途中で殺すと成功する run を殺す。
/// 止めるのは終端 (`assistant.error`) だけ。再試行は [`retry_notice`] で進捗として見せ、待つか
/// 中断するかはユーザーが決める。
pub fn early_abort(line: &ParsedLine) -> Option<CliError> {
    match line {
        ParsedLine::AssistantError { error, message } if error == "authentication_failed" => {
            Some(CliError::Auth { message: message.clone() })
        }
        _ => None,
    }
}

/// `api_retry` を人が読む 1 行にする (UI の進捗ログ用)。それ以外は None。
pub fn retry_notice(line: &ParsedLine) -> Option<String> {
    match line {
        ParsedLine::ApiRetry { attempt, max_retries, retry_delay_ms, error_status, error } => Some(format!(
            "再試行 {attempt}/{max_retries} ({error}{}) — {:.1} 秒待ち",
            error_status.map(|s| format!(", HTTP {s}")).unwrap_or_default(),
            *retry_delay_ms as f64 / 1000.0
        )),
        _ => None,
    }
}

/// 全行を畳んだ結果。
#[derive(Debug, Clone, PartialEq)]
pub struct StreamFold {
    pub session_id: Option<String>,
    /// assistant text を順に連結したもの (result 本文が空の時の代替)。
    pub transcript: String,
    pub result_text: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub structured: Option<Value>,
}

/// 行列を畳む。認証失敗は `CliError::Auth`、is_error の result は `Auth` か `Shape`、
/// result が無ければ `Shape` (途中で死んだ)。
pub fn fold_lines<'a, I: IntoIterator<Item = &'a str>>(lines: I) -> Result<StreamFold, CliError> {
    let mut fold = StreamFold {
        session_id: None,
        transcript: String::new(),
        result_text: String::new(),
        cost_usd: None,
        duration_ms: None,
        structured: None,
    };
    let mut auth_error: Option<String> = None;
    let mut saw_result = false;
    let mut result_is_error = false;
    let mut raw_tail = String::new();
    for line in lines {
        match parse_line(line) {
            ParsedLine::Init { session_id } => fold.session_id = Some(session_id),
            ParsedLine::Text(t) => fold.transcript.push_str(&t),
            ParsedLine::AssistantError { error, message } => {
                if error == "authentication_failed" || message.to_lowercase().contains("authenticate") {
                    auth_error = Some(message.clone());
                }
                fold.transcript.push_str(&message);
            }
            // 再試行は結果に影響しない (成功にも失敗にも混じる)。UI 向けの文言は retry_notice。
            ParsedLine::ApiRetry { .. } => {}
            ParsedLine::Result { is_error, text, cost_usd, duration_ms, structured } => {
                saw_result = true;
                result_is_error = is_error;
                fold.result_text = text;
                fold.cost_usd = cost_usd;
                fold.duration_ms = duration_ms;
                fold.structured = structured;
            }
            ParsedLine::Other(raw) => {
                if !raw.is_empty() {
                    raw_tail = raw;
                }
            }
        }
    }
    if let Some(message) = auth_error {
        return Err(CliError::Auth { message });
    }
    if !saw_result {
        return Err(CliError::Shape { detail: "result 行が無い (途中で終了)".into(), raw: head(&raw_tail) });
    }
    if result_is_error {
        let msg = if fold.result_text.is_empty() { fold.transcript.clone() } else { fold.result_text.clone() };
        if msg.to_lowercase().contains("authenticate") {
            return Err(CliError::Auth { message: msg });
        }
        return Err(CliError::Shape { detail: "result.is_error".into(), raw: head(&msg) });
    }
    Ok(fold)
}

fn head(s: &str) -> String {
    s.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-07 実測 (claude 2.1.223、Claude デスクトップの子セッション内 = OAuth 非継承)。
    const AUTH_FAILED: &str = include_str!("../fixtures/claude_auth_failed.jsonl");

    #[test]
    fn real_auth_failure_fixture_is_classified_as_auth() {
        let err = fold_lines(AUTH_FAILED.lines()).unwrap_err();
        match err {
            CliError::Auth { message } => assert!(message.contains("OAuth session expired"), "{message}"),
            other => panic!("Auth であるべき: {other:?}"),
        }
    }

    #[test]
    fn fixture_lines_parse_into_init_assistant_error_result() {
        let parsed: Vec<ParsedLine> = AUTH_FAILED.lines().map(parse_line).collect();
        assert_eq!(parsed.len(), 3);
        assert!(matches!(&parsed[0], ParsedLine::Init { session_id } if !session_id.is_empty()));
        assert!(matches!(&parsed[1], ParsedLine::AssistantError { error, .. } if error == "authentication_failed"));
        assert!(matches!(&parsed[2], ParsedLine::Result { is_error: true, .. }));
    }

    /// 2026-09-08 実測 (ユーザー端末): 401 を 10 回再試行 (約 2.5 分) してから result.is_error。
    const AUTH_RETRY_LOOP: &str = include_str!("../fixtures/claude_auth_retry_loop.jsonl");

    #[test]
    fn api_retry_lines_are_parsed_not_passed_through() {
        let retries: Vec<ParsedLine> = AUTH_RETRY_LOOP
            .lines()
            .map(parse_line)
            .filter(|p| matches!(p, ParsedLine::ApiRetry { .. }))
            .collect();
        assert_eq!(retries.len(), 10, "10 回の再試行が全部 ApiRetry として読めるべき");
        assert!(matches!(
            &retries[0],
            ParsedLine::ApiRetry { attempt: 1, max_retries: 10, error, error_status: Some(401), .. }
                if error == "authentication_failed"
        ));
    }

    #[test]
    fn auth_retry_loop_fixture_folds_to_auth() {
        assert!(matches!(fold_lines(AUTH_RETRY_LOOP.lines()).unwrap_err(), CliError::Auth { .. }));
    }

    /// 初案「認証の初回再試行で打ち切る」は成功 fixture で反証された (401 × 7 の後に成功)。
    /// 再試行では止めず、終端の assistant.error だけで止める。
    #[test]
    fn auth_retry_is_not_an_abort_signal_only_terminal_error_is() {
        let first_retry =
            AUTH_RETRY_LOOP.lines().map(parse_line).find(|p| matches!(p, ParsedLine::ApiRetry { .. })).unwrap();
        assert_eq!(early_abort(&first_retry), None, "401 の再試行後に成功する run が実在する");
        assert_eq!(
            retry_notice(&first_retry).as_deref(),
            Some("再試行 1/10 (authentication_failed, HTTP 401) — 0.5 秒待ち")
        );
        let terminal =
            AUTH_RETRY_LOOP.lines().map(parse_line).find(|p| matches!(p, ParsedLine::AssistantError { .. })).unwrap();
        assert!(matches!(early_abort(&terminal), Some(CliError::Auth { .. })));
        assert_eq!(early_abort(&ParsedLine::Text("x".into())), None);
        assert_eq!(retry_notice(&ParsedLine::Text("x".into())), None);
    }

    /// 2026-09-08 実測 (ユーザーレベルの ANTHROPIC_API_KEY を適用して採取): `--json-schema` の成功。
    /// 構造化出力は assistant の `StructuredOutput` tool_use → user の tool_result → result 行の
    /// `structured_output` (と `result` 文字列の JSON) に出る。途中に非認証の api_retry と
    /// 未知 type `rate_limit_event` を含む。
    const JSON_SCHEMA_OK: &str = include_str!("../fixtures/claude_json_schema_ok.jsonl");

    #[test]
    fn real_success_fixture_yields_structured_output() {
        let f = fold_lines(JSON_SCHEMA_OK.lines()).expect("成功 run は Ok");
        assert_eq!(f.structured, Some(serde_json::json!({"ok": true, "note": "fixture"})));
        // result 文字列にも同じ JSON が入る (fenced 救済の二次経路が同じ答えを出す)。
        assert_eq!(promo_core::fenced::parse_json_object(&f.result_text).unwrap(), f.structured.clone().unwrap());
        assert!(f.cost_usd.unwrap() > 0.0);
        assert!(f.duration_ms.unwrap() > 0);
        assert!(f.session_id.is_some());
        // 構造化タスクでは text ブロックが無く transcript は空でよい。
        assert_eq!(f.transcript, "");
    }

    /// この成功 run は **401 authentication_failed の再試行を 7 回**含む。ここで止めていたら成功を殺していた。
    #[test]
    fn real_success_fixture_has_auth_retries_that_must_not_abort() {
        let parsed: Vec<ParsedLine> = JSON_SCHEMA_OK.lines().map(parse_line).collect();
        let retries: Vec<&ParsedLine> = parsed.iter().filter(|p| matches!(p, ParsedLine::ApiRetry { .. })).collect();
        assert_eq!(retries.len(), 7);
        assert!(retries.iter().all(|r| matches!(r, ParsedLine::ApiRetry { error, error_status: Some(401), .. } if error == "authentication_failed")));
        for r in &retries {
            assert_eq!(early_abort(r), None, "{r:?}");
        }
        // 未知 type (rate_limit_event / user) は Other で素通し (捨てない)。
        assert!(parsed.iter().any(|p| matches!(p, ParsedLine::Other(s) if s.contains("rate_limit_event"))));
        // redact_stream.py は `"type": "user"` と空白入りで整形するので、区切りに依存しない語で探す。
        assert!(parsed.iter().any(|p| matches!(p, ParsedLine::Other(s) if s.contains("tool_result"))));
    }

    #[test]
    fn success_stream_folds_text_cost_and_structured() {
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
            r#"{"type":"system","subtype":"hook_started","hook_name":"x"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello "},{"type":"tool_use","name":"Read"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"world"}]}}"#,
            r#"{"type":"result","is_error":false,"result":"{\"ok\":true}","total_cost_usd":0.0123,"duration_ms":1500,"structured_output":{"ok":true}}"#,
        ];
        let f = fold_lines(lines).unwrap();
        assert_eq!(f.session_id.as_deref(), Some("s1"));
        assert_eq!(f.transcript, "Hello world");
        assert_eq!(f.result_text, "{\"ok\":true}");
        assert_eq!(f.cost_usd, Some(0.0123));
        assert_eq!(f.duration_ms, Some(1500));
        assert_eq!(f.structured, Some(serde_json::json!({"ok": true})));
    }

    #[test]
    fn missing_result_is_shape_with_raw_tail() {
        let lines = [r#"{"type":"system","subtype":"init","session_id":"s1"}"#, "garbage line"];
        match fold_lines(lines).unwrap_err() {
            CliError::Shape { detail, raw } => {
                assert!(detail.contains("result"));
                assert_eq!(raw, "garbage line");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_types_and_non_json_are_passed_through_not_dropped() {
        assert_eq!(
            parse_line(r#"{"type":"future_thing","x":1}"#),
            ParsedLine::Other(r#"{"type":"future_thing","x":1}"#.into())
        );
        assert_eq!(parse_line("plain text from aider\r\n"), ParsedLine::Other("plain text from aider".into()));
        assert_eq!(parse_line("   "), ParsedLine::Other(String::new()));
    }

    #[test]
    fn result_error_without_auth_is_shape() {
        let lines = [r#"{"type":"result","is_error":true,"result":"rate limited"}"#];
        assert!(matches!(fold_lines(lines).unwrap_err(), CliError::Shape { .. }));
    }
}
