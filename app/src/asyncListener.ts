/**
 * 非同期に登録するリスナー (Tauri の `listen` / `onDragDropEvent`) を、部品の寿命に結びつける (rev60)。
 *
 * 登録は IPC の往復を待つので、終わる前に部品が外される (開発時の HMR で作り直される) ことがある。
 * 以前は外す関数がまだ null のまま外し損ね、リスナーが残り続けた — 画像を 1 回ドロップすると 4 枚並んだ (ユーザー報告 2026-10-03)。
 * ここでは「外す」を先に受け付けておき、登録が終わった時点で外されていれば**すぐ外す**。
 */
export function disposableListener(
  register: () => Promise<() => void>,
  onError: (e: unknown) => void = () => {},
): () => void {
  let disposed = false;
  let off: (() => void) | null = null;
  register().then((u) => {
    if (disposed) u();
    else off = u;
  }, onError);
  return () => {
    if (disposed) return;
    disposed = true;
    off?.();
    off = null;
  };
}
