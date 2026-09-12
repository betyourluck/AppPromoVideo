//! CLI の生ログ (契約 `CliRawLog`)。
//!
//! stream-json の **stdout の行だけ**を受信順にそのまま書く。stderr は混ぜない — NDJSON のままなら
//! `fixtures/*.jsonl` にそのまま流用できる (stderr は `RunFailed.stderr_tail` が末尾 4 KB を持つ)。
//!
//! 2026-09-12 起点: live の解析が result 行を出さずに落ちた時、手掛かりが `Shape.raw` の 300 字しか
//! 残っていなかった。行が届くたびに書くので、timeout / cancel / kill でも届いたところまでは残る。

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 残す本数 (契約 `CliRawLog.retention`)。
pub const CLI_LOG_KEEP: usize = 20;

const DIR: &str = "cli-logs";

/// 開いている生ログ。`open` が None を返したら黙って諦める (fail-open)。
pub struct RawLog {
    path: PathBuf,
    file: File,
}

impl RawLog {
    /// `cwd/cli-logs/<UTC YYYYMMDD-HHMMSS>-<pid>.jsonl` を開く。
    ///
    /// **fail-open**: ディレクトリが作れない・ファイルが開けない時は `None`。ログのために run は落とさない。
    pub fn open(cwd: &Path, pid: u32) -> Option<Self> {
        let dir = cwd.join(DIR);
        fs::create_dir_all(&dir).ok()?;
        // 自分の 1 本ぶんを空けてから刈る。
        prune(&dir, CLI_LOG_KEEP.saturating_sub(1));
        let path = dir.join(format!("{}-{pid}.jsonl", stamp()));
        let file = File::create(&path).ok()?;
        Some(Self { path, file })
    }

    /// **何を起動したか**を兄弟ファイル `<同名>.invocation.json` に書く。
    ///
    /// `.jsonl` を claude の封筒だけに保つため (fixture に流用できる)、argv は別ファイルにする。
    /// **本文は argv に載らない契約**なので、ここに指示本文は現れない。
    pub fn write_invocation(&self, program: &str, args: &[String], cwd: &Path) {
        let v = serde_json::json!({ "program": program, "args": args, "cwd": cwd.display().to_string() });
        if let Ok(text) = serde_json::to_string_pretty(&v) {
            let _ = fs::write(self.path.with_extension("invocation.json"), text);
        }
    }

    /// 1 行書く。書けなくても run は続ける。
    pub fn write_line(&mut self, line: &str) {
        let _ = writeln!(self.file, "{line}");
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// 名前 (先頭が UTC の時刻) の昇順で古いものから消し、`keep` 本だけ残す。
fn prune(dir: &Path, keep: usize) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort();
    for old in files.iter().take(files.len() - keep) {
        let _ = fs::remove_file(old);
        // 起動の記録も道連れにする (孤児にしない)。
        let _ = fs::remove_file(old.with_extension("invocation.json"));
    }
}

fn stamp() -> String {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
    promo_core::export::format_utc_compact(ms)
}
