//! パッケージの書き出し (契約 `ExportPackage`)。整形は promo_core::export。

use std::fs;
use std::path::{Path, PathBuf};

use promo_core::export::{PromoJson, package_dir_name, scenes_markdown};

/// `export_dir/<app>_Promo_Package/` を作り、promo.json / scenes.md / snapshots/ を書く。戻りはパッケージのパス。
pub fn write_package(export_dir: &Path, promo: &PromoJson) -> Result<PathBuf, String> {
    let dir = export_dir.join(package_dir_name(&promo.summary.app_name));
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

    #[test]
    fn writes_json_markdown_and_snapshot_copies() {
        let root = std::env::temp_dir().join(format!("pipeline_export_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let shot = root.join("Dash.PNG");
        fs::write(&shot, b"png-bytes").unwrap();
        let promo = PromoJson {
            project_path: "D:/proj".into(),
            snapshot_paths: vec![shot.to_string_lossy().to_string()],
            video_concept: "calm".into(),
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
        };
        let dir = write_package(&root.join("out"), &promo).unwrap();
        assert!(dir.ends_with("Task_Flow_Promo_Package"));
        let back: PromoJson = serde_json::from_str(&fs::read_to_string(dir.join("promo.json")).unwrap()).unwrap();
        assert_eq!(back, promo);
        assert!(fs::read_to_string(dir.join("scenes.md")).unwrap().contains("# Task Flow"));
        assert_eq!(fs::read(dir.join("snapshots/snapshot_01.png")).unwrap(), b"png-bytes");
        assert!(!dir.join("promo.tmp").exists());
    }
}
