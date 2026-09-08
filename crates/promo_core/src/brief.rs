//! RepoBrief (契約 `RepoBrief`) — CLI に stdin で渡す圧縮マニフェストの**純粋部分**。
//!
//! FS を読む収集は `crates/pipeline` (Phase B) の責務。ここは「集めた素材を上限で刈り込み、
//! 全 kind で同一バイト列に整形する」だけ (査読 6: 純関数と IO の分離)。
//! 上限 (査読 3: 最低保証サイズの定義) は契約に凍結し、テストで固定する。

use serde::{Deserialize, Serialize};

/// 刈り込み前の素材 (収集側が作る)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BriefInputs {
    pub root_name: String,
    /// `find` 相当の相対パス列 (収集側が除外規則を適用済み)。1 行 1 パス。
    pub tree_lines: Vec<String>,
    pub readme: Option<String>,
    /// (相対パス, 本文) の列。収集側は候補順 (Cargo.toml / package.json / pyproject.toml / pubspec.yaml / go.mod / *.csproj) で渡す。
    pub manifests: Vec<(String, String)>,
    pub snapshots: Vec<SnapshotMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// 上限 (契約 `RepoBrief.limits`)。全 kind で同じ値。
///
/// 最初の案 (tree 400 行・README 8000 字・manifest 5×2000 字・総量 32 KB) は**総量が部分の和より
/// 小さく**、最悪ケースのテストで落ちた (2026-09-07 Red)。部分の上限は文字数で決めるので総量も
/// 文字数で決め、tree の 1 行にも上限を置く (パスは無制限に長くなり得る)。
/// 最悪 = 400×101 + 8,000 + 5×2,000 + 見出し ≈ 59.5k 字 < 64k。UTF-8 で最大 3 倍の 192 KB でも
/// claude の stdin 上限 10 MB より 2 桁小さい。
pub const TREE_MAX_LINES: usize = 400;
pub const TREE_LINE_MAX_CHARS: usize = 100;
pub const README_MAX_CHARS: usize = 8_000;
pub const MANIFEST_MAX_FILES: usize = 5;
pub const MANIFEST_MAX_CHARS: usize = 2_000;
/// render() の総量上限 (文字数)。同じリポジトリなら claude でも aider でも同じ土台が届く。
pub const RENDER_MAX_CHARS: usize = 64_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoBrief {
    pub root_name: String,
    pub tree: String,
    pub tree_truncated: bool,
    pub readme: Option<String>,
    pub readme_truncated: bool,
    pub manifests: Vec<ManifestHead>,
    pub snapshots: Vec<SnapshotMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestHead {
    pub path: String,
    pub head: String,
    pub truncated: bool,
}

fn take_chars(s: &str, max: usize) -> (String, bool) {
    let out: String = s.chars().take(max).collect();
    let truncated = s.chars().nth(max).is_some();
    (out, truncated)
}

/// 素材を上限で刈り込む (純粋)。
pub fn compress(inputs: &BriefInputs) -> RepoBrief {
    let tree_truncated = inputs.tree_lines.len() > TREE_MAX_LINES;
    let tree = inputs
        .tree_lines
        .iter()
        .take(TREE_MAX_LINES)
        .map(|l| {
            let (t, cut) = take_chars(l, TREE_LINE_MAX_CHARS);
            if cut { format!("{t}…") } else { t }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (readme, readme_truncated) = match &inputs.readme {
        Some(r) => {
            let (t, tr) = take_chars(r, README_MAX_CHARS);
            (Some(t), tr)
        }
        None => (None, false),
    };
    let manifests = inputs
        .manifests
        .iter()
        .take(MANIFEST_MAX_FILES)
        .map(|(path, body)| {
            let (head, truncated) = take_chars(body, MANIFEST_MAX_CHARS);
            ManifestHead { path: path.clone(), head, truncated }
        })
        .collect();
    RepoBrief {
        root_name: inputs.root_name.clone(),
        tree,
        tree_truncated,
        readme,
        readme_truncated,
        manifests,
        snapshots: inputs.snapshots.clone(),
    }
}

impl RepoBrief {
    /// stdin に流す本文 (決定論・全 kind 共通)。`RENDER_MAX_CHARS` を超えない (テストで固定)。
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("# Repository brief: {}\n\n", self.root_name));
        s.push_str("## File tree (depth <= 3");
        if self.tree_truncated {
            s.push_str(&format!(", first {TREE_MAX_LINES} entries"));
        }
        s.push_str(")\n```\n");
        s.push_str(&self.tree);
        s.push_str("\n```\n\n");
        if let Some(r) = &self.readme {
            s.push_str("## README");
            if self.readme_truncated {
                s.push_str(" (truncated)");
            }
            s.push('\n');
            s.push_str(r);
            s.push_str("\n\n");
        }
        for m in &self.manifests {
            s.push_str(&format!("## {}{}\n```\n{}\n```\n\n", m.path, if m.truncated { " (head)" } else { "" }, m.head));
        }
        if !self.snapshots.is_empty() {
            s.push_str("## UI snapshots (metadata only)\n");
            for sn in &self.snapshots {
                s.push_str(&format!("- {} ({}x{})\n", sn.path, sn.width, sn.height));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big_inputs() -> BriefInputs {
        BriefInputs {
            root_name: "sample".into(),
            tree_lines: (0..1000).map(|i| format!("src/mod_{i}/file_{i}.rs")).collect(),
            readme: Some("R".repeat(20_000)),
            manifests: (0..8).map(|i| (format!("m{i}.toml"), "M".repeat(5_000))).collect(),
            snapshots: vec![SnapshotMeta { path: "shots/a.png".into(), width: 1920, height: 1080 }],
        }
    }

    #[test]
    fn caps_are_enforced_and_flagged() {
        let b = compress(&big_inputs());
        assert_eq!(b.tree.lines().count(), TREE_MAX_LINES);
        assert!(b.tree_truncated);
        assert_eq!(b.readme.as_ref().unwrap().chars().count(), README_MAX_CHARS);
        assert!(b.readme_truncated);
        assert_eq!(b.manifests.len(), MANIFEST_MAX_FILES);
        assert!(b.manifests.iter().all(|m| m.head.chars().count() == MANIFEST_MAX_CHARS && m.truncated));
    }

    #[test]
    fn worst_case_render_fits_total_budget() {
        // 上限いっぱいの素材 (長い多バイトのパス名・多バイト README) でも RENDER_MAX_CHARS を超えない。
        let mut inp = big_inputs();
        inp.tree_lines = (0..1000).map(|i| format!("{}/{i}.rs", "あ".repeat(300))).collect();
        inp.readme = Some("あ".repeat(20_000));
        inp.manifests = (0..8).map(|i| (format!("m{i}.toml"), "あ".repeat(5_000))).collect();
        let b = compress(&inp);
        assert!(b.tree.lines().all(|l| l.chars().count() <= TREE_LINE_MAX_CHARS + 1), "行が長すぎる");
        let rendered = b.render();
        let n = rendered.chars().count();
        assert!(n <= RENDER_MAX_CHARS, "{n} > {RENDER_MAX_CHARS}");
    }

    #[test]
    fn small_inputs_are_not_flagged_and_render_is_deterministic() {
        let inp = BriefInputs {
            root_name: "tiny".into(),
            tree_lines: vec!["README.md".into(), "src/main.rs".into()],
            readme: Some("# tiny\nhello".into()),
            manifests: vec![("Cargo.toml".into(), "[package]\nname = \"tiny\"".into())],
            snapshots: vec![],
        };
        let b = compress(&inp);
        assert!(!b.tree_truncated && !b.readme_truncated && !b.manifests[0].truncated);
        let r1 = b.render();
        let r2 = compress(&inp).render();
        assert_eq!(r1, r2);
        assert!(r1.contains("# Repository brief: tiny"));
        assert!(r1.contains("src/main.rs"));
        assert!(r1.contains("name = \"tiny\""));
        assert!(!r1.contains("UI snapshots"));
    }

    #[test]
    fn readme_absent_is_omitted() {
        let inp = BriefInputs { root_name: "x".into(), ..Default::default() };
        let r = compress(&inp).render();
        assert!(!r.contains("## README"));
    }
}
