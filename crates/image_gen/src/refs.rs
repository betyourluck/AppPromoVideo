//! 参照画像の枚数上限 (契約 `ImageGenConfig.reference_limits`、rev2 査読 9)。
//!
//! 上限 (`provider::max_refs` = Kataribe の凍結値、全プロバイダ 3) と、既定で送る枚数を別に持つ。
//! OpenAI `images/edits` は Kataribe の live が 1 枚のみなので既定 1。切り詰めたら**黙らず**報告する。

use crate::provider::{Provider, RefImage, max_refs};

/// 既定で送る枚数 (契約 `default_send`)。
pub fn default_send(provider: Provider) -> usize {
    match provider {
        Provider::Openai => 1,
        Provider::Gemini => 3,
        Provider::Comfy => 3,
    }
}

/// 切り詰めの報告 (UI に『N 枚のうち先頭 M 枚』と出す)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Truncated {
    pub total: usize,
    pub sent: usize,
    pub limit_by: &'static str,
}

/// 送る参照を選ぶ (純粋)。`requested` = ユーザーが送りたい枚数 (None なら既定)。
/// 送る枚数 = min(requested or default, api_max, refs.len())。先頭から (ユーザーの並び順)。
pub fn select_refs(provider: Provider, refs: &[RefImage], requested: Option<usize>) -> (Vec<RefImage>, Option<Truncated>) {
    let api = max_refs(provider);
    let want = requested.unwrap_or_else(|| default_send(provider));
    let (n, limit_by) = if want <= api { (want, if requested.is_some() { "requested" } else { "default" }) } else { (api, "api_max") };
    let n = n.min(refs.len());
    let sent: Vec<RefImage> = refs.iter().take(n).cloned().collect();
    let truncated = if n < refs.len() { Some(Truncated { total: refs.len(), sent: n, limit_by }) } else { None };
    (sent, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(n: usize) -> Vec<RefImage> {
        (0..n).map(|i| RefImage { name: format!("s{i}.png"), mime: "image/png".into(), bytes: vec![i as u8] }).collect()
    }

    #[test]
    fn openai_defaults_to_one_and_reports_truncation() {
        let (sent, t) = select_refs(Provider::Openai, &refs(3), None);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].name, "s0.png", "先頭から (ユーザーの並び順)");
        assert_eq!(t, Some(Truncated { total: 3, sent: 1, limit_by: "default" }));
    }

    #[test]
    fn requested_is_capped_by_api_max() {
        let (sent, t) = select_refs(Provider::Gemini, &refs(5), Some(10));
        assert_eq!(sent.len(), 3);
        assert_eq!(t, Some(Truncated { total: 5, sent: 3, limit_by: "api_max" }));
        // Kataribe の max_refs は全プロバイダ 3 (OpenAI の API 上限 16 は本 workspace では未検証 → 採らない)。
        let (sent, t) = select_refs(Provider::Openai, &refs(5), Some(4));
        assert_eq!(sent.len(), 3);
        assert_eq!(t.unwrap().limit_by, "api_max");
        let (sent, t) = select_refs(Provider::Openai, &refs(5), Some(2));
        assert_eq!(sent.len(), 2);
        assert_eq!(t.unwrap().limit_by, "requested");
    }

    #[test]
    fn no_truncation_when_everything_fits() {
        let (sent, t) = select_refs(Provider::Comfy, &refs(2), None);
        assert_eq!(sent.len(), 2);
        assert_eq!(t, None);
        let (sent, t) = select_refs(Provider::Comfy, &[], None);
        assert!(sent.is_empty() && t.is_none());
    }
}
