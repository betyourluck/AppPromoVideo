//! RepoBrief の収集 (FS)。契約 `RepoBrief` — 除外規則と候補順はここが正。刈り込みは `promo_core::brief`。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use promo_core::brief::{BriefInputs, SnapshotMeta};

/// tree から除外するディレクトリ名 (契約)。
pub const EXCLUDED_DIRS: [&str; 8] = [".git", "node_modules", "target", "dist", "build", ".venv", "__pycache__", ".idea"];
/// tree から除外する拡張子。
pub const EXCLUDED_EXT: [&str; 1] = ["lock"];
/// tree の深さ (契約: 3)。
pub const TREE_DEPTH: usize = 3;
/// マニフェスト候補 (契約の順)。`*.csproj` は root 直下の glob。
pub const MANIFEST_CANDIDATES: [&str; 6] = ["Cargo.toml", "package.json", "pyproject.toml", "pubspec.yaml", "go.mod", "*.csproj"];
/// スナップショットとして受ける拡張子 (大文字小文字を区別しない)。
pub const SNAPSHOT_EXT: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// 相対パス列 (深さ ≤ 3、除外適用、ソート済み)。ディレクトリは末尾 `/`。
pub fn tree_lines(root: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    walk(root, root, 1, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if EXCLUDED_DIRS.contains(&name.as_str()) {
                continue;
            }
            out.push(format!("{rel}/"));
            if depth < TREE_DEPTH {
                walk(root, &path, depth + 1, out)?;
            }
        } else {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if EXCLUDED_EXT.contains(&ext.as_str()) {
                continue;
            }
            out.push(rel);
        }
    }
    Ok(())
}

/// README*.md の最初の 1 本 (大文字小文字を区別しない、名前順)。
pub fn read_readme(root: &Path) -> Option<String> {
    let mut names: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            n.starts_with("readme") && n.ends_with(".md")
        })
        .collect();
    names.sort();
    names.first().and_then(|p| fs::read_to_string(p).ok())
}

/// マニフェストを候補順に読む (存在するものだけ)。
pub fn read_manifests(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for cand in MANIFEST_CANDIDATES {
        if let Some(suffix) = cand.strip_prefix('*') {
            let mut found: Vec<PathBuf> = fs::read_dir(root)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.path())
                        .filter(|p| p.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.ends_with(suffix)))
                        .collect()
                })
                .unwrap_or_default();
            found.sort();
            for p in found {
                if let Ok(body) = fs::read_to_string(&p) {
                    out.push((p.file_name().unwrap().to_string_lossy().to_string(), body));
                }
            }
        } else if let Ok(body) = fs::read_to_string(root.join(cand)) {
            out.push((cand.to_string(), body));
        }
    }
    out
}

/// スナップショットの寸法 (ヘッダのみ)。非対応形式・読めないものは `Err` (契約: 不正パスのバリデーション)。
pub fn snapshot_meta(path: &Path) -> Result<SnapshotMeta, String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !SNAPSHOT_EXT.contains(&ext.as_str()) {
        return Err(format!("非対応の画像形式: {} (png/jpg/jpeg/webp)", path.display()));
    }
    let size = imagesize::size(path).map_err(|e| format!("画像を読めません {}: {e}", path.display()))?;
    Ok(SnapshotMeta { path: path.to_string_lossy().to_string(), width: size.width as u32, height: size.height as u32 })
}

/// 収集の入口。`snapshots` の検証失敗は全件まとめて返す (最初の 1 件で止めない)。
pub fn collect_brief(project: &Path, snapshots: &[PathBuf]) -> Result<BriefInputs, String> {
    if !project.is_dir() {
        return Err(format!("リポジトリのパスがディレクトリではありません: {}", project.display()));
    }
    let root_name = project.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "repo".into());
    let tree = tree_lines(project).map_err(|e| format!("tree を読めません: {e}"))?;
    let mut metas = Vec::new();
    let mut errors = Vec::new();
    for s in snapshots {
        match snapshot_meta(s) {
            Ok(m) => metas.push(m),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(BriefInputs { root_name, tree_lines: tree, readme: read_readme(project), manifests: read_manifests(project), snapshots: metas })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("pipeline_collect_{}_{}", std::process::id(), rand_suffix()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, "x").unwrap();
    }

    #[test]
    fn tree_applies_exclusions_and_depth() {
        let r = tmp();
        touch(&r.join("src/main.rs"));
        touch(&r.join("src/a/b/c/deep.rs")); // 深さ 4 → 出ない (src/a/b/ までは出る)
        touch(&r.join("node_modules/x/index.js"));
        touch(&r.join("target/debug/app.exe"));
        touch(&r.join("Cargo.lock"));
        touch(&r.join(".git/HEAD"));
        let lines = tree_lines(&r).unwrap();
        assert!(lines.contains(&"src/main.rs".to_string()));
        assert!(lines.contains(&"src/a/b/".to_string()));
        assert!(!lines.iter().any(|l| l.contains("deep.rs")));
        assert!(!lines.iter().any(|l| l.starts_with("node_modules") || l.starts_with("target") || l.starts_with(".git")));
        assert!(!lines.iter().any(|l| l.ends_with(".lock")));
        assert!(lines.windows(2).all(|w| w[0] <= w[1]), "ソート済み");
    }

    #[test]
    fn readme_is_case_insensitive_and_manifests_follow_candidate_order() {
        let r = tmp();
        fs::write(r.join("ReadMe.md"), "# hi").unwrap();
        fs::write(r.join("package.json"), "{}").unwrap();
        fs::write(r.join("Cargo.toml"), "[package]").unwrap();
        fs::write(r.join("App.csproj"), "<Project/>").unwrap();
        assert_eq!(read_readme(&r).as_deref(), Some("# hi"));
        let m = read_manifests(&r);
        let names: Vec<&str> = m.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["Cargo.toml", "package.json", "App.csproj"]);
    }

    #[test]
    fn snapshot_meta_reads_png_header_and_rejects_other_formats() {
        let r = tmp();
        // 最小の PNG (1x1)。imagesize はヘッダだけ見る。
        let png: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R', 0, 0, 0, 0x20, 0, 0,
            0, 0x10, 8, 6, 0, 0, 0,
        ];
        let p = r.join("Shot.PNG");
        fs::write(&p, png).unwrap();
        let m = snapshot_meta(&p).unwrap();
        assert_eq!((m.width, m.height), (32, 16));
        let bad = r.join("x.gif");
        fs::write(&bad, b"GIF89a").unwrap();
        assert!(snapshot_meta(&bad).unwrap_err().contains("非対応"));
        let missing = r.join("none.png");
        assert!(snapshot_meta(&missing).unwrap_err().contains("読めません"));
    }

    #[test]
    fn collect_brief_reports_all_snapshot_errors_at_once() {
        let r = tmp();
        fs::write(r.join("README.md"), "# t").unwrap();
        let err = collect_brief(&r, &[r.join("a.gif"), r.join("b.png")]).unwrap_err();
        assert!(err.contains("a.gif") && err.contains("b.png"));
        let err = collect_brief(&r.join("nope"), &[]).unwrap_err();
        assert!(err.contains("ディレクトリではありません"));
    }
}
