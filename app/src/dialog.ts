/**
 * アプリ内のメッセージボックス (rev26)。ブラウザ標準の確認ダイアログの代わり。
 *
 * 標準ダイアログは見出しに配信元の URL (`localhost:1421 の内容`) を出し、見た目もアプリから外れる
 * (ユーザー報告 2026-09-11)。表示は `MessageBox.vue` が 1 つだけ持ち、状態はここが持つ。
 *
 * **一度に出すのは 1 件**。出ている間に次の `ask` が来たら待たせ、答えた順に返す
 * (2 つを重ねて出すと、どちらに答えたのか分からなくなる)。
 */
import { reactive } from "vue";
import { t } from "./i18n";

export interface AskOptions {
  message: string;
  title?: string;
  /** 肯定側のボタン。既定「OK」。 */
  ok?: string;
  /** 否定側のボタン。既定「キャンセル」。 */
  cancel?: string;
  /** 取り返しのつかない操作 (削除など)。肯定側を警告色にし、最初の焦点を否定側に置く。 */
  danger?: boolean;
}

/** いま出ている 1 件 (表示に要る値だけ。既定は埋め済み)。 */
export interface Shown {
  message: string;
  title: string;
  ok: string;
  cancel: string;
  danger: boolean;
}

export const messageBox = reactive({ current: null as Shown | null });

interface Pending {
  shown: Shown;
  resolve: (ok: boolean) => void;
}

const waiting: Pending[] = [];
let resolveCurrent: ((ok: boolean) => void) | null = null;

function showNext(): void {
  const next = waiting.shift();
  messageBox.current = next ? next.shown : null;
  resolveCurrent = next ? next.resolve : null;
}

/** 確認を出し、答え (肯定 = true) を待つ。 */
export function ask(opts: AskOptions): Promise<boolean> {
  return new Promise((resolve) => {
    waiting.push({
      shown: {
        message: opts.message,
        title: opts.title ?? t("common.confirm"),
        ok: opts.ok ?? t("common.ok"),
        cancel: opts.cancel ?? t("common.cancel"),
        danger: opts.danger ?? false,
      },
      resolve,
    });
    if (!messageBox.current) showNext();
  });
}

/** 出ている 1 件に答える。次が待っていればそれを出してから返す。何も出ていなければ何もしない。 */
export function answer(ok: boolean): void {
  const resolve = resolveCurrent;
  if (!resolve) return;
  showNext();
  resolve(ok);
}

export function isMessageBoxOpen(): boolean {
  return messageBox.current !== null;
}

/** テスト用: 表示と待ち行列を空にする。 */
export function resetMessageBox(): void {
  waiting.length = 0;
  resolveCurrent = null;
  messageBox.current = null;
}
