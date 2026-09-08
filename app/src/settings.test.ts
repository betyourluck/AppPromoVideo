import { describe, expect, it } from "vitest";
import {
  defaultImageGenSettings,
  migrateImageGenSettings,
  pickCaptionFont,
  toBackendCaption,
  migrateProjectSettings,
  splitArgs,
  toBackendCli,
  toBackendConfig,
  workflowAcceptsRefs,
} from "./settings";

describe("toBackendCli", () => {
  it("引数を空白で分け、空の model は null、timeout は 30 未満を切り上げる", () => {
    const b = toBackendCli({ kind: "claude", executable: " claude ", extraArgs: "  --dangerously-skip-permissions   --x ", model: "", timeoutSecs: 5, maxTurns: 0, oauthOnly: true });
    expect(b.oauth_only).toBe(true);
    expect(b.executable).toBe("claude");
    expect(b.extra_args).toEqual(["--dangerously-skip-permissions", "--x"]);
    expect(b.model).toBeNull();
    expect(b.timeout_secs).toBe(30);
    expect(b.max_turns).toBe(12);
  });
  it("空の executable は kind の既定名に倒す", () => {
    expect(toBackendCli({ kind: "aider", executable: "", extraArgs: "", model: "m", timeoutSecs: 600, maxTurns: 3, oauthOnly: false }).executable).toBe("aider");
    expect(splitArgs("")).toEqual([]);
  });
});

describe("toBackendConfig (Kataribe spec 26 の逆向き漏れ封鎖)", () => {
  it("comfy の negative / workflow / seed は他プロバイダへ漏れない", () => {
    const s = defaultImageGenSettings();
    s.perProvider.comfy.workflowJson = '{"1":{}}';
    s.perProvider.comfy.negative = "lowres";
    s.lockSeed = true;
    s.provider = "openai";
    const b = toBackendConfig(s, "16:9");
    expect(b.negative).toBe("");
    expect(b.workflow_json).toBeNull();
    expect(b.lock_seed).toBe(false);
    expect(b.base_url).toBe("https://api.openai.com/v1");
    expect(b.model).toBe("gpt-image-1-mini");
    expect(b.shape).toBe("landscape");
    s.provider = "comfy";
    const c = toBackendConfig(s, "9:16");
    expect(c.workflow_json).toBe('{"1":{}}');
    expect(c.negative).toBe("lowres");
    expect(c.lock_seed).toBe(true);
    expect(c.shape).toBe("portrait");
    expect(c.model).toBe("");
  });
  it("カスタム baseUrl はそのプロバイダのスロットにだけ残る", () => {
    const s = defaultImageGenSettings();
    s.provider = "gemini";
    s.perProvider.gemini.baseUrl = "https://generativelanguage.googleapis.com/v1beta";
    expect(toBackendConfig(s, "1:1").base_url).toContain("/v1beta");
    s.provider = "comfy";
    expect(toBackendConfig(s, "1:1").base_url).toBe("http://127.0.0.1:8188");
  });
});

describe("migrate", () => {
  it("欠損・不正は既定で埋め、既知の値は残す", () => {
    const m = migrateImageGenSettings({ provider: "comfy", detail: "bogus", seed: -3, perProvider: { comfy: { model: 7, timeoutSecs: 99 } } });
    expect(m.provider).toBe("comfy");
    expect(m.detail).toBe("standard");
    expect(m.seed).toBe(12345);
    expect(m.perProvider.comfy.model).toBe("");
    expect(m.perProvider.comfy.timeoutSecs).toBe(99);
    expect(m.perProvider.openai.baseUrl).toBe("https://api.openai.com/v1");
    // rev5: 面の貼り方。既定は perspective、知らない値は既定に落ちる。
    expect(migrateImageGenSettings(null).plateMode).toBe("perspective");
    expect(migrateImageGenSettings({ plateMode: "frontal" }).plateMode).toBe("frontal");
    expect(migrateImageGenSettings({ plateMode: "tilted" }).plateMode).toBe("perspective");
    expect(migrateImageGenSettings(null).provider).toBe("gemini");
    const p = migrateProjectSettings({ seconds: 45, aspect: "4:3", snapshots: ["a.png", 3], lang: "en" });
    expect(p.seconds).toBe(30);
    expect(p.aspect).toBe("16:9");
    expect(p.snapshots).toEqual(["a.png"]);
    expect(p.lang).toBe("en");
  });
  it("見出しは enabled かつフォント指定のときだけ backend へ (既定 ON、フォントは実行時に解決)", () => {
    // 2026-09-08 に既定 ON へ (ユーザー判断)。fontPath は空のままなので、
    // 未解決のうちは backend へ渡さない = 「ON なのに焼けない」ではなく「焼かずに進む」。
    expect(defaultImageGenSettings().caption.enabled).toBe(true);
    expect(defaultImageGenSettings().caption.fontPath).toBe("");
    expect(toBackendCaption({ enabled: false, fontPath: "C:/f.ttc", fontIndex: 0, sizeRatio: 0.05, position: "bottom" })).toBeNull();
    expect(toBackendCaption({ enabled: true, fontPath: "", fontIndex: 0, sizeRatio: 0.05, position: "bottom" })).toBeNull();
    expect(toBackendCaption({ enabled: true, fontPath: "C:/f.ttc", fontIndex: 2, sizeRatio: 0.9, position: "top" })).toEqual({ font_path: "C:/f.ttc", font_index: 2, size_ratio: 0.2, position: "top" });
    const m = migrateImageGenSettings({ caption: { enabled: true, fontPath: "x.ttf", sizeRatio: -1, position: "middle" } });
    expect(m.caption).toEqual({ enabled: true, fontPath: "x.ttf", fontIndex: 0, sizeRatio: 0.055, position: "bottom" });
  });
  it("workflow の差し込み口検査", () => {
    expect(workflowAcceptsRefs('{"a":"%ref_1%"}')).toBe(true);
    expect(workflowAcceptsRefs("{}")).toBe(false);
  });
});

describe("caption の migrate", () => {
  it("キーが無いときは既定に落ちる (false 固定にしない)", () => {
    // rev8 で既定が ON になったので、`enabled` を持たない古い / 部分的な保存は既定に従うべき。
    const m = migrateImageGenSettings({ caption: { fontPath: "x.ttf" } });
    expect(m.caption.enabled).toBe(true);
    expect(m.caption.fontPath).toBe("x.ttf");
  });

  it("明示された false は尊重する (ユーザーが切ったものを勝手に戻さない)", () => {
    expect(migrateImageGenSettings({ caption: { enabled: false } }).caption.enabled).toBe(false);
  });
});

describe("pickCaptionFont (見出しの自動選択)", () => {
  const f = (family: string, has_japanese = true, source: "system" | "user" = "system") => ({
    path: `C:/Windows/Fonts/${family}.ttc`,
    index: 0,
    family,
    source,
    has_japanese,
  });

  it("日本語グリフを持たないフォントは選ばない", () => {
    expect(pickCaptionFont([f("Arial", false), f("Segoe UI", false)])).toBeNull();
    expect(pickCaptionFont([])).toBeNull();
  });

  it("画像に焼くので明朝より太めのゴシックを選ぶ", () => {
    // 実機にあった顔ぶれ。明朝・教科書体が先に並んでいても拾わない。
    const found = pickCaptionFont([
      f("BIZ UDMincho Medium"),
      f("UD Digi Kyokasho N-R"),
      f("Yu Mincho"),
      f("Yu Gothic Bold"),
      f("MS Gothic"),
    ]);
    expect(found?.family).toBe("Yu Gothic Bold");
  });

  it("family 名が日本語のフォントを取りこぼさない (実機の Windows 11 の顔ぶれ)", () => {
    // 実測 2026-09-08: 游ゴシック / メイリオ は**日本語名**で登録されている。
    // ASCII の "yu gothic" / "meiryo" だけを見ていると一生当たらない。
    expect(pickCaptionFont([f("游明朝"), f("HGP教科書体"), f("游ゴシック")])?.family).toBe("游ゴシック");
    expect(pickCaptionFont([f("HGP明朝B"), f("メイリオ")])?.family).toBe("メイリオ");
    // 半角カナの ｺﾞｼｯｸ も拾う (HG シリーズ)。
    expect(pickCaptionFont([f("HGP行書体"), f("HGPｺﾞｼｯｸE")])?.family).toBe("HGPｺﾞｼｯｸE");
    // 本命: 日本語名のゴシックが、優先語に当たらない ASCII 名に負けないこと。
    // 同点だと localeCompare で ASCII が先に来るので、優先語に日本語が無いと必ず負ける。
    expect(pickCaptionFont([f("Aharoni Bold"), f("游ゴシック")])?.family).toBe("游ゴシック");
    expect(pickCaptionFont([f("Aharoni Bold"), f("メイリオ")])?.family).toBe("メイリオ");
    expect(pickCaptionFont([f("Aharoni Bold"), f("BIZ UDゴシック")])?.family).toBe("BIZ UDゴシック");
    // ゴシックが何も無ければ明朝でも返す (焼けないよりまし)。
    expect(pickCaptionFont([f("游明朝")])?.family).toBe("游明朝");
  });

  it("明朝しか無ければ諦めずにそれを返す (焼けないより焼ける方がよい)", () => {
    expect(pickCaptionFont([f("Yu Mincho")])?.family).toBe("Yu Mincho");
  });

  it("ユーザーが app_data/fonts に置いたものを最優先する", () => {
    const found = pickCaptionFont([f("Yu Gothic Bold"), f("SomeBrandFont", true, "user")]);
    expect(found?.family).toBe("SomeBrandFont");
  });

  it("優先語に当たったものは、何にも当たらないものより必ず強い", () => {
    // スコアが 100 - index*10 だったので、リストの末尾 ("sans") に当たると -10 になり、
    // 何にも当たらない 0 に負けていた。実例: "Noto Sans CJK JP" は "noto sans jp" に
    // 当たらず ("cjk" が挟まる) "sans" にだけ当たる。
    expect(pickCaptionFont([f("Aharoni Bold"), f("Noto Sans CJK JP")])?.family).toBe("Noto Sans CJK JP");
    expect(pickCaptionFont([f("Aharoni Bold"), f("Some Gothic")])?.family).toBe("Some Gothic");
  });

  it("同点なら family 名で決める (実行のたびに変わらない)", () => {
    const a = pickCaptionFont([f("Zeta Gothic"), f("Alpha Gothic")]);
    const b = pickCaptionFont([f("Alpha Gothic"), f("Zeta Gothic")]);
    expect(a?.family).toBe(b?.family);
  });
});

