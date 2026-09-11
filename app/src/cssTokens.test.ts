import { describe, expect, it } from "vitest";

/**
 * rev30: 部品が参照するトークン (CSS 変数) は、どこかで定義されていること。
 *
 * フォールバックの無い `var(--x)` が未定義を指すと、その宣言ごと無効になり**黙って親の値に落ちる**
 * (ビルドも型検査も通る)。実例: デザインの修正で main.css から `--fs-h-xl` の定義が消え、
 * ScenePanel の結果のアプリ名 (`.app`) だけが見出しの大きさを失っていた。
 *
 * これは**機械の網**: トークンを撤去・改名した時、参照の追従漏れがここで落ちる。
 * ソースは noBrowserDialogs.test.ts と同じく `import.meta.glob` (`?raw`) で読む (`node:fs` は build の型検査で落ちる)。
 */
const files = import.meta.glob<string>("./**/*.{ts,vue,css}", { query: "?raw", import: "default", eager: true });

const sources = Object.entries(files).filter(([path]) => !path.endsWith(".test.ts"));

// 定義: `--name:` (main.css の :root / テーマ、部品の scoped style、インラインの style="--x: …")。
const DEFINITION = /(--[\w-]+)\s*:/g;
// 参照: フォールバックの無い `var(--name)` だけ。`var(--name, …)` は未定義でも値が残るので対象外。
const BARE_USE = /var\(\s*(--[\w-]+)\s*\)/g;

function definedTokens(texts: string[]): Set<string> {
  const out = new Set<string>();
  for (const text of texts) for (const m of text.matchAll(DEFINITION)) out.add(m[1]);
  return out;
}

describe("CSS トークン", () => {
  it("フォールバック無しで参照するトークンは、すべてどこかで定義されている", () => {
    const defined = definedTokens(sources.map(([, text]) => text));
    const hits: string[] = [];
    for (const [path, text] of sources) {
      text.split("\n").forEach((line, i) => {
        for (const m of line.matchAll(BARE_USE)) {
          if (!defined.has(m[1])) hits.push(`${path}:${i + 1}: ${m[1]}`);
        }
      });
    }
    expect(hits).toEqual([]);
  });

  it("網そのものが効く — main.css と部品を読んでいて、定義・参照・フォールバックを見分ける", () => {
    // glob が css を拾わないと、全参照が未定義になって上のテストが常に落ちる (逆に部品を拾わないと常に通る)。
    const paths = sources.map(([p]) => p);
    expect(paths).toContain("./assets/main.css");
    expect(paths).toContain("./components/ScenePanel.vue");
    expect(paths).not.toContain("./cssTokens.test.ts");
    // パスを拾っても中身が空なら同じく無力 (vitest は既定で CSS を処理せず、空の文字列を返しうる)。
    expect(files["./assets/main.css"]).toContain("--fs-scale:");

    const defined = definedTokens(["  --fs-h-lg: calc(15px * var(--fs-scale-heading));", '<div style="--local: 1px">']);
    expect(defined.has("--fs-h-lg")).toBe(true);
    expect(defined.has("--local")).toBe(true);
    // 参照 (`var(--fs-scale-heading)`) は定義に数えない。
    expect(defined.has("--fs-scale-heading")).toBe(false);

    const uses = (line: string) => [...line.matchAll(BARE_USE)].map((m) => m[1]);
    expect(uses("  font-size: var(--fs-h-xl);")).toEqual(["--fs-h-xl"]);
    expect(uses("  background: var(--backdrop, rgb(0 0 0 / 0.65));")).toEqual([]);
    expect(uses("  color: rgb(var(--muted) / 0.7);")).toEqual(["--muted"]);
  });
});
