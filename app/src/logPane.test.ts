import { describe, expect, it } from "vitest";

import { LOG_PANE_KEY, bodyColumns, loadLogPaneVisible, saveLogPaneVisible } from "./logPane";
import { messages } from "./i18n";

/**
 * rev66 (2026-10-15 ユーザー「右ペインの進捗は非表示にできるように。タイトルバーにトグルのアイコンを」)。
 */
function memory(init: Record<string, string> = {}) {
  const m = new Map(Object.entries(init));
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    map: m,
  };
}

describe("進捗ペインの出し入れ (rev66)", () => {
  it("既定は表示。hidden を保存した時だけ隠す", () => {
    expect(loadLogPaneVisible(memory())).toBe(true);
    expect(loadLogPaneVisible(memory({ [LOG_PANE_KEY]: "hidden" }))).toBe(false);
    expect(loadLogPaneVisible(memory({ [LOG_PANE_KEY]: "garbage" }))).toBe(true);
    expect(loadLogPaneVisible(null)).toBe(true);
    // storage が読めない環境 (例外) でも表示に倒す。
    expect(loadLogPaneVisible({ getItem: () => { throw new Error("blocked"); } })).toBe(true);
  });

  it("保存して読み戻すと同じ。書けなくても落ちない", () => {
    const s = memory();
    saveLogPaneVisible(s, false);
    expect(s.map.get(LOG_PANE_KEY)).toBe("hidden");
    expect(loadLogPaneVisible(s)).toBe(false);
    saveLogPaneVisible(s, true);
    expect(loadLogPaneVisible(s)).toBe(true);
    expect(() => saveLogPaneVisible({ setItem: () => { throw new Error("full"); } }, false)).not.toThrow();
  });

  it("隠すと列は 2 つになり、結果が広がる", () => {
    expect(bodyColumns(true)).toBe("390px 1fr 320px");
    expect(bodyColumns(false)).toBe("390px 1fr");
  });

  it("アイコンの説明が 3 言語にある (出す / 隠す)", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      expect(messages[lang]["titleBar.showLog"].length).toBeGreaterThan(0);
      expect(messages[lang]["titleBar.hideLog"].length).toBeGreaterThan(0);
    }
  });
});

/** 結線の網: タイトルバーにトグルがあり、App が列と表示をこのモジュールから取る (部品同士は互いを知らない)。 */
const files = import.meta.glob<string>("./**/*.vue", { query: "?raw", import: "default", eager: true });
describe("進捗ペインの結線 (rev66)", () => {
  it("タイトルバーがトグルを出し、出ている間は on", () => {
    const tb = files["./components/TitleBar.vue"];
    expect(tb).toContain("toggle-log");
    expect(tb).toMatch(/:class="\{ on: logVisible \}"/);
  });
  it("App が列を bodyColumns から取り、右ペインを v-show で残す", () => {
    const app = files["./App.vue"];
    expect(app).toContain("bodyColumns(logPaneVisible)");
    expect(app).toMatch(/<aside v-show="logPaneVisible" class="right">/);
    expect(app).toContain("@toggle-log=\"toggleLogPane\"");
  });
});
