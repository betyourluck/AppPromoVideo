//! fenced JSON 救済 (契約 `CliInvocation.structured = fenced_json`)。
//!
//! 構造化出力の口を持たない CLI (aider / custom) と、claude の `structured_output` が空だった
//! 時の保険。「JSON だけ出せ」と指示しても散文で包まれることがある。
//!
//! 規則 (spec 01 rev2 決定 3、査読 4 起点):
//! 1. `` ```json `` フェンスがあれば **最後の**フェンスの中身 (LLM は最終案を末尾に置く)。
//! 2. 無ければ本文中の **括弧バランスが取れた最上位オブジェクトのうち最後のもの**。
//!    文字列リテラル内の `{` `}` (エスケープ含む) は数えない。
//! 3. 「最初の `{` から最後の `}`」は採らない — 2 つのオブジェクトを跨いで拾う (Red で再現済み)。
//!
//! Kataribe failures #29 の no-tools フォールバックと同型。

/// テキストから JSON オブジェクトらしい部分を切り出す。見つからなければ None。
pub fn extract_json_object(text: &str) -> Option<&str> {
    if let Some(inner) = last_json_fence(text) {
        if let Some(obj) = last_balanced_object(inner) {
            return Some(obj);
        }
    }
    last_balanced_object(text)
}

/// 最後の ```json フェンスの中身 (閉じフェンスが無ければ末尾まで)。
fn last_json_fence(text: &str) -> Option<&str> {
    let open = "```json";
    let start = text.rfind(open)? + open.len();
    let body = &text[start..];
    let end = body.find("```").unwrap_or(body.len());
    Some(body[..end].trim())
}

/// 括弧バランスの取れた最上位 `{…}` のうち最後のもの。文字列内の括弧は無視する。
fn last_balanced_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut last: Option<(usize, usize)> = None;
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' if depth > 0 => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    last = Some((start, i + 1));
                }
            }
            _ => {}
        }
    }
    last.map(|(s, e)| &text[s..e])
}

/// 切り出して parse まで行う。parse 失敗は生テキストを添えて返す (再生成の燃料)。
pub fn parse_json_object(text: &str) -> Result<serde_json::Value, String> {
    let slice = extract_json_object(text).ok_or_else(|| format!("JSON オブジェクトが無い: {}", head(text)))?;
    serde_json::from_str(slice).map_err(|e| format!("JSON として読めない ({e}): {}", head(slice)))
}

fn head(s: &str) -> String {
    s.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_json_fence() {
        let t = "Here you go:\n```json\n{\"a\": 1}\n```\nand {\"b\": 2} trailing";
        assert_eq!(extract_json_object(t), Some("{\"a\": 1}"));
    }

    #[test]
    fn falls_back_to_balanced_object_in_prose() {
        let t = "Sure! {\"scenes\": [{\"id\": 1}]} Hope this helps.";
        assert_eq!(extract_json_object(t), Some("{\"scenes\": [{\"id\": 1}]}"));
    }

    /// 査読 4 (2026-09-07): 複数の {} があると誤抽出する → 最後のフェンス優先 + 括弧バランス。
    #[test]
    fn last_fence_wins_over_first() {
        let t = "Draft:\n```json\n{\"x\": 1}\n```\nFinal answer:\n```json\n{\"x\": 2}\n```";
        assert_eq!(extract_json_object(t), Some("{\"x\": 2}"));
    }

    #[test]
    fn balanced_scan_ignores_braces_inside_strings() {
        let t = "note {\"a\": \"}\", \"b\": {\"c\": \"{\"}} done";
        assert_eq!(extract_json_object(t), Some("{\"a\": \"}\", \"b\": {\"c\": \"{\"}}"));
    }

    #[test]
    fn multiple_bare_objects_take_the_last_balanced_one() {
        let t = "Option A: {\"a\": 1}\nOption B (final): {\"b\": {\"n\": 2}}\nthanks";
        assert_eq!(extract_json_object(t), Some("{\"b\": {\"n\": 2}}"));
    }

    #[test]
    fn unbalanced_tail_does_not_swallow_a_good_object() {
        let t = "{\"ok\": true} and then a broken one {\"bad\": ";
        assert_eq!(extract_json_object(t), Some("{\"ok\": true}"));
    }

    #[test]
    fn escaped_quote_inside_string_does_not_end_the_string() {
        let t = "{\"s\": \"say \\\"}\\\" now\"}";
        assert_eq!(extract_json_object(t), Some(t));
        assert!(parse_json_object(t).is_ok());
    }

    #[test]
    fn unclosed_fence_still_yields_object() {
        let t = "```json\n{\"a\": 1}";
        assert_eq!(extract_json_object(t), Some("{\"a\": 1}"));
    }

    #[test]
    fn none_when_no_object() {
        assert_eq!(extract_json_object("no json here"), None);
        assert_eq!(extract_json_object("} {"), None);
        assert_eq!(extract_json_object("{\"open\": "), None);
    }

    #[test]
    fn parse_error_keeps_raw_head() {
        let err = parse_json_object("{not json}").unwrap_err();
        assert!(err.contains("{not json}"));
    }
}
