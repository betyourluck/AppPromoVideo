import { beforeEach, describe, expect, it } from "vitest";
import { answer, ask, isMessageBoxOpen, messageBox, resetMessageBox } from "./dialog";

describe("ask / answer (rev26)", () => {
  beforeEach(() => resetMessageBox());

  it("答えるまで出ていて、OK なら true で閉じる", async () => {
    const p = ask({ message: "閉じますか？" });
    expect(isMessageBoxOpen()).toBe(true);
    expect(messageBox.current).toEqual({ message: "閉じますか？", title: "確認", ok: "OK", cancel: "キャンセル", danger: false });
    answer(true);
    await expect(p).resolves.toBe(true);
    expect(isMessageBoxOpen()).toBe(false);
  });

  it("キャンセルなら false", async () => {
    const p = ask({ message: "削除しますか？", ok: "削除する", danger: true });
    expect(messageBox.current?.ok).toBe("削除する");
    expect(messageBox.current?.danger).toBe(true);
    answer(false);
    await expect(p).resolves.toBe(false);
  });

  it("2 つ目は 1 つ目に答えるまで出さず、答えた順に返す", async () => {
    const first = ask({ message: "一" });
    const second = ask({ message: "二" });
    expect(messageBox.current?.message).toBe("一");
    answer(false);
    await expect(first).resolves.toBe(false);
    expect(messageBox.current?.message).toBe("二");
    answer(true);
    await expect(second).resolves.toBe(true);
    expect(isMessageBoxOpen()).toBe(false);
  });

  it("何も出ていない時の answer は何もしない", () => {
    expect(() => answer(true)).not.toThrow();
    expect(isMessageBoxOpen()).toBe(false);
  });
});
