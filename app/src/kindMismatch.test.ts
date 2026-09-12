import { describe, expect, it } from "vitest";

import { messages } from "./i18n";

/**
 * rev40: 種類と実行ファイルの食い違いを設定画面で止める。
 *
 * 2026-09-12 の事故: 種類が `claude` のまま実行ファイルだけ `agy` にでき、claude の argv が
 * 別系統の CLI に飛んでいた。判定そのものは Rust 側 (`kind_mismatches_version`) の PoC が持つ。
 */
describe("種類の食い違いの文言", () => {
  const KEY = "settings.kindMismatch" as const;

  it("3 言語にあり、差し込みの口を持つ", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      const text = messages[lang][KEY];
      expect(text).toContain("{kind}");
      expect(text).toContain("{version}");
    }
  });

  /** 強調は `<b>` / `<mono>`。markdown の `**` は Rich.vue が解さず、そのまま画面に出る。 */
  it("markdown の強調を書かない", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      expect(messages[lang][KEY]).not.toContain("**");
    }
  });
});
