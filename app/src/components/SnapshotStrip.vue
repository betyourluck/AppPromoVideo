<script setup lang="ts">
/**
 * UI スナップショットの帯。横スクロールのサムネイル / クリックで拡大 (Lightbox) /
 * ファイルのドラッグ&ドロップ (Tauri の drag-drop event = OS のパスが取れる) /
 * クリップボードの画像を Ctrl+V で貼り付け (backend が app_data/snapshots に保存)。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useStore } from "../store";
import { baseName, imageFilesFrom, stripDataUrl } from "../snapshots";
import { t } from "../i18n";
import Lightbox from "./Lightbox.vue";
import Icon from "./Icon.vue";

const store = useStore();
const dragging = ref(false);
const lightbox = ref<number | null>(null);
let unlistenDrop: (() => void) | null = null;

const items = computed(() =>
  store.project.snapshots.map((p) => ({
    key: p,
    url: store.snapshotUrls[p] ?? "",
    title: baseName(p),
    subtitle: p,
  })),
);

async function onPaste(e: ClipboardEvent) {
  if (store.running) return;
  const files = imageFilesFrom(e.clipboardData?.items);
  if (files.length === 0) return;
  e.preventDefault();
  for (const f of files) {
    const dataUrl = await new Promise<string>((res, rej) => {
      const r = new FileReader();
      r.onload = () => res(String(r.result));
      r.onerror = () => rej(r.error);
      r.readAsDataURL(f);
    });
    const parsed = stripDataUrl(dataUrl);
    if (!parsed) continue;
    await store.addClipboardImage(parsed.base64, parsed.mime);
  }
}

onMounted(async () => {
  window.addEventListener("paste", onPaste);
  store.loadSnapshotUrls();
  try {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    unlistenDrop = await getCurrentWebview().onDragDropEvent((ev) => {
      if (store.running) {
        dragging.value = false;
        return;
      }
      const p = ev.payload;
      if (p.type === "enter" || p.type === "over") dragging.value = true;
      else if (p.type === "leave") dragging.value = false;
      else if (p.type === "drop") {
        dragging.value = false;
        store.addSnapshotPaths(p.paths);
      }
    });
  } catch (e) {
    console.warn("[SnapshotStrip] drag-drop unavailable:", e);
  }
});

onBeforeUnmount(() => {
  window.removeEventListener("paste", onPaste);
  unlistenDrop?.();
});

function remove(key: string) {
  const i = store.project.snapshots.indexOf(key);
  if (i >= 0) store.removeSnapshot(i);
  if (lightbox.value !== null && lightbox.value >= store.project.snapshots.length) {
    lightbox.value = store.project.snapshots.length ? store.project.snapshots.length - 1 : null;
  }
}
</script>

<template>
  <div class="strip-wrap" :class="{ dragging }">
    <div class="row header-row" style="justify-content: space-between">
      <span class="muted" style="font-size: var(--fs-sm)">{{ t('snapshots.title') }}</span>
      <button class="btn small" :disabled="store.running" :title="t('snapshots.addTitle')" @click="store.pickSnapshots()">
        <Icon name="plus" :size="13" />
        <span>{{ t('common.add') }}</span>
      </button>
    </div>

    <div v-if="store.project.snapshots.length === 0" class="empty" :class="{ disabled: store.running }" @click="store.running ? undefined : store.pickSnapshots()">
      <Icon name="upload" :size="20" class="empty-icon" />
      <span class="empty-text">{{ t('snapshots.dropHint') }}</span>
      <span class="empty-sub">{{ t('snapshots.clickHint') }}</span>
    </div>

    <div v-else class="strip">
      <figure v-for="(p, i) in store.project.snapshots" :key="p" class="thumb" :class="{ disabled: store.running }" :title="p" @click="store.running ? undefined : (lightbox = i)">
        <img v-if="store.snapshotUrls[p]" :src="store.snapshotUrls[p]" :alt="baseName(p)" />
        <div v-else class="ph muted">…</div>
        <figcaption>{{ baseName(p) }}</figcaption>
        <button class="x-btn" :disabled="store.running" :title="t('snapshots.remove')" @click.stop="store.removeSnapshot(i)">
          <Icon name="x" :size="11" />
        </button>
      </figure>
    </div>

    <div class="muted footer-hint" style="font-size: var(--fs-xs)">
      <span>{{ t('snapshots.count', { count: store.project.snapshots.length }) }}</span>
      <span v-if="store.project.snapshots.length > 0"> · {{ t('snapshots.zoomHint') }}</span>
    </div>

    <Lightbox
      v-if="lightbox !== null && items.length"
      :items="items"
      :index="Math.min(lightbox, items.length - 1)"
      :removable="!store.running"
      @update:index="lightbox = $event"
      @close="lightbox = null"
      @remove="remove"
    />
  </div>
</template>

<style scoped>
.strip-wrap {
  border: 1px dashed transparent;
  border-radius: var(--radius-md);
  padding: 6px 2px;
  margin: 6px 0;
  transition: all var(--trans-fast);
}
.strip-wrap.dragging {
  border-color: rgb(var(--accent2));
  background: rgb(var(--accent2) / 0.08);
}
.header-row {
  margin-bottom: 6px;
}
.empty {
  border: 1.5px dashed rgb(var(--line));
  border-radius: var(--radius-lg);
  padding: 22px 14px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  text-align: center;
  cursor: pointer;
  background: rgb(var(--panel) / 0.4);
  transition: all var(--trans-fast);
  margin: 6px 0;
}
.empty:hover {
  border-color: rgb(var(--accent2));
  background: rgb(var(--panel-hover));
}
.empty.disabled {
  opacity: 0.5;
  cursor: not-allowed;
  pointer-events: none;
}
.empty-icon {
  color: rgb(var(--muted));
  margin-bottom: 2px;
}
.empty:hover .empty-icon {
  color: rgb(var(--accent2));
}
.empty-text {
  font-size: var(--fs-sm);
  color: rgb(var(--text));
  font-weight: var(--fw-semi);
}
.empty-sub {
  font-size: var(--fs-xs);
  color: rgb(var(--muted));
}
.strip {
  display: flex;
  gap: 10px;
  overflow-x: auto;
  padding: 6px 2px;
}
.thumb {
  position: relative;
  flex: 0 0 auto;
  width: 132px;
  margin: 0;
  cursor: zoom-in;
  transition: transform var(--trans-fast);
}
.thumb:hover {
  transform: translateY(-2px);
}
.thumb.disabled {
  cursor: default;
}
.thumb.disabled:hover {
  transform: none;
}
.thumb img,
.ph {
  width: 132px;
  height: 84px;
  object-fit: cover;
  border-radius: var(--radius-md);
  border: 1px solid rgb(var(--line));
  background: rgb(var(--bg));
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color var(--trans-fast), box-shadow var(--trans-fast);
}
.thumb:hover img {
  border-color: rgb(var(--accent2));
  box-shadow: var(--shadow-sm);
}
.thumb.disabled:hover img {
  border-color: rgb(var(--line));
  box-shadow: none;
}
figcaption {
  font-size: var(--fs-xs);
  color: rgb(var(--muted));
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 3px;
  text-align: center;
}
.x-btn {
  position: absolute;
  top: 4px;
  right: 4px;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-full);
  border: none;
  background: rgb(0 0 0 / 0.65);
  color: #ffffff;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  padding: 0;
  transition: background var(--trans-fast), transform var(--trans-fast);
}
.x-btn:hover {
  background: rgb(var(--warn));
  transform: scale(1.1);
}
.x-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
  pointer-events: none;
}
.footer-hint {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 4px;
}
</style>
