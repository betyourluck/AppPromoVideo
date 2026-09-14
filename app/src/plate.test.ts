import { describe, expect, it } from "vitest";
import { plateSnapshotIndex, snapshotChipLabel, tiltLabel, tiltValue } from "./plate";

const scene = (cut_kind: "product" | "mood", snapshot_index: number | null) => ({ cut_kind, snapshot_index });
const plate = (snapshot_index: number | null) => ({ snapshot_index });

describe("plateSnapshotIndex (rev55、backend の plate_snapshot_index と同じケース)", () => {
  // crates/pipeline/src/reference.rs の a_mood_cut_takes_a_plate_only_when_a_person_adds_one と同じ 4 ケース。
  it("mood は人が足した時だけ貼る", () => {
    expect(plateSnapshotIndex(scene("mood", null), null)).toBe(null);
    expect(plateSnapshotIndex(scene("mood", null), plate(2))).toBe(2);
    expect(plateSnapshotIndex(scene("mood", null), plate(null))).toBe(null);
    expect(plateSnapshotIndex(scene("mood", 1), null)).toBe(null);
  });

  // 同 a_product_cut_keeps_the_existing_precedence と同じ 4 ケース。
  it("product は人の選び直しが LLM に勝ち、どちらも無ければ 0", () => {
    expect(plateSnapshotIndex(scene("product", 1), null)).toBe(1);
    expect(plateSnapshotIndex(scene("product", 1), plate(3))).toBe(3);
    expect(plateSnapshotIndex(scene("product", null), null)).toBe(0);
    expect(plateSnapshotIndex(scene("product", 2), { yaw_degrees: 10 })).toBe(2);
  });
});

describe("snapshotChipLabel (rev55、結果ペインの chip)", () => {
  // 2026-09-14 実データ: run 20260912-035429 の scene 3 は chip が「snap 0」、編集ダイアログは「3 枚目」、
  // 実際に貼ったのも 3 枚目 (index 2)。
  it("選び直した番号を 1 始まりで出す", () => {
    expect(snapshotChipLabel(scene("product", 0), plate(2))).toBe("product · snap 3");
    expect(snapshotChipLabel(scene("product", null), null)).toBe("product · snap 1");
  });

  it("mood は面を足した時だけ番号を出す", () => {
    expect(snapshotChipLabel(scene("mood", null), plate(1))).toBe("mood · snap 2");
    expect(snapshotChipLabel(scene("mood", null), null)).toBe("mood");
  });
});

describe("傾きスライダーの基準 (rev21)", () => {
  it("触っていない時は LLM が書いた傾きを見せる — 0° ではない", () => {
    // 従来の欠陥: つまみは 0° にあるのに、実際に焼かれる絵は 18° 傾いていた。
    expect(tiltValue(null, 18)).toBe(18);
    expect(tiltLabel(null, 18)).toBe("18° (LLM)");
    expect(tiltLabel(null, -12)).toBe("-12° (LLM)");
  });

  it("0° は正面。LLM が傾きを書いていないシーンはそう見せる", () => {
    expect(tiltValue(null, 0)).toBe(0);
    expect(tiltLabel(null, 0)).toBe("0° (正面)");
  });

  it("自分で動かした値はそのまま出す (LLM の印は付けない)", () => {
    expect(tiltValue(6, 18)).toBe(6);
    expect(tiltLabel(6, 18)).toBe("6°");
    // 自分で 0 にしたときも「正面」と分かるようにする。
    expect(tiltLabel(0, 18)).toBe("0° (正面)");
  });

  it("LLM の値と同じ数値を自分で選んでも、表示は自分の値として出る", () => {
    expect(tiltLabel(18, 18)).toBe("18°");
  });
});
