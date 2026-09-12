/**
 * 初回起動のナビゲーション (rev44、ユーザー要望) の**純粋な芯**。
 *
 * 移植元は Lorekeel (Kataribe `app/src/tour.ts`、2026-09-07)。`spotlightBox` / `placeCard` は
 * あちらで実機に当てて詰めた判断なので**そのまま写す**。変えたのは ①歩数と対象 ②「初回」の
 * 見分け方 ③**複数の要素をまとめて照らす** (`spotlightUnion`) — 入力ペインは「実行ボタン以外を
 * まとめて囲む」ので、要素を包む div を足さずに済ませたかった (包むと余白が変わる)。
 *
 * ここに置くのは DOM に依らない判断だけ = 提示層で唯一テストできる部分。
 * 描画そのもの (要素の測定・トランジション) は FirstRunTour.vue。
 */

/** 見たかどうかの永続キー。`apppromo.` 接頭辞なので設定ミラー (settings.json) の射程に入る。 */
export const TOUR_DONE_KEY = "apppromo.tourDone";

/** 4 歩の対象。`data-tour` 属性の値で、実物の要素に付いている。順序が案内の順序。 */
export const TOUR_STEPS = ["input", "settings", "run", "result"] as const;
export type TourStep = (typeof TOUR_STEPS)[number];

/**
 * 出すかどうか。**初回だけ** = 見た印が無く、かつ既に使っている痕跡も無いとき。
 *
 * 更新で初めてこの機構が入った人は、リポジトリを選んでいるか run を持っている。
 * その人に出すと「知っている手順を教えられる」形になるので、痕跡があれば出さない
 * (印を立てるのは呼び出し側の責務)。
 */
export function shouldShowTour(input: { done: boolean; hasProject: boolean; runCount: number }): boolean {
  if (input.done) return false;
  return !input.hasProject && input.runCount === 0;
}

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** 対象の矩形を `pad` だけ広げる (負の座標には出さない = 画面外へ滲まない)。 */
export function spotlightBox(target: Box, pad: number): Box {
  const left = Math.max(0, target.left - pad);
  const top = Math.max(0, target.top - pad);
  return {
    left,
    top,
    width: target.width + (target.left - left) + pad,
    height: target.height + (target.top - top) + pad,
  };
}

/**
 * 複数の矩形をひとつに束ねる (rev44)。
 *
 * 入力ペインは「実行ボタン**以外**をまとめて囲む」ので、先頭の欄と末尾の行に印を付けて
 * その間を覆う。**包む div を足さない**ため — 足すと余白の出方が変わる。
 * 空の入力 (対象が 1 つも見つからない) では null を返す = 呼ぶ側が代替の置き場を決める。
 */
export function spotlightUnion(boxes: Box[]): Box | null {
  const seen = boxes.filter((b) => b.width > 0 && b.height > 0);
  if (seen.length === 0) return null;
  const left = Math.min(...seen.map((b) => b.left));
  const top = Math.min(...seen.map((b) => b.top));
  const right = Math.max(...seen.map((b) => b.left + b.width));
  const bottom = Math.max(...seen.map((b) => b.top + b.height));
  return { left, top, width: right - left, height: bottom - top };
}

export type CardSide = "below" | "above" | "right" | "left";

export interface CardPlacement {
  left: number;
  top: number;
  side: CardSide;
  /**
   * 対象を指す三角の位置 (カードの辺に沿った px)。下/上なら左端からの x、右/左なら上端からの y。
   * カードを画面内へ寄せても三角は**対象の中心**を指し続ける (固定位置だと寄せた瞬間に
   * 何も指さなくなる)。カードの角には掛からないよう 18px 内側に収める。
   */
  arrow: number;
}

/**
 * カードの置き場。対象の**下 → 上 → 右 → 左**の順に、カードが画面に収まる最初の側を選ぶ。
 * どの側にも収まらなければ下に置いて画面内へ押し戻す (小さい窓でも消えない)。
 */
export function placeCard(
  spot: Box,
  viewport: { width: number; height: number },
  card: { width: number; height: number },
  gap: number,
): CardPlacement {
  const clampX = (x: number) => Math.min(Math.max(8, x), Math.max(8, viewport.width - card.width - 8));
  const clampY = (y: number) => Math.min(Math.max(8, y), Math.max(8, viewport.height - card.height - 8));
  const cx = spot.left + spot.width / 2;
  const cy = spot.top + spot.height / 2;
  const arrowX = (left: number) => Math.min(Math.max(18, cx - left), Math.max(18, card.width - 18));
  const arrowY = (top: number) => Math.min(Math.max(18, cy - top), Math.max(18, card.height - 18));

  const belowTop = spot.top + spot.height + gap;
  if (belowTop + card.height <= viewport.height) {
    const left = clampX(spot.left);
    return { left, top: belowTop, side: "below", arrow: arrowX(left) };
  }
  const aboveTop = spot.top - gap - card.height;
  if (aboveTop >= 0) {
    const left = clampX(spot.left);
    return { left, top: aboveTop, side: "above", arrow: arrowX(left) };
  }
  const rightLeft = spot.left + spot.width + gap;
  if (rightLeft + card.width <= viewport.width) {
    const top = clampY(spot.top);
    return { left: rightLeft, top, side: "right", arrow: arrowY(top) };
  }
  const leftLeft = spot.left - gap - card.width;
  if (leftLeft >= 0) {
    const top = clampY(spot.top);
    return { left: leftLeft, top, side: "left", arrow: arrowY(top) };
  }
  const left = clampX(spot.left);
  return { left, top: clampY(belowTop), side: "below", arrow: arrowX(left) };
}
