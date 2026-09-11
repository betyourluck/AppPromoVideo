import { describe, expect, it } from "vitest";
import { parseRich } from "./rich";

/**
 * rev30: 辞書の文言に**強調と等幅だけ**を持たせる (`<b>…</b>` / `<mono>…</mono>`)。
 *
 * 説明文は段落の途中に強調や等幅のパスを挟むが、断片ごとにキーを分けると語順が言語で変わるので訳せない。
 * `v-html` は使わない — 認める印は 2 つだけで、それ以外の `<` はただの文字として出す
 * (辞書にも差し込む値にも HTML を注入させない)。
 */
describe("parseRich", () => {
  it("印の無い文はそのまま 1 つ", () => {
    expect(parseRich("まだ履歴がありません。")).toEqual([{ text: "まだ履歴がありません。", b: false, mono: false }]);
  });

  it("強調と等幅を区切る", () => {
    expect(parseRich("出ているのは<b>焼き上がり</b>です。<mono>app_data/fonts</mono> に置く")).toEqual([
      { text: "出ているのは", b: false, mono: false },
      { text: "焼き上がり", b: true, mono: false },
      { text: "です。", b: false, mono: false },
      { text: "app_data/fonts", b: false, mono: true },
      { text: " に置く", b: false, mono: false },
    ]);
  });

  it("知らない山括弧は文字として残す (パスの <パッケージ> や差し込んだ値)", () => {
    expect(parseRich("<mono><パッケージ>/runs/<日時>/</mono> に保存")).toEqual([
      { text: "<パッケージ>/runs/<日時>/", b: false, mono: true },
      { text: " に保存", b: false, mono: false },
    ]);
    expect(parseRich('<script>alert("x")</script>')).toEqual([{ text: '<script>alert("x")</script>', b: false, mono: false }]);
  });

  it("入れ子は両方の印を持つ", () => {
    expect(parseRich("<b>強調の中の<mono>code</mono></b>")).toEqual([
      { text: "強調の中の", b: true, mono: false },
      { text: "code", b: true, mono: true },
    ]);
  });

  it("閉じ忘れは文末まで、対の無い閉じ印は無視する (落とさずに出す)", () => {
    expect(parseRich("前<b>後")).toEqual([
      { text: "前", b: false, mono: false },
      { text: "後", b: true, mono: false },
    ]);
    expect(parseRich("前</b>後")).toEqual([{ text: "前後", b: false, mono: false }]);
  });
});
