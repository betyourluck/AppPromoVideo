<script setup lang="ts">
/**
 * 3 ペイン: 左 = 入力 / 中 = 結果 (要約・シーン表・参照画像) / 右 = 進捗ログ。
 * 状態は store、backend との往復も store。ここはレイアウトとダイアログの開閉だけ。
 */
import { onMounted, ref } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import InputPane from "./components/InputPane.vue";
import ScenePanel from "./components/ScenePanel.vue";
import LogPanel from "./components/LogPanel.vue";
import { useStore } from "./store";

const store = useStore();
const settingsOpen = ref(false);

onMounted(() => {
  store.listenProgress();
  store.checkCli();
});
</script>

<template>
  <div class="shell">
    <TitleBar :busy="store.running || store.imaging" @open-settings="settingsOpen = true" />
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
    <div v-if="store.toast" class="toast">{{ store.toast }}</div>
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
