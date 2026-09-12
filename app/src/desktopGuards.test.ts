import { describe, expect, it } from "vitest";

const files = import.meta.glob<string>("./**/*.{ts,css,vue}", { query: "?raw", import: "default", eager: true });
const main = files["./main.ts"];
const css = files["./assets/main.css"];

/**
 * 配布ビルドの締め (rev47、ユーザー要望)。ブラウザ由来の操作をアプリから隠す。
 *
 * **dev では締めない** — 右クリック → 検証と再読み込みは開発の道具。判別は `import.meta.env.DEV`。
 * これは機械の網: 締めが外れたり、dev まで巻き込んだらここで落ちる。
 */
describe("配布ビルドの締め", () => {
  it("締めは dev では効かせない", () => {
    expect(main).toContain("if (!import.meta.env.DEV)");
    // 締めの中身がすべて条件の内側にあること (条件より前に書いたら dev でも効いてしまう)。
    const guard = main.indexOf("if (!import.meta.env.DEV)");
    expect(main.indexOf("contextmenu")).toBeGreaterThan(guard);
    expect(main.indexOf('"F5"')).toBeGreaterThan(guard);
    expect(main.indexOf("dataset.locked")).toBeGreaterThan(guard);
  });

  it("右クリックを抑止する。ただし入力欄と選択中テキストの上では残す", () => {
    expect(main).toContain("contextmenu");
    expect(main).toContain('closest("input, textarea, [contenteditable]")');
    expect(main).toContain("getSelection()");
  });

  /** 右クリックの「更新」を潰しても、F5 / Ctrl+R は WebView2 のアクセラレータで通る。 */
  it("F5 と Ctrl+R を抑止する", () => {
    expect(main).toMatch(/e\.key === "F5"/);
    expect(main).toMatch(/e\.key === "r"/);
  });

  it("文字を選べなくする印を立てる", () => {
    expect(main).toContain("dataset.locked");
    expect(css).toContain(":root[data-locked]");
    expect(css).toMatch(/:root\[data-locked\]\s*\{[^}]*user-select:\s*none/);
  });

  /**
   * **選べるままにするもの。** ここを塞ぐと「ログを貼って報告する」導線ごと潰れる。
   * 入力欄・プロンプト (pre)・エラー文 (.warn)・進捗ログ (.selectable)。
   */
  it("入力欄・プロンプト・エラー文・ログは選べるまま", () => {
    for (const sel of ["input", "textarea", "[contenteditable]", "pre", ".warn", ".selectable"]) {
      expect(css, `${sel} が免除されていない`).toContain(`:root[data-locked] ${sel}`);
    }
    expect(files["./components/LogPanel.vue"]).toContain('class="log selectable"');
  });

  /** 網自身の検出力: ソースを読めていなければ、どの assert も空文字を見て落ちる。 */
  it("ソースを実際に読めている", () => {
    expect(main.length).toBeGreaterThan(200);
    expect(css.length).toBeGreaterThan(1000);
  });
});
