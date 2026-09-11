import { describe, expect, it } from "vitest";
import indexHtml from "../index.html?raw";
import mainTs from "./main.ts?raw";
import mainCss from "./assets/main.css?raw";
import interCss from "@fontsource-variable/inter/index.css?raw";
import notoCss from "@fontsource-variable/noto-sans-jp/index.css?raw";

/**
 * rev30: UI の書体は**同梱する** (Inter / Noto Sans JP、@fontsource-variable)。外から読まない。
 *
 * デザインの修正で index.html に Google Fonts の <link> が入ったが、tauri.conf.json の CSP は
 * style-src に fonts.googleapis.com を、font-src (未指定 → default-src 'self') に fonts.gstatic.com を含まず、
 * 読めない。この機体には Inter が入っていないので、指定した書体は一度も表示されない形だった。
 * 同梱すればオフラインでも同じ見た目になり、起動のたびに外へ通信しない (ユーザー判断 2026-09-11)。
 *
 * これは**機械の網**: 外部の読み込みに戻したり、トークンの書体名がパッケージの宣言名とずれたりしたら落ちる
 * (@fontsource-variable の宣言名は `Inter Variable` で、`Inter` と書くと手元に無い限り当たらない)。
 */

/** @font-face が宣言する font-family 名の集合。 */
function declaredFamilies(css: string): Set<string> {
  return new Set([...css.matchAll(/font-family:\s*['"]([^'"]+)['"]/g)].map((m) => m[1]));
}

/** トークン (`--font-ui: "A", "B", system-ui`) の書体名を並び順のまま。 */
function tokenFamilies(css: string, token: string): string[] {
  const m = css.match(new RegExp(`${token}:\\s*([^;]+);`));
  if (!m) return [];
  return m[1].split(",").map((s) => s.trim().replace(/^['"]|['"]$/g, ""));
}

// <link href> / <script src> / @import に書かれた外部 URL。
const EXTERNAL = /(?:href|src)\s*=\s*["']https?:\/\/|@import\s+(?:url\()?["']?https?:\/\//;

describe("UI の書体", () => {
  it("index.html は外部の CSS・フォント・スクリプトを読まない (CSP で止まり、オフラインでも読めない)", () => {
    const hits = indexHtml.split("\n").filter((line) => EXTERNAL.test(line)).map((l) => l.trim());
    expect(hits).toEqual([]);
  });

  it("main.ts が同梱フォントを読み込む", () => {
    expect(mainTs).toMatch(/import\s+["']@fontsource-variable\/inter["']/);
    expect(mainTs).toMatch(/import\s+["']@fontsource-variable\/noto-sans-jp["']/);
  });

  it("--font-ui の先頭は同梱の Inter → Noto Sans JP、--font-chrome の先頭は同梱の Inter", () => {
    const [inter] = [...declaredFamilies(interCss)];
    const [noto] = [...declaredFamilies(notoCss)];
    expect(tokenFamilies(mainCss, "--font-ui").slice(0, 2)).toEqual([inter, noto]);
    expect(tokenFamilies(mainCss, "--font-chrome")[0]).toBe(inter);
  });

  it("網そのものが効く — 実物を読んでいて、宣言名とトークンと外部 URL を見分ける", () => {
    // 読み込みが空だと (vitest は既定で CSS を空にする)、上のテストは誤って通るか誤って落ちる。
    expect(indexHtml).toContain('<div id="app">');
    expect(mainTs).toContain("createApp");
    expect(mainCss).toContain("--font-ui:");
    expect([...declaredFamilies(interCss)]).toEqual(["Inter Variable"]);
    expect([...declaredFamilies(notoCss)]).toEqual(["Noto Sans JP Variable"]);

    expect(tokenFamilies(':root { --font-ui: "Inter Variable", "Noto Sans JP", system-ui; }', "--font-ui")).toEqual([
      "Inter Variable",
      "Noto Sans JP",
      "system-ui",
    ]);
    expect(EXTERNAL.test('<link href="https://fonts.googleapis.com/css2?family=Inter" rel="stylesheet" />')).toBe(true);
    expect(EXTERNAL.test('<script type="module" src="/src/main.ts"></script>')).toBe(false);
    expect(EXTERNAL.test('@import url("https://fonts.googleapis.com/css2");')).toBe(true);
  });
});
