<script setup lang="ts">
/**
 * アプリ内のメッセージボックス (rev26)。App.vue に 1 つだけ置き、状態は `dialog.ts` が持つ。
 * ブラウザ標準の確認ダイアログは見出しに配信元の URL を出すので使わない。
 *
 * - Esc と背景クリックは**否定側**。取り消しが安全側になるように呼び出し側で文言を選ぶ。
 * - 最初の焦点は肯定側。`danger` (削除など) の時だけ否定側 — Enter の連打で消さないため。
 * - keydown は **capture 段階で受けて伝播を止める**。止めないと、下にあるダイアログ (シーン編集・
 *   Lightbox) の Esc も同じ打鍵で反応し、確認の後ろで画面が閉じる。
 * - Tab は 2 つのボタンの間に閉じ込める (後ろの画面の入力へ焦点が抜けない)。
 */
import { nextTick, onBeforeUnmount, ref, watch } from "vue";
import { answer, messageBox } from "../dialog";

const okBtn = ref<HTMLButtonElement | null>(null);
const cancelBtn = ref<HTMLButtonElement | null>(null);
/** 開く前に焦点があった要素。閉じたら戻す。 */
let restoreFocus: HTMLElement | null = null;

function onKey(e: KeyboardEvent) {
  if (!messageBox.current) return;
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    answer(false);
  } else if (e.key === "Tab") {
    e.preventDefault();
    e.stopPropagation();
    (document.activeElement === okBtn.value ? cancelBtn.value : okBtn.value)?.focus();
  }
}

watch(
  () => messageBox.current,
  async (cur, prev) => {
    if (cur && !prev) {
      restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      window.addEventListener("keydown", onKey, true);
    }
    if (!cur) {
      window.removeEventListener("keydown", onKey, true);
      restoreFocus?.focus();
      restoreFocus = null;
      return;
    }
    await nextTick();
    (cur.danger ? cancelBtn.value : okBtn.value)?.focus();
  },
);
onBeforeUnmount(() => window.removeEventListener("keydown", onKey, true));
</script>

<template>
  <Teleport to="body">
    <div v-if="messageBox.current" class="mb-backdrop" @click.self="answer(false)">
      <div class="mb panel" role="alertdialog" aria-modal="true" :aria-label="messageBox.current.title">
        <b class="mb-title">{{ messageBox.current.title }}</b>
        <p class="mb-message">{{ messageBox.current.message }}</p>
        <div class="mb-foot">
          <button ref="cancelBtn" class="btn" @click="answer(false)">{{ messageBox.current.cancel }}</button>
          <button
            ref="okBtn"
            class="btn"
            :class="messageBox.current.danger ? 'danger' : 'primary'"
            @click="answer(true)"
          >
            {{ messageBox.current.ok }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.mb-backdrop {
  position: fixed;
  inset: 0;
  background: rgb(0 0 0 / 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  /* すべてのダイアログ (履歴・設定 40 / シーン編集 45 / トースト 50 / Lightbox 60) の上。 */
  z-index: 70;
}
.mb {
  width: min(420px, 92vw);
  display: flex;
  flex-direction: column;
  gap: 10px;
  box-shadow: 0 12px 40px rgb(0 0 0 / 0.45);
}
.mb-title {
  font-size: 14px;
}
.mb-message {
  margin: 0;
  white-space: pre-wrap;
  line-height: 1.6;
}
.mb-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}
</style>
