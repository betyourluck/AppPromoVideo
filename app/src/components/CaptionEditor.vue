<script setup lang="ts">
/**
 * 1 シーンぶんの見出しの上書き (契約 `caption.per_scene`、rev9)。
 *
 * 設定の値が**既定**で、ここで変えたぶんだけがその scene に効く。焼き直しは `base/` に
 * 残した「焼く前の合成」から行うので、**生成 API は呼ばず、何度やっても劣化しない**。
 */
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { FontEntry } from "../types";

const props = defineProps<{ sceneId: number; hasText: boolean }>();
const store = useStore();

const open = ref(false);
const busy = ref(false);
const fonts = ref<FontEntry[]>([]);

/** 既定 = 設定の値。ここを起点に、触ったフィールドだけ差し替える。 */
const base = computed(() => store.image.caption);
const position = ref<"top" | "bottom">(base.value.position);
const sizeRatio = ref(base.value.sizeRatio);
const color = ref(base.value.color);
const fontKey = ref(`${base.value.fontPath}#${base.value.fontIndex}`);

async function toggle() {
  open.value = !open.value;
  if (open.value && !fonts.value.length) {
    try {
      fonts.value = await invoke<FontEntry[]>("list_fonts");
    } catch (e) {
      store.push("error", `フォント一覧を取れません: ${e}`);
    }
  }
}

function currentSpec() {
  const i = fontKey.value.lastIndexOf("#");
  return {
    font_path: i >= 0 ? fontKey.value.slice(0, i) : fontKey.value,
    font_index: i >= 0 ? Number(fontKey.value.slice(i + 1)) || 0 : 0,
    size_ratio: Math.min(0.2, Math.max(0.02, sizeRatio.value || 0.055)),
    position: position.value,
    color: /^#[0-9a-fA-F]{6}$/.test(color.value) ? color.value : "#FFFFFF",
  };
}

async function apply() {
  busy.value = true;
  try {
    await store.reburnCaption(props.sceneId, currentSpec());
  } finally {
    busy.value = false;
  }
}

/** 見出しを消す (base をそのまま戻す)。 */
async function clear() {
  busy.value = true;
  try {
    await store.reburnCaption(props.sceneId, null);
  } finally {
    busy.value = false;
  }
}

/** 設定の既定に戻す (このシーンの上書きを捨てる)。 */
function reset() {
  position.value = base.value.position;
  sizeRatio.value = base.value.sizeRatio;
  color.value = base.value.color;
  fontKey.value = `${base.value.fontPath}#${base.value.fontIndex}`;
}
</script>

<template>
  <div class="cap">
    <button class="btn small" :disabled="!hasText" :title="hasText ? '' : 'このシーンには copy_text がありません'" @click="toggle">
      見出し {{ open ? "▲" : "▼" }}
    </button>
    <div v-if="open" class="panel-in">
      <p v-if="!fonts.length" class="muted note">フォント一覧を読み込み中…</p>
      <div class="row">
        <label class="field" style="flex: 2">
          <span>フォント</span>
          <select v-model="fontKey">
            <option v-for="f in fonts" :key="f.path + f.index" :value="`${f.path}#${f.index}`">
              {{ f.has_japanese ? "🇯🇵 " : "" }}{{ f.family }}
            </option>
          </select>
        </label>
        <label class="field" style="flex: 1">
          <span>位置</span>
          <select v-model="position">
            <option value="top">上</option>
            <option value="bottom">下</option>
          </select>
        </label>
      </div>
      <div class="row">
        <label class="field" style="flex: 1">
          <span>大きさ</span>
          <input v-model.number="sizeRatio" type="number" min="0.02" max="0.2" step="0.005" />
        </label>
        <label class="field" style="flex: 1">
          <span>色</span>
          <input v-model="color" type="color" />
        </label>
      </div>
      <div class="row" style="gap: 4px">
        <button class="btn small" :disabled="busy" @click="apply">{{ busy ? "焼き直し中…" : "適用" }}</button>
        <button class="btn small" :disabled="busy" @click="reset">既定に戻す</button>
        <button class="btn small" :disabled="busy" @click="clear">見出しを消す</button>
      </div>
      <p class="muted note">
        焼く前の画像から焼き直すので、何度変えても劣化しません。生成の費用もかかりません。
      </p>
    </div>
  </div>
</template>

<style scoped>
.cap {
  margin-top: 4px;
}
.panel-in {
  margin-top: 6px;
  padding: 8px;
  border: 1px solid var(--line, rgb(255 255 255 / 0.1));
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
</style>
