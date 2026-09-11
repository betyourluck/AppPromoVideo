import { describe, expect, it } from "vitest";

/**
 * rev26: ブラウザ標準の確認・通知ダイアログを使わない。
 *
 * WebView の標準ダイアログは見出しに**配信元の URL** (`localhost:1421 の内容`) を出し、
 * アプリの見た目からも外れる (ユーザー報告 2026-09-11、スクリーンショットつき)。
 * 確認は `dialog.ts` の `ask` と `MessageBox.vue` (Web モーダル) を通す。
 *
 * これは**機械の網**: 新しく書いた画面が標準ダイアログに戻っても、ここで落ちる。
 *
 * ソースは Vite の `import.meta.glob` (`?raw`) で文字列として読む。初版は `node:fs` で読んだが、
 * build の型検査 (vue-tsc) がテストも対象にしており、Node の型定義が無いので落ちた。
 */
const files = import.meta.glob<string>("./**/*.{ts,vue}", { query: "?raw", import: "default", eager: true });

const sources = Object.entries(files).filter(([path]) => !path.endsWith(".test.ts"));

// `store.confirm(` のようなメソッドは対象外 (直前が `.` なら除く)。`window.confirm(` は対象。
// 名前と括弧の間に空白を許さない — 画面の文言「video prompt (text-to-video fallback)」に反応していた。
const BROWSER_DIALOG = /(?<![\w.$])(?:window\.)?(confirm|alert|prompt)\(/;

describe("ブラウザ標準のダイアログ", () => {
  it("src のどこからも呼ばない (URL が見出しに出るため)", () => {
    const hits: string[] = [];
    for (const [path, text] of sources) {
      text.split("\n").forEach((line, i) => {
        if (BROWSER_DIALOG.test(line)) hits.push(`${path}:${i + 1}: ${line.trim()}`);
      });
    }
    expect(hits).toEqual([]);
  });

  it("網そのものが効く — ファイルを実際に読んでいて、呼び出しと文言を見分ける", () => {
    // glob が何も拾わないと、上のテストは常に空で通ってしまう。
    expect(sources.map(([p]) => p)).toContain("./components/CaptionEditor.vue");
    expect(sources.map(([p]) => p)).not.toContain("./noBrowserDialogs.test.ts");
    expect(BROWSER_DIALOG.test('if (!confirm("x")) return;')).toBe(true);
    expect(BROWSER_DIALOG.test('window.alert("x")')).toBe(true);
    expect(BROWSER_DIALOG.test("prompt('x')")).toBe(true);
    expect(BROWSER_DIALOG.test("await store.confirm(x)")).toBe(false);
    expect(BROWSER_DIALOG.test("const confirmed = ask(opts)")).toBe(false);
    // 画面の文言は呼び出しではない (初版の網が実際に誤検出した行)。
    expect(BROWSER_DIALOG.test("<summary>video prompt (text-to-video fallback)</summary>")).toBe(false);
  });
});
