//! 解析 → シーン構成。「LLM は書き、Rust は検める」の結線。
//!
//! 再生成ループ: `validate_scene_plan` の違反を `repair_suffix` で本文に戻して最大 `MAX_REPAIRS` 回。
//! 直らなければ最後の違反を添えて失敗 (黙って通さない)。Kataribe の adjudicate → 却下理由 → 再生成と同型。

use cli_runner::runner::RunFailed;
use promo_core::brief::SnapshotMeta;
use promo_core::plan::{AnalyzedSummary, Aspect, PlanViolation, PlateMode, ScenePlan, schema_for_scene_plan, schema_for_summary, validate_scene_plan};
use promo_core::prompts::{Language, analysis_prompt, repair_suffix, scene_prompt};
use serde_json::Value;

use crate::task::TaskRunner;

/// 再生成の上限 (初回 + 修復 2 回)。
pub const MAX_REPAIRS: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("{0}")]
    Cli(#[from] RunFailed),
    /// 構造化出力が無い / 型に合わない。raw を保持。
    #[error("LLM の出力を {what} として読めません: {detail}\n{raw}")]
    Shape { what: &'static str, detail: String, raw: String },
    /// 修復しても違反が残った。
    #[error("シーン構成が {attempts} 回で検査を通りませんでした: {}", .violations.iter().map(promo_core::describe_violation).collect::<Vec<_>>().join("; "))]
    StillInvalid { attempts: usize, violations: Vec<PlanViolation> },
}

/// 1 段の報告 (UI の「何回目で通ったか」「いくらかかったか」)。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StageReport {
    pub attempts: usize,
    /// **None = 記録なし** (費用を返さない CLI がある)。0 で埋めない。
    pub cost_usd: Option<f64>,
    pub duration_ms: u64,
    /// 各試行で残った違反 (最後は空なら成功)。
    pub violations_per_attempt: Vec<Vec<PlanViolation>>,
    /// 各試行で CLI が名乗ったモデル (rev25)。重複はここでは消さない — 整列と重複除去は `RunStats::new`。
    pub models: Vec<String>,
}

/// 費用の足し算。**片方でも不明なら合計は不明。**
///
/// 分かっている分だけ足すと「一部しか数えていない総額」という別種の嘘になる。
/// agy は費用を返さないので (契約 `AgyStreamLine.cost`)、この分岐は実際に通る。
pub fn add_cost(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        _ => None,
    }
}

fn structured_or_shape(ok: &cli_runner::runner::RunOk, what: &'static str) -> Result<Value, PipelineError> {
    match &ok.structured {
        Some(v) => Ok(v.clone()),
        None => Err(PipelineError::Shape { what, detail: "構造化出力が無い".into(), raw: head(&ok.text) }),
    }
}

fn head(s: &str) -> String {
    s.chars().take(400).collect()
}

/// タスク 1: 解析。
pub async fn analyze(
    runner: &dyn TaskRunner,
    brief_text: &str,
    concept: &str,
    language: Language,
) -> Result<(AnalyzedSummary, StageReport), PipelineError> {
    let prompt = analysis_prompt(brief_text, concept, language);
    let schema = schema_for_summary();
    let ok = runner.run_task(&prompt, Some(&schema)).await?;
    let v = structured_or_shape(&ok, "AnalyzedSummary")?;
    let summary: AnalyzedSummary = serde_json::from_value(v.clone())
        .map_err(|e| PipelineError::Shape { what: "AnalyzedSummary", detail: e.to_string(), raw: head(&v.to_string()) })?;
    Ok((summary, StageReport { attempts: 1, cost_usd: ok.cost_usd, duration_ms: ok.duration_ms, violations_per_attempt: vec![vec![]], models: ok.model.clone().into_iter().collect() }))
}

/// タスク 2: シーン構成 + 検査ループ。
#[allow(clippy::too_many_arguments)] // 契約の入力をそのまま取る (要約 / 概念 / 尺 / 比率 / 言語 / スナップショット / 面の貼り方)
pub async fn plan_scenes(
    runner: &dyn TaskRunner,
    summary: &AnalyzedSummary,
    concept: &str,
    total_seconds: u32,
    aspect: Aspect,
    language: Language,
    snapshots: &[SnapshotMeta],
    plate_mode: PlateMode,
) -> Result<(ScenePlan, StageReport), PipelineError> {
    let base = scene_prompt(summary, concept, total_seconds, aspect, language, snapshots, plate_mode);
    let schema = schema_for_scene_plan();
    // 費用は「0 回の試行 = 0」から積む。不明を 1 つでも足した時点で不明に落ちる (add_cost)。
    let mut report = StageReport { cost_usd: Some(0.0), ..Default::default() };
    let mut prompt = base.clone();
    for attempt in 1..=(1 + MAX_REPAIRS) {
        report.attempts = attempt;
        let ok = runner.run_task(&prompt, Some(&schema)).await?;
        report.cost_usd = add_cost(report.cost_usd, ok.cost_usd);
        report.duration_ms += ok.duration_ms;
        // rev25: CLI が名乗ったモデルを試行ごとに積む (名乗らない CLI では何も足さない)。
        report.models.extend(ok.model.clone());
        let v = structured_or_shape(&ok, "ScenePlan")?;
        let plan: ScenePlan = serde_json::from_value(v.clone())
            .map_err(|e| PipelineError::Shape { what: "ScenePlan", detail: e.to_string(), raw: head(&v.to_string()) })?;
        let violations = validate_scene_plan(&plan, snapshots.len(), plate_mode);
        report.violations_per_attempt.push(violations.clone());
        if violations.is_empty() {
            return Ok((plan, report));
        }
        if attempt == 1 + MAX_REPAIRS {
            return Err(PipelineError::StillInvalid { attempts: attempt, violations });
        }
        prompt = format!("{base}{}", repair_suffix(&v.to_string(), &violations));
    }
    unreachable!("ループは必ず return する")
}

/// **promo.json に残す再生成の記録を組み立てる唯一の場所** (rev25)。
///
/// それまで CLI (`promo run`) と GUI (`start_run`) が `RunStats::new(r2.attempts, …, r1.cost_usd + r2.cost_usd)` を
/// 別々に手で組んでいた。項目を足すたびに両側を思い出す必要があり、#18 (片側だけ直る) の再発条件だった。
pub fn run_stats(analyze: &StageReport, plan: &StageReport) -> promo_core::RunStats {
    let models: Vec<String> = analyze.models.iter().chain(&plan.models).cloned().collect();
    promo_core::RunStats::new(plan.attempts, &plan.violations_per_attempt, add_cost(analyze.cost_usd, plan.cost_usd), &models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::TaskRunner;
    use cli_runner::runner::RunOk;
    use promo_core::plan::{Scene, VisualIdentity};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    /// 呼ばれるたびに canned 応答を順に返し、受け取った本文を記録する fake (Sync のため Mutex)。
    struct Fake {
        replies: Mutex<Vec<Value>>,
        prompts: Mutex<Vec<String>>,
        /// 費用を返さない CLI (agy) の形。`false` なら毎回 None を返す。
        reports_cost: bool,
    }

    impl Fake {
        fn new(replies: Vec<Value>) -> Self {
            Self { replies: Mutex::new(replies), prompts: Mutex::new(vec![]), reports_cost: true }
        }

        /// agy のように費用を返さない CLI。
        fn without_cost(replies: Vec<Value>) -> Self {
            Self { replies: Mutex::new(replies), prompts: Mutex::new(vec![]), reports_cost: false }
        }
    }

    impl TaskRunner for Fake {
        fn run_task<'a>(
            &'a self,
            prompt: &'a str,
            _schema: Option<&'a Value>,
        ) -> Pin<Box<dyn Future<Output = Result<RunOk, RunFailed>> + Send + 'a>> {
            self.prompts.lock().unwrap().push(prompt.to_string());
            let v = self.replies.lock().unwrap().remove(0);
            // 何回目の呼び出しかをモデル名に写す (試行をまたいで運ばれるかを見分けるため)。
            let n = self.prompts.lock().unwrap().len();
            Box::pin(async move {
                Ok(RunOk {
                    text: v.to_string(),
                    structured: Some(v),
                    cost_usd: if self.reports_cost { Some(0.01) } else { None },
                    duration_ms: 10,
                    model: Some(format!("model-{n}")),
                })
            })
        }
    }

    fn summary() -> AnalyzedSummary {
        AnalyzedSummary {
            app_name: "T".into(),
            one_liner: "o".into(),
            core_value: "c".into(),
            target_audience: "t".into(),
            differentiators: vec!["a".into(), "b".into(), "c".into()],
            hook_copy: "h".into(),
            visual_identity: VisualIdentity { palette: vec![], mood: "m".into(), ui_traits: vec![] },
        }
    }

    fn scene(id: u32, image_prompt: &str) -> Scene {
        Scene {
            scene_id: id,
            cut_kind: promo_core::plan::CutKind::Mood,
            snapshot_index: None,
            plate_tilt: None,
            motion_prompt: "Slow push-in.".into(),
            duration_seconds: 5,
            shot_type: "Wide".into(),
            video_prompt: "A calm office, slow pan.".into(),
            copy_text: "c".into(),
            image_prompt: image_prompt.into(),
            reference_image: None,
        }
    }

    fn plan(scenes: Vec<Scene>) -> Value {
        serde_json::to_value(ScenePlan { total_seconds: 15, aspect: Aspect::Landscape, scenes }).unwrap()
    }

    #[tokio::test]
    async fn analyze_parses_structured_summary() {
        let fake = Fake::new(vec![serde_json::to_value(summary()).unwrap()]);
        let (s, r) = analyze(&fake, "BRIEF", "concept", Language::Ja).await.unwrap();
        assert_eq!(s, summary());
        assert_eq!(r.attempts, 1);
        assert!(fake.prompts.lock().unwrap()[0].contains("BRIEF"));
    }

    #[tokio::test]
    async fn invalid_plan_is_sent_back_with_violations_then_accepted() {
        let bad = plan(vec![scene(1, "Same as the screenshot"), scene(2, "ok"), scene(3, "ok")]);
        let good = plan(vec![scene(1, "A desk"), scene(2, "ok"), scene(3, "ok")]);
        let fake = Fake::new(vec![bad, good]);
        let (p, r) = plan_scenes(&fake, &summary(), "concept", 15, Aspect::Landscape, Language::Ja, &[], PlateMode::default()).await.unwrap();
        assert_eq!(p.scenes.len(), 3);
        assert_eq!(r.attempts, 2);
        assert_eq!(r.violations_per_attempt.len(), 2);
        assert!(r.violations_per_attempt[1].is_empty());
        let prompts = fake.prompts.lock().unwrap();
        assert!(!prompts[0].contains("previous answer was rejected"));
        assert!(prompts[1].contains("previous answer was rejected"));
        assert!(prompts[1].contains("mentions the input (`screenshot`)"));
        assert!(prompts[1].contains("Same as the screenshot"), "前回の出力を添えて直させる");
        assert!((r.cost_usd.unwrap() - 0.02).abs() < 1e-9, "コストは試行の合計");
    }

    /// rev25: 試行ごとに CLI が名乗ったモデルを落とさず運ぶ (再生成の途中で変わっても残る)。
    #[tokio::test]
    async fn models_named_by_the_cli_are_carried_across_attempts() {
        let bad = plan(vec![scene(1, "Same as the screenshot"), scene(2, "ok"), scene(3, "ok")]);
        let good = plan(vec![scene(1, "A desk"), scene(2, "ok"), scene(3, "ok")]);
        let fake = Fake::new(vec![bad, good]);
        let (_, r) = plan_scenes(&fake, &summary(), "concept", 15, Aspect::Landscape, Language::Ja, &[], PlateMode::default()).await.unwrap();
        assert_eq!(r.models, vec!["model-1".to_string(), "model-2".to_string()]);
        let fake = Fake::new(vec![serde_json::to_value(summary()).unwrap()]);
        let (_, a) = analyze(&fake, "BRIEF", "concept", Language::Ja).await.unwrap();
        assert_eq!(a.models, vec!["model-1".to_string()]);
    }

    /// **費用を返さない CLI があるので、不明を 0 で埋めない** (rev42、agy 実測)。
    /// 片方でも不明なら合計は不明 — 分かっている分だけ足すと「一部しか数えていない総額」になる。
    #[test]
    fn an_unknown_cost_poisons_the_sum_instead_of_counting_as_zero() {
        assert_eq!(add_cost(Some(0.3), Some(0.7)), Some(1.0));
        assert_eq!(add_cost(Some(0.3), None), None, "分かっている分だけ足さない");
        assert_eq!(add_cost(None, Some(0.7)), None);
        assert_eq!(add_cost(None, None), None);
    }

    /// 費用を返さない CLI で走らせた段は「記録なし」。**0.000 USD と描かない。**
    #[tokio::test]
    async fn a_stage_run_by_a_cli_without_cost_reports_no_cost() {
        let fake = Fake::without_cost(vec![serde_json::to_value(summary()).unwrap()]);
        let (_, a) = analyze(&fake, "BRIEF", "concept", Language::Ja).await.unwrap();
        assert_eq!(a.cost_usd, None, "費用を返さない CLI の段は記録なし");

        let good = plan(vec![scene(1, "A desk"), scene(2, "ok"), scene(3, "ok")]);
        let fake = Fake::without_cost(vec![good]);
        let (_, p) = plan_scenes(&fake, &summary(), "concept", 15, Aspect::Landscape, Language::Ja, &[], PlateMode::default()).await.unwrap();
        assert_eq!(p.cost_usd, None);

        // 合計も記録なしのまま promo.json へ運ばれる。
        assert_eq!(run_stats(&a, &p).cost_usd, None);
    }

    /// `run_stats` は**全項目**を運ぶ (#18 の処方: 組み立てを関数にして全フィールドを写す PoC を 1 本)。
    #[test]
    fn run_stats_carries_every_field_from_both_stages() {
        let analyze = StageReport {
            attempts: 1,
            cost_usd: Some(0.3),
            duration_ms: 1,
            violations_per_attempt: vec![vec![]],
            models: vec!["claude-sonnet-5".into()],
        };
        let plan = StageReport {
            attempts: 2,
            cost_usd: Some(0.7),
            duration_ms: 2,
            violations_per_attempt: vec![vec![PlanViolation::NoProductCut], vec![]],
            models: vec!["claude-sonnet-5".into(), "claude-sonnet-5".into()],
        };
        let s = run_stats(&analyze, &plan);
        assert_eq!(s.plan_attempts, 2);
        assert!((s.cost_usd.unwrap() - 1.0).abs() < 1e-9, "analyze + plan の合計");
        assert_eq!(s.violation_kinds, vec!["no_product_cut"]);
        assert_eq!(s.models, Some(vec!["claude-sonnet-5".to_string()]), "重複なし");
    }

    #[tokio::test]
    async fn still_invalid_after_max_repairs_fails_loudly() {
        let bad = plan(vec![scene(1, "x"), scene(2, "x")]); // 2 scene = 件数違反
        let fake = Fake::new(vec![bad.clone(), bad.clone(), bad]);
        let err = plan_scenes(&fake, &summary(), "c", 15, Aspect::Landscape, Language::En, &[], PlateMode::default()).await.unwrap_err();
        match err {
            PipelineError::StillInvalid { attempts, violations } => {
                assert_eq!(attempts, 1 + MAX_REPAIRS);
                assert!(violations.contains(&PlanViolation::SceneCount { got: 2 }));
            }
            other => panic!("{other}"),
        }
        assert_eq!(fake.prompts.lock().unwrap().len(), 1 + MAX_REPAIRS);
    }

    #[tokio::test]
    async fn wrong_shape_keeps_raw() {
        let fake = Fake::new(vec![serde_json::json!({"not": "a plan"})]);
        let err = plan_scenes(&fake, &summary(), "c", 15, Aspect::Landscape, Language::En, &[], PlateMode::default()).await.unwrap_err();
        assert!(matches!(err, PipelineError::Shape { what: "ScenePlan", ref raw, .. } if raw.contains("not")));
    }
}
