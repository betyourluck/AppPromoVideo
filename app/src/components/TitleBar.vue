<script setup lang="ts">
/**
 * カスタムタイトルバー (decorations:false)。Kataribe TitleBar の縮約。
 * ドラッグは data-tauri-drag-region、ウィンドウ操作は @tauri-apps/api/window を動的 import。
 */
import { theme, toggleTheme } from "../theme";

defineProps<{ busy?: boolean }>();
const emit = defineEmits<{ (e: "open-settings"): void }>();

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
    <div data-tauri-drag-region class="brand">
      <span class="outcasts">Outcasts</span> AppPromoVideo
      <span v-if="busy" class="chip accent" style="margin-left: 8px">実行中</span>
    </div>
    <div data-tauri-drag-region class="spacer"></div>
    <button class="tb-btn" :title="theme === 'dark' ? 'ライトへ' : 'ダークへ'" @click="toggleTheme">
      <svg v-if="theme === 'dark'" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
      </svg>
      <svg v-else width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round">
        <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
      </svg>
    </button>
    <button class="tb-btn" title="設定" @click="emit('open-settings')">
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
      </svg>
    </button>
    <div class="sep"></div>
    <button class="tb-btn" title="最小化" @click="win('minimize')">
      <svg width="11" height="11" viewBox="0 0 10 10"><line x1="0" y1="5" x2="10" y2="5" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="tb-btn" title="最大化" @click="win('toggleMaximize')">
      <svg width="11" height="11" viewBox="0 0 10 10"><rect x="0.6" y="0.6" width="8.8" height="8.8" fill="none" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="tb-btn close" title="閉じる" @click="win('close')">
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
  height: 32px;
  flex-shrink: 0;
  background: rgb(var(--panel));
  border-bottom: 1px solid rgb(var(--line));
  user-select: none;
}
.brand {
  padding: 0 12px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  pointer-events: none;
}
.outcasts {
  color: oklch(0.63 0.24 303);
  text-shadow: 0 0 6px oklch(0.63 0.24 303 / 0.8), 0 0 18px oklch(0.63 0.24 303 / 0.4);
}
.spacer {
  flex: 1;
  height: 100%;
}
.sep {
  width: 1px;
  height: 16px;
  margin: 0 4px;
  background: rgb(var(--line));
}
.tb-btn {
  width: 44px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: none;
  color: rgb(var(--muted));
  cursor: pointer;
}
.tb-btn:hover {
  background: rgb(var(--text) / 0.08);
  color: rgb(var(--text));
}
.tb-btn.close:hover {
  background: #e53935;
  color: #fff;
}
</style>
