import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

/**
 * rev31: 履歴は全画面になり、「開く」が**成功したら**メイン画面へ戻る。失敗したら履歴画面に残ってエラーを出す
 * (履歴画面ではログのペインも入力ペインのエラー表示も見えない)。
 * そのために `openRun` は成否を返す。画面の切り替えそのものは DOM の無い vitest では確かめられない。
 */
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { useStore } from "./store";

describe("store.openRun", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    invoke.mockReset();
  });

  it("読めたら true を返し、結果ペインに run を戻す", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "open_run") return { promo: { summary: { app_name: "Demo" } }, package_dir: "D:/pkg/runs/1", images: [] };
      throw new Error(`unexpected ${cmd}`);
    });
    const store = useStore();
    expect(await store.openRun("D:/pkg/runs/1")).toBe(true);
    expect(store.result?.package_dir).toBe("D:/pkg/runs/1");
    expect(store.error).toBe("");
  });

  it("読めなければ false を返し、エラーを残し、今の結果は変えない", async () => {
    invoke.mockRejectedValue(new Error("promo.json が見つかりません"));
    const store = useStore();
    expect(await store.openRun("D:/gone")).toBe(false);
    expect(store.error).toContain("promo.json が見つかりません");
    expect(store.result).toBeNull();
  });
});
