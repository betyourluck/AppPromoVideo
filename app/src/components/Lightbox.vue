<script setup lang="ts">
/**
 * 画像の拡大表示。← → で前後、Esc で閉じる。`removable` なら削除ボタンを出す (スナップショット用)。
 * 画像は data URL (backend の image_data_url)。
 */
import { computed, onBeforeUnmount, onMounted } from "vue";
import { t } from "../i18n";
import Icon from "./Icon.vue";

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
    <button v-if="items.length > 1" class="nav left" :title="t('lightbox.prev')" @click="go(-1)">
      <Icon name="chevron-left" :size="32" stroke-width="2.2" />
    </button>
    <figure v-if="cur" class="fig">
      <img :src="cur.url" :alt="cur.title" />
      <figcaption>
        <span class="title">{{ cur.title }}</span>
        <span v-if="cur.subtitle" class="muted"> — {{ cur.subtitle }}</span>
        <span class="muted"> ({{ index + 1 }}/{{ items.length }})</span>
        <button v-if="removable" class="btn small danger" style="margin-left: 10px" @click="emit('remove', cur.key)">
          <Icon name="trash" :size="13" />
          <span>{{ t('lightbox.removeThis') }}</span>
        </button>
        <button class="btn small" style="margin-left: 6px" @click="emit('close')">
          <Icon name="x" :size="13" />
          <span>{{ t('lightbox.close') }}</span>
        </button>
      </figcaption>
    </figure>
    <button v-if="items.length > 1" class="nav right" :title="t('lightbox.next')" @click="go(1)">
      <Icon name="chevron-right" :size="32" stroke-width="2.2" />
    </button>
  </div>
</template>

<style scoped>
.lb {
  position: fixed;
  inset: 0;
  background: rgb(0 0 0 / 0.82);
  backdrop-filter: blur(12px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 60;
  animation: lbFade 0.15s ease-out;
}
@keyframes lbFade {
  from { opacity: 0; }
  to { opacity: 1; }
}
.fig {
  margin: 0;
  max-width: 92vw;
  max-height: 92vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}
.fig img {
  max-width: 92vw;
  max-height: 82vh;
  object-fit: contain;
  border-radius: var(--radius-lg);
  box-shadow: 0 16px 48px rgb(0 0 0 / 0.65);
  background: #111;
  border: 1px solid rgb(255 255 255 / 0.1);
}
figcaption {
  color: #eee;
  font-size: var(--fs-md);
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  justify-content: center;
  gap: 4px;
}
.title {
  font-weight: var(--fw-semi);
}
.nav {
  position: fixed;
  top: 50%;
  transform: translateY(-50%);
  width: 44px;
  height: 80px;
  border: none;
  background: rgb(255 255 255 / 0.08);
  color: #fff;
  cursor: pointer;
  border-radius: var(--radius-md);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--trans-fast);
}
.nav:hover {
  background: rgb(255 255 255 / 0.22);
  transform: translateY(-50%) scale(1.05);
}
.left {
  left: 16px;
}
.right {
  right: 16px;
}
</style>
