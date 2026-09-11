import { describe, expect, it } from "vitest";

/**
 * rev30: 画面に出る文言は i18n.ts の辞書を通す。**コード中に日本語を直書きしない。**
 *
 * デザインの修正で UI を 3 言語 (ja / en / zh-CN) に切り替えられるようになったが、移したのは一部で、
 * 設定・履歴・シーン編集のダイアログ、トースト、確認ダイアログの既定値は日本語のままだった。
 * 英語を選んでも、そこだけ日本語で出る。
 *
 * これは**機械の網**: 新しく書いた文言が直書きに戻ると、ここで落ちる。
 * コメントは対象外 (台帳の言語は日本語)。対象外のファイルと理由は `EXEMPT` に書く。
 * backend (Rust) から来るログやエラーの文言はこの網の外 — 画面に出るが、ここでは数えない。
 */
const files = import.meta.glob<string>("./**/*.{ts,vue}", { query: "?raw", import: "default", eager: true });

const EXEMPT: Record<string, string> = {
  "./i18n.ts": "辞書そのもの",
  // 見出しフォントの自動選択で、実機のフォント名 (游ゴシック / メイリオ など) と照合するデータ。画面の文言ではない。
  "./settings.ts": "フォント名の照合データ",
};

const sources = Object.entries(files).filter(([path]) => !path.endsWith(".test.ts") && !(path in EXEMPT));

const JAPANESE = /[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]/u;

/** コメントを空白に置き換える (行番号を保つため改行は残す)。 */
export function stripComments(text: string): string {
  const blank = (m: string) => m.replace(/[^\n]/g, " ");
  return text
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/(^|[^:"'`\\])\/\/[^\n]*/g, (m, pre: string) => pre + blank(m.slice(pre.length)));
}

/** 直書きの日本語がある行 (`console.*` は開発者向けなので除く)。 */
function hardcoded(text: string): { line: number; text: string }[] {
  const out: { line: number; text: string }[] = [];
  stripComments(text)
    .split("\n")
    .forEach((line, i) => {
      if (JAPANESE.test(line) && !/\bconsole\.\w+\(/.test(line)) out.push({ line: i + 1, text: line.trim() });
    });
  return out;
}

describe("直書きの日本語", () => {
  it("画面の文言は i18n の辞書を通す (コメント以外に日本語を書かない)", () => {
    const hits = sources.flatMap(([path, text]) => hardcoded(text).map((h) => `${path}:${h.line}: ${h.text.slice(0, 80)}`));
    expect(hits).toEqual([]);
  });

  it("網そのものが効く — 実物を読んでいて、コメントと文言と開発者向けログを見分ける", () => {
    const paths = sources.map(([p]) => p);
    expect(paths).toContain("./components/SettingsDialog.vue");
    expect(paths).toContain("./store.ts");
    expect(paths).not.toContain("./i18n.ts");
    expect(files["./components/SettingsDialog.vue"]).toContain("<template>");

    // 文言は拾う。
    expect(hardcoded('  return props.snapshots.includes(path) ? "撮り直し" : "入力に追加";')).toHaveLength(1);
    expect(hardcoded("<p class=\"muted note\">まだ履歴がありません。</p>")).toHaveLength(1);
    // コメントは拾わない (JSDoc / 行末 / テンプレート)。行番号はずれない。
    const sample = ["/**", " * 撮り直しを数え直す", " */", 'const a = 1; // 既定', "<!-- 結果ペイン -->", 'const b = "表示";'].join("\n");
    expect(hardcoded(sample)).toEqual([{ line: 6, text: 'const b = "表示";' }]);
    // URL の `//` をコメントの始まりと取り違えない。
    expect(hardcoded('const u = "https://example.com"; const s = "表示";')).toHaveLength(1);
    // 開発者向けのログは対象外。
    expect(hardcoded('console.warn("[settingsMirror] 保存に失敗:", e);')).toEqual([]);
  });
});
