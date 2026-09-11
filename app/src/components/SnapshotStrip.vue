<script setup lang="ts">
/**
 * UI スナップショットの帯。横スクロールのサムネイル / クリックで拡大 (Lightbox) /
 * ファイルのドラッグ&ドロップ (Tauri の drag-drop event = OS のパスが取れる) /
 * クリップボードの画像を Ctrl+V で貼り付け (backend が app_data/snapshots に保存)。
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useStore } from "../store";
import { baseName, imageFilesFrom, stripDataUrl } from "../snapshots";
import Lightbox from "./Lightbox.vue";

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
    <div class="row" style="justify-content: space-between">
      <span class="muted" style="font-size: var(--fs-sm)">UI スナップショット (png / jpg / webp) — ドロップ、Ctrl+V、または</span>
      <button class="btn small" @click="store.pickSnapshots()">追加…</button>
    </div>
    <div v-if="store.project.snapshots.length === 0" class="empty muted">ここに画像をドロップ / クリップボードの画像を Ctrl+V</div>
    <div v-else class="strip">
      <figure v-for="(p, i) in store.project.snapshots" :key="p" class="thumb" :title="p" @click="lightbox = i">
        <img v-if="store.snapshotUrls[p]" :src="store.snapshotUrls[p]" :alt="baseName(p)" />
        <div v-else class="ph muted">…</div>
        <figcaption>{{ baseName(p) }}</figcaption>
        <button class="x" title="外す" @click.stop="store.removeSnapshot(i)">×</button>
      </figure>
    </div>
    <div class="muted" style="font-size: var(--fs-sm)">{{ store.project.snapshots.length }} 枚 · クリックで拡大</div>
    <Lightbox
      v-if="lightbox !== null && items.length"
      :items="items"
      :index="Math.min(lightbox, items.length - 1)"
      removable
      @update:index="lightbox = $event"
      @close="lightbox = null"
      @remove="remove"
    />
  </div>
</template>

<style scoped>
.strip-wrap {
  border: 1px dashed transparent;
  border-radius: 8px;
  padding: 4px;
  margin: 6px 0;
}
.strip-wrap.dragging {
  border-color: rgb(var(--accent2));
  background: rgb(var(--accent2) / 0.08);
}
.empty {
  border: 1px dashed rgb(var(--line));
  border-radius: 8px;
  padding: 18px 8px;
  text-align: center;
  font-size: var(--fs-sm);
  margin: 6px 0;
}
.strip {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  padding: 6px 2px;
}
.thumb {
  position: relative;
  flex: 0 0 auto;
  width: 132px;
  margin: 0;
  cursor: zoom-in;
}
.thumb img,
.ph {
  width: 132px;
  height: 84px;
  object-fit: cover;
  border-radius: 6px;
  border: 1px solid rgb(var(--line));
  background: rgb(var(--bg));
  display: flex;
  align-items: center;
  justify-content: center;
}
.thumb:hover img {
  border-color: rgb(var(--accent2));
}
figcaption {
  font-size: var(--fs-xs);
  color: rgb(var(--muted));
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.x {
  position: absolute;
  top: 2px;
  right: 2px;
  width: 20px;
  height: 20px;
  border-radius: 999px;
  border: none;
  background: rgb(0 0 0 / 0.6);
  color: #fff;
  cursor: pointer;
  font-size: var(--fs-md);
  line-height: 20px;
  padding: 0;
}
</style>
