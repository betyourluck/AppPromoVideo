/**
 * 画面の切り替え (rev31 履歴 / rev35 設定 / rev36 トグル)。
 *
 * タイトルバーの呼び出しアイコンは**トグル**: 開いている画面のアイコンをもう一度押すと「戻る」と同じ動きになる
 * (ユーザー 2026-09-12「直感的にその動きが欲しい」)。別の画面を開いている時は、メインを経由せずそちらへ切り替える。
 */
export type View = "main" | "runs" | "settings";

export function nextView(current: View, target: Exclude<View, "main">): View {
  return current === target ? "main" : target;
}
