/**
 * rev49: 実行中の段 (中央ペインの実行中ブロック)。
 *
 * backend は進捗 event `promo-progress` の `stage` に brief / analyze / plan / images を入れる (lib.rs の `emit`)。
 * cli / ui / error の行は段ではない。段は**最後に出た段の行**で決まり、
 * 今の run の範囲 (最後の `ui` 行 = 実行開始 から後ろ) だけを見る — ログは run をまたいで残るので、
 * 前の run の `images` を今の段と読まないため。
 */
export const RUN_PHASES = ["brief", "analyze", "plan", "images"] as const;
export type RunPhase = (typeof RUN_PHASES)[number];
export type StepState = "done" | "active" | "todo";

function isPhase(s: string): s is RunPhase {
  return (RUN_PHASES as readonly string[]).includes(s);
}

/** 今の run の中で最後に出た段。無ければ null。 */
export function currentPhase(log: readonly { stage: string }[]): RunPhase | null {
  let start = 0;
  for (let i = log.length - 1; i >= 0; i--) {
    if (log[i].stage === "ui") {
      start = i;
      break;
    }
  }
  for (let i = log.length - 1; i >= start; i--) {
    const s = log[i].stage;
    if (isPhase(s)) return s;
  }
  return null;
}

/** 段の一覧と状態。段がまだ無ければ最初の段を active にする (固まって見せない)。 */
export function phaseSteps(current: RunPhase | null, imageEnabled: boolean): { phase: RunPhase; state: StepState }[] {
  const phases: readonly RunPhase[] = imageEnabled ? RUN_PHASES : RUN_PHASES.filter((p) => p !== "images");
  const idx = current == null ? 0 : Math.max(0, phases.indexOf(current));
  return phases.map((phase, i) => ({ phase, state: i < idx ? "done" : i === idx ? "active" : "todo" }));
}

/** 経過時間 `m:ss`。時計が戻っても負にしない。 */
export function elapsedLabel(startTs: number, nowTs: number): string {
  const total = Math.max(0, Math.floor((nowTs - startTs) / 1000));
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}
