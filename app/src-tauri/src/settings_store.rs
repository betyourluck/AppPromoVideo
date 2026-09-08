//! UI 設定ミラー (Kataribe settings_store の写し、接頭辞だけ `apppromo.`)。
//!
//! WebView の localStorage は bundle identifier 別のプロファイルに住むため、identifier の変更や
//! プロファイル破損で設定が丸ごと消える (Kataribe 2026-08-28 実機)。`app_data_dir/settings.json` を
//! 背後の耐久コピーとして持つ。localStorage が正本のまま、frontend が `apppromo.*` 全キーの
//! スナップショットを write-through で送ってくる。書き込みは tmp→rename の原子的置換。
//! 検証は「接頭辞つきのキーだけ・値は文字列だけ」— それ以外は壊れたデータで復元に乗せない。

use std::path::Path;

pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const KEY_PREFIX: &str = "apppromo.";

pub fn validate(text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err(format!("設定スナップショットが大きすぎます ({} bytes)", text.len()));
    }
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("JSON として読めません: {e}"))?;
    let obj = v.as_object().ok_or("スナップショットは object であるべきです")?;
    for (k, val) in obj {
        if !k.starts_with(KEY_PREFIX) {
            return Err(format!("未知の接頭辞のキー: {k}"));
        }
        if !val.is_string() {
            return Err(format!("キー {k} の値が文字列ではありません"));
        }
    }
    Ok(())
}

pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("設定フォルダの作成に失敗: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("一時ファイルの書き込みに失敗: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("原子的置換に失敗: {e}"))
}

pub fn read_valid(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    match validate(&text) {
        Ok(()) => Some(text),
        Err(e) => {
            eprintln!("[settings_store] {} を無視します: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_only_prefixed_string_map() {
        assert!(validate(r#"{"apppromo.theme":"dark","apppromo.cli":"{}"}"#).is_ok());
        assert!(validate("{}").is_ok());
        assert!(validate(r#"["apppromo.theme"]"#).is_err());
        assert!(validate(r#"{"apppromo.n":18}"#).is_err());
        assert!(validate(r#"{"kataribe.theme":"x"}"#).is_err(), "別アプリの接頭辞は通さない");
        assert!(validate("not json").is_err());
    }

    #[test]
    fn write_atomic_roundtrips_and_rejects_garbage() {
        let dir = std::env::temp_dir().join(format!("apppromo_settings_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let text = r#"{"apppromo.theme":"light"}"#;
        write_atomic(&path, text).unwrap();
        assert_eq!(read_valid(&path).as_deref(), Some(text));
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::write(&path, "garbage").unwrap();
        assert_eq!(read_valid(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_valid(&path), None);
    }
}
