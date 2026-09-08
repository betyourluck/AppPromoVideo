import { describe, expect, it } from "vitest";
import {
  defaultImageGenSettings,
  migrateImageGenSettings,
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
    expect(migrateImageGenSettings(null).provider).toBe("gemini");
    const p = migrateProjectSettings({ seconds: 45, aspect: "4:3", snapshots: ["a.png", 3], lang: "en" });
    expect(p.seconds).toBe(30);
    expect(p.aspect).toBe("16:9");
    expect(p.snapshots).toEqual(["a.png"]);
    expect(p.lang).toBe("en");
  });
  it("見出しは enabled かつフォント指定のときだけ backend へ (既定 OFF)", () => {
    expect(defaultImageGenSettings().caption.enabled).toBe(false);
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
