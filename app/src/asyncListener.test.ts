import { describe, expect, it, vi } from "vitest";
import { disposableListener } from "./asyncListener";

/**
 * rev60: 画像を 1 回ドロップすると 4 枚並んだ (ユーザー報告 2026-10-03)。ドロップのリスナーは登録が非同期 (動的 import + IPC 4 往復) で、
 * 登録が終わる前に部品が作り直される (開発時の HMR) と、外す関数がまだ無いまま外し損ねて残っていた。
 */
describe("disposableListener (rev60)", () => {
  /** 登録の完了を手で進められる偽物。 */
  function pending() {
    const unlisten = vi.fn();
    let resolve!: (u: () => void) => void;
    const register = () => new Promise<() => void>((r) => (resolve = r));
    return { unlisten, register, finish: () => resolve(unlisten) };
  }

  it("登録が終わる前に外したら、終わった時点ですぐ外す (漏らさない)", async () => {
    const p = pending();
    const dispose = disposableListener(p.register);
    dispose();
    p.finish();
    await Promise.resolve();
    await Promise.resolve();
    expect(p.unlisten).toHaveBeenCalledTimes(1);
  });

  it("登録が終わってから外せば普通に外れる。2 度外しても 1 回だけ", async () => {
    const p = pending();
    const dispose = disposableListener(p.register);
    p.finish();
    await Promise.resolve();
    await Promise.resolve();
    expect(p.unlisten).not.toHaveBeenCalled();
    dispose();
    dispose();
    expect(p.unlisten).toHaveBeenCalledTimes(1);
  });

  it("登録に失敗したら onError に渡し、外しても投げない", async () => {
    const onError = vi.fn();
    const dispose = disposableListener(() => Promise.reject(new Error("no tauri")), onError);
    await Promise.resolve();
    await Promise.resolve();
    expect(onError).toHaveBeenCalledTimes(1);
    expect(() => dispose()).not.toThrow();
  });
});
