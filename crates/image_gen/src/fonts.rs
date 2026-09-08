//! フォントの列挙 (契約 `caption.fonts`、2026-09-08 ユーザー方針: システムのインストール済み + 自分で置いたフォント)。
//!
//! - **システム**: OS 標準のフォントフォルダ (Windows は `%WINDIR%\Fonts` と `%LOCALAPPDATA%\Microsoft\Windows\Fonts`)。
//! - **ユーザー**: `app_data/fonts` に置いたファイル (アプリが作る)。
//!
//! ファミリー名は ttf-parser の name テーブルから (日本語名があればそれを優先)。TTC は中の face ごとに 1 件。
//! 見出しは日本語が主なので、`has_japanese` (U+3042 あ のグリフがあるか) を付けて UI が並べ替えられるようにする。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontSource {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontEntry {
    pub path: String,
    /// TTC 内の face 番号 (単体フォントは 0)。
    pub index: u32,
    pub family: String,
    pub source: FontSource,
    pub has_japanese: bool,
}

/// OS 標準のフォントフォルダ。存在しないものは呼び出し側で無視される。
pub fn system_font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(w) = std::env::var("WINDIR") {
            v.push(PathBuf::from(w).join("Fonts"));
        }
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        v.push(PathBuf::from("/System/Library/Fonts"));
        v.push(PathBuf::from("/Library/Fonts"));
        if let Ok(h) = std::env::var("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        v.push(PathBuf::from("/usr/share/fonts"));
        v.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(h) = std::env::var("HOME") {
            v.push(PathBuf::from(&h).join(".fonts"));
            v.push(PathBuf::from(&h).join(".local/share/fonts"));
        }
    }
    v
}

fn is_font_file(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("ttf" | "otf" | "ttc" | "otc"))
}

/// name テーブルからファミリー名。日本語 (Windows lang 0x0411) → 英語 → 何でも、の順。無ければ None。
fn family_name(face: &ttf_parser::Face<'_>) -> Option<String> {
    use ttf_parser::name_id::{FAMILY, TYPOGRAPHIC_FAMILY};
    let mut ja: Option<String> = None;
    let mut en: Option<String> = None;
    let mut any: Option<String> = None;
    for id in [TYPOGRAPHIC_FAMILY, FAMILY] {
        for n in face.names().into_iter().filter(|n| n.name_id == id) {
            let Some(s) = n.to_string() else { continue };
            if n.language_id == 0x0411 && ja.is_none() {
                ja = Some(s.clone());
            }
            if n.language_id == 0x0409 && en.is_none() {
                en = Some(s.clone());
            }
            if any.is_none() {
                any = Some(s);
            }
        }
        if ja.is_some() {
            break;
        }
    }
    ja.or(en).or(any)
}

/// 1 ファイルの全 face を読む (壊れたファイルは空)。
pub fn describe_font_file(path: &Path, source: FontSource) -> Vec<FontEntry> {
    let Ok(data) = std::fs::read(path) else { return vec![] };
    let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    (0..n)
        .filter_map(|i| {
            let face = ttf_parser::Face::parse(&data, i).ok()?;
            Some(FontEntry {
                path: path.to_string_lossy().to_string(),
                index: i,
                family: family_name(&face).unwrap_or_else(|| stem.clone()),
                source,
                has_japanese: face.glyph_index('あ').is_some(),
            })
        })
        .collect()
}

fn scan(dir: &Path, source: FontSource, out: &mut Vec<FontEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && is_font_file(p)).collect();
    files.sort();
    for f in files {
        out.extend(describe_font_file(&f, source));
    }
}

/// システム + ユーザーの全フォント。日本語あり → ユーザー → ファミリー名 の順に並べる。
pub fn list_fonts(user_dir: &Path) -> Vec<FontEntry> {
    let mut out = Vec::new();
    scan(user_dir, FontSource::User, &mut out);
    for d in system_font_dirs() {
        scan(&d, FontSource::System, &mut out);
    }
    out.sort_by(|a, b| {
        b.has_japanese
            .cmp(&a.has_japanese)
            .then((b.source == FontSource::User).cmp(&(a.source == FontSource::User)))
            .then(a.family.to_lowercase().cmp(&b.family.to_lowercase()))
            .then(a.index.cmp(&b.index))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dir_and_ordering() {
        let dir = std::env::temp_dir().join(format!("apppromo_fonts_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("not_a_font.txt"), "x").unwrap();
        std::fs::write(dir.join("broken.ttf"), b"garbage").unwrap();
        let list = list_fonts(&dir);
        assert!(list.iter().all(|f| !f.path.ends_with("not_a_font.txt") && !f.path.ends_with("broken.ttf")), "非フォント・壊れは載らない");
        // 並び: 日本語あり が先。
        let first_no_jp = list.iter().position(|f| !f.has_japanese);
        let last_jp = list.iter().rposition(|f| f.has_japanese);
        if let (Some(a), Some(b)) = (first_no_jp, last_jp) {
            assert!(b < a, "日本語ありが前に並ぶ");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows 実機にある日本語フォントを 1 本読めること (無い機体では skip)。
    #[test]
    fn reads_a_japanese_system_font_if_present() {
        let candidates = ["BIZ-UDGothicB.ttc", "YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"];
        let Some(dir) = system_font_dirs().into_iter().find(|d| d.is_dir()) else { return };
        for c in candidates {
            let p = dir.join(c);
            if p.exists() {
                let faces = describe_font_file(&p, FontSource::System);
                assert!(!faces.is_empty(), "{c} を読めない");
                assert!(faces.iter().any(|f| f.has_japanese), "{c} に『あ』が無い扱い: {faces:?}");
                assert!(faces.iter().all(|f| !f.family.is_empty()));
                return;
            }
        }
    }
}
