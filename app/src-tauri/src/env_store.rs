//! 画像 API キーの置き場 (契約 `config_sources.env_file`): `app_data/.env` の
//! `IMAGE_API_KEY_OPENAI` / `IMAGE_API_KEY_GEMINI`。LLM のキーは**持たない** (CLI の認証に委ねる)。
//! 読みは .env → プロセス環境 (`OPENAI_API_KEY` / `GEMINI_API_KEY`) の順。frontend にはキーの
//! 有無 (bool) だけ返し、値は WebView に置かない。

use std::path::Path;

use image_gen::Provider;

pub fn env_name(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Openai => Some("IMAGE_API_KEY_OPENAI"),
        Provider::Gemini => Some("IMAGE_API_KEY_GEMINI"),
        Provider::Comfy => None,
    }
}

fn fallback_env(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::Openai => Some("OPENAI_API_KEY"),
        Provider::Gemini => Some("GEMINI_API_KEY"),
        Provider::Comfy => None,
    }
}

/// `.env` を読んで `KEY=VALUE` の map にする (コメント・空行は飛ばす)。
pub fn read_env(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            if t.starts_with('#') {
                return None;
            }
            let (k, v) = t.split_once('=')?;
            Some((k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        })
        .collect()
}

/// キーの実効値 (.env → プロセス環境)。無ければ空。
pub fn image_api_key(env_path: &Path, provider: Provider) -> String {
    let Some(name) = env_name(provider) else { return String::new() };
    if let Some((_, v)) = read_env(env_path).into_iter().find(|(k, _)| k == name) {
        if !v.trim().is_empty() {
            return v;
        }
    }
    fallback_env(provider)
        .and_then(|n| std::env::var(n).ok())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_default()
}

/// `.env` の該当キーを差し替え or 追記 (Kataribe `upsert_env` の写し)。空値は行を消す。
pub fn upsert_env(path: &Path, key: &str, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    let mut seen = false;
    for line in existing.lines() {
        let t = line.trim_start();
        let is_key = !t.starts_with('#') && t.split_once('=').map(|(k, _)| k.trim_end() == key).unwrap_or(false);
        if is_key {
            seen = true;
            if !value.is_empty() {
                out.push(format!("{key}={value}"));
            }
        } else {
            out.push(line.to_string());
        }
    }
    if !seen && !value.is_empty() {
        out.push(format!("{key}={value}"));
    }
    std::fs::write(path, out.join("\n") + "\n").map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_appends_and_deletes_without_touching_other_lines() {
        let dir = std::env::temp_dir().join(format!("apppromo_env_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join(".env");
        upsert_env(&p, "IMAGE_API_KEY_OPENAI", "sk-1").unwrap();
        std::fs::write(&p, format!("# comment\n{}OTHER=x\n", std::fs::read_to_string(&p).unwrap())).unwrap();
        upsert_env(&p, "IMAGE_API_KEY_OPENAI", "sk-2").unwrap();
        upsert_env(&p, "IMAGE_API_KEY_GEMINI", "g-1").unwrap();
        let env = read_env(&p);
        assert_eq!(env.iter().find(|(k, _)| k == "IMAGE_API_KEY_OPENAI").unwrap().1, "sk-2");
        assert_eq!(env.iter().find(|(k, _)| k == "OTHER").unwrap().1, "x");
        assert_eq!(image_api_key(&p, Provider::Gemini), "g-1");
        assert_eq!(image_api_key(&p, Provider::Comfy), "");
        upsert_env(&p, "IMAGE_API_KEY_OPENAI", "").unwrap();
        assert!(!std::fs::read_to_string(&p).unwrap().contains("IMAGE_API_KEY_OPENAI"));
        assert!(std::fs::read_to_string(&p).unwrap().contains("# comment"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
