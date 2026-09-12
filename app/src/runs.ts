/**
 * 履歴の行から導く純関数 (rev15)。
 *
 * 目的は表示の飾りではなく**数えられること**。2026-09-08 の実測で frontal の run だけ費用が
 * 約 1.5 倍 (1.422 USD 対 0.919 / 0.944) だったが、attempts を残していなかったので
 * 「ProductBackdropAngled で再生成が発火した」という推測を確認できなかった。
 */

import { t } from "./i18n";
import type { RunStats, StageInfo } from "./types";

export interface AttemptsView {
  /** 表に出す文字。記録が無ければ "—"。 */
  text: string;
  /** 2 回以上かかった (= 再生成が発火した)。強調に使う。 */
  retried: boolean;
  /** ホバーで出す説明。違反の種別があれば理由として添える。 */
  title: string;
}

/**
 * `plan_attempts` を表示に落とす。**0 と未定義は「1 回」ではなく「記録なし」**。
 * rev14 以前の行を 1 と描くと、集計に偽の分母が混ざる。
 */
/**
 * 比較中 (2 つ選んでいる時) は、比較対象を**選んだ順 (A → B) で表の先頭**に出す (rev32)。
 * 比較中は下に画像が並ぶので表に使える高さが限られ、下の方の run を選ぶとその行が見えなくなって
 * 外せなかった (ユーザー報告 2026-09-11、スクリーンショットつき)。比較していない時は並びを変えない。
 * 元の配列は書き換えない。
 */
export function pinCompared<T extends { run_dir: string }>(runs: T[], compare: string[]): T[] {
  if (compare.length < 2) return runs.slice();
  const pinned = compare.map((dir) => runs.find((r) => r.run_dir === dir)).filter((r): r is T => r !== undefined);
  return [...pinned, ...runs.filter((r) => !compare.includes(r.run_dir))];
}

export function describeAttempts(attempts: number, kinds: string[] = []): AttemptsView {
  if (!attempts) return { text: "—", retried: false, title: t("runs.noRecord") };
  const why = kinds.length ? ` — ${kinds.join(", ")}` : "";
  return { text: String(attempts), retried: attempts > 1, title: t("runs.passedOn", { n: attempts, why }) };
}

/** 結果ペインの見出しに出す数値 (rev36)。`*Known` が false の chip は**出さない** (無い記録を描かない)。 */
export interface RunHeaderStats {
  attempts: AttemptsView;
  attemptsKnown: boolean;
  cost: string;
  costKnown: boolean;
  durationsKnown: boolean;
}

/**
 * 見出しの数値は**正本 (promo.json の `run_stats`)** から出す (rev36)。
 *
 * 履歴から開いた run は実行時の stage (attempts / cost_usd / duration_ms) を持たないので、
 * それを描くと「0 回目で通過」「0.000 USD」という**存在しない記録**が出る (ユーザー報告 2026-09-12、スクリーンショット)。
 * 記録が無い run (rev14 以前) は 0 ではなく「—」— 0 を 1 回と描かない rev15 の規律と同じ。
 */
export function runHeaderStats(stats: RunStats | null | undefined, analyze: StageInfo, plan: StageInfo): RunHeaderStats {
  const durationsKnown = analyze.duration_ms + plan.duration_ms > 0;
  if (stats) {
    return {
      attempts: describeAttempts(stats.plan_attempts, stats.violation_kinds),
      attemptsKnown: stats.plan_attempts > 0,
      cost: stats.cost_usd.toFixed(3),
      costKnown: true,
      durationsKnown,
    };
  }
  const cost = analyze.cost_usd + plan.cost_usd;
  const known = durationsKnown || cost > 0 || plan.attempts > 0;
  return {
    attempts: describeAttempts(plan.attempts),
    attemptsKnown: plan.attempts > 0,
    cost: known ? cost.toFixed(3) : "—",
    costKnown: known,
    durationsKnown,
  };
}
