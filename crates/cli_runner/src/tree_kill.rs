//! 子孫ごと kill する (契約 `CliError.kill_semantics`、spec 01 rev2 決定 5)。
//!
//! `tokio::process::Child::kill` は Windows で `TerminateProcess` = **その 1 プロセスだけ**を殺す。
//! claude が内部で spawn した `rg` や、aider の子は孤児化して次の実行を壊す (failures #2 の残留と同型)。
//!
//! - **Windows**: Job Object。spawn 直後に `AssignProcessToJobObject`、`KILL_ON_JOB_CLOSE` を立てる
//!   (このプロセスが死んでも木ごと消える)。止める時は `TerminateJobObject`。
//!   限界: spawn からアタッチまでの数マイクロ秒に生まれた孫は Job に入らない (tokio は
//!   `CREATE_SUSPENDED` を出していない)。実用上は無視できるが、機序として記録する。
//! - **Unix**: `process_group(0)` で spawn → `killpg(SIGTERM)` (claude は SIGTERM で実行中 Bash の木を
//!   止めて exit 143 する — 公式) → 猶予 → `killpg(SIGKILL)`。**この機体 (Windows) では未コンパイル・未検証。**

use std::io;
use std::time::Duration;

use tokio::process::{Child, Command};

/// SIGTERM 後の猶予 (Unix のみ意味を持つ)。
pub const TERM_GRACE: Duration = Duration::from_secs(5);

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::io::RawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    };

    pub struct Tree {
        job: HANDLE,
    }

    // SAFETY: Job ハンドルはスレッド親和性を持たない kernel オブジェクト。
    unsafe impl Send for Tree {}
    unsafe impl Sync for Tree {}

    impl Tree {
        pub fn prepare(_cmd: &mut Command) -> io::Result<Self> {
            // SAFETY: 引数は null (無名 Job・既定セキュリティ)。失敗は null ハンドルで返る。
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: 構造体はゼロ初期化した正しいサイズの領域。
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                let e = io::Error::last_os_error();
                unsafe { CloseHandle(job) };
                return Err(e);
            }
            Ok(Tree { job })
        }

        pub fn attach(&self, child: &Child) -> io::Result<()> {
            let h: RawHandle = child.raw_handle().ok_or_else(|| io::Error::other("child handle unavailable"))?;
            // SAFETY: h は生きている子プロセスのハンドル (Child が所有)。
            let ok = unsafe { AssignProcessToJobObject(self.job, h as HANDLE) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub async fn kill(&self, child: &mut Child) {
            // SAFETY: self.job は有効な Job ハンドル。木ごと exit code 1 で終了させる。
            unsafe { TerminateJobObject(self.job, 1) };
            let _ = child.wait().await;
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // KILL_ON_JOB_CLOSE: 最後のハンドルが閉じると木ごと消える (通常終了後なら no-op)。
            unsafe { CloseHandle(self.job) };
        }
    }

    /// テスト用: pid のプロセスが生きているか。
    pub fn process_alive(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::STILL_ACTIVE;
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        // SAFETY: 問い合わせ専用の最小権限で開く。失敗は null。
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = unsafe { GetExitCodeProcess(h, &mut code) };
        unsafe { CloseHandle(h) };
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

#[cfg(unix)]
mod imp {
    use super::*;

    pub struct Tree {
        // Mutex: future が Send であるために Sync が要る (Cell は Sync でない)。
        pgid: std::sync::Mutex<Option<i32>>,
    }

    impl Tree {
        pub fn prepare(cmd: &mut Command) -> io::Result<Self> {
            cmd.process_group(0);
            Ok(Tree { pgid: std::sync::Mutex::new(None) })
        }

        pub fn attach(&self, child: &Child) -> io::Result<()> {
            let pid = child.id().ok_or_else(|| io::Error::other("child pid unavailable"))? as i32;
            *self.pgid.lock().unwrap() = Some(pid);
            Ok(())
        }

        pub async fn kill(&self, child: &mut Child) {
            let pgid = *self.pgid.lock().unwrap();
            let Some(pgid) = pgid else {
                let _ = child.kill().await;
                return;
            };
            // SAFETY: pgid は自分が spawn したプロセスグループ。
            unsafe { libc::killpg(pgid, libc::SIGTERM) };
            if tokio::time::timeout(TERM_GRACE, child.wait()).await.is_err() {
                unsafe { libc::killpg(pgid, libc::SIGKILL) };
                let _ = child.wait().await;
            }
        }
    }

    /// テスト用: pid のプロセスが生きているか (signal 0)。
    pub fn process_alive(pid: u32) -> bool {
        // SAFETY: signal 0 は送信せず存在検査だけ行う。
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

pub use imp::{Tree, process_alive};
