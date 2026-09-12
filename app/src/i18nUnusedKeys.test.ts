import { describe, expect, it } from "vitest";

import { messages } from "./i18n";

/**
 * 2026-09-12: 画面の作り替え (rev31〜37) で文言のキーが張り替わり、**古い名前が辞書に残った**。
 * 12 個が誰からも参照されないまま 3 言語ぶん (ja / en / zh-CN) 居座っていた
 * (`runs.empty` → `runs.none`、`caption.originalText` → `caption.restoreFirst` のような改名の取り残し)。
 *
 * 型は「辞書に無いキーを使うこと」は止めるが、「使われないキーが辞書に残ること」は止めない。
 * これは**機械の網**: 消し忘れたキーがここで落ちる。
 *
 * 参照の数え方は**引用符で囲まれた完全一致**。`t(cond ? "a.x" : "a.y")` のような分岐も拾える。
 * 部分一致にすると `runs.delete` が `runs.deleteTitle` に隠れる (初版の穴。実際に 1 個見逃していた)。
 * テストファイルは参照元に数えない — テストだけが呼んでいるキーは画面で使われていないため。
 */
const files = import.meta.glob<string>("./**/*.{ts,vue}", { query: "?raw", import: "default", eager: true });

const sources = Object.entries(files).filter(([path]) => !path.endsWith(".test.ts") && path !== "./i18n.ts");

const blob = sources.map(([, text]) => text).join("\n");

/** ソースに現れる形。キーの前後を引用符で閉じ、`runs.delete` が `runs.deleteTitle` に当たらないようにする。 */
function quoted(key: string): string[] {
  return [`"${key}"`, `'${key}'`];
}

describe("i18n の辞書", () => {
  it("使われないキーを残さない", () => {
    const unused = Object.keys(messages.ja).filter((key) => !quoted(key).some((q) => blob.includes(q)));
    expect(unused).toEqual([]);
  });

  it("3 言語が同じキーを持つ", () => {
    const ja = Object.keys(messages.ja).sort();
    expect(Object.keys(messages.en).sort()).toEqual(ja);
    expect(Object.keys(messages["zh-CN"]).sort()).toEqual(ja);
  });

  /** 網自身の検出力: 参照元を集められていなければ「全部未使用」になるので、そこで気づけない。 */
  it("参照元を実際に読めている", () => {
    expect(sources.length).toBeGreaterThan(10);
    expect(quoted("runs.none").some((q) => blob.includes(q))).toBe(true);
  });
});
