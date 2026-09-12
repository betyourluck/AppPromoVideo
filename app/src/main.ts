import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
// UI の書体は同梱する (rev30)。CSP が外部フォントを通さず、オフラインでも同じ見た目にするため。
import "@fontsource-variable/inter";
import "@fontsource-variable/noto-sans-jp";
import "./assets/main.css";
import { applyTheme } from "./theme";
import { initSettingsMirror } from "./settingsMirror";

// テーマを mount 前に <html> へ反映 (保存済みライトで開いてもダークが一瞬映らない)。既定ダーク。
applyTheme();

// 配布ビルドでは WebView の既定右クリックメニューと F5 / Ctrl+R を抑止する (Kataribe と同じ理由:
// 「更新」が実行中の表示を吹き飛ばす)。入力欄と選択テキストの上ではネイティブメニューを残す。
if (!import.meta.env.DEV) {
  // 文字の選択を止める印 (rev47)。**CSS は dev か配布かを知らない**ので、ここで <html> に立てる。
  // 何を選べるままにするかは main.css の `:root[data-locked]` 側が決める
  // (入力欄・プロンプト・進捗ログは選べる — ログを貼れなくなるのは実害)。
  document.documentElement.dataset.locked = "1";
  window.addEventListener("contextmenu", (e) => {
    const t = e.target instanceof HTMLElement ? e.target : null;
    const editable = t?.closest("input, textarea, [contenteditable]");
    const hasSelection = !!window.getSelection()?.toString();
    if (!editable && !hasSelection) e.preventDefault();
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "F5" || ((e.ctrlKey || e.metaKey) && (e.key === "r" || e.key === "R"))) e.preventDefault();
  });
}

// 設定ミラー (localStorage → app_data/settings.json の write-through + 新プロファイルへの復元)
// を張ってから mount する。復元したときは reload が来るので mount しない。
initSettingsMirror().then((restored) => {
  if (restored) return;
  createApp(App).use(createPinia()).mount("#app");
});
