//! パッケージの書き出し (契約 `ExportPackage`)。整形は promo_core::export。

use std::fs;
use std::path::{Path, PathBuf};

use promo_core::export::{PromoJson, package_dir_name, scenes_markdown};

/// `export_dir/<app>_Promo_Package/runs/<run_id>/` を作り、promo.json / scenes.md / snapshots/ を書く。
/// 戻りは **run のパス** (rev7: run ごとに隔離する。以前は同じパッケージを上書きして過去の出力を消していた)。
/// run のフォルダを返す (`<export_dir>/<app>_Promo_Package/runs/<run_id>/`、契約 `ExportPackage`)。
pub fn run_dir_of(export_dir: &Path, app_name: &str, run_id: &str) -> PathBuf {
    export_dir.join(package_dir_name(app_name)).join("runs").join(run_id)
}

/// 既にある run_id の一覧 (新しい順に並ぶよう文字列でソート済み)。無ければ空。
pub fn existing_run_ids(export_dir: &Path, app_name: &str) -> Vec<String> {
    let runs = export_dir.join(package_dir_name(app_name)).join("runs");
    let mut ids: Vec<String> = fs::read_dir(runs)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    ids.sort();
    ids
}

pub fn write_package(export_dir: &Path, promo: &PromoJson, run_id: &str) -> Result<PathBuf, String> {
    let dir = run_dir_of(export_dir, &promo.summary.app_name, run_id);
    fs::create_dir_all(dir.join("snapshots")).map_err(|e| format!("export フォルダを作れません {}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(promo).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("promo.json"), json.as_bytes())?;
    write_atomic(&dir.join("scenes.md"), scenes_markdown(&promo.summary, &promo.plan).as_bytes())?;
    for (i, s) in promo.snapshot_paths.iter().enumerate() {
        let src = Path::new(s);
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
        let dst = dir.join("snapshots").join(format!("snapshot_{:02}.{ext}", i + 1));
        fs::copy(src, &dst).map_err(|e| format!("スナップショットを写せません {}: {e}", src.display()))?;
    }
    Ok(dir)
}

/// tmp → rename の原子的置換 (Kataribe settings_store と同じ流儀)。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| format!("書けません {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("置換できません {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use promo_core::plan::{AnalyzedSummary, Aspect, Scene, ScenePlan, VisualIdentity};

    fn promo(shot: &Path) -> PromoJson {
        PromoJson {
            project_path: "D:/proj".into(),
            snapshot_paths: vec![shot.to_string_lossy().to_string()],
            video_concept: "calm".into(),
            caption_overrides: Default::default(),
            plate_mode: Default::default(),
            summary: AnalyzedSummary {
                app_name: "Task Flow".into(),
                one_liner: "o".into(),
                core_value: "c".into(),
                target_audience: "t".into(),
                differentiators: vec![],
                hook_copy: "h".into(),
                visual_identity: VisualIdentity { palette: vec![], mood: String::new(), ui_traits: vec![] },
            },
            plan: ScenePlan {
                total_seconds: 15,
                aspect: Aspect::Square,
                scenes: vec![Scene {
                    scene_id: 1,
                    cut_kind: promo_core::plan::CutKind::Mood,
                    snapshot_index: None,
                    plate_tilt: None,
                    motion_prompt: "Slow push-in.".into(),
                    duration_seconds: 5,
                    shot_type: "Wide".into(),
                    video_prompt: "v".into(),
                    copy_text: "c".into(),
                    image_prompt: "i".into(),
                    reference_image: None,
                }],
            },
        }
    }


    /// rev7: 2 回目の run が 1 回目を上書きして消していた (契約 `ExportPackage.run_isolation`)。
    #[test]
    fn a_second_run_does_not_touch_the_first() {
        let root = std::env::temp_dir().join(format!("pipeline_runs_{}_{}", std::process::id(), line!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let shot = root.join("ui.png");
        fs::write(&shot, b"x").unwrap();

        let mut first = promo(&shot);
        first.summary.one_liner = "1 回目".into();
        let d1 = write_package(&root, &first, "20260908-120000").unwrap();
        // 1 回目に画像が 1 枚あるとする (generate_images が後から置くもの)。
        fs::write(d1.join("scene_09_ref_01.png"), b"old").unwrap();

        let mut second = promo(&shot);
        second.summary.one_liner = "2 回目".into();
        let d2 = write_package(&root, &second, "20260908-130000").unwrap();

        assert_ne!(d1, d2, "run ごとに別のフォルダ");
        assert!(d1.ends_with("runs/20260908-120000"), "{}", d1.display()); // Path::ends_with は区切りを問わない
        assert!(d1.join("promo.json").exists() && d2.join("promo.json").exists());
        let kept = fs::read_to_string(d1.join("promo.json")).unwrap();
        assert!(kept.contains("1 回目"), "1 回目の promo.json は残る");
        assert!(d1.join("scene_09_ref_01.png").exists(), "1 回目の画像も残る");
        assert!(!d2.join("scene_09_ref_01.png").exists(), "2 回目に前回の画像が混ざらない");
        assert!(d1.join("snapshots/snapshot_01.png").exists() && d2.join("snapshots/snapshot_01.png").exists());
        // パッケージは 1 つ、その下に run が 2 つ。
        let pkg = d1.parent().unwrap().parent().unwrap();
        assert_eq!(fs::read_dir(pkg.join("runs")).unwrap().count(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn writes_json_markdown_and_snapshot_copies() {
        let root = std::env::temp_dir().join(format!("pipeline_export_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let shot = root.join("Dash.PNG");
        fs::write(&shot, b"png-bytes").unwrap();
        let promo = promo(&shot);
        let dir = write_package(&root.join("out"), &promo, "20260908-090000").unwrap();
        assert!(dir.to_string_lossy().contains("Task_Flow_Promo_Package"));
        let back: PromoJson = serde_json::from_str(&fs::read_to_string(dir.join("promo.json")).unwrap()).unwrap();
        assert_eq!(back, promo);
        assert!(fs::read_to_string(dir.join("scenes.md")).unwrap().contains("# Task Flow"));
        assert_eq!(fs::read(dir.join("snapshots/snapshot_01.png")).unwrap(), b"png-bytes");
        assert!(!dir.join("promo.tmp").exists());
    }
}
