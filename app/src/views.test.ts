import { describe, expect, it } from "vitest";
import { nextView } from "./views";

/**
 * rev36: タイトルバーの呼び出しアイコンは**トグル**。開いている画面のアイコンをもう一度押すとメインへ戻る
 * (ユーザー 2026-09-12「直感的にその動きが欲しい」)。
 */
describe("nextView", () => {
  it("メインからはその画面を開く", () => {
    expect(nextView("main", "runs")).toBe("runs");
    expect(nextView("main", "settings")).toBe("settings");
  });

  it("開いている画面のアイコンをもう一度押すとメインへ戻る (= 戻ると同じ)", () => {
    expect(nextView("runs", "runs")).toBe("main");
    expect(nextView("settings", "settings")).toBe("main");
  });

  it("別の画面を開いている時は、そちらへ切り替える (メインを経由しない)", () => {
    expect(nextView("runs", "settings")).toBe("settings");
    expect(nextView("settings", "runs")).toBe("runs");
  });
});
