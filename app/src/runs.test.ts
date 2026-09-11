import { describe, expect, it } from "vitest";
import { describeAttempts, pinCompared } from "./runs";

describe("pinCompared (rev32、比較中は比較対象を表の先頭に)", () => {
  // ユーザー報告 2026-09-11 (スクリーンショット): 比較中は表が縮んで上の 2 行しか見えず、
  // 下の方の run を比較に選ぶと、その行が見えなくなって外せなかった。
  const runs = [{ run_dir: "r1" }, { run_dir: "r2" }, { run_dir: "r3" }, { run_dir: "r4" }];

  it("2 つ選んでいれば、選んだ順 (A → B) で先頭に出し、残りは元の順", () => {
    expect(pinCompared(runs, ["r4", "r2"]).map((r) => r.run_dir)).toEqual(["r4", "r2", "r1", "r3"]);
  });

  it("比較していない時 (0 か 1 つ) は並びを変えない", () => {
    expect(pinCompared(runs, []).map((r) => r.run_dir)).toEqual(["r1", "r2", "r3", "r4"]);
    expect(pinCompared(runs, ["r3"]).map((r) => r.run_dir)).toEqual(["r1", "r2", "r3", "r4"]);
  });

  it("一覧に無い run_dir は飛ばし、元の配列は書き換えない", () => {
    expect(pinCompared(runs, ["gone", "r3"]).map((r) => r.run_dir)).toEqual(["r3", "r1", "r2", "r4"]);
    expect(runs.map((r) => r.run_dir)).toEqual(["r1", "r2", "r3", "r4"]);
  });
});

describe("describeAttempts", () => {
  it("記録が無い行は 1 回だと言わない", () => {
    // rev14 以前の 3 行は plan_attempts を持たない。0 を「1 回」と描くと、
    // 「このモードは再生成が起きやすいか」の集計に偽の分母が混ざる。
    expect(describeAttempts(0)).toEqual({ text: "—", retried: false, title: "記録なし (rev14 以前の run)" });
    expect(describeAttempts(undefined as unknown as number)).toEqual({
      text: "—",
      retried: false,
      title: "記録なし (rev14 以前の run)",
    });
  });

  it("一発で通った run と再生成した run を見分けられる", () => {
    expect(describeAttempts(1)).toMatchObject({ text: "1", retried: false });
    expect(describeAttempts(2)).toMatchObject({ text: "2", retried: true });
    expect(describeAttempts(3)).toMatchObject({ text: "3", retried: true });
  });

  it("違反の種別があれば理由として添える", () => {
    const got = describeAttempts(2, ["product_backdrop_angled", "motion_prompt_empty"]);
    expect(got.title).toBe("2 回目で通過 — product_backdrop_angled, motion_prompt_empty");
    expect(describeAttempts(1, []).title).toBe("1 回目で通過");
  });
});
