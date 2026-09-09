import { describe, expect, it } from "vitest";
import { describeAttempts } from "./runs";

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
