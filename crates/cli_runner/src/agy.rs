//! agy (`--input-format stream-json --output-format stream-json`) の解析 (純粋)。契約 `AgyStreamLine`。
//!
//! **claude とは別系統の CLI。** タグは `event` で、終端は `result.status` (`is_error` ではない)。
//! 費用は返らない (トークン数だけ) ので `cost_usd` は None のまま — トークンから計算して埋めない。
//!
//! **JSON でない散文行 (`jetski: …`) が stdout に混ざる。** 捨てない — `EmptySuccess` の理由は
//! その行にしか書かれていない (2026-09-12 実測)。既知でない行は `Other` として保持する
//! (Kataribe #75: 受信側の allowlist は「誤って落とすコストが高い配送」では既定で通す)。

use serde_json::Value;

use crate::error::CliError;

/// 1 行の解釈 (契約 `AgyStreamLine`)。
#[derive(Debug, Clone, PartialEq)]
pub enum AgyLine {
    Init { conversation_id: String, cwd: Option<String>, permission_mode: Option<String> },
    /// `step_update` の `agent_response` が運ぶ人が読む断片。
    Text(String),
    /// `step_update` の `tool`。見張り (契約 `IsolationGuarantee.watchdog`) が見る。
    ///
    /// `error` は `state: "ERROR"` の時の `tool_info.error.message` の **1 行目**。
    /// ツールが失敗すると agy はシェル (`run_command`) へ逃げ、見張りはそこで止める —
    /// **逃げた理由はこの 1 行にしか無い** (2026-09-13 実機: 壊れた PreToolUse hook で全ツールが失敗していた)。
    Tool { name: String, state: String, target: String, error: Option<String> },
    Result {
        status: String,
        response: String,
        error: Option<String>,
        duration_seconds: Option<f64>,
        structured: Option<Value>,
    },
    /// 既知でない event / JSON でない行 (`jetski: …` 等)。**捨てない**。
    Other(String),
}

/// 読み取り専用として通すツール (契約 `IsolationGuarantee.watchdog.allowed`)。
///
/// **allow リスト = 既定で止める。** deny リストにしない — agy のツールは 57 個あり、
/// `sed_file` / `multi_replace_file_content` / `notebook_edit` / browser 系 / `generate_image` を
/// 数え漏らすとそこが穴になる。
///
/// `finish` は副作用の無い完了合図。**tool event としては未観測**だが、外すと正常な run を
/// 殺しかねないので入れてある (live で要確認)。
pub const AGY_ALLOWED_TOOLS: [&str; 5] = ["view_file", "list_dir", "find_by_name", "grep_search", "finish"];

/// stdin に流す 1 行 (契約 `AgyStreamLine.input`)。
///
/// **`-p` / `--print` は使わない** — あれは値 (prompt) を取るので本文が argv に載り、契約を破る。
pub fn user_message_line(body: &str) -> String {
    let v = serde_json::json!({ "event": "user", "message": { "content": body } });
    format!("{v}\n")
}

/// 1 行を解釈する。
pub fn parse_line(line: &str) -> AgyLine {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() {
        return AgyLine::Other(String::new());
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return AgyLine::Other(line.to_string()),
    };
    match v.get("event").and_then(Value::as_str) {
        Some("init") => AgyLine::Init {
            conversation_id: v.get("conversation_id").and_then(Value::as_str).unwrap_or("").to_string(),
            cwd: v.pointer("/init/cwd").and_then(Value::as_str).map(str::to_string),
            permission_mode: v.pointer("/init/permission_mode").and_then(Value::as_str).map(str::to_string),
        },
        Some("step_update") => {
            let s = v.get("step_update").cloned().unwrap_or(Value::Null);
            match s.get("step_type").and_then(Value::as_str) {
                Some("tool") => AgyLine::Tool {
                    name: s.get("tool_name").and_then(Value::as_str).unwrap_or("").to_string(),
                    state: s.get("state").and_then(Value::as_str).unwrap_or("").to_string(),
                    target: tool_target(s.get("tool_info")),
                    error: tool_error(s.get("tool_info")),
                },
                _ => AgyLine::Text(s.get("text_delta").and_then(Value::as_str).unwrap_or("").to_string()),
            }
        }
        Some("result") => {
            let r = v.get("result").cloned().unwrap_or(Value::Null);
            AgyLine::Result {
                status: r.get("status").and_then(Value::as_str).unwrap_or("").to_string(),
                response: r.get("response").and_then(Value::as_str).unwrap_or("").to_string(),
                error: r.get("error").and_then(Value::as_str).filter(|e| !e.is_empty()).map(str::to_string),
                duration_seconds: r.get("duration_seconds").and_then(Value::as_f64),
                structured: r.get("structured_output").filter(|s| !s.is_null()).cloned(),
            }
        }
        _ => AgyLine::Other(line.to_string()),
    }
}

/// `tool_info.parameters` から人が読む対象 (パス / コマンド) を 1 つ拾う。無ければ空。
fn tool_target(info: Option<&Value>) -> String {
    let Some(params) = info.and_then(|i| i.get("parameters")).and_then(Value::as_object) else {
        return String::new();
    };
    params.values().find_map(|v| v.as_str()).unwrap_or("").to_string()
}

/// `tool_info.error.message` の 1 行目 (空なら None)。stderr のスタックトレースは生ログに残るので運ばない。
fn tool_error(info: Option<&Value>) -> Option<String> {
    let msg = info.and_then(|i| i.pointer("/error/message")).and_then(Value::as_str)?;
    let first = msg.lines().next().unwrap_or("").trim_end_matches('\r').trim();
    (!first.is_empty()).then(|| first.to_string())
}

/// 進捗ログに出す 1 行。`ACTIVE` は「何を読んだか」、`ERROR` は「なぜ次の手が要ったか」。
///
/// 見張り (`early_abort`) が許可外のツールで止めた時、ユーザーに見えるのはそのエラーだけで、
/// **その直前にツールが失敗していた事実**が無いと「なぜシェルに逃げたか」が読めない
/// (2026-09-13 実機: `view_file` が hook の故障で ERROR → `run_command` → 見張り。進捗には
/// `ツール: view_file` しか出ておらず、原因は生ログを開くまで分からなかった)。
pub fn tool_notice(line: &AgyLine) -> Option<String> {
    match line {
        AgyLine::Tool { name, state, target, .. } if state == "ACTIVE" => Some(format!("ツール: {name} {target}")),
        AgyLine::Tool { name, state, target, error } if state == "ERROR" => {
            let why = error.as_deref().unwrap_or("(理由なし)");
            Some(format!("ツール失敗: {name} {target} — {why}"))
        }
        _ => None,
    }
}

/// 許可外のツールか (契約 `IsolationGuarantee.watchdog`)。
pub fn tool_is_allowed(name: &str) -> bool {
    AGY_ALLOWED_TOOLS.contains(&name)
}

/// この行を見た時点で**木ごと止める**べきエラー。
///
/// **検出であって予防ではない。** `state: "ACTIVE"` を見た時点で、その書き込みはもう始まっている
/// (2026-09-12 実測: `write_to_file` は headless でも拒否されず、実際にファイルが作られた)。
/// 止められるのは 2 手目以降。
pub fn early_abort(line: &AgyLine) -> Option<CliError> {
    match line {
        AgyLine::Tool { name, target, .. } if !tool_is_allowed(name) => {
            Some(CliError::ToolNotAllowed { tool: name.clone(), target: target.clone() })
        }
        _ => None,
    }
}

/// 全行を畳んだ結果 (claude の `StreamFold` に対応)。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AgyFold {
    pub conversation_id: Option<String>,
    pub transcript: String,
    pub result_text: String,
    pub duration_ms: Option<u64>,
    pub structured: Option<Value>,
}

/// 行列を畳む。
///
/// - result 行が無い → `Shape` (途中で死んだ)。raw には**最後の JSON でない行**を載せる
///   (2026-09-12 の障害では、それが唯一の手掛かりだった)。
/// - `status != SUCCESS` → `Auth` (認証語を含む時) か `Shape`。
/// - `SUCCESS` でも本文も構造化出力も空 → `EmptySuccess`。理由は散文行にしか無いので note に運ぶ。
pub fn fold_lines<'a, I: IntoIterator<Item = &'a str>>(lines: I) -> Result<AgyFold, CliError> {
    let mut fold = AgyFold::default();
    let mut result: Option<(String, String, Option<String>)> = None;
    let mut plain_tail = String::new();
    for line in lines {
        match parse_line(line) {
            AgyLine::Init { conversation_id, .. } => fold.conversation_id = Some(conversation_id),
            AgyLine::Text(t) => fold.transcript.push_str(&t),
            AgyLine::Tool { .. } => {}
            AgyLine::Result { status, response, error, duration_seconds, structured } => {
                fold.result_text = response.clone();
                fold.duration_ms = duration_seconds.map(|s| (s * 1000.0) as u64);
                fold.structured = structured;
                result = Some((status, response, error));
            }
            AgyLine::Other(raw) => {
                if !raw.is_empty() {
                    plain_tail = raw;
                }
            }
        }
    }
    let Some((status, response, error)) = result else {
        return Err(CliError::Shape { detail: "result 行が無い (途中で終了)".into(), raw: head(&plain_tail) });
    };
    if status != "SUCCESS" {
        let msg = error.unwrap_or_else(|| plain_tail.clone());
        if msg.to_lowercase().contains("authenticate") {
            return Err(CliError::Auth { message: msg });
        }
        return Err(CliError::Shape { detail: format!("result.status={status}"), raw: head(&msg) });
    }
    if response.trim().is_empty() && fold.structured.is_none() {
        // 理由は JSON でない行にしか書かれていない (`jetski: … auto-denied`)。
        return Err(CliError::EmptySuccess { note: head(&plain_tail) });
    }
    Ok(fold)
}

fn head(s: &str) -> String {
    s.chars().take(300).collect()
}
