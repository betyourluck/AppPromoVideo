import { describe, expect, it } from "vitest";
import { baseName, isImagePath, mergePaths, stripDataUrl } from "./snapshots";

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
