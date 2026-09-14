//! **子プロセスを起動する唯一の口** (rev56)。Windows でコンソールの窓を出さない。
//!
//! 配布ビルドのアプリは `windows_subsystem = "windows"` (コンソールを持たない GUI) なので、そこから
//! コンソール用のプログラム (`claude` / `agy` / `git` …) を起動すると、**起動のたびに黒い窓が開く**
//! (2026-09-14 ユーザー報告、v0.1.2 の exe。タイトル「claude」の空の窓)。dev ビルドはアプリ自身が
//! コンソールを持ち、子がそれを共有するので窓が出ず、気づけなかった。
//!
//! 処方は `CREATE_NO_WINDOW`。子は見えないコンソールを持って動くので、stdout / stderr の読み取りも、
//! 子がさらに起動する孫 (`rg` 等) も変わらない。Fuseforks の MCP 起動 (`fuseforks-core/src/mcp.rs`) と同じ。
//! Job Object (`tree_kill`) は起動フラグを使わないので、ぶつからない。
//!
//! **ここ以外で `Command::new` を書かない** — 付け忘れは `no_direct_command_new` が落とす。
//! 窓が出ないこと自体は、Windows の配布ビルドを起動しないと観測できない (起動フラグは外から読めない)。

use std::ffi::OsStr;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `std::process::Command::new` の代わり。Windows では窓を出さない。
pub fn std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `tokio::process::Command::new` の代わり。Windows では窓を出さない
/// (tokio の `Command` は Windows で `creation_flags` を直に持つ)。
pub fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// `Command::new(` を直に書いた行番号 (1 始まり)。コメント行は数えない。
    fn direct_command_new(src: &str) -> Vec<usize> {
        src.lines()
            .enumerate()
            .filter(|(_, l)| {
                let t = l.trim_start();
                !t.starts_with("//") && t.contains("Command::new(")
            })
            .map(|(i, _)| i + 1)
            .collect()
    }

    fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// 網そのものの検出力 (陽性 / 陰性)。空を返す実装で通らないこと。
    #[test]
    fn detector_finds_direct_spawns_and_ignores_the_helper() {
        assert_eq!(direct_command_new("let c = std::process::Command::new(\"git\");"), vec![1]);
        assert_eq!(direct_command_new("a\n    let c = tokio::process::Command::new(exe);"), vec![2]);
        assert!(direct_command_new("let c = cli_runner::no_window::std_command(\"git\");").is_empty());
        assert!(direct_command_new("    // Command::new( はコメントなので数えない").is_empty());
    }

    /// rev56: 子プロセスはすべてこのモジュールを通す (配布ビルドの Windows で窓を出さない)。
    #[test]
    fn no_direct_command_new() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let roots = ["crates/cli_runner/src", "crates/pipeline/src", "crates/promo_core/src", "crates/image_gen/src", "app/src-tauri/src"];
        // 除外: この file 自身 / fake_cli (テストの中でだけ動く偽 CLI。cargo test のコンソールから起動される)。
        let skip = ["crates/cli_runner/src/no_window.rs", "crates/cli_runner/src/bin/fake_cli.rs"];
        let mut scanned = 0;
        let mut hits = Vec::new();
        for r in roots {
            let mut files = Vec::new();
            rs_files(&root.join(r), &mut files);
            for f in files {
                let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                if skip.contains(&rel.as_str()) {
                    continue;
                }
                scanned += 1;
                let src = std::fs::read_to_string(&f).unwrap();
                hits.extend(direct_command_new(&src).into_iter().map(|n| format!("{rel}:{n}")));
            }
        }
        // 走査の空振り (roots を見失って 0 ファイル) を「違反なし」と読まない。
        assert!(scanned >= 30, "走査したファイルが少なすぎる ({scanned})。roots のパスを見失っている");
        assert!(hits.is_empty(), "Command::new を直に書かず cli_runner::no_window を通す: {hits:#?}");
    }
}
