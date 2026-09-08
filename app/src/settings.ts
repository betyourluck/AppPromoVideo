/**
 * 設定 (非秘密) — localStorage `apppromo.*`。契約 config_sources.localStorage。
 *
 * - `cli`: LLM CLI の指定 (キー欄なし = CLI の認証に委ねる。契約 決定 10 の 2 層構造の片側)。
 * - `image`: 画像生成。プロバイダ別スロット (Kataribe spec 26 の写し) で、切替時に値が漏れない。
 *   API キーは backend の app_data/.env に置き、ここには持たない。
 * - `project`: 直近の入力 (リポジトリ / スナップショット / 世界観 / 尺 / 比率 / 言語 / 出力先)。
 */
import type { Aspect, FontEntry, Language } from "./types";

// --- LLM CLI ---------------------------------------------------------------

export type CliKind = "claude" | "aider" | "custom";

export interface CliSettings {
  kind: CliKind;
  executable: string;
  /** 空白区切りの追加引数 (例 --dangerously-skip-permissions)。 */
  extraArgs: string;
  model: string;
  timeoutSecs: number;
  maxTurns: number;
  /** 子 CLI に環境変数の鍵を渡さず、claude auth login の OAuth を使わせる。 */
  oauthOnly: boolean;
}

export const DEFAULT_CLI_EXE: Record<CliKind, string> = { claude: "claude", aider: "aider", custom: "" };

export function defaultCliSettings(): CliSettings {
  return { kind: "claude", executable: "claude", extraArgs: "", model: "", timeoutSecs: 600, maxTurns: 12, oauthOnly: false };
}

/** backend `CliSettings` (snake_case)。 */
export interface BackendCliSettings {
  kind: CliKind;
  executable: string;
  extra_args: string[];
  model: string | null;
  timeout_secs: number;
  max_turns: number;
  oauth_only: boolean;
}

export function splitArgs(s: string): string[] {
  return s.split(/\s+/).map((a) => a.trim()).filter((a) => a.length > 0);
}

export function toBackendCli(s: CliSettings): BackendCliSettings {
  return {
    kind: s.kind,
    executable: s.executable.trim() || DEFAULT_CLI_EXE[s.kind] || "claude",
    extra_args: splitArgs(s.extraArgs),
    model: s.model.trim() ? s.model.trim() : null,
    timeout_secs: Math.max(30, Math.floor(s.timeoutSecs) || 600),
    max_turns: Math.max(1, Math.floor(s.maxTurns) || 12),
    oauth_only: s.oauthOnly === true,
  };
}

// --- 画像生成 (Kataribe imageGen.ts perProvider の写し) -------------------------

export type ImageProvider = "openai" | "gemini" | "comfy";
export type ImageDetail = "standard" | "high" | "highest";
export type ImagePromptStyle = "tags" | "prose";

export interface ProviderSlot {
  baseUrl: string;
  model: string;
  style: ImagePromptStyle | "";
  negative: string;
  workflowJson: string;
  timeoutSecs: number;
}

export interface ImageGenSettings {
  /** 解析後に参照画像も作るか。 */
  enabled: boolean;
  provider: ImageProvider;
  detail: ImageDetail;
  /** 空なら backend が VisualIdentity から style anchor を合成する (決定 8)。 */
  userPrefix: string;
  lockSeed: boolean;
  seed: number;
  /** 先頭から何シーン分作るか (0 = 全部)。 */
  maxScenes: number;
  /** 送る参照枚数 (0 = プロバイダ既定)。 */
  requestedRefs: number;
  perProvider: Record<ImageProvider, ProviderSlot>;
  /** 見出し (copy) の焼き込み。既定 OFF (アプリで焼くか手で焼くかはユーザー判断待ち)。 */
  caption: CaptionSettings;
  /**
   * 製品カットで実スクショを貼るときの面の扱い (契約 PlateMode、rev5)。
   * perspective = 背景のパースに合わせて射影変換で傾ける (既定)。
   * frontal = 正面固定。背景側のアングル指定も検査で弾く。
   */
  plateMode: PlateMode;
}

export type PlateMode = "perspective" | "frontal";

export interface CaptionSettings {
  enabled: boolean;
  fontPath: string;
  fontIndex: number;
  /** 文字の高さ / canvas の高さ。 */
  sizeRatio: number;
  position: "top" | "bottom";
  /** 文字色 `#RRGGBB` (rev9)。空で白。 */
  color: string;
}

export function defaultCaptionSettings(): CaptionSettings {
  // 既定 ON (ユーザー判断 2026-09-08)。fontPath は空のまま — 実行直前に pickCaptionFont で埋める。
  return { enabled: true, fontPath: "", fontIndex: 0, sizeRatio: 0.055, position: "bottom", color: "#FFFFFF" };
}

/** backend へ渡す形 (enabled かつフォント指定ありのときだけ)。 */
export function toBackendCaption(c: CaptionSettings): { font_path: string; font_index: number; size_ratio: number; position: "top" | "bottom"; color: string } | null {
  if (!c.enabled || !c.fontPath.trim()) return null;
  return {
    font_path: c.fontPath,
    font_index: Math.max(0, Math.floor(c.fontIndex) || 0),
    size_ratio: Math.min(0.2, Math.max(0.02, c.sizeRatio || 0.055)),
    position: c.position === "top" ? "top" : "bottom",
    color: /^#[0-9a-fA-F]{6}$/.test(c.color) ? c.color : "#FFFFFF",
  };
}

export const DEFAULT_BASE_URL: Record<ImageProvider, string> = {
  openai: "https://api.openai.com/v1",
  gemini: "https://generativelanguage.googleapis.com",
  comfy: "http://127.0.0.1:8188",
};

export const DEFAULT_MODEL: Record<ImageProvider, string> = {
  openai: "gpt-image-1-mini",
  gemini: "gemini-3.1-flash-lite-image",
  comfy: "",
};

const PROVIDERS: ImageProvider[] = ["openai", "gemini", "comfy"];

/** ComfyUI の negative 既定 (Kataribe 2026-08-26 実測。空だと参照の無地背景が構図に漏れる)。 */
export const COMFY_DEFAULT_NEGATIVE =
  "lowres, bad anatomy, low quality, blurry, jpeg artifacts, grey background, plain background, border, split screen";

export function defaultSlot(p: ImageProvider): ProviderSlot {
  return {
    baseUrl: DEFAULT_BASE_URL[p],
    model: DEFAULT_MODEL[p],
    style: "",
    negative: p === "comfy" ? COMFY_DEFAULT_NEGATIVE : "",
    workflowJson: "",
    timeoutSecs: 0,
  };
}

export function defaultImageGenSettings(): ImageGenSettings {
  return {
    enabled: false,
    provider: "gemini",
    detail: "standard",
    userPrefix: "",
    lockSeed: false,
    seed: 12345,
    maxScenes: 0,
    requestedRefs: 0,
    perProvider: { openai: defaultSlot("openai"), gemini: defaultSlot("gemini"), comfy: defaultSlot("comfy") },
    caption: defaultCaptionSettings(),
    plateMode: "perspective",
  };
}

export function currentSlot(s: ImageGenSettings): ProviderSlot {
  return s.perProvider?.[s.provider] ?? defaultSlot(s.provider);
}

function sanitizeSlot(p: ImageProvider, raw: unknown): ProviderSlot {
  const base = defaultSlot(p);
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Partial<ProviderSlot>;
  return {
    baseUrl: typeof r.baseUrl === "string" ? r.baseUrl : base.baseUrl,
    model: typeof r.model === "string" ? r.model : base.model,
    style: r.style === "tags" || r.style === "prose" ? r.style : "",
    negative: typeof r.negative === "string" ? r.negative : "",
    workflowJson: typeof r.workflowJson === "string" ? r.workflowJson : "",
    timeoutSecs: typeof r.timeoutSecs === "number" && r.timeoutSecs > 0 ? r.timeoutSecs : 0,
  };
}

/** 保存データの読み込み (欠損は既定で埋める。純関数)。 */
export function migrateImageGenSettings(raw: unknown): ImageGenSettings {
  const out = defaultImageGenSettings();
  if (!raw || typeof raw !== "object") return out;
  const r = raw as Record<string, unknown>;
  if (typeof r.enabled === "boolean") out.enabled = r.enabled;
  if (r.provider === "openai" || r.provider === "gemini" || r.provider === "comfy") out.provider = r.provider;
  if (r.detail === "standard" || r.detail === "high" || r.detail === "highest") out.detail = r.detail;
  if (typeof r.userPrefix === "string") out.userPrefix = r.userPrefix;
  if (typeof r.lockSeed === "boolean") out.lockSeed = r.lockSeed;
  if (typeof r.seed === "number" && Number.isFinite(r.seed) && r.seed >= 0) out.seed = Math.floor(r.seed);
  if (typeof r.maxScenes === "number" && r.maxScenes >= 0) out.maxScenes = Math.floor(r.maxScenes);
  if (typeof r.requestedRefs === "number" && r.requestedRefs >= 0) out.requestedRefs = Math.floor(r.requestedRefs);
  if (r.perProvider && typeof r.perProvider === "object") {
    const pp = r.perProvider as Record<string, unknown>;
    for (const p of PROVIDERS) out.perProvider[p] = p in pp ? sanitizeSlot(p, pp[p]) : defaultSlot(p);
  }
  if (r.plateMode === "perspective" || r.plateMode === "frontal") out.plateMode = r.plateMode;
  if (r.caption && typeof r.caption === "object") {
    const c = r.caption as Partial<CaptionSettings>;
    out.caption = {
      // キーが無い保存は既定へ落とす (false 固定にしない)。明示された false は尊重する。
      enabled: typeof c.enabled === "boolean" ? c.enabled : defaultCaptionSettings().enabled,
      fontPath: typeof c.fontPath === "string" ? c.fontPath : "",
      fontIndex: typeof c.fontIndex === "number" && c.fontIndex >= 0 ? Math.floor(c.fontIndex) : 0,
      sizeRatio: typeof c.sizeRatio === "number" && c.sizeRatio > 0 ? c.sizeRatio : 0.055,
      position: c.position === "top" ? "top" : "bottom",
      color: /^#[0-9a-fA-F]{6}$/.test(String(c.color ?? "")) ? String(c.color) : "#FFFFFF",
    };
  }
  return out;
}

/** backend `image_gen::ImageGenConfig` (snake_case)。 */
export interface BackendImageGenConfig {
  provider: ImageProvider;
  base_url: string;
  model: string;
  shape: "square" | "landscape" | "portrait";
  detail: ImageDetail;
  style: ImagePromptStyle | null;
  user_prefix: string;
  negative: string;
  workflow_json: string | null;
  timeout_secs: number | null;
  lock_seed: boolean;
  seed: number;
}

export function shapeForAspect(a: Aspect): BackendImageGenConfig["shape"] {
  return a === "16:9" ? "landscape" : a === "9:16" ? "portrait" : "square";
}

/**
 * 実効値写像 (Kataribe spec 26 で凍結)。**他プロバイダのスロットは一切読まない**。negative /
 * workflow_json / lock_seed は comfy 以外で送らない (逆向き漏れの封鎖)。shape は動画の比率から写す。
 */
export function toBackendConfig(s: ImageGenSettings, aspect: Aspect): BackendImageGenConfig {
  const slot = currentSlot(s);
  return {
    provider: s.provider,
    base_url: slot.baseUrl.trim() || DEFAULT_BASE_URL[s.provider],
    model: slot.model.trim() || (s.provider === "comfy" ? "" : DEFAULT_MODEL[s.provider]),
    shape: shapeForAspect(aspect),
    detail: s.detail,
    style: slot.style || null,
    user_prefix: s.userPrefix,
    negative: s.provider === "comfy" ? slot.negative : "",
    workflow_json: s.provider === "comfy" && slot.workflowJson.trim() ? slot.workflowJson : null,
    timeout_secs: slot.timeoutSecs > 0 ? Math.floor(slot.timeoutSecs) : null,
    lock_seed: s.provider === "comfy" && s.lockSeed,
    seed: Math.max(0, Math.floor(s.seed) || 0),
  };
}

export function supportsNegative(p: ImageProvider): boolean {
  return p === "comfy";
}

export function workflowAcceptsRefs(workflowJson: string): boolean {
  return /%ref_1%/.test(workflowJson);
}

// --- 直近の入力 -----------------------------------------------------------------

export interface ProjectSettings {
  projectPath: string;
  snapshots: string[];
  concept: string;
  seconds: 15 | 30 | 60;
  aspect: Aspect;
  lang: Language;
  exportDir: string;
}

export function defaultProjectSettings(): ProjectSettings {
  return { projectPath: "", snapshots: [], concept: "", seconds: 30, aspect: "16:9", lang: "ja", exportDir: "" };
}

export function migrateProjectSettings(raw: unknown): ProjectSettings {
  const out = defaultProjectSettings();
  if (!raw || typeof raw !== "object") return out;
  const r = raw as Record<string, unknown>;
  if (typeof r.projectPath === "string") out.projectPath = r.projectPath;
  if (Array.isArray(r.snapshots)) out.snapshots = r.snapshots.filter((x): x is string => typeof x === "string");
  if (typeof r.concept === "string") out.concept = r.concept;
  if (r.seconds === 15 || r.seconds === 30 || r.seconds === 60) out.seconds = r.seconds;
  if (r.aspect === "16:9" || r.aspect === "9:16" || r.aspect === "1:1") out.aspect = r.aspect;
  if (r.lang === "ja" || r.lang === "en") out.lang = r.lang;
  if (typeof r.exportDir === "string") out.exportDir = r.exportDir;
  return out;
}

// --- localStorage ---------------------------------------------------------------

export const KEYS = { cli: "apppromo.cli", image: "apppromo.image", project: "apppromo.project" } as const;

function loadJson(key: string): unknown {
  try {
    const raw = localStorage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function loadCliSettings(): CliSettings {
  const r = loadJson(KEYS.cli) as Partial<CliSettings> | null;
  const d = defaultCliSettings();
  if (!r) return d;
  return {
    kind: r.kind === "aider" || r.kind === "custom" ? r.kind : "claude",
    executable: typeof r.executable === "string" ? r.executable : d.executable,
    extraArgs: typeof r.extraArgs === "string" ? r.extraArgs : "",
    model: typeof r.model === "string" ? r.model : "",
    timeoutSecs: typeof r.timeoutSecs === "number" ? r.timeoutSecs : d.timeoutSecs,
    maxTurns: typeof r.maxTurns === "number" ? r.maxTurns : d.maxTurns,
    oauthOnly: r.oauthOnly === true,
  };
}

export function loadImageGenSettings(): ImageGenSettings {
  return migrateImageGenSettings(loadJson(KEYS.image));
}

export function loadProjectSettings(): ProjectSettings {
  return migrateProjectSettings(loadJson(KEYS.project));
}

export function save(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage が塞がれた環境 */
  }
}

/**
 * 見出しを焼くフォントを自動で選ぶ (純粋)。既定 ON にしたので、フォント未選択のまま
 * 「ON なのに何も焼かれない」を起こさないための解決役。
 *
 * 規律: ①日本語グリフ必須 (copy_text は日本語) ②ユーザーが app_data/fonts に置いたものを最優先
 * (意図して置いたものより機械の推測を上に置かない) ③**画像に焼くので太めのゴシック**を優先する —
 * 細い明朝や教科書体は写真の上で読めない ④同点は family 名で決める (実行のたびに変わらない)。
 */
export function pickCaptionFont(fonts: FontEntry[]): FontEntry | null {
  // 左ほど強い。実機 (Windows 11、日本語あり 118 件) の family 名から選んだ。
  // **日本語名を必ず入れる**: 游ゴシック / メイリオ は日本語名で登録されており、
  // ASCII の "yu gothic" / "meiryo" だけでは一生当たらない (実測 2026-09-08)。
  const PREFER = [
    "noto sans jp",
    "游ゴシック",
    "yu gothic",
    "メイリオ",
    "meiryo",
    "biz udゴシック",
    "biz udgothic",
    "ms gothic",
    // 総称。半角カナ (HG シリーズ) も見る。
    "ゴシック",
    "ｺﾞｼｯｸ",
    "gothic",
    "sans",
  ];
  const AVOID = ["mincho", "明朝", "serif", "kyokasho", "教科書", "行書", "ポップ", "ﾎﾟｯﾌﾟ", "brush", "script"];

  const score = (e: FontEntry): number => {
    const n = e.family.toLowerCase();
    let s = e.source === "user" ? 1000 : 0;
    // 当たったものは必ず 0 より上に出す (末尾に当たって負に落ちると、
    // 何にも当たらないフォントに負ける。実例: "Noto Sans CJK JP" は "sans" にしか当たらない)。
    const hit = PREFER.findIndex((k) => n.includes(k));
    if (hit >= 0) s += (PREFER.length - hit) * 10;
    if (AVOID.some((k) => n.includes(k))) s -= 50;
    if (n.includes("bold") || n.includes("b ")) s += 5; // 焼き込みは太い方が読める
    return s;
  };

  const cands = fonts.filter((e) => e.has_japanese);
  if (!cands.length) return null;
  return cands.slice().sort((a, b) => score(b) - score(a) || a.family.localeCompare(b.family))[0];
}

