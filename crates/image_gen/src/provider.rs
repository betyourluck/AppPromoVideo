//! 画像生成 (spec 24, 2026-08-20): 物語に合わせた挿絵を背景と文字の間に重ねる — の backend 側。
//!
//! **ここは提示層**。正本 (gm_core) にも語りにも prompt caching にも触れない。
//! プロバイダのワイヤ (OpenAI Images / Gemini 画像 / ComfyUI) の **encode/decode は純関数**で
//! PoC で固め、HTTP だけを [`generate`] が担う (llm_client の adapter seam と同じ流儀)。
//!
//! **HTTP は全部 backend** — CSP の `connect-src` は localhost のみで、クラウドのキーを WebView に
//! 置かない。ComfyUI も backend から叩く (リモートでも CSP 対象外)。表示は data URL
//! (`img-src data:` は開いている)。契約は data_contract `ImageGeneration` が正。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// --- 設定 (frontend の localStorage から渡る非秘密。キーは backend が .env からマージ) ------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Openai,
    Gemini,
    Comfy,
}

/// UI の 1 軸 (形)。プロバイダ語彙への写像は [`SizeMap`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Square,
    Landscape,
    Portrait,
}

/// UI の 1 軸 (解像度段)。`Highest` は openai の `quality: high` のみ (高コスト)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    #[default]
    Standard,
    High,
    Highest,
}

/// プロンプト様式 (プロンプト書きの出力形)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptStyle {
    Tags,
    Prose,
}

impl PromptStyle {
    /// 既定はプロバイダに倒す (openai/gemini=自然文、comfy=タグ)。
    pub fn default_for(provider: Provider) -> Self {
        match provider {
            Provider::Openai | Provider::Gemini => PromptStyle::Prose,
            Provider::Comfy => PromptStyle::Tags,
        }
    }
}

/// frontend から渡る非秘密の設定。API キーは含まない (backend が `.env` からマージする)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenConfig {
    pub provider: Provider,
    /// openai: `https://api.openai.com` / gemini: `https://generativelanguage.googleapis.com` /
    /// comfy: `http://127.0.0.1:8188`。末尾スラッシュは正規化する。
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub detail: Detail,
    /// None なら [`PromptStyle::default_for`]。
    #[serde(default)]
    pub style: Option<PromptStyle>,
    /// ユーザーのスタイル接頭辞 (プロンプト書きの素材)。
    #[serde(default)]
    pub user_prefix: String,
    /// ネガティブプロンプト (comfy のみ使う)。
    #[serde(default)]
    pub negative: String,
    /// comfy の API 形式ワークフロー JSON (文字列)。
    #[serde(default)]
    pub workflow_json: Option<String>,
    /// タイムアウト秒の上書き (None ならプロバイダ別既定)。
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// seed 固定 (spec 27 B-4)。**seed を持つのは ComfyUI だけ**なので他プロバイダでは効かない
    /// (UI 側で無効表示)。固定しないと参照やプロンプトの A/B にサンプリングの運が混じり、
    /// 「変わった気がする」しか返らない。
    #[serde(default)]
    pub lock_seed: bool,
    #[serde(default)]
    pub seed: u64,
}

impl ImageGenConfig {
    pub fn style(&self) -> PromptStyle {
        self.style.unwrap_or_else(|| PromptStyle::default_for(self.provider))
    }

    pub fn base(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }

    /// プロバイダ別の既定タイムアウト (契約 `timeouts`)。
    pub fn timeout(&self) -> Duration {
        let secs = self.timeout_secs.unwrap_or(match self.provider {
            Provider::Openai => 120,
            Provider::Gemini => 90,
            Provider::Comfy => 600,
        });
        Duration::from_secs(secs.max(5))
    }

    /// 既定モデル (欄が空のとき)。
    pub fn model_or_default(&self) -> &str {
        if !self.model.trim().is_empty() {
            return self.model.trim();
        }
        match self.provider {
            // 既定は軽量モデル (2026-08-20 ユーザー決定): 挿絵は都度・手動なので安い側に倒す。
            // 上位 (gpt-image-1 / gemini-3-pro-image-preview 等) は欄に書けば使える。
            Provider::Openai => "gpt-image-1-mini",
            Provider::Gemini => "gemini-3.1-flash-lite-image",
            Provider::Comfy => "",
        }
    }
}

// --- サイズ写像 (契約 `size_preset`) ---------------------------------------------------

pub struct SizeMap;

impl SizeMap {
    pub fn openai_size(shape: Shape) -> &'static str {
        match shape {
            Shape::Square => "1024x1024",
            Shape::Landscape => "1536x1024",
            Shape::Portrait => "1024x1536",
        }
    }
    /// quality で消費が 16 倍動く (high 4160 tok / low 272 tok) → standard=low。
    pub fn openai_quality(detail: Detail) -> &'static str {
        match detail {
            Detail::Standard => "low",
            Detail::High => "medium",
            Detail::Highest => "high",
        }
    }
    pub fn gemini_aspect(shape: Shape) -> &'static str {
        match shape {
            Shape::Square => "1:1",
            Shape::Landscape => "16:9",
            Shape::Portrait => "9:16",
        }
    }
    pub fn gemini_size(detail: Detail) -> &'static str {
        match detail {
            Detail::Standard => "1K",
            Detail::High | Detail::Highest => "2K",
        }
    }
    /// SDXL 系の既定 (`%width%`×`%height%`)。
    pub fn comfy_dims(shape: Shape) -> (u32, u32) {
        match shape {
            Shape::Square => (1024, 1024),
            Shape::Landscape => (1344, 768),
            Shape::Portrait => (768, 1344),
        }
    }
}

// --- 設定画集 = 参照画像 (spec 25) ------------------------------------------------------

/// 添付する参照画像 1 枚 (harness の `SheetImage` を backend へ写したもの)。
#[derive(Debug, Clone)]
pub struct RefImage {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// プロバイダ別の参照枚数上限 (契約 settings_sheets。gemini-2.5-flash-image の公式上限 3)。
pub fn max_refs(provider: Provider) -> usize {
    match provider {
        Provider::Openai => 3,
        Provider::Gemini => 3,
        Provider::Comfy => 3,
    }
}

// --- エラー (契約 `errors`) -----------------------------------------------------------

#[derive(Debug)]
pub enum ImageGenError {
    Unauthorized,
    /// 429。`detail` は本文の要点 — Google は**どのクォータに当たったか**を本文で名指しする
    /// (`free_tier ... limit: 0` = キーのプロジェクトが無課金、が実際に出た。2026-08-20 実測:
    /// 捨てていたせいで「課金は入っているのに 429」の切り分けに手が要った)。
    RateLimited { detail: String },
    Blocked { reason: String },
    Timeout { provider: Provider },
    ComfyNodeError { node: String, msg: String },
    Api { status: u16, body: String },
    Network { detail: String },
    /// 応答の形が合わない / 画像が入っていない (本文を保持 — #34 同型)。
    Shape { detail: String, raw: String },
    Config(String),
}

impl std::fmt::Display for ImageGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageGenError::Unauthorized => write!(f, "認証に失敗しました (API キーを確認してください)"),
            ImageGenError::RateLimited { detail } => {
                write!(f, "レート制限に達しました: {detail}")
            }
            ImageGenError::Blocked { reason } => {
                write!(f, "プロバイダが生成をブロックしました (理由: {reason})")
            }
            ImageGenError::Timeout { provider } => {
                write!(f, "{provider:?} の応答がタイムアウトしました")
            }
            ImageGenError::ComfyNodeError { node, msg } => {
                write!(f, "ComfyUI のノード {node} でエラー: {msg}")
            }
            ImageGenError::Api { status, body } => {
                let head: String = body.chars().take(300).collect();
                write!(f, "API エラー (status={status}): {head}")
            }
            ImageGenError::Network { detail } => write!(f, "接続できません: {detail}"),
            ImageGenError::Shape { detail, raw } => {
                let head: String = raw.chars().take(300).collect();
                write!(f, "応答の形が想定と違います ({detail}): {head}")
            }
            ImageGenError::Config(s) => write!(f, "設定エラー: {s}"),
        }
    }
}

impl From<reqwest::Error> for ImageGenError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            // provider はここでは分からないので呼び出し側が詰め直す (Comfy)。
            return ImageGenError::Network { detail: "timeout".into() };
        }
        let mut detail = e.to_string();
        let mut cause: Option<&dyn std::error::Error> = std::error::Error::source(&e);
        while let Some(inner) = cause {
            detail.push_str(": ");
            detail.push_str(&inner.to_string());
            cause = inner.source();
        }
        ImageGenError::Network { detail }
    }
}

/// HTTP ステータスからの分類 (純粋)。
pub fn classify_status(status: u16, body: String) -> ImageGenError {
    match status {
        401 | 403 => ImageGenError::Unauthorized,
        429 => ImageGenError::RateLimited { detail: rate_limit_detail(&body) },
        _ => ImageGenError::Api { status, body },
    }
}

/// 429 本文から人が読む 1 行を絞る (純粋)。Google は `Quota exceeded for metric: ...` の行で
/// free_tier / 分あたり等を名指しするので、あればその行 (先頭 1 本)。無ければ本文の先頭 200 字。
fn rate_limit_detail(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(msg) = v.pointer("/error/message").and_then(Value::as_str) {
            if let Some(line) = msg.lines().find(|l| l.contains("Quota exceeded")) {
                return line.trim_start_matches('*').trim().to_string();
            }
            return msg.chars().take(200).collect();
        }
    }
    let head: String = body.chars().take(200).collect();
    if head.trim().is_empty() {
        "少し待って押し直してください".to_string()
    } else {
        head
    }
}

// --- base64 (依存を増やさない最小実装: 標準 alphabet・パディングあり) -------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// 表示用の data URL。
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64_encode(bytes))
}

// --- OpenAI Images (契約 `openai`) -------------------------------------------------------

pub fn encode_openai(cfg: &ImageGenConfig, prompt: &str) -> Value {
    json!({
        "model": cfg.model_or_default(),
        "prompt": prompt,
        "size": SizeMap::openai_size(cfg.shape),
        "quality": SizeMap::openai_quality(cfg.detail),
        "n": 1,
        "output_format": "png",
    })
}

pub fn openai_endpoint(cfg: &ImageGenConfig) -> String {
    let base = cfg.base();
    if base.ends_with("/v1") {
        format!("{base}/images/generations")
    } else {
        format!("{base}/v1/images/generations")
    }
}

pub fn openai_probe_endpoint(cfg: &ImageGenConfig) -> String {
    let base = cfg.base();
    if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    }
}

pub fn openai_edits_endpoint(cfg: &ImageGenConfig) -> String {
    let base = cfg.base();
    if base.ends_with("/v1") {
        format!("{base}/images/edits")
    } else {
        format!("{base}/v1/images/edits")
    }
}

/// multipart の 1 部品 (純粋データ。HTTP ドライバが `reqwest::multipart::Form` に写す)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPart {
    Text { key: String, value: String },
    File { key: String, file_name: String, mime: String, bytes: Vec<u8> },
}

/// 参照ありのときの `POST /v1/images/edits` 部品列 (spec 25 rev2 で凍結):
/// `image[]` ×N, `prompt`, `model`, `size`, `quality`, `n`。**`input_fidelity` は送らない**
/// (2026-08-21 実測で決着: 既定モデル `gpt-image-1-mini` は `high` を 400 で拒む・`low` は無指定と
/// usage 完全一致・`gpt-image-1` では画像入力トークンが 20.2× になる。specs/25「実測」節)。
/// キー名は OpenAI 公式 curl の `-F "image[]=@..."` に合わせる。
pub fn openai_edit_parts(cfg: &ImageGenConfig, prompt: &str, refs: &[RefImage]) -> Vec<FormPart> {
    let mut parts = Vec::new();
    for r in refs {
        parts.push(FormPart::File {
            key: "image[]".into(),
            file_name: r.name.clone(),
            mime: r.mime.clone(),
            bytes: r.bytes.clone(),
        });
    }
    parts.push(FormPart::Text { key: "prompt".into(), value: prompt.to_string() });
    parts.push(FormPart::Text { key: "model".into(), value: cfg.model_or_default().to_string() });
    parts.push(FormPart::Text { key: "size".into(), value: SizeMap::openai_size(cfg.shape).to_string() });
    parts.push(FormPart::Text { key: "quality".into(), value: SizeMap::openai_quality(cfg.detail).to_string() });
    parts.push(FormPart::Text { key: "n".into(), value: "1".into() });
    parts
}

fn form_from_parts(parts: Vec<FormPart>) -> Result<reqwest::multipart::Form, ImageGenError> {
    let mut form = reqwest::multipart::Form::new();
    for p in parts {
        form = match p {
            FormPart::Text { key, value } => form.text(key, value),
            FormPart::File { key, file_name, mime, bytes } => {
                let part = reqwest::multipart::Part::bytes(bytes)
                    .file_name(file_name)
                    .mime_str(&mime)
                    .map_err(|e| ImageGenError::Config(format!("mime: {e}")))?;
                form.part(key, part)
            }
        };
    }
    Ok(form)
}

/// `data[0].b64_json` → bytes。`url` だけの応答は Shape (v1 では取りに行かない)。
pub fn decode_openai(body: &str) -> Result<Vec<u8>, ImageGenError> {
    let v: Value = serde_json::from_str(body).map_err(|e| ImageGenError::Shape {
        detail: format!("JSON でない: {e}"),
        raw: body.to_string(),
    })?;
    let b64 = v
        .pointer("/data/0/b64_json")
        .and_then(Value::as_str)
        .ok_or_else(|| ImageGenError::Shape {
            detail: "data[0].b64_json が無い".into(),
            raw: body.to_string(),
        })?;
    base64_decode(b64).ok_or_else(|| ImageGenError::Shape {
        detail: "b64_json が base64 でない".into(),
        raw: String::new(),
    })
}

// --- Gemini 画像 (契約 `gemini`) ----------------------------------------------------------

/// `refs` は **テキストの前**に `inlineData` で並べる (spec 25)。0 枚なら spec 24 と同じ形。
pub fn encode_gemini(cfg: &ImageGenConfig, prompt: &str, refs: &[RefImage]) -> Value {
    let mut parts: Vec<Value> = refs
        .iter()
        .map(|r| json!({ "inlineData": { "mimeType": r.mime, "data": base64_encode(&r.bytes) } }))
        .collect();
    parts.push(json!({ "text": prompt }));
    json!({
        "contents": [{ "role": "user", "parts": parts }],
        "generationConfig": {
            "responseModalities": ["IMAGE", "TEXT"],
            "imageConfig": {
                "aspectRatio": SizeMap::gemini_aspect(cfg.shape),
                "imageSize": SizeMap::gemini_size(cfg.detail),
            }
        }
    })
}

/// `v1beta` 必須。base がホスト直でも `/v1beta` 込みでも受ける (llm_client の gemini と同じ)。
pub fn gemini_endpoint(cfg: &ImageGenConfig) -> String {
    let base = cfg.base();
    let model = cfg.model_or_default();
    if base.ends_with("/v1beta") {
        format!("{base}/models/{model}:generateContent")
    } else {
        format!("{base}/v1beta/models/{model}:generateContent")
    }
}

pub fn gemini_probe_endpoint(cfg: &ImageGenConfig) -> String {
    let base = cfg.base();
    let model = cfg.model_or_default();
    if base.ends_with("/v1beta") {
        format!("{base}/models/{model}")
    } else {
        format!("{base}/v1beta/models/{model}")
    }
}

/// `candidates[0].content.parts[].inlineData` → (mime, bytes)。**ブロックは 200 + 空** で来る
/// ので `promptFeedback.blockReason` / 候補の `finishReason` を理由として返す (#61 の写し)。
pub fn decode_gemini(body: &str) -> Result<(String, Vec<u8>), ImageGenError> {
    let v: Value = serde_json::from_str(body).map_err(|e| ImageGenError::Shape {
        detail: format!("JSON でない: {e}"),
        raw: body.to_string(),
    })?;
    if let Some(reason) = v.pointer("/promptFeedback/blockReason").and_then(Value::as_str) {
        return Err(ImageGenError::Blocked { reason: reason.to_string() });
    }
    let parts = v
        .pointer("/candidates/0/content/parts")
        .and_then(Value::as_array);
    if let Some(parts) = parts {
        for part in parts {
            if let Some(data) = part.pointer("/inlineData/data").and_then(Value::as_str) {
                let mime = part
                    .pointer("/inlineData/mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/png")
                    .to_string();
                let bytes = base64_decode(data).ok_or_else(|| ImageGenError::Shape {
                    detail: "inlineData.data が base64 でない".into(),
                    raw: String::new(),
                })?;
                return Ok((mime, bytes));
            }
        }
    }
    // 画像が無い: 候補段の finishReason を理由に (SAFETY / RECITATION / PROHIBITED_CONTENT …)。
    if let Some(reason) = v.pointer("/candidates/0/finishReason").and_then(Value::as_str) {
        if reason != "STOP" {
            return Err(ImageGenError::Blocked { reason: reason.to_string() });
        }
    }
    Err(ImageGenError::Shape {
        detail: "inlineData が無い (テキストだけ返った可能性)".into(),
        raw: body.to_string(),
    })
}

// --- ComfyUI (契約 `comfy`) ---------------------------------------------------------------

pub struct ComfyVars<'a> {
    pub prompt: &'a str,
    pub negative: &'a str,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    /// `/upload/image` で上げた参照の返名 (spec 25)。`%ref_1%`.. を**ある分だけ**置換する。
    /// 足りない分は空文字で埋めない (LoadImage の File not found) — 置換後に
    /// [`comfy_prune_unfilled_refs`] が**ノードごと外す** (2026-08-22 改訂)。
    pub refs: &'a [String],
}

/// プレースホルダ置換。**`serde_json::Value` を歩いて型を保つ**: 文字列値が**ちょうど**
/// `%width%` / `%height%` / `%seed%` なら数値 JSON に、`%prompt%` / `%negative%` は文字列内の
/// 部分一致も含めて文字列のまま差し替える。未置換は呼び出し側が既定値を渡すので残らない
/// (残すと ComfyUI が invalid literal で落ちる)。
pub fn comfy_substitute(workflow: &Value, vars: &ComfyVars<'_>) -> Value {
    match workflow {
        Value::String(s) => match s.as_str() {
            "%width%" => Value::from(vars.width),
            "%height%" => Value::from(vars.height),
            "%seed%" => Value::from(vars.seed),
            _ => {
                let mut t = s.clone();
                if t.contains("%prompt%") {
                    t = t.replace("%prompt%", vars.prompt);
                }
                if t.contains("%negative%") {
                    t = t.replace("%negative%", vars.negative);
                }
                // 文字列欄に数値プレースホルダが部分一致で混じる形 (例 "%width%x%height%") も
                // 文字列として埋める。
                if t.contains("%width%") {
                    t = t.replace("%width%", &vars.width.to_string());
                }
                if t.contains("%height%") {
                    t = t.replace("%height%", &vars.height.to_string());
                }
                if t.contains("%seed%") {
                    t = t.replace("%seed%", &vars.seed.to_string());
                }
                for (i, name) in vars.refs.iter().enumerate() {
                    let ph = format!("%ref_{}%", i + 1);
                    if t.contains(&ph) {
                        t = t.replace(&ph, name);
                    }
                }
                Value::String(t)
            }
        },
        Value::Array(a) => Value::Array(a.iter().map(|v| comfy_substitute(v, vars)).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), comfy_substitute(v, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 未置換の `%ref_n%` を持つノードを**ノードごと外し**、そこへ繋がる辺 (`[id, slot]`) を他ノードの
/// `inputs` から切る (純粋、spec 25 改訂 2026-08-22)。置換で埋まらなかった LoadImage を残すと
/// ComfyUI が File not found で落ちるので、**1 本のワークフローで画集 0〜3 枚を受ける**ための剪定。
/// 辺を切った先の入力は optional であること (`TextEncodeQwenImageEditPlus.image1..3` のように) —
/// required なら ComfyUI が `node_errors` で名指しし `ComfyNodeError` として surface する。
/// `%ref_%` を持たないワークフローは byte 不変。
pub fn comfy_prune_unfilled_refs(workflow: &Value) -> Value {
    fn has_unfilled_ref(v: &Value) -> bool {
        match v {
            Value::String(s) => {
                let mut rest = s.as_str();
                while let Some(i) = rest.find("%ref_") {
                    let tail = &rest[i + 5..];
                    let digits: usize = tail.bytes().take_while(u8::is_ascii_digit).count();
                    if digits > 0 && tail[digits..].starts_with('%') {
                        return true;
                    }
                    rest = &rest[i + 5..];
                }
                false
            }
            Value::Array(a) => a.iter().any(has_unfilled_ref),
            Value::Object(m) => m.values().any(has_unfilled_ref),
            _ => false,
        }
    }
    let Some(nodes) = workflow.as_object() else {
        return workflow.clone();
    };
    let dead: std::collections::BTreeSet<&str> = nodes
        .iter()
        .filter(|(_, n)| n.get("inputs").is_some_and(has_unfilled_ref))
        .map(|(id, _)| id.as_str())
        .collect();
    if dead.is_empty() {
        return workflow.clone();
    }
    let is_edge_to_dead = |v: &Value| -> bool {
        v.as_array()
            .and_then(|a| a.first())
            .and_then(|id| match id {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .is_some_and(|id| dead.contains(id.as_str()))
    };
    let mut out = serde_json::Map::new();
    for (id, node) in nodes {
        if dead.contains(id.as_str()) {
            continue;
        }
        let mut node = node.clone();
        if let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) {
            inputs.retain(|_, v| !is_edge_to_dead(v));
        }
        out.insert(id.clone(), node);
    }
    Value::Object(out)
}

/// ワークフロー JSON の形を検査する (純粋)。**UI 形式** (`nodes`/`links` 配列 = ComfyUI の通常保存)
/// を貼ると `/prompt` は `prompt_no_outputs` としか言わないので、先回りして「API 形式で書き出せ」と
/// 言う (2026-08-20 ユーザー実機で観測)。API 形式 = `{"<id>": {"class_type": ..., "inputs": {...}}, ...}`。
pub fn check_comfy_workflow_shape(wf: &Value) -> Result<(), ImageGenError> {
    let Some(obj) = wf.as_object() else {
        return Err(ImageGenError::Config("ワークフロー JSON はオブジェクトでなければなりません".into()));
    };
    let looks_ui = obj.get("nodes").is_some_and(Value::is_array)
        && (obj.contains_key("links") || obj.contains_key("version") || obj.contains_key("last_node_id"));
    if looks_ui {
        return Err(ImageGenError::Config(
            "UI 形式のワークフロー (nodes/links) が貼られています。ComfyUI で「Save (API Format)」\
             (設定で Dev mode を有効化) から書き出した API 形式の JSON を貼ってください"
                .into(),
        ));
    }
    let has_node = obj.values().any(|v| v.get("class_type").is_some());
    if !has_node {
        return Err(ImageGenError::Config(
            "API 形式のワークフローに見えません (class_type を持つノードが 1 つも無い)".into(),
        ));
    }
    Ok(())
}

/// `POST /prompt` のボディ。
pub fn comfy_prompt_body(workflow: Value, client_id: &str) -> Value {
    json!({ "prompt": workflow, "client_id": client_id })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComfyImageRef {
    pub filename: String,
    pub subfolder: String,
    pub kind: String,
}

/// `/history/{id}` の応答から最初の画像参照を拾う。**未完了 (空 / id 無し) は `Ok(None)`** =
/// ポーリング継続。`status.status_str == "error"` や `node_errors` は `ComfyNodeError`。
pub fn comfy_first_image(history: &Value, prompt_id: &str) -> Result<Option<ComfyImageRef>, ImageGenError> {
    let entry = match history.get(prompt_id) {
        Some(e) => e,
        None => return Ok(None),
    };
    // エラーの抽出。
    if let Some(status) = entry.get("status") {
        let failed = status.get("status_str").and_then(Value::as_str) == Some("error");
        if failed {
            let (node, msg) = status
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|msgs| {
                    msgs.iter().find_map(|m| {
                        let arr = m.as_array()?;
                        if arr.first()?.as_str()? != "execution_error" {
                            return None;
                        }
                        let d = arr.get(1)?;
                        Some((
                            d.get("node_id").map(|n| n.to_string()).unwrap_or_default(),
                            d.get("exception_message")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        ))
                    })
                })
                .unwrap_or_else(|| ("?".into(), "execution error".into()));
            return Err(ImageGenError::ComfyNodeError { node, msg });
        }
    }
    if let Some(errs) = entry.get("node_errors").and_then(Value::as_object) {
        if let Some((node, e)) = errs.iter().next() {
            let msg = e
                .pointer("/errors/0/message")
                .and_then(Value::as_str)
                .unwrap_or("node error")
                .to_string();
            return Err(ImageGenError::ComfyNodeError { node: node.clone(), msg });
        }
    }
    let outputs = match entry.get("outputs").and_then(Value::as_object) {
        Some(o) => o,
        None => return Ok(None),
    };
    for (_node, out) in outputs {
        if let Some(images) = out.get("images").and_then(Value::as_array) {
            for img in images {
                if let Some(filename) = img.get("filename").and_then(Value::as_str) {
                    return Ok(Some(ComfyImageRef {
                        filename: filename.to_string(),
                        subfolder: img.get("subfolder").and_then(Value::as_str).unwrap_or("").to_string(),
                        kind: img.get("type").and_then(Value::as_str).unwrap_or("output").to_string(),
                    }));
                }
            }
        }
    }
    Ok(None)
}

pub fn comfy_view_url(base: &str, r: &ComfyImageRef) -> String {
    fn enc(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
    format!(
        "{}/view?filename={}&subfolder={}&type={}",
        base.trim_end_matches('/'),
        enc(&r.filename),
        enc(&r.subfolder),
        enc(&r.kind)
    )
}

/// `/upload/image` の応答 `{name, subfolder, type}` から LoadImage に差す名前 (subfolder があれば
/// `subfolder/name`)。
pub fn comfy_uploaded_name(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let name = v.get("name")?.as_str()?.to_string();
    match v.get("subfolder").and_then(Value::as_str) {
        Some(sub) if !sub.is_empty() => Some(format!("{sub}/{name}")),
        _ => Some(name),
    }
}

// --- 保存名 (契約 `storage`) ------------------------------------------------------------

/// `{stamp}_{slug}_T{turn}.png`。slug = フォルダ名を ASCII 英数と `-_` だけに落とし、空なら
/// タイトルの FNV-1a 8 桁 hex。パス要素を含まない (PoC で固定)。
pub fn image_file_name(stamp: &str, package_dir_name: &str, title: &str, turn: u32) -> String {
    let mut slug: String = package_dir_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(40)
        .collect();
    if slug.is_empty() {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in title.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        slug = format!("{:08x}", (h & 0xffff_ffff) as u32);
    }
    let stamp: String = stamp.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    format!("{stamp}_{slug}_T{turn}.png")
}

// --- HTTP ドライバ ------------------------------------------------------------------------

pub struct Generated {
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// 1 枚生成する。`api_key` は openai/gemini で必須 (comfy は無視)。
pub async fn generate(
    cfg: &ImageGenConfig,
    api_key: &str,
    prompt: &str,
    seed: u64,
    refs: &[RefImage],
) -> Result<Generated, ImageGenError> {
    let refs = &refs[..refs.len().min(max_refs(cfg.provider))];
    let http = reqwest::Client::builder()
        .timeout(cfg.timeout())
        .build()
        .map_err(ImageGenError::from)?;
    match cfg.provider {
        Provider::Openai => {
            if api_key.trim().is_empty() {
                return Err(ImageGenError::Config("OpenAI の API キーが未設定です".into()));
            }
            // 参照ありのときだけ /images/edits (multipart)。無ければ従来の /generations (JSON)。
            let resp = if refs.is_empty() {
                let body = encode_openai(cfg, prompt);
                http.post(openai_endpoint(cfg)).bearer_auth(api_key).json(&body).send().await
            } else {
                let form = form_from_parts(openai_edit_parts(cfg, prompt, refs))?;
                http.post(openai_edits_endpoint(cfg)).bearer_auth(api_key).multipart(form).send().await
            }
            .map_err(|e| timeout_or(e, cfg.provider))?;
            let status = resp.status().as_u16();
            let text = resp.text().await.map_err(ImageGenError::from)?;
            if !(200..300).contains(&status) {
                return Err(classify_status(status, text));
            }
            let bytes = decode_openai(&text)?;
            Ok(Generated { mime: "image/png".into(), bytes })
        }
        Provider::Gemini => {
            if api_key.trim().is_empty() {
                return Err(ImageGenError::Config("Gemini の API キーが未設定です".into()));
            }
            let body = encode_gemini(cfg, prompt, refs);
            let resp = http
                .post(gemini_endpoint(cfg))
                .header("x-goog-api-key", api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| timeout_or(e, cfg.provider))?;
            let status = resp.status().as_u16();
            let text = resp.text().await.map_err(ImageGenError::from)?;
            if !(200..300).contains(&status) {
                return Err(classify_status(status, text));
            }
            let (mime, bytes) = decode_gemini(&text)?;
            Ok(Generated { mime, bytes })
        }
        Provider::Comfy => {
            let wf_text = cfg
                .workflow_json
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| ImageGenError::Config("ComfyUI のワークフロー JSON が未設定です".into()))?;
            let wf: Value = serde_json::from_str(wf_text)
                .map_err(|e| ImageGenError::Config(format!("ワークフロー JSON を読めません: {e}")))?;
            check_comfy_workflow_shape(&wf)?;
            let (w, h) = SizeMap::comfy_dims(cfg.shape);
            let base = cfg.base().to_string();
            // 参照を先に /upload/image へ (spec 25)。返名を %ref_n% に差す。
            let mut ref_names: Vec<String> = Vec::new();
            for r in refs {
                let part = reqwest::multipart::Part::bytes(r.bytes.clone())
                    .file_name(r.name.clone())
                    .mime_str(&r.mime)
                    .map_err(|e| ImageGenError::Config(format!("mime: {e}")))?;
                let form = reqwest::multipart::Form::new()
                    .part("image", part)
                    .text("overwrite", "true");
                let resp = http
                    .post(format!("{base}/upload/image"))
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| timeout_or(e, cfg.provider))?;
                let status = resp.status().as_u16();
                let text = resp.text().await.map_err(ImageGenError::from)?;
                if !(200..300).contains(&status) {
                    return Err(classify_status(status, text));
                }
                let name = comfy_uploaded_name(&text).ok_or_else(|| ImageGenError::Shape {
                    detail: "/upload/image の応答に name が無い".into(),
                    raw: text.clone(),
                })?;
                ref_names.push(name);
            }
            let vars = ComfyVars { prompt, negative: &cfg.negative, seed, width: w, height: h, refs: &ref_names };
            let substituted = comfy_prune_unfilled_refs(&comfy_substitute(&wf, &vars));
            let client_id = format!("kataribe-{}", seed);
            let resp = http
                .post(format!("{base}/prompt"))
                .json(&comfy_prompt_body(substituted, &client_id))
                .send()
                .await
                .map_err(|e| timeout_or(e, cfg.provider))?;
            let status = resp.status().as_u16();
            let text = resp.text().await.map_err(ImageGenError::from)?;
            if !(200..300).contains(&status) {
                return Err(classify_status(status, text));
            }
            let v: Value = serde_json::from_str(&text).map_err(|e| ImageGenError::Shape {
                detail: format!("/prompt の応答が JSON でない: {e}"),
                raw: text.clone(),
            })?;
            let prompt_id = v
                .get("prompt_id")
                .and_then(Value::as_str)
                .ok_or_else(|| ImageGenError::Shape { detail: "prompt_id が無い".into(), raw: text.clone() })?
                .to_string();
            // 1 秒間隔ポーリング。発行直後の 404/空は正常。
            let deadline = std::time::Instant::now() + cfg.timeout();
            let image_ref = loop {
                if std::time::Instant::now() > deadline {
                    return Err(ImageGenError::Timeout { provider: Provider::Comfy });
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                let r = http
                    .get(format!("{base}/history/{prompt_id}"))
                    .send()
                    .await
                    .map_err(|e| timeout_or(e, cfg.provider))?;
                if !r.status().is_success() {
                    continue;
                }
                let hist: Value = match r.json().await {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(img) = comfy_first_image(&hist, &prompt_id)? {
                    break img;
                }
            };
            let r = http
                .get(comfy_view_url(&base, &image_ref))
                .send()
                .await
                .map_err(|e| timeout_or(e, cfg.provider))?;
            let status = r.status().as_u16();
            if !(200..300).contains(&status) {
                return Err(classify_status(status, String::new()));
            }
            let bytes = r.bytes().await.map_err(ImageGenError::from)?.to_vec();
            Ok(Generated { mime: "image/png".into(), bytes })
        }
    }
}

fn timeout_or(e: reqwest::Error, provider: Provider) -> ImageGenError {
    if e.is_timeout() {
        ImageGenError::Timeout { provider }
    } else {
        ImageGenError::from(e)
    }
}

/// 接続テスト (契約 `commands.image_gen_probe`)。画像は作らない。
pub async fn probe(cfg: &ImageGenConfig, api_key: &str) -> Result<String, ImageGenError> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(ImageGenError::from)?;
    let resp = match cfg.provider {
        Provider::Openai => http.get(openai_probe_endpoint(cfg)).bearer_auth(api_key).send().await,
        Provider::Gemini => http
            .get(gemini_probe_endpoint(cfg))
            .header("x-goog-api-key", api_key)
            .send()
            .await,
        Provider::Comfy => http.get(format!("{}/system_stats", cfg.base())).send().await,
    }
    .map_err(|e| timeout_or(e, cfg.provider))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return Ok("接続できました".into());
    }
    if status == 404 && cfg.provider == Provider::Gemini {
        return Err(ImageGenError::Config(format!(
            "モデル '{}' が見つかりません (404)",
            cfg.model_or_default()
        )));
    }
    let body = resp.text().await.unwrap_or_default();
    Err(classify_status(status, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(provider: Provider) -> ImageGenConfig {
        ImageGenConfig {
            provider,
            base_url: match provider {
                Provider::Openai => "https://api.openai.com/v1/".into(),
                Provider::Gemini => "https://generativelanguage.googleapis.com".into(),
                Provider::Comfy => "http://127.0.0.1:8188/".into(),
            },
            model: String::new(),
            shape: Shape::Landscape,
            detail: Detail::Standard,
            style: None,
            user_prefix: String::new(),
            negative: "lowres".into(),
            workflow_json: None,
            timeout_secs: None,
            lock_seed: false,
            seed: 0,
        }
    }

    /// 【写像表と既定】UI の 1 軸がプロバイダ語彙に写る (契約 size_preset)。quality の既定は low
    /// (16 倍のコスト差)。様式の既定はプロバイダに倒す。endpoint は /v1 の有無を吸収する。
    #[test]
    fn size_preset_maps_per_provider_and_defaults_follow_provider() {
        let o = cfg(Provider::Openai);
        let body = encode_openai(&o, "a cat");
        assert_eq!(body["size"], "1536x1024");
        assert_eq!(body["quality"], "low");
        assert_eq!(body["model"], "gpt-image-1-mini");
        assert_eq!(body["n"], 1);
        assert!(body.get("negative_prompt").is_none(), "OpenAI に欄は無い");
        assert_eq!(openai_endpoint(&o), "https://api.openai.com/v1/images/generations");
        assert_eq!(o.style(), PromptStyle::Prose);

        let g = cfg(Provider::Gemini);
        let body = encode_gemini(&g, "a cat", &[]);
        assert_eq!(body["generationConfig"]["imageConfig"]["aspectRatio"], "16:9");
        assert_eq!(body["generationConfig"]["imageConfig"]["imageSize"], "1K");
        assert_eq!(body["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(
            gemini_endpoint(&g),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite-image:generateContent"
        );
        assert_eq!(g.style(), PromptStyle::Prose);

        let c = cfg(Provider::Comfy);
        assert_eq!(SizeMap::comfy_dims(c.shape), (1344, 768));
        assert_eq!(c.style(), PromptStyle::Tags);
        assert_eq!(c.timeout(), Duration::from_secs(600), "comfy は長い");
        assert_eq!(o.timeout(), Duration::from_secs(120));
    }

    /// 【OpenAI decode】data[0].b64_json を bytes に / 無ければ raw を保持した Shape。
    #[test]
    fn openai_decode_reads_b64_json_and_keeps_raw_on_shape_mismatch() {
        let png = [0x89u8, b'P', b'N', b'G', 0, 1, 2];
        let body = format!(r#"{{"created":1,"data":[{{"b64_json":"{}"}}]}}"#, base64_encode(&png));
        assert_eq!(decode_openai(&body).unwrap(), png);
        let err = decode_openai(r#"{"data":[{"url":"https://x/y.png"}]}"#).unwrap_err();
        assert!(matches!(err, ImageGenError::Shape { raw, .. } if raw.contains("url")));
        assert!(matches!(classify_status(401, String::new()), ImageGenError::Unauthorized));
        assert!(matches!(classify_status(429, String::new()), ImageGenError::RateLimited { .. }));
        // Google の 429 はどのクォータかを本文で名指しする — free_tier limit: 0 (= キーの
        // プロジェクトが無課金) をトーストまで運ぶ (2026-08-20 実測の観測穴)。
        let google = r#"{"error":{"code":429,"message":"You exceeded your current quota.
* Quota exceeded for metric: generativelanguage.googleapis.com/generate_content_free_tier_requests, limit: 0, model: gemini-2.5-flash-preview-image
Please retry in 13s.","status":"RESOURCE_EXHAUSTED"}}"#;
        assert!(matches!(classify_status(429, google.into()),
            ImageGenError::RateLimited { detail } if detail.contains("free_tier") && detail.contains("limit: 0")));
    }

    /// 【Gemini decode】inlineData を拾う / **200+空のブロック**は理由つき Blocked
    /// (promptFeedback.blockReason と候補 finishReason の二段) / テキストだけは Shape。
    #[test]
    fn gemini_decode_reads_inline_data_and_surfaces_block_reason() {
        let png = [1u8, 2, 3, 4];
        let body = format!(
            r#"{{"candidates":[{{"content":{{"parts":[{{"text":"here"}},{{"inlineData":{{"mimeType":"image/png","data":"{}"}}}}]}},"finishReason":"STOP"}}]}}"#,
            base64_encode(&png)
        );
        let (mime, bytes) = decode_gemini(&body).unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, png);

        let blocked = r#"{"promptFeedback":{"blockReason":"PROHIBITED_CONTENT"}}"#;
        assert!(matches!(decode_gemini(blocked).unwrap_err(), ImageGenError::Blocked { reason } if reason == "PROHIBITED_CONTENT"));
        let safety = r#"{"candidates":[{"finishReason":"SAFETY","safetyRatings":[]}]}"#;
        assert!(matches!(decode_gemini(safety).unwrap_err(), ImageGenError::Blocked { reason } if reason == "SAFETY"));
        let text_only = r#"{"candidates":[{"content":{"parts":[{"text":"no image"}]},"finishReason":"STOP"}]}"#;
        assert!(matches!(decode_gemini(text_only).unwrap_err(), ImageGenError::Shape { .. }));
    }

    /// 【ComfyUI 置換は型を保つ】"%width%" 単独の文字列値は**数値 JSON** に、%prompt% は文字列
    /// (部分一致・エスケープ込み) に、"%width%x%height%" のような文字列内は文字列のまま埋まる。
    /// 未知のプレースホルダは無いので残らない。
    #[test]
    fn comfy_substitute_keeps_json_types_and_fills_every_placeholder() {
        let wf: Value = serde_json::from_str(
            r#"{
              "3": {"class_type":"KSampler","inputs":{"seed":"%seed%","steps":20,"latent_image":["5",0]}},
              "5": {"class_type":"EmptyLatentImage","inputs":{"width":"%width%","height":"%height%","batch_size":1}},
              "6": {"class_type":"CLIPTextEncode","inputs":{"text":"masterpiece, %prompt%"}},
              "7": {"class_type":"CLIPTextEncode","inputs":{"text":"%negative%"}},
              "9": {"class_type":"SaveImage","inputs":{"filename_prefix":"kataribe_%width%x%height%"}}
            }"#,
        )
        .unwrap();
        let vars = ComfyVars { prompt: "a \"quoted\" cat", negative: "lowres", seed: 42, width: 1344, height: 768, refs: &[] };
        let out = comfy_substitute(&wf, &vars);
        assert_eq!(out["5"]["inputs"]["width"], json!(1344), "数値 JSON (クォート無し)");
        assert_eq!(out["5"]["inputs"]["height"], json!(768));
        assert_eq!(out["3"]["inputs"]["seed"], json!(42));
        assert_eq!(out["3"]["inputs"]["steps"], json!(20), "触らない欄は不変");
        assert_eq!(out["6"]["inputs"]["text"], "masterpiece, a \"quoted\" cat");
        assert_eq!(out["7"]["inputs"]["text"], "lowres");
        assert_eq!(out["9"]["inputs"]["filename_prefix"], "kataribe_1344x768");
        let dumped = serde_json::to_string(&out).unwrap();
        assert!(!dumped.contains('%'), "プレースホルダが残らない: {dumped}");
        let body = comfy_prompt_body(out, "cid");
        assert_eq!(body["client_id"], "cid");
        assert!(body["prompt"]["3"].is_object());
    }

    /// 【ComfyUI history】発行直後の空/別 id は Ok(None)=継続、完了で最初の画像参照、
    /// status error / node_errors は ComfyNodeError。view URL はエンコード済み。
    #[test]
    fn comfy_history_polls_until_image_and_surfaces_node_errors() {
        assert_eq!(comfy_first_image(&json!({}), "p1").unwrap(), None);
        assert_eq!(comfy_first_image(&json!({"other": {}}), "p1").unwrap(), None);
        let pending = json!({"p1": {"status": {"status_str": "running", "completed": false}, "outputs": {}}});
        assert_eq!(comfy_first_image(&pending, "p1").unwrap(), None);
        let done = json!({"p1": {"status": {"status_str":"success","completed":true},
            "outputs": {"9": {"images": [{"filename":"kataribe_00001_.png","subfolder":"sub dir","type":"output"}]}}}});
        let r = comfy_first_image(&done, "p1").unwrap().unwrap();
        assert_eq!(r.filename, "kataribe_00001_.png");
        assert_eq!(
            comfy_view_url("http://127.0.0.1:8188/", &r),
            "http://127.0.0.1:8188/view?filename=kataribe_00001_.png&subfolder=sub%20dir&type=output"
        );
        let failed = json!({"p1": {"status": {"status_str":"error","completed":false,
            "messages":[["execution_start",{}],["execution_error",{"node_id":"3","exception_message":"CUDA out of memory"}]]}}});
        assert!(matches!(comfy_first_image(&failed, "p1").unwrap_err(),
            ImageGenError::ComfyNodeError { node, msg } if node.contains('3') && msg.contains("CUDA")));
        let node_err = json!({"p1": {"node_errors": {"6": {"errors": [{"message":"Required input is missing"}]}}}});
        assert!(matches!(comfy_first_image(&node_err, "p1").unwrap_err(),
            ImageGenError::ComfyNodeError { node, .. } if node == "6"));
    }

    /// 【保存名】パス要素を含まない / 日本語フォルダ名は空 slug → タイトルの FNV 8 桁 / turn 付き。
    #[test]
    fn image_file_name_is_path_safe_and_falls_back_to_fnv_for_non_ascii() {
        let n = image_file_name("20260820_123456", "lakeside_manor", "湖畔の洋館", 12);
        assert_eq!(n, "20260820_123456_lakeside_manor_T12.png");
        let j = image_file_name("20260820_123456", "湖畔の洋館", "湖畔の洋館", 3);
        assert!(j.starts_with("20260820_123456_") && j.ends_with("_T3.png"), "{j}");
        let slug = &j["20260820_123456_".len()..j.len() - "_T3.png".len()];
        assert_eq!(slug.len(), 8, "FNV 8 桁 hex: {slug}");
        assert!(slug.chars().all(|c| c.is_ascii_hexdigit()));
        let evil = image_file_name("../x", "..\\..", "t", 1);
        assert!(!evil.contains('/') && !evil.contains('\\') && !evil.contains(".."), "{evil}");
    }

    /// 【設定画集 (spec 25)】Gemini は inlineData をテキストの前に並べ 0 枚なら spec 24 と同じ形 /
    /// OpenAI は参照ありで edits の部品列 (image[] ×N, prompt, model, size, quality, n、input_fidelity 無し)
    /// で 0 枚なら従来 JSON / ComfyUI は %ref_n% をある分だけ差す (足りない分の剪定は次のテスト)。
    #[test]
    fn reference_sheets_are_carried_per_provider_and_absent_means_byte_identical() {
        let r = |n: &str| RefImage { name: n.into(), mime: "image/png".into(), bytes: vec![1, 2, 3] };
        let refs = vec![r("01_cast.png"), r("02_bg.png")];

        let g = cfg(Provider::Gemini);
        let with = encode_gemini(&g, "a cat", &refs);
        let parts = with["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0]["inlineData"]["mimeType"], "image/png");
        assert_eq!(parts[0]["inlineData"]["data"], base64_encode(&[1, 2, 3]));
        assert_eq!(parts[2]["text"], "a cat");
        assert_eq!(encode_gemini(&g, "a cat", &[]), encode_gemini(&g, "a cat", &Vec::new()));
        assert_eq!(encode_gemini(&g, "a cat", &[])["contents"][0]["parts"].as_array().unwrap().len(), 1);

        let o = cfg(Provider::Openai);
        let parts = openai_edit_parts(&o, "a cat", &refs);
        let keys: Vec<&str> = parts
            .iter()
            .map(|p| match p {
                FormPart::Text { key, .. } | FormPart::File { key, .. } => key.as_str(),
            })
            .collect();
        assert_eq!(keys, vec!["image[]", "image[]", "prompt", "model", "size", "quality", "n"]);
        assert!(!keys.contains(&"input_fidelity"), "送らない (既定モデルが 400 で拒む・specs/25 実測)");
        assert!(matches!(&parts[3], FormPart::Text { value, .. } if value == "gpt-image-1-mini"));
        assert!(matches!(&parts[5], FormPart::Text { value, .. } if value == "low"));
        assert_eq!(openai_edits_endpoint(&o), "https://api.openai.com/v1/images/edits");
        assert_eq!(max_refs(Provider::Gemini), 3);

        let wf: Value = serde_json::from_str(
            r#"{"10":{"class_type":"LoadImage","inputs":{"image":"%ref_1%"}},
                "11":{"class_type":"LoadImage","inputs":{"image":"%ref_2%"}},
                "6":{"class_type":"CLIPTextEncode","inputs":{"text":"%prompt%"}}}"#,
        )
        .unwrap();
        let names = vec!["sheet_a.png".to_string()];
        let vars = ComfyVars { prompt: "p", negative: "", seed: 1, width: 1024, height: 1024, refs: &names };
        let out = comfy_substitute(&wf, &vars);
        assert_eq!(out["10"]["inputs"]["image"], "sheet_a.png");
        assert_eq!(out["11"]["inputs"]["image"], "%ref_2%", "置換だけでは足りない分は残る (外すのは prune の仕事)");
        assert_eq!(comfy_uploaded_name(r#"{"name":"x.png","subfolder":"","type":"input"}"#).as_deref(), Some("x.png"));
        assert_eq!(comfy_uploaded_name(r#"{"name":"x.png","subfolder":"refs","type":"input"}"#).as_deref(), Some("refs/x.png"));
    }

    /// 【未置換 %ref_n% の剪定 (spec 25 改訂 2026-08-22)】画集が足りない分の LoadImage は**ノードごと外し**、
    /// そこへ繋がる辺 (`[id, slot]`) を他ノードの inputs から切る — 残すと LoadImage が File not found で落ちる
    /// (旧契約「残す」はワークフロー側で枚数を揃えろという意味だったが、画集 1 枚の盤面と 3 枚の盤面で
    /// ワークフローを貼り替えるのは誰もしない = 実機で参照が死んでいた真因)。1 本のワークフローで 0〜3 枚を受ける。
    #[test]
    fn unfilled_ref_placeholders_are_pruned_with_their_edges() {
        let wf: Value = serde_json::from_str(
            r#"{"10":{"class_type":"LoadImage","inputs":{"image":"%ref_1%"}},
                "11":{"class_type":"LoadImage","inputs":{"image":"%ref_2%"}},
                "12":{"class_type":"LoadImage","inputs":{"image":"%ref_3%"}},
                "80":{"class_type":"TextEncodeQwenImageEditPlus","inputs":{"clip":["62",0],"vae":["63",0],"prompt":"%prompt%","image1":["10",0],"image2":["11",0],"image3":["12",0]}},
                "62":{"class_type":"CLIPLoader","inputs":{"clip_name":"x","type":"krea2"}},
                "63":{"class_type":"VAELoader","inputs":{"vae_name":"v"}}}"#,
        )
        .unwrap();
        let one = vec!["sheet_a.png".to_string()];
        let vars = ComfyVars { prompt: "p", negative: "", seed: 1, width: 1024, height: 1024, refs: &one };
        let out = comfy_prune_unfilled_refs(&comfy_substitute(&wf, &vars));
        assert_eq!(out["10"]["inputs"]["image"], "sheet_a.png");
        assert!(out.get("11").is_none() && out.get("12").is_none(), "足りない分はノードごと外す");
        let enc = out["80"]["inputs"].as_object().unwrap();
        assert_eq!(enc["image1"], json!(["10", 0]));
        assert!(!enc.contains_key("image2") && !enc.contains_key("image3"), "外したノードへの辺も切る");
        assert_eq!(enc["clip"], json!(["62", 0]), "無関係の辺は触らない");
        assert_eq!(enc["prompt"], "p");
        assert!(out.get("62").is_some() && out.get("63").is_some());

        // 0 枚: LoadImage は全部消え、エンコーダは画像なしのテキスト符号化として残る。
        let none: Vec<String> = vec![];
        let vars0 = ComfyVars { prompt: "p", negative: "", seed: 1, width: 1024, height: 1024, refs: &none };
        let out0 = comfy_prune_unfilled_refs(&comfy_substitute(&wf, &vars0));
        assert!(out0.get("10").is_none() && out0.get("11").is_none() && out0.get("12").is_none());
        let enc0 = out0["80"]["inputs"].as_object().unwrap();
        assert!(!enc0.keys().any(|k| k.starts_with("image")));

        // %ref_% を持たないワークフローは byte 不変 (spec 24 の経路に影響しない)。
        let plain: Value = serde_json::from_str(r#"{"6":{"class_type":"CLIPTextEncode","inputs":{"text":"p","clip":["4",1]}}}"#).unwrap();
        assert_eq!(comfy_prune_unfilled_refs(&plain), plain);
    }

    /// 【ComfyUI ワークフローの形】UI 形式 (nodes/links) は API 形式で書き出せと名指し / ノードの無い
    /// オブジェクトも拒否 / API 形式は通る (実機の prompt_no_outputs を先回りする)。
    #[test]
    fn comfy_workflow_shape_rejects_ui_format_with_guidance() {
        let ui = json!({"last_node_id": 9, "nodes": [{"id": 1, "type": "KSampler"}], "links": [], "version": 0.4});
        let err = check_comfy_workflow_shape(&ui).unwrap_err();
        assert!(matches!(&err, ImageGenError::Config(m) if m.contains("API Format")), "{err}");
        assert!(matches!(check_comfy_workflow_shape(&json!({"foo": 1})).unwrap_err(), ImageGenError::Config(_)));
        assert!(matches!(check_comfy_workflow_shape(&json!([])).unwrap_err(), ImageGenError::Config(_)));
        let api = json!({"3": {"class_type": "KSampler", "inputs": {}}, "9": {"class_type": "SaveImage", "inputs": {}}});
        assert!(check_comfy_workflow_shape(&api).is_ok());
    }

    /// 【base64】往復と URL-safe の受理。
    #[test]
    fn base64_round_trips() {
        for n in 0..8 {
            let v: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&v)).unwrap(), v);
        }
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_decode("aGk").unwrap(), b"hi");
        assert!(data_url("image/png", b"x").starts_with("data:image/png;base64,"));
    }
}

/// live 実測 (spec 24 Phase D)。実キーが要るので ignore。
/// `OPENAI_API_KEY` / `GEMINI_API_KEY` (または `IMAGE_API_KEY_*`) を読み、各 1 枚を最低コスト設定で
/// 生成して `LOREKEEL_IMAGE_OUT` のフォルダへ書く (目視用)。
///
/// ```text
/// cargo test image_gen::live -- --ignored --nocapture
/// ```
#[cfg(test)]
mod live {
    use super::*;

    fn key(names: &[&str]) -> Option<String> {
        names.iter().find_map(|n| std::env::var(n).ok().filter(|v| !v.trim().is_empty()))
    }

    fn out_dir() -> std::path::PathBuf {
        std::env::var("LOREKEEL_IMAGE_OUT").map(Into::into).unwrap_or_else(|_| std::env::temp_dir())
    }

    async fn run(provider: Provider, key: &str) {
        let cfg = ImageGenConfig {
            provider,
            base_url: match provider {
                Provider::Openai => "https://api.openai.com/v1".into(),
                Provider::Gemini => "https://generativelanguage.googleapis.com".into(),
                Provider::Comfy => "http://127.0.0.1:8188".into(),
            },
            model: String::new(),
            shape: Shape::Landscape,
            detail: Detail::Standard,
            style: None,
            user_prefix: String::new(),
            negative: String::new(),
            workflow_json: None,
            timeout_secs: None,
            lock_seed: false,
            seed: 0,
        };
        let probe = probe(&cfg, key).await;
        eprintln!("{provider:?} probe: {:?}", probe.as_ref().map_err(|e| e.to_string()));
        let prompt = "A dusty entrance hall of an old lakeside mansion at dusk, a chandelier covered in dust,                       a man in his thirties in a worn coat holding a bag, soft warm light from a window,                       watercolor illustration, muted colors.";
        let t0 = std::time::Instant::now();
        match generate(&cfg, key, prompt, 42, &[]).await {
            Ok(g) => {
                let ext = if g.mime == "image/jpeg" { "jpg" } else { "png" };
                let path = out_dir().join(format!("kataribe_live_{provider:?}.{ext}"));
                std::fs::write(&path, &g.bytes).unwrap();
                eprintln!(
                    "{provider:?} OK: mime={} bytes={} elapsed={:.1}s -> {}",
                    g.mime,
                    g.bytes.len(),
                    t0.elapsed().as_secs_f32(),
                    path.display()
                );
                assert!(g.bytes.len() > 1000);
            }
            Err(e) => panic!("{provider:?} failed: {e}"),
        }
    }

    /// 設定画集つき (spec 25 Phase C)。`LOREKEEL_SHEET` に参照画像のパス。
    async fn run_with_sheet(provider: Provider, key: &str) {
        let Ok(sheet) = std::env::var("LOREKEEL_SHEET") else {
            eprintln!("skip: LOREKEEL_SHEET 未設定");
            return;
        };
        let bytes = std::fs::read(&sheet).expect("参照画像が読める");
        let refs = vec![RefImage { name: "01_cast.png".into(), mime: "image/png".into(), bytes }];
        let cfg = ImageGenConfig {
            provider,
            base_url: match provider {
                Provider::Openai => "https://api.openai.com/v1".into(),
                Provider::Gemini => "https://generativelanguage.googleapis.com".into(),
                Provider::Comfy => "http://127.0.0.1:8188".into(),
            },
            model: String::new(),
            shape: Shape::Landscape,
            detail: Detail::Standard,
            style: None,
            user_prefix: String::new(),
            negative: String::new(),
            workflow_json: None,
            timeout_secs: None,
            lock_seed: false,
            seed: 0,
        };
        // **参照そのものを指す語は書かない** (failures #85: 画像モデルが被写体の指示と読み、
        // 資料集のような分割画像を作る)。見た目は具体語で書き、1 場面であることを明示する。
        let prompt = "A man with a short dark beard, a green coat and brown boots sits at the                       dusty grand piano in a dim hall, one hand on the keys, looking over his                       shoulder toward the viewer. A single continuous scene, one frame, no panels                       or split screen. Watercolor illustration, muted colors.";
        let t0 = std::time::Instant::now();
        match generate(&cfg, key, prompt, 43, &refs).await {
            Ok(g) => {
                let path = out_dir().join(format!("kataribe_live_{provider:?}_ref.png"));
                std::fs::write(&path, &g.bytes).unwrap();
                eprintln!("{provider:?} with ref OK: bytes={} elapsed={:.1}s -> {}", g.bytes.len(), t0.elapsed().as_secs_f32(), path.display());
            }
            Err(e) => panic!("{provider:?} with ref failed: {e}"),
        }
    }

    #[tokio::test]
    #[ignore = "実キーが要る live テスト"]
    async fn openai_with_reference_sheet() {
        let Some(k) = key(&["IMAGE_API_KEY_OPENAI", "OPENAI_API_KEY"]) else { return };
        run_with_sheet(Provider::Openai, &k).await;
    }

    #[tokio::test]
    #[ignore = "実キーが要る live テスト"]
    async fn gemini_with_reference_sheet() {
        let Some(k) = key(&["IMAGE_API_KEY_GEMINI", "GEMINI_API_KEY"]) else { return };
        run_with_sheet(Provider::Gemini, &k).await;
    }

    /// ComfyUI 参照 live (spec 25 改訂 2026-08-22)。テスト用フィクスチャ `testdata/comfy_ref_workflow.json` (%ref_1..3%) を
    /// **Kataribe の実経路** (upload → 置換 → 剪定 → /prompt → ポーリング → /view) で通す。
    /// 1 枚 (LoadImage 2 つを剪定) と 0 枚 (3 つ全部剪定・エンコーダは画像なし) の両方が PNG を返すこと。
    /// `COMFY_BASE_URL` (例 http://192.168.0.3:8188) と `LOREKEEL_SHEET` が要る。
    #[tokio::test]
    #[ignore = "実機 ComfyUI が要る live テスト"]
    async fn comfy_with_reference_sheet_prunes_unfilled_slots() {
        let Some(base) = key(&["COMFY_BASE_URL"]) else { return };
        let Ok(sheet) = std::env::var("LOREKEEL_SHEET") else {
            eprintln!("skip: LOREKEEL_SHEET 未設定");
            return;
        };
        let bytes = std::fs::read(&sheet).expect("参照画像が読める");
        let cfg = ImageGenConfig {
            provider: Provider::Comfy,
            base_url: base,
            model: String::new(),
            shape: Shape::Square,
            detail: Detail::Standard,
            style: None,
            user_prefix: String::new(),
            negative: "low quality, bad anatomy".into(),
            workflow_json: Some(include_str!("testdata/comfy_ref_workflow.json").to_string()),
            timeout_secs: None,
            lock_seed: false,
            seed: 0,
        };
        let prompt = "anime RPG game style, the same woman knight as in the reference sheet (see reference),                       standing on a castle rampart at dawn, wind in her hair, looking at the horizon.                       The background fills the entire frame, no plain backdrop or border.";
        for (label, refs) in [
            ("one_sheet", vec![RefImage { name: "01_cast.png".into(), mime: "image/png".into(), bytes: bytes.clone() }]),
            ("no_sheet", vec![]),
        ] {
            let t0 = std::time::Instant::now();
            match generate(&cfg, "", prompt, 47, &refs).await {
                Ok(g) => {
                    let path = out_dir().join(format!("kataribe_live_comfy_{label}.png"));
                    std::fs::write(&path, &g.bytes).unwrap();
                    eprintln!("Comfy {label} OK: bytes={} elapsed={:.1}s -> {}", g.bytes.len(), t0.elapsed().as_secs_f32(), path.display());
                    assert!(g.bytes.len() > 1000);
                }
                Err(e) => panic!("Comfy {label} failed: {e}"),
            }
        }
    }

    #[tokio::test]
    #[ignore = "実キーが要る live テスト"]
    async fn openai_generates_one_image() {
        let Some(k) = key(&["IMAGE_API_KEY_OPENAI", "OPENAI_API_KEY"]) else {
            eprintln!("skip: no OpenAI key");
            return;
        };
        run(Provider::Openai, &k).await;
    }

    #[tokio::test]
    #[ignore = "実キーが要る live テスト"]
    async fn gemini_generates_one_image() {
        let Some(k) = key(&["IMAGE_API_KEY_GEMINI", "GEMINI_API_KEY"]) else {
            eprintln!("skip: no Gemini key");
            return;
        };
        run(Provider::Gemini, &k).await;
    }
}

