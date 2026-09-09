/** スナップショット入力の純関数 (ドロップ / 貼り付け / 一覧)。IO は store。 */

export const SNAPSHOT_EXT = ["png", "jpg", "jpeg", "webp"];

/** 画像として受ける拡張子か (大文字小文字を区別しない)。 */
export function isImagePath(p: string): boolean {
  const ext = p.split(".").pop()?.toLowerCase() ?? "";
  return SNAPSHOT_EXT.includes(ext);
}

/** 既存に無いものだけを順に足す (重複はパス完全一致)。画像以外は落として返す。 */
export function mergePaths(existing: string[], incoming: string[]): { next: string[]; skipped: string[] } {
  const next = [...existing];
  const skipped: string[] = [];
  for (const p of incoming) {
    if (!isImagePath(p)) {
      skipped.push(p);
      continue;
    }
    if (!next.includes(p)) next.push(p);
  }
  return { next, skipped };
}

/** `data:image/png;base64,....` から base64 本体だけを取る。形が違えば null。 */
export function stripDataUrl(dataUrl: string): { mime: string; base64: string } | null {
  const m = /^data:(image\/[a-zA-Z0-9.+-]+);base64,(.+)$/s.exec(dataUrl);
  if (!m) return null;
  return { mime: m[1], base64: m[2] };
}

/** clipboardData / DataTransfer の items から画像 File を拾う。 */
export function imageFilesFrom(items: DataTransferItemList | null | undefined): File[] {
  const out: File[] = [];
  if (!items) return out;
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (it.kind === "file" && it.type.startsWith("image/")) {
      const f = it.getAsFile();
      if (f) out.push(f);
    }
  }
  return out;
}

export function baseName(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}

/** はめ込みに使える 1 枚 (rev20)。 */
export interface SnapshotChoice {
  /** run の中での番号。`null` = **まだ run に写していない** (選ばれた時に写す)。 */
  index: number | null;
  path: string;
  inRun: boolean;
}

/**
 * はめ込みで選べる一覧を組み立てる (rev20)。
 *
 * 従来は run を作った時 (最初に「解析」を押した時) の一覧しか選べなかった。コピー文に合う画面が
 * 無いときは**入力ペインに足せばそこから選べる**のが自然、というユーザー判断 (2026-09-09)。
 *
 * 並びは **run の番号順が先**で、入力ペインで後から足したぶんが下に付く。
 * **入力ペインから外されても run のぶんは消さない** — run は自己完結していて (rev10)、
 * 入力の一覧はこの run の履歴ではないから。
 */
export function snapshotChoices(runPaths: string[], inputPaths: string[]): SnapshotChoice[] {
  const inRun = new Set(runPaths);
  return [
    ...runPaths.map((path, index) => ({ index, path, inRun: true })),
    ...inputPaths.filter((p) => !inRun.has(p)).map((path) => ({ index: null, path, inRun: false })),
  ];
}
