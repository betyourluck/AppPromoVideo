import { describe, expect, it } from "vitest";
import { currentPhase, elapsedLabel, phaseSteps } from "./runPhase";

/**
 * rev49: 実行中に画面が固まって見える (ユーザー 2026-09-13、スクリーンショット)。
 * 中央ペインは実行中も「左でリポジトリと…」の案内のままで、ボタンの矢印も回らなかった (回転の定義が無かった)。
 * 段は backend の進捗 event の stage (brief → analyze → plan → images) から導く。cli / ui / error の行は段を動かさない。
 */
describe("currentPhase (進捗ログから今の段を導く)", () => {
  const line = (stage: string) => ({ stage, text: "" });

  it("段の行が無ければ null (実行開始の直後)", () => {
    expect(currentPhase([line("ui"), line("cli")])).toBeNull();
  });

  it("最後に出た段の行が今の段。cli / error の行は段を動かさない", () => {
    expect(currentPhase([line("ui"), line("brief"), line("cli"), line("analyze"), line("cli")])).toBe("analyze");
    expect(currentPhase([line("analyze"), line("plan"), line("error")])).toBe("plan");
  });

  it("知らない stage は無視する", () => {
    expect(currentPhase([line("brief"), line("mystery")])).toBe("brief");
  });

  it("前の run の段は見ない — 最後の ui 行 (実行開始) から後ろだけ", () => {
    expect(currentPhase([line("plan"), line("images"), line("ui"), line("cli")])).toBeNull();
    expect(currentPhase([line("images"), line("ui"), line("brief")])).toBe("brief");
  });
});

describe("phaseSteps (中央ペインの段の一覧)", () => {
  it("画像 off なら 3 段、今の段より前は done・今は active・後は todo", () => {
    expect(phaseSteps("analyze", false)).toEqual([
      { phase: "brief", state: "done" },
      { phase: "analyze", state: "active" },
      { phase: "plan", state: "todo" },
    ]);
  });

  it("画像 on なら images の段が末尾に足される", () => {
    expect(phaseSteps("plan", true).map((s) => s.phase)).toEqual(["brief", "analyze", "plan", "images"]);
    expect(phaseSteps("images", true)[3]).toEqual({ phase: "images", state: "active" });
  });

  it("段がまだ無ければ最初の段を active にする (固まって見せない)", () => {
    expect(phaseSteps(null, false)[0]).toEqual({ phase: "brief", state: "active" });
  });
});

describe("elapsedLabel (経過時間。アニメーションを切った環境でも動いていることが分かる)", () => {
  it("m:ss", () => {
    expect(elapsedLabel(1000, 1000)).toBe("0:00");
    expect(elapsedLabel(1000, 1000 + 9 * 1000)).toBe("0:09");
    expect(elapsedLabel(1000, 1000 + 262 * 1000)).toBe("4:22");
  });

  it("時計が戻っても負にならない", () => {
    expect(elapsedLabel(5000, 1000)).toBe("0:00");
  });
});
