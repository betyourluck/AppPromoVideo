import { describe, expect, it } from "vitest";

import { DEFAULT_CLI_EXE, toBackendCli } from "./settings";
import { messages } from "./i18n";

/**
 * 2026-09-12: agy を kind として足した。**既定は claude のまま** — agy は CLI 側で書き込み系の
 * ツールを止められないので (data_contract の `IsolationGuarantee`)、選ぶのは明示的な操作にする。
 */
describe("agy の設定", () => {
  it("実行ファイルの既定を持つ", () => {
    expect(DEFAULT_CLI_EXE.agy).toBe("agy");
  });

  it("kind がそのまま backend へ渡る", () => {
    const b = toBackendCli({ kind: "agy", executable: "", extraArgs: "", model: "", timeoutSecs: 600, maxTurns: 12, oauthOnly: false });
    expect(b.kind).toBe("agy");
    expect(b.executable).toBe("agy");
  });

  it("警告の文言が 3 言語にあり、検出であって予防でないと書いてある", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      const text = messages[lang]["settings.agyWarning"];
      expect(text.length).toBeGreaterThan(40);
    }
    expect(messages.ja["settings.agyWarning"]).toContain("予防ではありません");
    expect(messages.en["settings.agyWarning"]).toContain("not prevention");
  });
});
