/**
 * UI 設定ミラー (Kataribe settingsMirror.ts の写し、接頭辞 `apppromo.`)。
 *
 * localStorage は WebView プロファイル別に住むため identifier 変更やプロファイル破損で消える。
 * `apppromo.*` 全キーを backend の `app_data/settings.json` へ write-through で写し、起動時に
 * localStorage が空 (新プロファイル) なら file から復元する。書き込みの捕捉は
 * `Storage.prototype` のフック 1 箇所 (書き込み箇所を列挙しない = 列挙は必ず漏れる)。
 */
import { invoke } from "@tauri-apps/api/core";

export const SETTINGS_PREFIX = "apppromo.";
const RESTORE_GUARD = "apppromo.mirror.restored";
const SAVE_DELAY_MS = 800;

export interface StorageLike {
  readonly length: number;
  key(i: number): string | null;
  getItem(k: string): string | null;
  setItem(k: string, v: string): void;
  removeItem(k: string): void;
}

export function collectSnapshot(storage: StorageLike): Record<string, string> {
  const snap: Record<string, string> = {};
  for (let i = 0; i < storage.length; i++) {
    const k = storage.key(i);
    if (!k || !k.startsWith(SETTINGS_PREFIX)) continue;
    const v = storage.getItem(k);
    if (v !== null) snap[k] = v;
  }
  return snap;
}

export function parseSnapshot(text: string): Record<string, string> | null {
  let v: unknown;
  try {
    v = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof v !== "object" || v === null || Array.isArray(v)) return null;
  const obj = v as Record<string, unknown>;
  for (const [k, val] of Object.entries(obj)) {
    if (!k.startsWith(SETTINGS_PREFIX) || typeof val !== "string") return null;
  }
  return obj as Record<string, string>;
}

export function shouldRestore(storage: StorageLike, snap: Record<string, string> | null): boolean {
  if (!snap || Object.keys(snap).length === 0) return false;
  return Object.keys(collectSnapshot(storage)).length === 0;
}

export function applySnapshot(storage: StorageLike, snap: Record<string, string>): void {
  for (const [k, v] of Object.entries(snap)) storage.setItem(k, v);
}

export function createMirror(
  storage: StorageLike,
  save: (json: string) => void,
  delayMs: number = SAVE_DELAY_MS,
  schedule: (fn: () => void, ms: number) => unknown = (fn, ms) => setTimeout(fn, ms),
  cancel: (h: unknown) => void = (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
): { onChange(key: string): void; flush(): void; saveNow(): void } {
  let timer: unknown = null;
  const fire = () => {
    timer = null;
    save(JSON.stringify(collectSnapshot(storage)));
  };
  return {
    onChange(key: string) {
      if (!key.startsWith(SETTINGS_PREFIX)) return;
      if (timer !== null) cancel(timer);
      timer = schedule(fire, delayMs);
    },
    flush() {
      if (timer === null) return;
      cancel(timer);
      fire();
    },
    saveNow: fire,
  };
}

export async function initSettingsMirror(): Promise<boolean> {
  let fileText: string | null;
  try {
    fileText = await invoke<string | null>("load_ui_settings");
  } catch {
    return false; // Tauri IPC が無い環境 (素の vite dev) — ミラー無しの通常起動。
  }
  try {
    const snap = fileText === null ? null : parseSnapshot(fileText);
    if (snap && shouldRestore(localStorage, snap) && !sessionStorage.getItem(RESTORE_GUARD)) {
      applySnapshot(localStorage, snap);
      sessionStorage.setItem(RESTORE_GUARD, "1");
      location.reload();
      return true;
    }
  } catch {
    /* storage が塞がれた環境では復元しない */
  }
  const mirror = createMirror(localStorage, (json) => {
    invoke("save_ui_settings", { json }).catch((e) => console.warn("[settingsMirror] 保存に失敗:", e));
  });
  const origSet = Storage.prototype.setItem;
  Storage.prototype.setItem = function (this: Storage, k: string, v: string) {
    origSet.call(this, k, v);
    if (this === window.localStorage) mirror.onChange(k);
  };
  const origRemove = Storage.prototype.removeItem;
  Storage.prototype.removeItem = function (this: Storage, k: string) {
    origRemove.call(this, k);
    if (this === window.localStorage) mirror.onChange(k);
  };
  mirror.saveNow();
  window.addEventListener("beforeunload", () => mirror.flush());
  return false;
}
