<script setup lang="ts">
/**
 * 3 ペイン: 左 = 入力 / 中 = 結果 (要約・シーン表・参照画像) / 右 = 進捗ログ。
 * 状態は store、backend との往復も store。ここはレイアウト、画面の切り替え (メイン / 履歴)、ダイアログの開閉だけ。
 */
import { onMounted, ref } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import RunsScreen from "./components/RunsScreen.vue";
import InputPane from "./components/InputPane.vue";
import ScenePanel from "./components/ScenePanel.vue";
import LogPanel from "./components/LogPanel.vue";
import MessageBox from "./components/MessageBox.vue";
import Icon from "./components/Icon.vue";
import { useStore } from "./store";

const store = useStore();
const settingsOpen = ref(false);
/**
 * 画面 (rev31)。履歴はダイアログではなく、3 ペインと入れ替える全画面 (ユーザー判断)。
 * メインは v-show で隠すだけ — 戻った時に入力・コピー先の選択・スクロール位置が残る。
 */
const view = ref<"main" | "runs">("main");

onMounted(() => {
  store.listenProgress();
  store.checkCli();
});
</script>

<template>
  <div class="shell">
    <TitleBar :busy="store.running || store.imaging" @open-settings="settingsOpen = true" @open-runs="view = 'runs'" />
    <div v-show="view === 'main'" class="body">
      <aside class="left">
        <InputPane @open-settings="settingsOpen = true" />
      </aside>
      <main class="center">
        <ScenePanel />
      </main>
      <aside class="right">
        <LogPanel />
      </aside>
    </div>
    <!-- 「開く」が成功した時と「戻る」で close が来る (失敗した時は履歴画面に残る)。 -->
    <RunsScreen v-if="view === 'runs'" @close="view = 'main'" />
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" />
    <div v-if="store.toast" class="toast">
      <Icon name="check" :size="16" />
      <span>{{ store.toast }}</span>
    </div>
    <!-- rev26: 確認はアプリ内のメッセージボックス 1 つに集める (ブラウザ標準は URL が出る)。 -->
    <MessageBox />
  </div>
</template>

<style scoped>
.shell {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 390px 1fr 320px;
  gap: 16px;
  padding: 12px 16px;
}
.left {
  min-height: 0;
  overflow: auto;
  padding-right: 16px;
  border-right: 1px solid rgb(var(--line));
}
.center {
  min-height: 0;
  overflow: auto;
}
.right {
  min-height: 0;
  overflow: auto;
}
/*
 * rev27: 3 つの列 (入力 / 結果 / 進捗) は枠を持たない (ユーザー「枠をなくしてみたい」)。
 * `.panel` はダイアログ (設定 / シーン編集 / メッセージボックス) でも使うので共通の定義は変えず、
 * 列の直下だけを平らにする。子の root が複数でも届くように :deep で書く。
 */
.left > :deep(.panel),
.center > :deep(.panel),
.right > :deep(.panel) {
  background: transparent;
  border: none;
  border-radius: 0;
}
</style>
