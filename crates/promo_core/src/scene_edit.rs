//! シーンの追加 / 並び替え / 削除 (契約 `SceneEdit`、rev43)。**純粋** — ファイルは触らない。
//!
//! 要は **`scene_id` の振り直しに、人の手編集とファイル名が付いてくるか**。
//! `scene_id` は検査で「位置 + 1」を強制されている (`SceneIdNotSequential`) ので、
//! 並べ替えと削除は必ず振り直しを伴う。ところが `scene_id` は 3 つの Map とファイル名の**鍵**でもある。
//! 付け替えを誤ると手編集が別のシーンに付き、**失敗のシグナルは一切出ない**。
//! だから振り直しの情報は [`SceneRemap`] 1 つに集め、追従はこの表だけを見て行う。

use std::collections::BTreeMap;

use crate::export::{PromoJson, reference_image_name};
use crate::plan::{CutKind, Scene, ScenePlan};

/// シーン数の下限・上限 (`validate_scene_plan` と同じ値)。
pub const SCENES_MIN: usize = 3;
pub const SCENES_MAX: usize = 8;

/// できる操作 (契約 `SceneEdit.ops`)。**白紙の追加は無い** — 検査が空欄を弾くので複製から始める。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneOp {
    MoveUp,
    MoveDown,
    Duplicate,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SceneEditError {
    #[error("シーン {scene_id} がありません")]
    UnknownScene { scene_id: u32 },
    #[error("これ以上減らせません (最少 {min} シーン)")]
    TooFew { min: usize },
    #[error("これ以上増やせません (最多 {max} シーン)")]
    TooMany { max: usize },
    #[error("端のシーンはこれ以上動かせません")]
    AtEdge,
    #[error("product カットが無くなるので消せません")]
    LastProductCut,
}

/// 振り直しの表 (旧 scene_id → 新 scene_id)。**追従はこの表だけを見て行う。**
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SceneRemap {
    /// 生き残ったシーンの 旧 → 新。番号が変わらないものも含む。
    pub moved: Vec<(u32, u32)>,
    /// 複製で生まれた scene_id。**参照画像は無い** (生成し直すまで出ない)。
    pub created: Option<u32>,
    /// 消えた scene_id。そのファイルは消す。
    pub removed: Option<u32>,
}

impl SceneRemap {
    /// 旧 id の行き先。消えた・元から無い時は None。
    pub fn moved_to(&self, old: u32) -> Option<u32> {
        self.moved.iter().find(|(o, _)| *o == old).map(|(_, n)| *n)
    }

    /// ファイルの付け替え手順 (契約 `SceneEdit.rename_order`)。
    ///
    /// **一時名を必ず経由する。** 1→2 と 2→1 の入れ替えを直接 rename すると片方を潰す。
    /// 呼ぶ側 (IO) は ①消えたシーンのファイルを消す ②この手順を順に適用する。
    /// `ref_index` は 1 始まりの参照画像番号。
    pub fn rename_steps_for(&self, ref_index: u32) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let changing: Vec<(u32, u32)> = self.moved.iter().copied().filter(|(o, n)| o != n).collect();
        for (old, _) in &changing {
            out.push((reference_image_name(*old, ref_index), tmp_name(*old, ref_index)));
        }
        for (old, new) in &changing {
            out.push((tmp_name(*old, ref_index), reference_image_name(*new, ref_index)));
        }
        out
    }

    /// 参照画像 1 枚ぶんの手順 (PoC・UI 向けの短縮形)。
    pub fn rename_steps(&self) -> Vec<(String, String)> {
        self.rename_steps_for(1)
    }
}

fn tmp_name(scene_id: u32, ref_index: u32) -> String {
    format!("{}.tmp", reference_image_name(scene_id, ref_index))
}

/// 尺の合計と `total_seconds` のずれ (秒)。**正なら長すぎ、負なら短すぎ。**
///
/// 直さない — 出すだけ (ユーザー決定 2026-09-12)。`validate_scene_plan` はそもそも合計を見ていない。
pub fn duration_drift(plan: &ScenePlan) -> i64 {
    let sum: u32 = plan.scenes.iter().map(|s| s.duration_seconds).sum();
    sum as i64 - plan.total_seconds as i64
}

/// 唯一の書き換え口 (契約 `SceneEdit.where`)。
///
/// **拒んだ時は `promo` を一切触らない** (rev37 と同じ規律 — 片方だけ書き換わると画面と promo.json が食い違う)。
pub fn apply_scene_edit(promo: &mut PromoJson, op: SceneOp, scene_id: u32) -> Result<SceneRemap, SceneEditError> {
    let idx = promo
        .plan
        .scenes
        .iter()
        .position(|s| s.scene_id == scene_id)
        .ok_or(SceneEditError::UnknownScene { scene_id })?;
    let n = promo.plan.scenes.len();

    // --- 拒否は全部ここで。触る前に決める。 ---
    match op {
        SceneOp::MoveUp if idx == 0 => return Err(SceneEditError::AtEdge),
        SceneOp::MoveDown if idx + 1 == n => return Err(SceneEditError::AtEdge),
        SceneOp::Remove if n <= SCENES_MIN => return Err(SceneEditError::TooFew { min: SCENES_MIN }),
        SceneOp::Duplicate if n >= SCENES_MAX => return Err(SceneEditError::TooMany { max: SCENES_MAX }),
        SceneOp::Remove => {
            let products = promo.plan.scenes.iter().filter(|s| s.cut_kind == CutKind::Product).count();
            if promo.plan.scenes[idx].cut_kind == CutKind::Product && products <= 1 {
                return Err(SceneEditError::LastProductCut);
            }
        }
        _ => {}
    }

    // --- 並べ替え。**旧 scene_id を持ったまま**動かし、最後にまとめて振り直す。 ---
    let mut scenes: Vec<Scene> = promo.plan.scenes.clone();
    let mut created_from: Option<u32> = None;
    let mut removed: Option<u32> = None;
    match op {
        SceneOp::MoveUp => scenes.swap(idx, idx - 1),
        SceneOp::MoveDown => scenes.swap(idx, idx + 1),
        SceneOp::Remove => {
            removed = Some(scene_id);
            scenes.remove(idx);
        }
        SceneOp::Duplicate => {
            let mut copy = scenes[idx].clone();
            // 一時的に衝突しない番号を持たせる (振り直しで正しくなる)。
            copy.scene_id = u32::MAX;
            created_from = Some(scene_id);
            scenes.insert(idx + 1, copy);
        }
    }

    // --- 振り直し (位置 + 1) と表の組み立て ---
    let mut remap = SceneRemap { removed, ..Default::default() };
    for (i, s) in scenes.iter_mut().enumerate() {
        let new_id = i as u32 + 1;
        if s.scene_id == u32::MAX {
            remap.created = Some(new_id);
        } else {
            remap.moved.push((s.scene_id, new_id));
        }
        s.scene_id = new_id;
    }

    // --- 手編集の追従。**この表だけを見る。** ---
    promo.caption_overrides = remap_map(&promo.caption_overrides, &remap, created_from);
    promo.plate_overrides = remap_map(&promo.plate_overrides, &remap, created_from);
    promo.original_copy = remap_map(&promo.original_copy, &remap, created_from);
    promo.plan.scenes = scenes;
    Ok(remap)
}

/// scene_id を鍵にした Map を振り直す。複製元の値は**新しいシーンにも写す** (見た目ごと複製する)。
fn remap_map<V: Clone>(src: &BTreeMap<u32, V>, remap: &SceneRemap, created_from: Option<u32>) -> BTreeMap<u32, V> {
    let mut out = BTreeMap::new();
    for (old, new) in &remap.moved {
        if let Some(v) = src.get(old) {
            out.insert(*new, v.clone());
        }
    }
    if let (Some(new), Some(from)) = (remap.created, created_from) {
        if let Some(v) = src.get(&from) {
            out.insert(new, v.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{CaptionOverride, PlateOverride};
    use crate::plan::{Aspect, ScenePlan};

    fn scene(id: u32, kind: CutKind) -> Scene {
        Scene {
            scene_id: id,
            cut_kind: kind,
            snapshot_index: if kind == CutKind::Product { Some(0) } else { None },
            plate_tilt: None,
            motion_prompt: "Slow push-in.".into(),
            duration_seconds: 4,
            shot_type: "Close-up".into(),
            video_prompt: "A desk.".into(),
            copy_text: format!("copy {id}"),
            image_prompt: "A desk.".into(),
            reference_image: None,
        }
    }

    fn summary() -> crate::plan::AnalyzedSummary {
        serde_json::from_value(serde_json::json!({
            "app_name": "App", "one_liner": "一行", "core_value": "価値",
            "target_audience": "誰か", "differentiators": ["a"], "hook_copy": "hook",
            "visual_identity": {"palette": ["#111111"], "mood": "calm", "ui_traits": ["flat"]}
        }))
        .expect("fixture")
    }

    /// n シーン。**product は最後の 1 枚だけ** — 「唯一の product は消せない」を試すため。
    fn promo_with_scenes(n: u32) -> PromoJson {
        let scenes: Vec<Scene> = (1..=n)
            .map(|i| scene(i, if i == n { CutKind::Product } else { CutKind::Mood }))
            .collect();
        PromoJson {
            project_path: "D:/proj".into(),
            snapshot_paths: vec!["C:/s/a.png".into()],
            video_concept: "concept".into(),
            summary: summary(),
            plan: ScenePlan { total_seconds: 15, aspect: Aspect::Landscape, scenes },
            caption_overrides: Default::default(),
            plate_overrides: Default::default(),
            original_copy: Default::default(),
            plate_mode: Default::default(),
            run_stats: None,
        }
    }

    /// 並び替えると scene_id は位置 + 1 に振り直され、**手編集も一緒に動く**。
    #[test]
    fn moving_a_scene_carries_the_human_edits_with_it() {
        let mut p = promo_with_scenes(4);
        p.caption_overrides.insert(3, CaptionOverride { color: Some("#123456".into()), ..Default::default() });
        p.plate_overrides.insert(3, PlateOverride { screen_ratio: Some(0.5), ..Default::default() });
        p.original_copy.insert(3, "最初の三番".into());

        let remap = apply_scene_edit(&mut p, SceneOp::MoveUp, 3).unwrap();

        assert_eq!(p.plan.scenes.iter().map(|s| s.scene_id).collect::<Vec<_>>(), vec![1, 2, 3, 4], "常に連番");
        assert_eq!(p.plan.scenes[1].copy_text, "copy 3", "中身が上がった");
        assert_eq!(p.plan.scenes[2].copy_text, "copy 2");
        assert_eq!(remap.moved_to(3), Some(2));
        assert_eq!(remap.moved_to(2), Some(3));

        assert_eq!(p.caption_overrides.get(&2).and_then(|c| c.color.clone()), Some("#123456".to_string()));
        assert_eq!(p.plate_overrides.get(&2).and_then(|c| c.screen_ratio), Some(0.5));
        assert_eq!(p.original_copy.get(&2).map(String::as_str), Some("最初の三番"));
        assert!(!p.caption_overrides.contains_key(&3), "元の位置に残骸を残さない");
    }

    /// 削除は後ろを詰め、消えたシーンの手編集は消える。
    #[test]
    fn removing_a_scene_shifts_the_rest_down() {
        let mut p = promo_with_scenes(4);
        p.caption_overrides.insert(2, CaptionOverride { color: Some("#222222".into()), ..Default::default() });
        p.caption_overrides.insert(4, CaptionOverride { color: Some("#444444".into()), ..Default::default() });

        let remap = apply_scene_edit(&mut p, SceneOp::Remove, 2).unwrap();

        assert_eq!(p.plan.scenes.len(), 3);
        assert_eq!(p.plan.scenes.iter().map(|s| s.scene_id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(p.plan.scenes[1].copy_text, "copy 3", "3 が 2 へ詰まった");
        assert_eq!(remap.removed, Some(2));
        assert_eq!(remap.moved_to(4), Some(3));
        assert_eq!(p.caption_overrides.get(&3).and_then(|c| c.color.clone()), Some("#444444".to_string()), "4 の手編集が 3 へ");
        assert_eq!(p.caption_overrides.len(), 1, "消えたシーンの手編集は残さない");
    }

    /// 複製は**見た目ごと**写す。参照画像は写さない (生成し直すまで無い)。
    #[test]
    fn duplicating_copies_the_scene_and_its_edits_but_not_the_images() {
        let mut p = promo_with_scenes(3);
        p.caption_overrides.insert(2, CaptionOverride { color: Some("#222222".into()), ..Default::default() });
        p.original_copy.insert(2, "最初の二番".into());

        let remap = apply_scene_edit(&mut p, SceneOp::Duplicate, 2).unwrap();

        assert_eq!(p.plan.scenes.len(), 4);
        assert_eq!(p.plan.scenes[1].copy_text, "copy 2");
        assert_eq!(p.plan.scenes[2].copy_text, "copy 2", "複製は隣に入る");
        assert_eq!(remap.created, Some(3));
        assert_eq!(p.caption_overrides.get(&3).and_then(|c| c.color.clone()), Some("#222222".to_string()), "手編集も写す");
        assert_eq!(p.original_copy.get(&3).map(String::as_str), Some("最初の二番"));
        assert_eq!(remap.moved_to(3), Some(4), "元の 3 は 4 へ");
    }

    /// 限界は**検査を待たずに拒む** (契約 `SceneEdit.limits`)。
    #[test]
    fn the_limits_are_refused_before_the_plan_becomes_invalid() {
        let mut three = promo_with_scenes(3);
        assert!(matches!(apply_scene_edit(&mut three, SceneOp::Remove, 1), Err(SceneEditError::TooFew { .. })));

        let mut eight = promo_with_scenes(8);
        assert!(matches!(apply_scene_edit(&mut eight, SceneOp::Duplicate, 1), Err(SceneEditError::TooMany { .. })));

        let mut p = promo_with_scenes(4);
        assert!(matches!(apply_scene_edit(&mut p, SceneOp::MoveUp, 1), Err(SceneEditError::AtEdge)));
        assert!(matches!(apply_scene_edit(&mut p, SceneOp::MoveDown, 4), Err(SceneEditError::AtEdge)));
        assert!(matches!(apply_scene_edit(&mut p, SceneOp::Remove, 99), Err(SceneEditError::UnknownScene { .. })));
    }

    /// 拒んだ時は **promo を触らない** (rev37 と同じ規律)。
    #[test]
    fn a_refused_edit_leaves_the_promo_untouched() {
        let mut p = promo_with_scenes(3);
        let before = p.clone();
        let _ = apply_scene_edit(&mut p, SceneOp::Remove, 1);
        assert_eq!(p, before);
    }

    /// 唯一の product カットは消せない。
    #[test]
    fn the_only_product_cut_cannot_be_removed() {
        let mut p = promo_with_scenes(4);
        assert!(matches!(apply_scene_edit(&mut p, SceneOp::Remove, 4), Err(SceneEditError::LastProductCut)));
        let mut q = promo_with_scenes(4);
        q.plan.scenes[0].cut_kind = CutKind::Product;
        assert!(apply_scene_edit(&mut q, SceneOp::Remove, 4).is_ok(), "product が 2 枚あれば消せる");
    }

    /// 尺は触らない。**合計のずれは出すだけ。**
    #[test]
    fn durations_are_never_touched_and_the_drift_is_reported() {
        let mut p = promo_with_scenes(4);
        assert_eq!(duration_drift(&p.plan), 1, "4 x 4s = 16 に対し尺 15");
        let before: Vec<u32> = p.plan.scenes.iter().map(|s| s.duration_seconds).collect();
        apply_scene_edit(&mut p, SceneOp::Remove, 2).unwrap();
        assert_eq!(
            p.plan.scenes.iter().map(|s| s.duration_seconds).collect::<Vec<_>>(),
            before[..3].to_vec(),
            "残りの尺を書き換えない"
        );
        assert_eq!(duration_drift(&p.plan), -3, "合計 12 秒 - 尺 15 秒");
    }

    /// **入れ替えでファイル名が衝突しない順序**を返す (契約 `SceneEdit.rename_order`)。
    #[test]
    fn the_rename_plan_never_overwrites_a_file_that_is_still_needed() {
        let mut p = promo_with_scenes(4);
        let remap = apply_scene_edit(&mut p, SceneOp::MoveUp, 2).unwrap();
        let steps = remap.rename_steps();
        assert_eq!(steps.len(), 4, "2 枚ぶんを一時名経由で: {steps:?}");
        assert!(steps[..2].iter().all(|(_, to)| to.ends_with(".tmp")), "先に全部を一時名へ: {steps:?}");
        assert!(steps[2..].iter().all(|(from, _)| from.ends_with(".tmp")), "そのあと最終名へ: {steps:?}");
        assert!(steps.iter().all(|(f, _)| f.contains("scene_01") || f.contains("scene_02")), "動かないシーンは触らない: {steps:?}");
    }

    /// PromoJson は serde で往復できる (形を壊していない)。
    #[test]
    fn the_edited_promo_still_round_trips() {
        let mut p = promo_with_scenes(4);
        apply_scene_edit(&mut p, SceneOp::Duplicate, 1).unwrap();
        let text = serde_json::to_string(&p).unwrap();
        let back: PromoJson = serde_json::from_str(&text).unwrap();
        assert_eq!(back, p);
    }
}
