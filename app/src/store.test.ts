import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

/**
 * rev31: 履歴は全画面になり、「開く」が**成功したら**メイン画面へ戻る。失敗したら履歴画面に残ってエラーを出す
 * (履歴画面ではログのペインも入力ペインのエラー表示も見えない)。
 * そのために `openRun` は成否を返す。画面の切り替えそのものは DOM の無い vitest では確かめられない。
 */
const { invoke, listen } = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));

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

describe("store.addSnapshotPaths (rev60)", () => {
  // ユーザー報告 2026-10-03「画像をドロップすると 4 重になる」。重複の判定を await の前にしていたので、
  // 同じドロップが同時に届くと、どの呼び出しも「まだ一覧に無い」と判定して push していた (実測: 同時 4 回 → 4 枚 / 順番 → 1 枚)。
  beforeEach(() => {
    setActivePinia(createPinia());
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => {
      await new Promise((r) => setTimeout(r, 5)); // validate_snapshot は IPC 往復 = 必ず非同期
      return cmd === "validate_snapshot" ? { path: "x", width: 1, height: 1 } : {};
    });
  });

  it("同じパスで同時に何度呼ばれても 1 枚だけ", async () => {
    const store = useStore();
    store.project.snapshots = [];
    await Promise.all([1, 2, 3, 4].map(() => store.addSnapshotPaths(["D:/shots/a.png"])));
    expect(store.project.snapshots).toEqual(["D:/shots/a.png"]);
  });

  it("別のパスは同時でも全部入る", async () => {
    const store = useStore();
    store.project.snapshots = [];
    await Promise.all([store.addSnapshotPaths(["D:/a.png"]), store.addSnapshotPaths(["D:/b.png"])]);
    expect([...store.project.snapshots].sort()).toEqual(["D:/a.png", "D:/b.png"]);
  });
});

describe("store.listenProgress (rev61)", () => {
  // rev60 で見つけた同じ形: 「登録済みか見てから await」は、終わる前にもう一度呼ばれると 2 つ登録し、進捗の行が二重になる。
  beforeEach(() => {
    setActivePinia(createPinia());
    listen.mockReset();
  });

  it("登録が終わる前に何度呼ばれても、登録は 1 回", async () => {
    listen.mockImplementation(async () => {
      await new Promise((r) => setTimeout(r, 5)); // IPC 往復
      return () => {};
    });
    const store = useStore();
    await Promise.all([store.listenProgress(), store.listenProgress(), store.listenProgress()]);
    expect(listen).toHaveBeenCalledTimes(1);
    await store.listenProgress();
    expect(listen).toHaveBeenCalledTimes(1);
  });

  it("登録に失敗したら、次の呼び出しでやり直せる", async () => {
    listen.mockRejectedValueOnce(new Error("no tauri")).mockResolvedValueOnce(() => {});
    const store = useStore();
    await store.listenProgress();
    await store.listenProgress();
    expect(listen).toHaveBeenCalledTimes(2);
    expect(store.unlisten).not.toBeNull();
  });
});
