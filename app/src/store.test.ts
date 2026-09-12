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
import type { PromoJson, RunResult } from "./types";

describe("store.updateScenePrompts (rev37)", () => {
  // ユーザー 2026-09-12「motion や video prompt は書き換えが可能のほうがよい」。
  // 書き換えた後の promo は **backend が返す** (手元で真似ると promo.json と食い違う。rev21 と同じ作法)。
  const zeroStage = { attempts: 0, cost_usd: 0, duration_ms: 0, violations: [] };
  const promo = (motion: string) =>
    ({ plan: { scenes: [{ scene_id: 1, motion_prompt: motion, video_prompt: "v" }] } }) as unknown as PromoJson;
  const result = (p: PromoJson) =>
    ({ promo: p, package_dir: "D:/pkg/runs/1", analyze: zeroStage, plan: zeroStage, brief_chars: 0 }) as unknown as RunResult;

  beforeEach(() => {
    setActivePinia(createPinia());
    invoke.mockReset();
  });

  it("保存できたら true を返し、backend が返した promo に差し替える", async () => {
    invoke.mockResolvedValue(promo("Slow pan right."));
    const store = useStore();
    store.result = result(promo("old motion"));
    expect(await store.updateScenePrompts(1, "  Slow pan right.  ", "v")).toBe(true);
    expect(store.result?.promo.plan.scenes[0].motion_prompt).toBe("Slow pan right.");
    expect(invoke).toHaveBeenCalledWith("update_scene_prompts", {
      runDir: "D:/pkg/runs/1",
      sceneId: 1,
      motionPrompt: "  Slow pan right.  ",
      videoPrompt: "v",
    });
  });

  it("拒まれたら false を返し、promo は変えない (空は backend が拒む)", async () => {
    invoke.mockRejectedValue(new Error("motion prompt が空です"));
    const store = useStore();
    store.result = result(promo("old motion"));
    expect(await store.updateScenePrompts(1, "   ", "v")).toBe(false);
    expect(store.result?.promo.plan.scenes[0].motion_prompt).toBe("old motion");
    expect(store.error).toContain("motion prompt が空です");
  });

  it("run を開いていなければ何もしない", async () => {
    const store = useStore();
    expect(await store.updateScenePrompts(1, "m", "v")).toBe(false);
    expect(invoke).not.toHaveBeenCalled();
  });
});

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
