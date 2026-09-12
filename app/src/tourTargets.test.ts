import { describe, expect, it } from "vitest";

import { TOUR_STEPS } from "./tour";
import { messages } from "./i18n";

/**
 * rev44: 案内の対象は `data-tour="…"` で実物の要素に付けた印。**部品同士は互いを知らない**ので、
 * 印を消しても案内は落ちず、画面中央に小さな丸が出るだけ = **静かに壊れる**。
 * これは機械の網: 歩に対する印がソースから消えたらここで落ちる。
 */
const files = import.meta.glob<string>("./**/*.vue", { query: "?raw", import: "default", eager: true });
const sources = Object.entries(files).filter(([path]) => !path.endsWith("FirstRunTour.vue"));
const blob = sources.map(([, text]) => text).join("\n");

describe("案内の対象", () => {
  it("すべての歩に印が付いた要素がある", () => {
    const missing = TOUR_STEPS.filter((s) => !blob.includes(`data-tour="${s}"`));
    expect(missing).toEqual([]);
  });

  /** 1 歩目は**実行ボタン以外をまとめて**囲むので、印が 2 つ要る (先頭の欄と末尾の行)。 */
  it("入力の歩は 2 か所に印がある (束ねて囲むため)", () => {
    expect(blob.split('data-tour="input"').length - 1).toBe(2);
  });

  /** 実行ボタンは 1 歩目に含めない (ユーザー指定)。 */
  it("実行ボタンの印は実行ボタンにだけ付く", () => {
    expect(blob.split('data-tour="run"').length - 1).toBe(1);
  });

  /** 網自身の検出力: ソースを読めていなければ「全部ある」でも「全部無い」でも通ってしまう。 */
  it("ソースを実際に読めている", () => {
    expect(sources.length).toBeGreaterThan(5);
    expect(blob).toContain("data-tour=");
  });
});

describe("案内の文言", () => {
  const KEYS = [
    "tour.welcomeTitle",
    "tour.welcomeLead",
    "tour.start",
    "tour.skip",
    "tour.next",
    "tour.back",
    "tour.finish",
    "tour.keys",
    ...TOUR_STEPS.flatMap((s) => [`tour.${s}.title`, `tour.${s}.body`]),
  ] as const;

  it("3 言語すべてにある", () => {
    for (const lang of ["ja", "en", "zh-CN"] as const) {
      for (const k of KEYS) expect(messages[lang][k as keyof (typeof messages)["ja"]], `${lang} / ${k}`).toBeTruthy();
    }
  });

  /** 案内の中から実行はさせない (ユーザー決定) ので、文言も「押してください」で終える。 */
  it("北極星を外さない — 動画そのものは作らないと書く", () => {
    expect(messages.ja["tour.welcomeLead"]).toContain("動画そのものは作りません");
    expect(messages.en["tour.welcomeLead"]).toContain("does not make the video");
  });
});
