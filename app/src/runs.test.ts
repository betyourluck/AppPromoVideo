import { describe, expect, it } from "vitest";
import { describeAttempts, pinCompared, runHeaderStats } from "./runs";

describe("runHeaderStats (rev36、結果ペインの見出しの数値)", () => {
  // ユーザー報告 2026-09-12 (スクリーンショット): 履歴から run を開くと「Passed on attempt 0」「0.000 USD」と出た。
  // 開いた run では stage の値が無いので 0 を描いていた。正本は promo.json の run_stats (rev15)。
  const zero = { attempts: 0, cost_usd: 0, duration_ms: 0, violations: [] };
  const stage = (attempts: number, cost_usd: number, duration_ms: number) => ({ attempts, cost_usd, duration_ms, violations: [] });
  // 実物 (D:/PV/.../runs/20260912-035429/promo.json) の値。
  const stats = { plan_attempts: 3, violation_kinds: ["product_backdrop_draws_screen"], cost_usd: 1.6736855 };

  it("開いた run は promo.json の記録から出す", () => {
    const got = runHeaderStats(stats, zero, zero);
    expect(got.attempts.text).toBe("3");
    expect(got.attempts.title).toBe("3 回目で通過 — product_backdrop_draws_screen");
    expect(got.cost).toBe("1.674");
    expect(got.durationsKnown).toBe(false);
  });

  it("記録の無い run (rev14 以前) は 0 ではなく「—」", () => {
    const got = runHeaderStats(null, zero, zero);
    expect(got.attempts.text).toBe("—");
    expect(got.cost).toBe("—");
    expect(got.durationsKnown).toBe(false);
  });

  it("実行直後は経過時間も出せる。費用は正本 (run_stats) を優先する", () => {
    const got = runHeaderStats(stats, stage(1, 0.6, 12000), stage(3, 1.07, 30000));
    expect(got.cost).toBe("1.674");
    expect(got.durationsKnown).toBe(true);
  });

  it("run_stats を持たない実行直後は stage の合計を出す", () => {
    const got = runHeaderStats(null, stage(1, 0.6, 12000), stage(2, 1.07, 30000));
    expect(got.attempts.text).toBe("2");
    expect(got.cost).toBe("1.670");
    expect(got.durationsKnown).toBe(true);
  });

  // rev50: ユーザーの疑問 (2026-09-13)「画像を生成していないのに費用?」。chip が何の費用かを書いていなかった。
  // 中身は解析 + 構成の LLM 費用で、参照画像の生成は含まない (data_contract の RunStats.cost_usd)。
  it("費用の chip は LLM の費用だと名乗り、画像生成を含まないと説明する", () => {
    const opened = runHeaderStats(stats, zero, zero);
    expect(opened.costLabel).toBe("LLM 1.674 USD");
    expect(opened.costTitle).toBe("解析と構成で LLM にかかった費用 (参照画像の生成は含まない)");
    // 実行直後は経過時間も説明に添える (以前はこちらだけが title だった)。
    const fresh = runHeaderStats(null, stage(1, 0.6, 12000), stage(2, 1.07, 30000));
    expect(fresh.costTitle).toBe("解析と構成で LLM にかかった費用 (参照画像の生成は含まない)\n解析 12000 ms / 構成 30000 ms");
  });
});

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

  // rev42: 費用を返さない CLI (agy) で走らせた run。**0.000 USD と描かない。**
  it("費用の記録が無ければ chip を出さない", () => {
    const noCost = { attempts: 1, cost_usd: null, duration_ms: 1000, violations: [] };
    const h = runHeaderStats(null, noCost, noCost);
    expect(h.costKnown).toBe(false);
    expect(h.cost).toBe("—");
    // 走ったこと自体 (duration) は分かるので、そちらは描く。
    expect(h.durationsKnown).toBe(true);
  });

  // 片方でも不明なら合計は不明 (backend の add_cost と同じ規律)。
  it("片方だけ費用が分かっても合計は出さない", () => {
    const known = { attempts: 1, cost_usd: 0.5, duration_ms: 1000, violations: [] };
    const unknown = { attempts: 1, cost_usd: null, duration_ms: 1000, violations: [] };
    expect(runHeaderStats(null, known, unknown).costKnown).toBe(false);
    expect(runHeaderStats(null, unknown, known).cost).toBe("—");
  });

  // 正本 (promo.json) に費用が無い run も同じ。
  it("正本に費用が無ければ chip を出さない", () => {
    const stats = { plan_attempts: 1, violation_kinds: [], cost_usd: null, models: null };
    const empty = { attempts: 0, cost_usd: null, duration_ms: 0, violations: [] };
    const h = runHeaderStats(stats, empty, empty);
    expect(h.attemptsKnown).toBe(true);
    expect(h.costKnown).toBe(false);
    expect(h.cost).toBe("—");
  });
});
