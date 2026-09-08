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
