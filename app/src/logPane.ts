/**
 * 右ペイン (進捗ログ) の出し入れ (rev66、契約 `DesktopUi.log_pane`)。
 *
 * タイトルバーのアイコンで出し入れする (ユーザー 2026-10-15)。隠しても失うものは無い — エラーは入力ペイン、
 * 実行中の最新 1 行は結果ペインの実行中ブロックに出て、ログは store が受け続ける (出し直せば溜まった分が見える)。
 * 保存は `apppromo.logPane` = `hidden` の時だけ隠す。既定・壊れた値・読めない storage は表示に倒す。
 */
import { ref } from "vue";

export const LOG_PANE_KEY = "apppromo.logPane";

type Reader = Pick<Storage, "getItem">;
type Writer = Pick<Storage, "setItem">;

export function loadLogPaneVisible(storage: Reader | null): boolean {
  try {
    return storage?.getItem(LOG_PANE_KEY) !== "hidden";
  } catch {
    return true;
  }
}

export function saveLogPaneVisible(storage: Writer | null, visible: boolean): void {
  try {
    storage?.setItem(LOG_PANE_KEY, visible ? "shown" : "hidden");
  } catch {
    /* storage が塞がれた環境 (出し入れはその場では効く) */
  }
}

/** 本体の列。隠すと 2 列になり、結果 (中央) が広がる。 */
export function bodyColumns(visible: boolean): string {
  return visible ? "390px 1fr 320px" : "390px 1fr";
}

function browserStorage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export const logPaneVisible = ref(loadLogPaneVisible(browserStorage()));

export function toggleLogPane(): void {
  logPaneVisible.value = !logPaneVisible.value;
  saveLogPaneVisible(browserStorage(), logPaneVisible.value);
}
