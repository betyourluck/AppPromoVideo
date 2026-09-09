<script setup lang="ts">
/**
 * 1 シーンぶんの上書き — 見出し (rev9) と、はめ込み (rev11、product のみ)。
 *
 * 設定の値が**既定**で、ここで変えたぶんだけがその scene に効く。やり直しは `base/` の
 * **素材** (product は背景 / mood は絵) から合成し直すので、**生成 API は呼ばず、劣化しない**。
 */
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { FontEntry } from "../types";

const props = defineProps<{ sceneId: number; hasText: boolean; isProduct: boolean }>();
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

/**
 * はめ込み (product のみ、rev11)。空欄 = 既定に従う。
 * 傾きの既定は LLM が書いた `scene.plate_tilt`、大きさとずらしの既定は見出しの帯が決める。
 */
const yaw = ref<number | null>(null);
const pitch = ref<number | null>(null);
const ratio = ref<number | null>(null);
const dx = ref<number | null>(null);
const dy = ref<number | null>(null);

/** 触ったフィールドだけ送る。null は「既定のまま」。 */
function currentPlate() {
  const p: Record<string, number> = {};
  if (yaw.value !== null) p.yaw_degrees = yaw.value;
  if (pitch.value !== null) p.pitch_degrees = pitch.value;
  if (ratio.value !== null) p.screen_ratio = ratio.value;
  if (dx.value !== null) p.x_offset_ratio = dx.value;
  if (dy.value !== null) p.y_offset_ratio = dy.value;
  return Object.keys(p).length ? p : null;
}

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
    await store.reburnCaption(props.sceneId, props.hasText ? currentSpec() : null, currentPlate());
  } finally {
    busy.value = false;
  }
}

/** 見出しを消す (base をそのまま戻す)。 */
async function clear() {
  busy.value = true;
  try {
    await store.reburnCaption(props.sceneId, null, currentPlate());
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
  yaw.value = null;
  pitch.value = null;
  ratio.value = null;
  dx.value = null;
  dy.value = null;
}
</script>

<template>
  <div class="cap">
    <button
      class="btn small"
      :disabled="!hasText && !isProduct"
      :title="hasText || isProduct ? '' : 'このシーンには copy_text も、はめ込むスクショもありません'"
      @click="toggle"
    >
      {{ hasText ? (isProduct ? "見出し / はめ込み" : "見出し") : "はめ込み" }} {{ open ? "▲" : "▼" }}
    </button>
    <div v-if="open" class="panel-in">
      <template v-if="hasText">
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
      </template>
      <template v-if="isProduct">
        <h4 class="sub" style="margin: 4px 0 0">はめ込み (空欄 = 既定のまま)</h4>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>左右の傾き</span>
            <input v-model.number="yaw" type="number" min="-35" max="35" step="1" placeholder="既定" />
          </label>
          <label class="field" style="flex: 1">
            <span>上下の傾き</span>
            <input v-model.number="pitch" type="number" min="-35" max="35" step="1" placeholder="既定" />
          </label>
          <label class="field" style="flex: 1">
            <span>大きさ</span>
            <input v-model.number="ratio" type="number" min="0.2" max="0.95" step="0.02" placeholder="既定" />
          </label>
        </div>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>横位置</span>
            <input v-model.number="dx" type="number" min="-0.4" max="0.4" step="0.02" placeholder="中央" />
          </label>
          <label class="field" style="flex: 1">
            <span>縦位置</span>
            <input v-model.number="dy" type="number" min="-0.4" max="0.4" step="0.02" placeholder="既定" />
          </label>
        </div>
        <p class="muted note">
          傾きは度、大きさは canvas に占める比、位置は中央からのずらし (canvas 比、正 = 右 / 下)。
          <b>縦位置を入れると見出しの帯のずらしを置き換えます。</b>
        </p>
      </template>

      <div class="row" style="gap: 4px">
        <button class="btn small" :disabled="busy" @click="apply">{{ busy ? "焼き直し中…" : "適用" }}</button>
        <button class="btn small" :disabled="busy" @click="reset">既定に戻す</button>
        <button v-if="hasText" class="btn small" :disabled="busy" @click="clear">見出しを消す</button>
      </div>
      <p class="muted note">
        背景から合成をやり直すので、何度変えても劣化しません。生成の費用もかかりません。
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
