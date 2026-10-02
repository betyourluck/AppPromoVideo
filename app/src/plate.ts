/**
 * はめ込みのつまみの**基準** (rev21)。
 *
 * 従来、傾きのスライダーは未指定のとき 0° に置かれていた。しかし実際に焼かれる傾きは
 * LLM が書いた `scene.plate_tilt` で、**つまみが 0° を指したまま絵は 18° 傾いている**ことがあった
 * (ユーザー指摘 2026-09-09)。つまみは**いま効いている値**を指すのが正しく、0° は正面を意味する。
 *
 * `fallback` は backend の `tilt_of`(= 合成が使う関数) が返した実効値。
 * `PlateMode::Frontal` や `plate_tilt` 無しでは 0 が来る = 本当に正面。
 */

import { t } from "./i18n";
import type { PlateOverride, Scene } from "./types";

/**
 * このシーンで貼るスクリーンショットの番号 (**0 始まり**、rev55)。`null` = 貼らない。
 *
 * **backend の `promo_core::export::plate_snapshot_index` と同じ規則。** Rust と TS に同じ式が 2 つあるので、
 * 両方のテストに同じケースを置いて食い違いを見張る (`plate.test.ts` / `crates/pipeline/src/reference.rs`)。
 * backend が番号を返しても、編集ダイアログの `hasPlate` は適用前の選び直しをその場で判定するのでこの式は残る —
 * 2 つのまま対のテストで見張る形で閉じた (ユーザー判断 2026-10-02)。
 * - product: 人の選び直しが LLM の指定に勝ち、どちらも無ければ 0
 * - mood: 人が足した時だけ貼る
 */
export function plateSnapshotIndex(
  scene: Pick<Scene, "cut_kind" | "snapshot_index">,
  plate: PlateOverride | null | undefined,
): number | null {
  const chosen = plate?.snapshot_index ?? null;
  return scene.cut_kind === "product" ? (chosen ?? scene.snapshot_index ?? 0) : chosen;
}

/**
 * 結果ペインの chip の文字 (rev55)。番号は**1 始まり** — ファイル名 `snapshots/snapshot_01` と
 * 編集ダイアログの「1 枚目」に揃える。以前は plan の番号を 0 始まりで出していて、選び直しも見ていなかった。
 */
export function snapshotChipLabel(
  scene: Pick<Scene, "cut_kind" | "snapshot_index">,
  plate: PlateOverride | null | undefined,
): string {
  const i = plateSnapshotIndex(scene, plate);
  return i === null ? scene.cut_kind : `${scene.cut_kind} · snap ${i + 1}`;
}

/** スライダーに置く値。未指定なら実効値 (LLM の傾き)。 */
export function tiltValue(override: number | null, fallback: number): number {
  return override ?? fallback;
}

/**
 * つまみの横に出す文字。**どこから来た値かまで見せる** —
 * 数字だけだと「自分で 18° にした」と「LLM が 18° と書いた」が区別できない。
 */
export function tiltLabel(override: number | null, fallback: number): string {
  const v = tiltValue(override, fallback);
  if (v === 0) return t("plate.frontal");
  return override === null ? `${v}° (LLM)` : `${v}°`;
}

/**
 * 大きさのつまみの基準 (rev57)。`plate_preview` が返す**実効の枠**と**等倍の位置**。
 * どちらも合成と同じ関数 (`layout_for` / `compose::native_screen_ratio`) から来る。
 */
export interface PlateBox {
  screen_ratio: number;
  /** スクショが等倍になる枠。面が無いシーンでは null。 */
  native_ratio: number | null;
}

/** 比の比較のゆとり。0.68 と 0.680 を別物と読まない。 */
const EPS = 0.005;

/**
 * スライダーに置く値 (rev57) = **いま効いている大きさ**。合成の縮尺は `min(枠 / スクショ, 1)` なので、
 * 拡大を許していない限り等倍 (`native_ratio`) より上には行かない。以前は既定を 0.78 と決め打ちし、
 * 帯ありの run (既定 0.68) でも、等倍で止まっている時も、効いていない値を指していた (#19 と同じ型)。
 */
export function sizeValue(ratio: number | null, upscale: boolean, box: PlateBox | null): number {
  if (!box) return ratio ?? 0.78;
  const r = ratio ?? box.screen_ratio;
  const n = box.native_ratio;
  if (n === null || (ratio !== null && upscale)) return r;
  return Math.min(r, n);
}

/**
 * つまみの横の文字 (rev57)。等倍で止まっているなら「等倍」、等倍を越えて引き伸ばしているなら
 * 「拡大 (ぼやける)」と出す — 数字だけでは、効いているのか・画素を引き伸ばしているのかが分からない。
 */
export function sizeLabel(ratio: number | null, upscale: boolean, box: PlateBox | null): string {
  const v = sizeValue(ratio, upscale, box);
  const head = ratio === null ? t("caption.default") : v.toFixed(2);
  const n = box?.native_ratio ?? null;
  if (n === null) return head;
  if (ratio !== null && upscale && ratio > n + EPS) return `${head} · ${t("caption.sizeUpscaled")}`;
  if (v >= n - EPS) return `${head} · ${t("caption.sizeNative")}`;
  return head;
}
