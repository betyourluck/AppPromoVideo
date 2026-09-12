//! run の索引 (契約 `RunIndex` / `RunRecord`、rev7)。
//!
//! rev6 まで、同じアプリに 2 回実行すると出力パッケージが上書きされ、**過去の生成物がディスク上で
//! 消えていた**。画像・動画の生成は「新しいものを過去と比べて選ぶ」作業なので、これは機能の否定に
//! あたる (ユーザー指摘 2026-09-08)。run は `<pkg>/runs/<run_id>/` で自己完結させ、ここはその一覧を持つ。
//!
//! **この索引はキャッシュであって正本ではない。** 正本は各 `run_dir/promo.json`。
//! 索引が指すフォルダが消えていたら `missing` として出すだけで、索引からは消さない
//! (ユーザーが移動しただけかもしれず、こちらの都合で履歴を捨てない)。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 索引が壊れて読めない時に捨てる上限 (settings_store と同じ考え方)。
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const FILE_NAME: &str = "runs.json";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunRecord {
    pub id: String,
    pub created_at: i64,
    pub app_name: String,
    pub project_path: String,
    pub package_dir: String,
    pub run_dir: String,
    pub seconds: u32,
    pub aspect: String,
    pub language: String,
    pub plate_mode: String,
    pub scene_count: usize,
    /// **None = 記録なし** (費用を返さない CLI がある。rev42)。既存の行は数値を持つので読み込みは互換。
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub image_provider: Option<String>,
    #[serde(default)]
    pub image_count: usize,
    /// シーン構成が検査を通るまでの回数 (rev15)。**0 = 記録なし** (rev14 以前の行)。1 = 一発。
    /// 正本は `run_dir/promo.json` の `run_stats` で、ここはその写し (一覧で並べて数えるため)。
    #[serde(default)]
    pub plan_attempts: usize,
    /// 途中で出た違反の種別 (`promo_core::violation_kind`)。重複なく整列済み。
    #[serde(default)]
    pub violation_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunIndex {
    pub version: u32,
    #[serde(default)]
    pub runs: Vec<RunRecord>,
}

impl Default for RunIndex {
    fn default() -> Self {
        RunIndex { version: VERSION, runs: vec![] }
    }
}

pub fn index_path(app_data: &Path) -> PathBuf {
    app_data.join(FILE_NAME)
}

/// 読めない・壊れている索引は**捨てずに既定を返す** (上書きは書き込み時に起きるので、
/// 壊れた file が残っていれば人が直せる)。
pub fn load(path: &Path) -> RunIndex {
    let Ok(text) = std::fs::read_to_string(path) else { return RunIndex::default() };
    if text.len() > MAX_BYTES {
        eprintln!("[runs_store] {} が大きすぎます ({} bytes) — 無視します", path.display(), text.len());
        return RunIndex::default();
    }
    match serde_json::from_str::<RunIndex>(&text) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("[runs_store] {} を読めません: {e} — 空の索引で続けます", path.display());
            RunIndex::default()
        }
    }
}

pub fn save(path: &Path, index: &RunIndex) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("索引フォルダを作れません: {e}"))?;
    }
    let text = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("索引を書けません: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("索引を置換できません: {e}"))
}

/// 同じ `run_dir` の記録があれば置き換え、無ければ足す。並びは**新しい順**に保つ。
///
/// 置き換えが要るのは、`generate_images` が後から画像の枚数を書き足すため。
pub fn upsert(index: &mut RunIndex, rec: RunRecord) {
    match index.runs.iter_mut().find(|r| r.run_dir == rec.run_dir) {
        Some(slot) => *slot = rec,
        None => index.runs.push(rec),
    }
    index.runs.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
}

/// 索引から 1 件外す (フォルダには触らない — 消すかどうかは呼び出し側の判断)。
pub fn forget(index: &mut RunIndex, run_dir: &str) -> bool {
    let before = index.runs.len();
    index.runs.retain(|r| r.run_dir != run_dir);
    index.runs.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn rec(id: &str, at: i64) -> RunRecord {
        RunRecord {
            id: id.into(),
            created_at: at,
            app_name: "Task Flow".into(),
            project_path: "D:/proj".into(),
            package_dir: "D:/out/Task_Flow_Promo_Package".into(),
            run_dir: format!("D:/out/Task_Flow_Promo_Package/runs/{id}"),
            seconds: 30,
            aspect: "16:9".into(),
            language: "ja".into(),
            plate_mode: "perspective".into(),
            scene_count: 6,
            cost_usd: Some(0.5),
            image_provider: None,
            image_count: 0,
            plan_attempts: 1,
            violation_kinds: vec![],
        }
    }

    #[test]
    fn upsert_replaces_the_same_run_and_keeps_newest_first() {
        let mut idx = RunIndex::default();
        upsert(&mut idx, rec("20260908-100000", 100));
        upsert(&mut idx, rec("20260908-120000", 300));
        upsert(&mut idx, rec("20260908-110000", 200));
        let ids: Vec<&str> = idx.runs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["20260908-120000", "20260908-110000", "20260908-100000"], "新しい順");

        // 画像を作った後の書き足しは、行を増やさず差し替える。
        let mut with_images = rec("20260908-110000", 200);
        with_images.image_provider = Some("gemini".into());
        with_images.image_count = 3;
        upsert(&mut idx, with_images);
        assert_eq!(idx.runs.len(), 3, "同じ run_dir は増えない");
        let updated = idx.runs.iter().find(|r| r.id == "20260908-110000").unwrap();
        assert_eq!(updated.image_count, 3);
        assert_eq!(updated.image_provider.as_deref(), Some("gemini"));
    }

    #[test]
    fn forget_removes_only_the_named_run() {
        let mut idx = RunIndex::default();
        upsert(&mut idx, rec("a", 1));
        upsert(&mut idx, rec("b", 2));
        assert!(forget(&mut idx, "D:/out/Task_Flow_Promo_Package/runs/a"));
        assert!(!forget(&mut idx, "D:/out/Task_Flow_Promo_Package/runs/a"), "2 回目は false");
        let ids: Vec<&str> = idx.runs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["b"]);
    }

    #[test]
    fn a_broken_index_falls_back_to_empty_without_destroying_the_file() {
        let dir = std::env::temp_dir().join(format!("apppromo_runs_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = index_path(&dir);
        std::fs::write(&p, "{ not json").unwrap();
        assert!(load(&p).runs.is_empty(), "壊れていても落ちない");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ not json", "file は残す (人が直せる)");
        // 往復。
        let mut idx = RunIndex::default();
        upsert(&mut idx, rec("20260908-100000", 100));
        save(&p, &idx).unwrap();
        assert_eq!(load(&p).runs, idx.runs);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod attempts_tests {
    use super::tests::rec;
    use super::*;

    /// rev14 以前の runs.json には attempts が無い。読めることと、**無いことが 0 (記録なし) として
    /// 残ること**の両方を要求する。ここを 1 に落とすと過去の 3 行が「一発で通った」と嘘をつき、
    /// 「このモードは再生成が起きやすいか」の集計に偽の分母が混ざる。
    #[test]
    fn an_older_index_loads_with_no_attempts_and_does_not_claim_one() {
        let old = r#"{"version":1,"runs":[{
            "id":"20260908-215200","created_at":100,"app_name":"Fuseforks","project_path":"D:/p",
            "package_dir":"D:/o/P","run_dir":"D:/o/P/runs/20260908-215200","seconds":30,
            "aspect":"16:9","language":"ja","plate_mode":"frontal","scene_count":4,
            "cost_usd":1.422,"image_provider":"gemini","image_count":4}]}"#;
        let idx: RunIndex = serde_json::from_str(old).unwrap();
        let r = &idx.runs[0];
        assert_eq!(r.plan_attempts, 0, "記録が無いことは 1 回ではない");
        assert!(r.violation_kinds.is_empty());
        assert_eq!(r.cost_usd, Some(1.422), "既存の列は壊れない");
    }

    /// 画像の枚数を後から書き足す `upsert` で、attempts を消さない
    /// (`update_run_images` は run 直後の記録を作り直さず差し替えるため)。
    #[test]
    fn attempts_survive_the_image_upsert() {
        let mut idx = RunIndex::default();
        let mut first = rec("20260909-100000", 100);
        first.plan_attempts = 2;
        first.violation_kinds = vec!["product_backdrop_angled".into()];
        upsert(&mut idx, first.clone());

        let mut with_images = first.clone();
        with_images.image_provider = Some("gemini".into());
        with_images.image_count = 4;
        upsert(&mut idx, with_images);

        let got = &idx.runs[0];
        assert_eq!(got.plan_attempts, 2);
        assert_eq!(got.violation_kinds, vec!["product_backdrop_angled"]);
        assert_eq!(got.image_count, 4);
    }
}
