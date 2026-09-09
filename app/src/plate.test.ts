import { describe, expect, it } from "vitest";
import { tiltLabel, tiltValue } from "./plate";

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
