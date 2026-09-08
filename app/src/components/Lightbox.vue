<script setup lang="ts">
/**
 * 画像の拡大表示。← → で前後、Esc で閉じる。`removable` なら削除ボタンを出す (スナップショット用)。
 * 画像は data URL (backend の image_data_url)。
 */
import { computed, onBeforeUnmount, onMounted } from "vue";

export interface LightboxItem {
  url: string;
  title: string;
  subtitle?: string;
  key: string;
}

const props = defineProps<{ items: LightboxItem[]; index: number; removable?: boolean }>();
const emit = defineEmits<{ (e: "close"): void; (e: "update:index", i: number): void; (e: "remove", key: string): void }>();

const cur = computed(() => props.items[props.index]);

function go(d: number) {
  if (props.items.length === 0) return;
  const n = (props.index + d + props.items.length) % props.items.length;
  emit("update:index", n);
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
  else if (e.key === "ArrowRight") go(1);
  else if (e.key === "ArrowLeft") go(-1);
}

onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="lb" @click.self="emit('close')">
    <button v-if="items.length > 1" class="nav left" title="前 (←)" @click="go(-1)">‹</button>
    <figure v-if="cur" class="fig">
      <img :src="cur.url" :alt="cur.title" />
      <figcaption>
        <span class="title">{{ cur.title }}</span>
        <span v-if="cur.subtitle" class="muted"> — {{ cur.subtitle }}</span>
        <span class="muted"> ({{ index + 1 }}/{{ items.length }})</span>
        <button v-if="removable" class="btn small danger" style="margin-left: 10px" @click="emit('remove', cur.key)">この画像を外す</button>
        <button class="btn small" style="margin-left: 6px" @click="emit('close')">閉じる (Esc)</button>
      </figcaption>
    </figure>
    <button v-if="items.length > 1" class="nav right" title="次 (→)" @click="go(1)">›</button>
  </div>
</template>

<style scoped>
.lb {
  position: fixed;
  inset: 0;
  background: rgb(0 0 0 / 0.78);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 60;
}
.fig {
  margin: 0;
  max-width: 92vw;
  max-height: 92vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.fig img {
  max-width: 92vw;
  max-height: 82vh;
  object-fit: contain;
  border-radius: 8px;
  box-shadow: 0 10px 40px rgb(0 0 0 / 0.6);
  background: #111;
}
figcaption {
  color: #eee;
  font-size: 12px;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  justify-content: center;
}
.title {
  font-weight: 600;
}
.nav {
  position: fixed;
  top: 50%;
  transform: translateY(-50%);
  width: 48px;
  height: 96px;
  border: none;
  background: rgb(255 255 255 / 0.08);
  color: #fff;
  font-size: 40px;
  cursor: pointer;
  border-radius: 8px;
}
.nav:hover {
  background: rgb(255 255 255 / 0.2);
}
.left {
  left: 16px;
}
.right {
  right: 16px;
}
</style>
