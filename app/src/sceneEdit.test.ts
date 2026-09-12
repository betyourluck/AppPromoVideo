import { describe, expect, it } from "vitest";

import { messages } from "./i18n";

/**
 * rev43: シーンの並び替え / 複製 / 削除。判定と付け替えは Rust 側の PoC が持つ
 * (`promo_core::scene_edit` と backend の IO テスト)。ここは**画面の文言**だけを見る。
 */
describe("シーン編集の文言", () => {
  const KEYS = [
    "scene.moveUp",
    "scene.moveDown",
    "scene.duplicate",
    "scene.remove",
    "scene.removeTitle",
    "scene.removeMessage",
    "scene.driftOver",
    "scene.driftUnder",
  ] as const;

  it("3 言語すべてにある", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      for (const k of KEYS) expect(messages[lang][k], `${lang} / ${k}`).toBeTruthy();
    }
  });

  /** 強調は `<b>` / `<mono>`。markdown の `**` は Rich.vue が解さず、そのまま画面に出る。 */
  it("markdown の強調を書かない", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      for (const k of KEYS) expect(messages[lang][k]).not.toContain("**");
    }
  });

  /** 削除は取り消せないので、**何が消えるか**を書く。 */
  it("削除の確認は参照画像も消えることと取り消せないことを書く", () => {
    expect(messages.ja["scene.removeMessage"]).toContain("参照画像");
    expect(messages.ja["scene.removeMessage"]).toContain("取り消せません");
    expect(messages.en["scene.removeMessage"]).toContain("cannot be undone");
  });

  /** 尺は**直さない**と明言する (自動調整したと誤解させない)。 */
  it("尺のずれは自動調整しないと書く", () => {
    for (const k of ["scene.driftOver", "scene.driftUnder"] as const) {
      expect(messages.ja[k]).toContain("自動では調整しません");
      expect(messages.en[k]).toContain("Not adjusted automatically");
      expect(messages[k === "scene.driftOver" ? "ja" : "ja"][k]).toContain("{total}");
    }
  });
});
