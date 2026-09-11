import { beforeEach, describe, expect, it } from "vitest";
import { currentLang, messages, setLang, t, UI_LANGUAGES } from "./i18n";

describe("i18n module", () => {
  beforeEach(() => {
    setLang("ja");
  });

  it("UI_LANGUAGES に ja, en, zh-CN が含まれている", () => {
    const codes = UI_LANGUAGES.map((l) => l.code);
    expect(codes).toContain("ja");
    expect(codes).toContain("en");
    expect(codes).toContain("zh-CN");
  });

  it("日本語 (ja) の翻訳が正しく取得できる", () => {
    setLang("ja");
    expect(currentLang.value).toBe("ja");
    expect(t("common.ok")).toBe("OK");
    expect(t("input.title")).toBe("入力");
    expect(t("titleBar.running")).toBe("実行中");
  });

  it("英語 (en) への切り替えと翻訳が正しく機能する", () => {
    setLang("en");
    expect(currentLang.value).toBe("en");
    expect(t("common.cancel")).toBe("Cancel");
    expect(t("input.title")).toBe("Input");
    expect(t("titleBar.running")).toBe("Running");
  });

  it("簡体字中国語 (zh-CN) への切り替えと翻訳が正しく機能する", () => {
    setLang("zh-CN");
    expect(currentLang.value).toBe("zh-CN");
    expect(t("common.cancel")).toBe("取消");
    expect(t("input.title")).toBe("输入");
    expect(t("titleBar.running")).toBe("运行中");
  });

  it("パラメータ置換 ({key}) が正常に機能する", () => {
    setLang("ja");
    expect(t("input.briefInfo", { chars: 120, tree_lines: 15 })).toBe("brief 120 字 / tree 15 行");

    setLang("en");
    expect(t("input.briefInfo", { chars: 120, tree_lines: 15 })).toBe("brief 120 chars / tree 15 lines");

    setLang("zh-CN");
    expect(t("input.briefInfo", { chars: 120, tree_lines: 15 })).toBe("摘要 120 字 / 结构树 15 行");
  });

  it("差し込む値に $ や {…} が入っていても、そのまま出す (置換パターンとして解釈しない)", () => {
    // rev30: 値を String.replace の置換文字列に渡すと、`$&` は照合した `{path}` に化ける
    // (Windows のパス `C:\$Recycle.Bin` やエラー文言が値に来る)。
    setLang("ja");
    expect(t("scene.toastExport", { path: "C:\\$Recycle.Bin\\$&" })).toBe("書き出しました: C:\\$Recycle.Bin\\$&");
    // 先に差し込んだ値の中の `{…}` を、後の置換が拾わない。
    expect(t("input.briefInfo", { chars: "{tree_lines}", tree_lines: 5 })).toBe("brief {tree_lines} 字 / tree 5 行");
  });

  it("すべての言語辞書でキーの整合性がある程度保たれている", () => {
    const jaKeys = Object.keys(messages.ja);
    const enKeys = Object.keys(messages.en);
    const zhKeys = Object.keys(messages["zh-CN"]);

    expect(jaKeys.length).toBeGreaterThan(30);
    expect(enKeys.length).toBe(jaKeys.length);
    expect(zhKeys.length).toBe(jaKeys.length);
  });
});

/**
 * rev30: 件数だけでは「1 つ欠けて 1 つ余る」を見逃す。**キーの集合**を ja (正本) と突き合わせる。
 * 欠けたキーは t() が黙って ja に落ちるので、英語の画面に日本語が 1 行だけ混ざる形で出る。
 */
function keyDiff(base: object, other: object): { missing: string[]; extra: string[] } {
  const b = Object.keys(base);
  const o = Object.keys(other);
  return { missing: b.filter((k) => !o.includes(k)), extra: o.filter((k) => !b.includes(k)) };
}

describe("辞書のキー集合", () => {
  it("en と zh-CN は ja と同じキーを過不足なく持つ", () => {
    expect(keyDiff(messages.ja, messages.en)).toEqual({ missing: [], extra: [] });
    expect(keyDiff(messages.ja, messages["zh-CN"])).toEqual({ missing: [], extra: [] });
  });

  it("網そのものが効く — 件数が同じでも、欠けと余りを見分ける", () => {
    const ja = { "a.one": "1", "a.two": "2" };
    const shifted = { "a.one": "1", "a.typo": "2" };
    expect(Object.keys(shifted).length).toBe(Object.keys(ja).length); // 旧テストはこれで通っていた
    expect(keyDiff(ja, shifted)).toEqual({ missing: ["a.two"], extra: ["a.typo"] });
  });
});
