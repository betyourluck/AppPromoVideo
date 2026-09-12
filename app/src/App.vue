<script setup lang="ts">
/**
 * 3 ペイン: 左 = 入力 / 中 = 結果 (要約・シーン表・参照画像) / 右 = 進捗ログ。
 * 状態は store、backend との往復も store。ここはレイアウト、画面の切り替え (メイン / 履歴 / 設定)、ダイアログの開閉だけ。
 */
import { defineAsyncComponent, onMounted, ref } from "vue";
import TitleBar from "./components/TitleBar.vue";
import SettingsScreen from "./components/SettingsScreen.vue";
import RunsScreen from "./components/RunsScreen.vue";
import InputPane from "./components/InputPane.vue";
import ScenePanel from "./components/ScenePanel.vue";
import LogPanel from "./components/LogPanel.vue";
import MessageBox from "./components/MessageBox.vue";
import Icon from "./components/Icon.vue";
import { useStore } from "./store";
import { nextView } from "./views";
import { TOUR_DONE_KEY, shouldShowTour } from "./tour";

// 初回起動のナビゲーション (rev44)。初回にしか読まれないので動的 import (chunk 境界)。
const FirstRunTour = defineAsyncComponent(() => import("./components/FirstRunTour.vue"));

const store = useStore();
/**
 * 画面 (rev31 履歴 / rev35 設定)。どちらもダイアログではなく、3 ペインと入れ替える全画面 (ユーザー判断)。
 * メインは v-show で隠すだけ — 戻った時に入力・コピー先の選択・スクロール位置が残る。
 */
const view = ref<"main" | "runs" | "settings">("main");

/**
 * タイトルバーの呼び出しは**トグル** (rev36、ユーザー「開いているときにもう一度クリックすると戻ると同じ動きに」)。
 * 設定から離れる時は保存する — 設定画面の「戻る」と同じ扱いにするため (項目ごとの保存はしているが、揃えておく)。
 */
function go(target: "runs" | "settings") {
  if (view.value === "settings") store.persist();
  view.value = nextView(view.value, target);
}

// 初回起動のナビゲーション。判定は tour.ts の shouldShowTour (純関数)。
const showTour = ref(false);
async function decideTour() {
  let done = false;
  try {
    done = !!localStorage.getItem(TOUR_DONE_KEY);
  } catch {
    return; // storage が読めない環境では出さない (印も立てられないので毎回出てしまう)
  }
  if (done) return;
  // 「使った痕跡」= リポジトリを選んでいる / run がある。数えられなければ 0 として扱う。
  let runCount = 0;
  try {
    await store.loadRuns();
    runCount = store.runs.length;
  } catch {
    /* 索引が読めなくても案内の判定は続ける */
  }
  showTour.value = shouldShowTour({ done, hasProject: !!store.project.projectPath.trim(), runCount });
  if (!showTour.value) {
    // 既に使っている人には出さない。**印だけ立てる** — 次に空の状態になっても出さないため。
    try {
      localStorage.setItem(TOUR_DONE_KEY, "1");
    } catch {
      /* 書けなくても実害は無い */
    }
  }
}

/** 案内をもう一度 (rev45)。**メイン画面に戻してから**出す — 対象の要素が居ないと照らせない。 */
function replayTour() {
  view.value = "main";
  showTour.value = true;
}

onMounted(() => {
  store.listenProgress();
  store.checkCli();
  void decideTour();
});
</script>

<template>
  <div class="shell">
    <TitleBar :busy="store.running || store.imaging" :view="view" @open-settings="go('settings')" @open-runs="go('runs')" />
    <div v-show="view === 'main'" class="body">
      <aside class="left">
        <InputPane @open-settings="view = 'settings'" />
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
    <SettingsScreen v-if="view === 'settings'" @close="view = 'main'" @show-tour="replayTour" />
    <div v-if="store.toast" class="toast">
      <Icon name="check" :size="16" />
      <span>{{ store.toast }}</span>
    </div>
    <!-- rev26: 確認はアプリ内のメッセージボックス 1 つに集める (ブラウザ標準は URL が出る)。 -->
    <MessageBox />
    <!-- 初回起動のナビゲーション (rev44)。**手順を教えるだけ** — 案内の中から実行はさせない。 -->
    <FirstRunTour v-if="showTour" @close="showTour = false" />
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
 * `.panel` はダイアログ (シーン編集 / メッセージボックス) でも使うので共通の定義は変えず、
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
