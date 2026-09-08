//! スタイルアンカー (契約 `ImageGenConfig.differences_from_kataribe`: user_prefix の既定は VisualIdentity から合成)。
//!
//! 参照画像プロンプトの接頭辞。**参照 / スクリーンショット / 資料を指す語は書かない** (Kataribe #85)。
//! palette は hex だけ拾う (LLM が「#15110E (ink, 焦げ茶)」のように説明を混ぜて返した実例 — 2026-09-08 live)。

use crate::plan::VisualIdentity;

/// `#RRGGBB` (大文字化) を文字列から拾う。無ければ None。
pub fn extract_hex(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'#')?;
    let hex: String = s[start + 1..].chars().take_while(|c| c.is_ascii_hexdigit()).take(6).collect();
    if hex.len() == 6 { Some(format!("#{}", hex.to_ascii_uppercase())) } else { None }
}

/// 画素算出の色で palette を置き換える (契約 決定 8: 空なら LLM の値を残す)。
pub fn apply_palette(identity: &mut VisualIdentity, measured: Vec<String>) {
    if !measured.is_empty() {
        identity.palette = measured;
    }
}

/// 接頭辞 1 本。空の要素は飛ばす。
pub fn style_anchor(identity: &VisualIdentity) -> String {
    let mut parts: Vec<String> = Vec::new();
    let hex: Vec<String> = identity.palette.iter().filter_map(|p| extract_hex(p)).collect();
    if !hex.is_empty() {
        parts.push(format!("Color palette: {}", hex.join(", ")));
    }
    if !identity.mood.trim().is_empty() {
        parts.push(format!("Mood: {}", identity.mood.trim()));
    }
    let traits: Vec<&str> = identity.ui_traits.iter().map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
    if !traits.is_empty() {
        parts.push(format!("UI style: {}", traits.join("; ")));
    }
    parts.join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_hex_tolerates_descriptions_and_rejects_short() {
        assert_eq!(extract_hex("#15110E (ink, 焦げ茶の黒/背景)").as_deref(), Some("#15110E"));
        assert_eq!(extract_hex("ember #d98a4a").as_deref(), Some("#D98A4A"));
        assert_eq!(extract_hex("#FFF"), None);
        assert_eq!(extract_hex("warm orange"), None);
    }

    #[test]
    fn anchor_joins_present_parts_only() {
        let id = VisualIdentity {
            palette: vec!["#15110E (ink)".into(), "not a color".into(), "#E8DDC8".into()],
            mood: " calm, ember light ".into(),
            ui_traits: vec!["dark theme".into(), "".into(), "serif type".into()],
        };
        assert_eq!(style_anchor(&id), "Color palette: #15110E, #E8DDC8. Mood: calm, ember light. UI style: dark theme; serif type");
        let empty = VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] };
        assert_eq!(style_anchor(&empty), "");
    }

    #[test]
    fn apply_palette_replaces_only_when_measured() {
        let mut id = VisualIdentity { palette: vec!["#000000".into()], mood: String::new(), ui_traits: vec![] };
        apply_palette(&mut id, vec![]);
        assert_eq!(id.palette, vec!["#000000"]);
        apply_palette(&mut id, vec!["#111111".into(), "#222222".into()]);
        assert_eq!(id.palette, vec!["#111111", "#222222"]);
    }
}
