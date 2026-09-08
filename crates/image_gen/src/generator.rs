//! `ImageGenerator` trait (契約) と HTTP 実装。プロンプト合成の純関数もここ。
//!
//! OpenAI / Gemini は `provider::generate` にそのまま委ねる。ComfyUI だけは待機判定 (`/queue` 併読 +
//! バックオフ + 進捗) を足すため、provider の純関数 (置換 / 剪定 / history 解釈 / view URL) を組み直して
//! 自前の HTTP ループを持つ。provider.rs 自体は Kataribe との diff を保つため触らない。

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use serde_json::Value;

use crate::comfy_wait::{QueueState, backoff, describe, queue_state};
use crate::provider::{
    Generated, ImageGenConfig, ImageGenError, Provider, RefImage, SizeMap, check_comfy_workflow_shape, classify_status,
    comfy_first_image, comfy_prompt_body, comfy_prune_unfilled_refs, comfy_substitute, comfy_uploaded_name, comfy_view_url,
    ComfyVars,
};

/// 参照つきの時だけ足す 1 場面規律 (Kataribe #85 ②: 分割・資料集レイアウトへの保険)。
pub const SINGLE_FRAME_CLAUSE: &str = "A single continuous scene, one frame, no panels or split screen.";

/// 参照画像用プロンプトの合成 (純粋)。`prefix` = スタイルアンカー (ImageGenConfig.user_prefix)。
/// 参照そのものを指す語は書かない (#85 ①) — 入力側 (`image_prompt`) は validate_scene_plan が既に弾いている。
pub fn compose_reference_prompt(prefix: &str, image_prompt: &str, has_refs: bool) -> String {
    let mut s = String::new();
    let prefix = prefix.trim();
    if !prefix.is_empty() {
        s.push_str(prefix.trim_end_matches(['.', '。']));
        s.push_str(". ");
    }
    s.push_str(image_prompt.trim());
    if has_refs {
        if !s.ends_with('.') {
            s.push('.');
        }
        s.push(' ');
        s.push_str(SINGLE_FRAME_CLAUSE);
    }
    s
}

/// 契約 `ImageGenerator`。`progress` は待機の見せ方 (ComfyUI のキュー位置など)。
/// Send 境界は Tauri の async command が future 全体に Send を要求するため (Phase D)。
pub trait ImageGenerator: Send + Sync {
    fn generate<'a>(
        &'a self,
        cfg: &'a ImageGenConfig,
        api_key: &'a str,
        prompt: &'a str,
        seed: u64,
        refs: &'a [RefImage],
        progress: &'a mut (dyn FnMut(String) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<Generated, ImageGenError>> + Send + 'a>>;

    fn probe<'a>(
        &'a self,
        cfg: &'a ImageGenConfig,
        api_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ImageGenError>> + Send + 'a>>;
}

/// 本番: HTTP。
pub struct HttpImageGenerator;

impl ImageGenerator for HttpImageGenerator {
    fn generate<'a>(
        &'a self,
        cfg: &'a ImageGenConfig,
        api_key: &'a str,
        prompt: &'a str,
        seed: u64,
        refs: &'a [RefImage],
        progress: &'a mut (dyn FnMut(String) + Send),
    ) -> Pin<Box<dyn Future<Output = Result<Generated, ImageGenError>> + Send + 'a>> {
        Box::pin(async move {
            match cfg.provider {
                Provider::Comfy => comfy_generate(cfg, prompt, seed, refs, progress).await,
                _ => crate::provider::generate(cfg, api_key, prompt, seed, refs).await,
            }
        })
    }

    fn probe<'a>(
        &'a self,
        cfg: &'a ImageGenConfig,
        api_key: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ImageGenError>> + Send + 'a>> {
        Box::pin(crate::provider::probe(cfg, api_key))
    }
}

/// `/queue` を読めなかった時の扱い: 判断不能 = 待機継続。
async fn read_queue(http: &reqwest::Client, base: &str) -> Option<Value> {
    let r = http.get(format!("{base}/queue")).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.json().await.ok()
}

/// ComfyUI: Kataribe の流れ (upload → 置換 → 剪定 → /prompt → /history → /view) に、
/// `/queue` 併読・バックオフ・進捗・消失の即失敗を足したもの。
async fn comfy_generate(
    cfg: &ImageGenConfig,
    prompt: &str,
    seed: u64,
    refs: &[RefImage],
    progress: &mut (dyn FnMut(String) + Send),
) -> Result<Generated, ImageGenError> {
    let refs = &refs[..refs.len().min(crate::provider::max_refs(Provider::Comfy))];
    let http = reqwest::Client::builder().timeout(cfg.timeout()).build().map_err(ImageGenError::from)?;
    let wf_text = cfg
        .workflow_json
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ImageGenError::Config("ComfyUI のワークフロー JSON が未設定です".into()))?;
    let wf: Value =
        serde_json::from_str(wf_text).map_err(|e| ImageGenError::Config(format!("ワークフロー JSON を読めません: {e}")))?;
    check_comfy_workflow_shape(&wf)?;
    let (w, h) = SizeMap::comfy_dims(cfg.shape);
    let base = cfg.base().to_string();

    let mut ref_names: Vec<String> = Vec::new();
    for r in refs {
        let part = reqwest::multipart::Part::bytes(r.bytes.clone())
            .file_name(r.name.clone())
            .mime_str(&r.mime)
            .map_err(|e| ImageGenError::Config(format!("mime: {e}")))?;
        let form = reqwest::multipart::Form::new().part("image", part).text("overwrite", "true");
        let resp = http.post(format!("{base}/upload/image")).multipart(form).send().await.map_err(ImageGenError::from)?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(ImageGenError::from)?;
        if !(200..300).contains(&status) {
            return Err(classify_status(status, text));
        }
        let name = comfy_uploaded_name(&text)
            .ok_or_else(|| ImageGenError::Shape { detail: "/upload/image の応答に name が無い".into(), raw: text.clone() })?;
        ref_names.push(name);
    }
    progress(format!("ComfyUI: 参照 {} 枚を送信", ref_names.len()));

    let vars = ComfyVars { prompt, negative: &cfg.negative, seed, width: w, height: h, refs: &ref_names };
    let substituted = comfy_prune_unfilled_refs(&comfy_substitute(&wf, &vars));
    let client_id = format!("apppromo-{seed}");
    let resp = http
        .post(format!("{base}/prompt"))
        .json(&comfy_prompt_body(substituted, &client_id))
        .send()
        .await
        .map_err(ImageGenError::from)?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(ImageGenError::from)?;
    if !(200..300).contains(&status) {
        return Err(classify_status(status, text));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| ImageGenError::Shape { detail: format!("/prompt の応答が JSON でない: {e}"), raw: text.clone() })?;
    let prompt_id = v
        .get("prompt_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ImageGenError::Shape { detail: "prompt_id が無い".into(), raw: text.clone() })?
        .to_string();
    progress("ComfyUI: キューに投入".into());

    let deadline = Instant::now() + cfg.timeout();
    let mut attempt: u32 = 0;
    let image_ref = loop {
        if Instant::now() > deadline {
            return Err(ImageGenError::Timeout { provider: Provider::Comfy });
        }
        tokio::time::sleep(backoff(attempt)).await;
        attempt += 1;
        let r = http.get(format!("{base}/history/{prompt_id}")).send().await.map_err(ImageGenError::from)?;
        if r.status().is_success() {
            if let Ok(hist) = r.json::<Value>().await {
                if let Some(img) = comfy_first_image(&hist, &prompt_id)? {
                    break img;
                }
            }
        }
        // history 未着 → queue で『待機』か『消失』かを分ける。読めなければ待機継続。
        if let Some(q) = read_queue(&http, &base).await {
            match queue_state(&q, &prompt_id) {
                Some(QueueState::Absent) => {
                    return Err(ImageGenError::ComfyNodeError {
                        node: "queue".into(),
                        msg: "prompt_id がキューにも履歴にも無い (消失)。ComfyUI 側のログを確認してください".into(),
                    });
                }
                Some(s) => progress(describe(&s)),
                None => {}
            }
        }
    };
    progress("ComfyUI: 画像を取得".into());
    let r = http.get(comfy_view_url(&base, &image_ref)).send().await.map_err(ImageGenError::from)?;
    let status = r.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(classify_status(status, String::new()));
    }
    let bytes = r.bytes().await.map_err(ImageGenError::from)?.to_vec();
    Ok(Generated { mime: "image/png".into(), bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_joins_prefix_and_prompt_and_adds_single_frame_only_with_refs() {
        let p = compose_reference_prompt("Warm ember light, parchment tones.", "A desk with dice.", false);
        assert_eq!(p, "Warm ember light, parchment tones. A desk with dice.");
        let p = compose_reference_prompt("", "A desk with dice", true);
        assert_eq!(p, format!("A desk with dice. {SINGLE_FRAME_CLAUSE}"));
        let p = compose_reference_prompt("  ", "  x.  ", false);
        assert_eq!(p, "x.");
    }

    #[test]
    fn compose_never_mentions_references() {
        let p = compose_reference_prompt("calm", "A desk.", true);
        for w in ["reference", "screenshot", "sheet", "attached"] {
            assert!(!p.to_lowercase().contains(w), "{w} が混入: {p}");
        }
    }
}
