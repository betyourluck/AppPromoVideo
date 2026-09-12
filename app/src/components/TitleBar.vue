<script setup lang="ts">
/**
 * カスタムタイトルバー (decorations:false)。Kataribe TitleBar の縮約。
 * ドラッグは data-tauri-drag-region、ウィンドウ操作は @tauri-apps/api/window を動的 import。
 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { theme, toggleTheme } from "../theme";
import { currentLang, setLang, t, UI_LANGUAGES, type UiLang } from "../i18n";
import Icon from "./Icon.vue";

/** `view` は今の画面。開いている画面のアイコンをアクセント色にして、押せば戻ることを見せる (rev36)。 */
defineProps<{ busy?: boolean; view?: "main" | "runs" | "settings" }>();
const emit = defineEmits<{ (e: "open-settings"): void; (e: "open-runs"): void }>();

const langMenuOpen = ref(false);

function selectLang(code: UiLang) {
  setLang(code);
  langMenuOpen.value = false;
}

function onWindowClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null;
  if (!target?.closest(".lang-wrap")) {
    langMenuOpen.value = false;
  }
}

onMounted(() => window.addEventListener("click", onWindowClick));
onBeforeUnmount(() => window.removeEventListener("click", onWindowClick));

async function win(method: "minimize" | "toggleMaximize" | "close") {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[method]();
  } catch (e) {
    console.warn(`[TitleBar] window.${method} unavailable:`, e);
  }
}
</script>

<template>
  <div data-tauri-drag-region class="tb">
    <div data-tauri-drag-region class="brand brand-line">
      <span class="outcasts">Outcasts</span> AppPromoVideo
      <span v-if="busy" class="chip accent" style="margin-left: 8px">{{ t('titleBar.running') }}</span>
    </div>
    <div data-tauri-drag-region class="spacer"></div>
    <button class="tb-btn" :disabled="busy" :title="theme === 'dark' ? t('titleBar.toLight') : t('titleBar.toDark')" @click="toggleTheme">
      <svg v-if="theme === 'dark'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
      </svg>
      <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
      </svg>
    </button>
    <div class="lang-wrap">
      <button class="tb-btn" :disabled="busy" :title="t('titleBar.language')" @click.stop="langMenuOpen = !langMenuOpen">
        <Icon name="globe" :size="15" />
      </button>
      <div v-if="langMenuOpen" class="lang-menu">
        <button
          v-for="l in UI_LANGUAGES"
          :key="l.code"
          class="lang-item"
          :class="{ active: currentLang === l.code }"
          @click.stop="selectLang(l.code)"
        >
          <span>{{ l.label }}</span>
          <Icon v-if="currentLang === l.code" name="check" :size="13" class="check-icon" />
        </button>
      </div>
    </div>
    <button class="tb-btn" :class="{ on: view === 'runs' }" :disabled="busy" :title="t('titleBar.history')" @click="emit('open-runs')">
      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.6">
        <path d="M3 12a9 9 0 1 0 3-6.7" />
        <path d="M3 4v5h5" />
        <path d="M12 7v5l3 2" />
      </svg>
    </button>
    <button class="tb-btn" :class="{ on: view === 'settings' }" :disabled="busy" :title="t('titleBar.settings')" @click="emit('open-settings')">
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
      </svg>
    </button>
    <div class="sep"></div>
    <button class="tb-btn" :title="t('titleBar.minimize')" @click="win('minimize')">
      <svg width="11" height="11" viewBox="0 0 10 10"><line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="tb-btn" :title="t('titleBar.maximize')" @click="win('toggleMaximize')">
      <svg width="11" height="11" viewBox="0 0 10 10"><rect x="0.6" y="0.6" width="8.8" height="8.8" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="tb-btn close" :title="t('titleBar.close')" @click="win('close')">
      <svg width="11" height="11" viewBox="0 0 10 10">
        <line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2" />
        <line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.tb {
  display: flex;
  align-items: center;
  height: 38px;
  flex-shrink: 0;
  background: rgb(var(--panel));
  border-bottom: 1px solid rgb(var(--line));
  user-select: none;
}
/* ウィンドウ枠の文字は試行の対象外 (rev29、ユーザー判断)。書体・大きさ・太さを試行前の値で固定。 */
/* 書体・太さ・字間・標の色は main.css の .brand-line / .outcasts (rev46)。ここは置き場の都合だけ。 */
.brand {
  padding: 0 14px;
  font-size: var(--fs-chrome);
  pointer-events: none;
}
.spacer {
  flex: 1;
  height: 100%;
}
.sep {
  width: 1px;
  height: 18px;
  margin: 0 4px;
  background: rgb(var(--line));
}
.tb-btn {
  width: 44px;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: rgb(var(--muted));
  cursor: pointer;
  transition: background var(--trans-fast), color var(--trans-fast);
}
.tb-btn:hover {
  background: rgb(var(--text) / 0.08);
  color: rgb(var(--text));
}
/* 開いている画面のアイコン (rev36)。もう一度押すと戻る。 */
.tb-btn.on {
  color: rgb(var(--accent));
  background: rgb(var(--accent) / 0.12);
}
.tb-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
  pointer-events: none;
}
.tb-btn.close:hover {
  background: #e53935;
  color: #fff;
}
.lang-wrap {
  position: relative;
  height: 100%;
  display: flex;
  align-items: center;
}
.lang-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  background: rgb(var(--panel));
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  padding: 4px;
  min-width: 110px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  z-index: 1000;
  backdrop-filter: blur(12px);
}
.lang-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  font-size: var(--fs-xs);
  font-weight: var(--fw-medium);
  color: rgb(var(--text));
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  text-align: left;
  transition: background var(--trans-fast);
}
.lang-item:hover {
  background: rgb(var(--text) / 0.08);
}
.lang-item.active {
  color: rgb(var(--accent));
  font-weight: var(--fw-bold);
  background: rgb(var(--accent) / 0.1);
}
.check-icon {
  color: rgb(var(--accent));
}
</style>
