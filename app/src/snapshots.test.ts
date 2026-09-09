import { describe, expect, it } from "vitest";
import { baseName, isImagePath, mergePaths, stripDataUrl, snapshotChoices } from "./snapshots";

describe("snapshots", () => {
  it("画像以外は落とし、重複は 1 つにし、順序を保つ", () => {
    const r = mergePaths(["a.png"], ["b.JPG", "a.png", "notes.txt", "c.webp", "b.JPG"]);
    expect(r.next).toEqual(["a.png", "b.JPG", "c.webp"]);
    expect(r.skipped).toEqual(["notes.txt"]);
    expect(isImagePath("D:\\x\\Shot.PNG")).toBe(true);
    expect(isImagePath("x.gif")).toBe(false);
  });
  it("data URL から base64 本体を取る", () => {
    expect(stripDataUrl("data:image/png;base64,AAAA")).toEqual({ mime: "image/png", base64: "AAAA" });
    expect(stripDataUrl("data:text/plain;base64,AAAA")).toBeNull();
    expect(stripDataUrl("AAAA")).toBeNull();
  });
  it("baseName は区切りを両方受ける", () => {
    expect(baseName("D:\\a\\b.png")).toBe("b.png");
    expect(baseName("/x/y/z.jpg")).toBe("z.jpg");
  });
});

describe("snapshotChoices", () => {
  it("run の中のものと、入力ペインで後から足したものを 1 つの一覧にする", () => {
    // rev20: 従来は run 作成時 (最初に解析を押した時) の一覧しか選べなかった。
    const got = snapshotChoices(["a.png", "b.png"], ["a.png", "b.png", "c.png"]);
    expect(got).toEqual([
      { index: 0, path: "a.png", inRun: true },
      { index: 1, path: "b.png", inRun: true },
      { index: null, path: "c.png", inRun: false },
    ]);
  });

  it("入力ペインから外されても、run に写したものは消えない", () => {
    // run は自己完結する (rev10)。入力の一覧はこの run の履歴ではない。
    const got = snapshotChoices(["a.png", "b.png"], ["c.png"]);
    expect(got.map((c) => c.path)).toEqual(["a.png", "b.png", "c.png"]);
    expect(got.filter((c) => c.inRun).length).toBe(2);
  });

  it("並びは run の番号順で、後から足したぶんが下に付く", () => {
    const got = snapshotChoices(["b.png"], ["z.png", "b.png", "a.png"]);
    expect(got.map((c) => c.path)).toEqual(["b.png", "z.png", "a.png"]);
    expect(got.map((c) => c.index)).toEqual([0, null, null]);
  });

  it("入力ペインが空でも run の一覧はそのまま出る", () => {
    expect(snapshotChoices(["a.png"], [])).toEqual([{ index: 0, path: "a.png", inRun: true }]);
  });
});
