<script setup lang="ts">
/**
 * 3 ペイン: 左 = 入力 / 中 = 結果 (要約・シーン表・参照画像) / 右 = 進捗ログ。
 * 状態は store、backend との往復も store。ここはレイアウトとダイアログの開閉だけ。
 */
import { onMounted, ref } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import RunsDialog from "./components/RunsDialog.vue";
import InputPane from "./components/InputPane.vue";
import ScenePanel from "./components/ScenePanel.vue";
import LogPanel from "./components/LogPanel.vue";
import MessageBox from "./components/MessageBox.vue";
import { useStore } from "./store";

const store = useStore();
const settingsOpen = ref(false);
const runsOpen = ref(false);

onMounted(() => {
  store.listenProgress();
  store.checkCli();
});
</script>

<template>
  <div class="shell">
    <TitleBar :busy="store.running || store.imaging" @open-settings="settingsOpen = true" @open-runs="runsOpen = true" />
    <div class="body">
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
    <SettingsDialog v-if="settingsOpen" @close="settingsOpen = false" />
    <RunsDialog v-if="runsOpen" @close="runsOpen = false" />
    <div v-if="store.toast" class="toast">{{ store.toast }}</div>
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
  grid-template-columns: 340px 1fr 320px;
  gap: 10px;
  padding: 10px;
}
.left,
.center,
.right {
  min-height: 0;
  overflow: auto;
}
</style>
