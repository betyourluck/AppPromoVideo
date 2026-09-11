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
