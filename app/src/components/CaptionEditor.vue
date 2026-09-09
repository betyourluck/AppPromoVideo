<script setup lang="ts">
/**
 * 1 シーンぶんの上書き — 見出し (rev9) と、はめ込み (rev11、product のみ)。
 *
 * 設定の値が**既定**で、ここで変えたぶんだけがその scene に効く。やり直しは `base/` の
 * **素材** (product は背景 / mood は絵) から合成し直すので、**生成 API は呼ばず、劣化しない**。
 */
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { FontEntry } from "../types";

const props = defineProps<{
  sceneId: number;
  hasText: boolean;
  isProduct: boolean;
  copyText: string;
  originalCopy: string | null;
  snapshots: string[];
  llmSnapshot: number | null;
}>();
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
/** コピー文 (rev12)。画像に焼かれ、scenes.md とクリップボードにも出る文そのもの。 */
const copy = ref(props.copyText);
watch(
  () => props.copyText,
  (v) => (copy.value = v),
);
const copyChanged = computed(() => copy.value !== props.copyText);

/** 使うスナップショット (rev13)。null = LLM の選択のまま。 */
const snapIndex = ref<number | null>(null);

/** 「適用」を押すまで絵は変わらない。押し忘れが分かるように印を出す。 */
const dirty = ref(false);
function touch() {
  dirty.value = true;
}

const yaw = ref<number | null>(null);
const pitch = ref<number | null>(null);
const ratio = ref<number | null>(null);
const dx = ref<number | null>(null);
const dy = ref<number | null>(null);

/**
 * プレビューのドラッグではめ込み位置を動かす (rev12)。
 * ずらしは canvas 幅・高さに対する比なので、表示サイズの比に直せばそのまま使える。
 */
const preview = ref<HTMLImageElement | null>(null);
let drag: { x: number; y: number; dx: number; dy: number } | null = null;

function onDown(e: PointerEvent) {
  if (!props.isProduct || !preview.value) return;
  drag = { x: e.clientX, y: e.clientY, dx: dx.value ?? 0, dy: dy.value ?? 0 };
  (e.target as HTMLElement).setPointerCapture(e.pointerId);
}
function onMove(e: PointerEvent) {
  if (!drag || !preview.value) return;
  const r = preview.value.getBoundingClientRect();
  const clamp = (v: number) => Math.max(-0.4, Math.min(0.4, v));
  dx.value = clamp(drag.dx + (e.clientX - drag.x) / r.width);
  dy.value = clamp(drag.dy + (e.clientY - drag.y) / r.height);
  dirty.value = true;
}
function onUp(e: PointerEvent) {
  if (!drag) return;
  drag = null;
  (e.target as HTMLElement).releasePointerCapture(e.pointerId);
  // rev13: ここでは焼かない。焼き直しは「適用」だけ (実測 debug 0.6 s / release 0.25 s、
  // 触るたびに走らせると操作にならない)。
}

/** スライダーの値 (`null` = 未指定 = 既定のまま)。 */
function show(v: number | null, unit = ""): string {
  return v === null ? "既定" : `${v}${unit}`;
}
function pickSnap(e: Event): number | null {
  const v = (e.target as HTMLSelectElement).value;
  return v === "" ? null : Number(v);
}
function baseName(p: string): string {
  return p.split(/[\/]/).pop() ?? p;
}
function num(e: Event): number {
  return Number((e.target as HTMLInputElement).value);
}

/** 触ったフィールドだけ送る。null は「既定のまま」。 */
function currentPlate() {
  const p: Record<string, number> = {};
  if (yaw.value !== null) p.yaw_degrees = yaw.value;
  if (pitch.value !== null) p.pitch_degrees = pitch.value;
  if (ratio.value !== null) p.screen_ratio = ratio.value;
  if (dx.value !== null) p.x_offset_ratio = dx.value;
  if (dy.value !== null) p.y_offset_ratio = dy.value;
  if (snapIndex.value !== null) p.snapshot_index = snapIndex.value;
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
    await store.reburnCaption(props.sceneId, copy.value.trim() ? currentSpec() : null, currentPlate(), copyChanged.value ? copy.value : null);
    dirty.value = false;
  } finally {
    busy.value = false;
  }
}

/** 見出しを消す (base をそのまま戻す)。 */
async function clear() {
  busy.value = true;
  try {
    await store.reburnCaption(props.sceneId, null, currentPlate(), null);
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
  snapIndex.value = null;
  dirty.value = true;
}

/** LLM が最初に書いた文へ戻す。 */
function restoreCopy() {
  if (props.originalCopy !== null) copy.value = props.originalCopy;
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
      <label class="field">
        <span>コピー文 (画像に焼かれ、scenes.md にも出ます)</span>
        <textarea v-model="copy" rows="2" placeholder="空にすると見出しを焼きません" @input="touch()" />
      </label>
      <div v-if="originalCopy !== null && copy !== originalCopy" class="row" style="gap: 4px">
        <button class="btn small" @click="restoreCopy">最初の文に戻す</button>
        <span class="muted" style="font-size: 11px">元: {{ originalCopy }}</span>
      </div>

      <template v-if="copy.trim()">
      <p v-if="!fonts.length" class="muted note">フォント一覧を読み込み中…</p>
      <div class="row">
        <label class="field" style="flex: 2">
          <span>フォント</span>
          <select v-model="fontKey" @change="touch()">
            <option v-for="f in fonts" :key="f.path + f.index" :value="`${f.path}#${f.index}`">
              {{ f.has_japanese ? "🇯🇵 " : "" }}{{ f.family }}
            </option>
          </select>
        </label>
        <label class="field" style="flex: 1">
          <span>位置</span>
          <select v-model="position" @change="touch()">
            <option value="top">上</option>
            <option value="bottom">下</option>
          </select>
        </label>
      </div>
      <div class="row">
        <label class="field" style="flex: 1">
          <span>大きさ <b class="mono">{{ sizeRatio.toFixed(3) }}</b></span>
          <input v-model.number="sizeRatio" type="range" min="0.02" max="0.2" step="0.005" @input="touch()" />
        </label>
        <label class="field" style="flex: 1">
          <span>色</span>
          <input v-model="color" type="color" @change="touch()" />
        </label>
      </div>
      </template>
      <template v-if="isProduct">
        <h4 class="sub" style="margin: 4px 0 0">はめ込み</h4>
        <label v-if="snapshots.length > 1" class="field">
          <span>使うスナップショット</span>
          <select :value="snapIndex ?? ''" @change="snapIndex = pickSnap($event); touch()">
            <option value="">LLM の選択のまま ({{ (llmSnapshot ?? 0) + 1 }} 枚目)</option>
            <option v-for="(sp, i) in snapshots" :key="sp" :value="i">{{ i + 1 }} 枚目 — {{ baseName(sp) }}</option>
          </select>
        </label>
        <img
          v-if="store.imageUrls[sceneId]"
          ref="preview"
          class="preview"
          :src="store.imageUrls[sceneId]"
          :alt="`scene ${sceneId}`"
          title="ドラッグで位置を動かす"
          @pointerdown.prevent="onDown"
          @pointermove="onMove"
          @pointerup="onUp"
          @pointercancel="onUp"
        />
        <label class="field">
          <span>左右の傾き <b class="mono">{{ show(yaw, "°") }}</b></span>
          <input :value="yaw ?? 0" type="range" min="-35" max="35" step="1" @input="yaw = num($event); touch()" />
        </label>
        <label class="field">
          <span>上下の傾き <b class="mono">{{ show(pitch, "°") }}</b></span>
          <input :value="pitch ?? 0" type="range" min="-35" max="35" step="1" @input="pitch = num($event); touch()" />
        </label>
        <label class="field">
          <span>大きさ <b class="mono">{{ show(ratio) }}</b></span>
          <input :value="ratio ?? 0.78" type="range" min="0.2" max="0.95" step="0.01" @input="ratio = num($event); touch()" />
        </label>
        <label class="field">
          <span>横位置 <b class="mono">{{ show(dx) }}</b></span>
          <input :value="dx ?? 0" type="range" min="-0.4" max="0.4" step="0.01" @input="dx = num($event); touch()" />
        </label>
        <label class="field">
          <span>縦位置 <b class="mono">{{ show(dy) }}</b></span>
          <input :value="dy ?? 0" type="range" min="-0.4" max="0.4" step="0.01" @input="dy = num($event); touch()" />
        </label>
        <p class="muted note">
          スライダーとドラッグは<b>値を変えるだけ</b>です。絵が変わるのは「適用」を押した時だけ
          (再合成は原寸で 0.6 秒ほどかかるため)。<b>縦位置を動かすと見出しの帯のずらしを置き換えます。</b>
        </p>
      </template>

      <div class="row" style="gap: 4px">
        <button class="btn small" :class="{ on: dirty }" :disabled="busy" @click="apply">
          {{ busy ? "焼き直し中…" : dirty ? "適用 (未反映)" : "適用" }}
        </button>
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
.preview {
  width: 100%;
  max-height: 32vh;
  object-fit: contain;
  border-radius: 6px;
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.preview:active {
  cursor: grabbing;
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
