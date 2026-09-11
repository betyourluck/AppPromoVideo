<script setup lang="ts">
/**
 * 1 シーンぶんの上書き — 見出し (rev9) と、はめ込み (rev11。rev24 から mood にも足せる)。
 *
 * 設定の値が**既定**で、ここで変えたぶんだけがその scene に効く。やり直しは `base/` の
 * **素材** (product は背景 / mood は絵) から合成し直すので、**生成 API は呼ばず、劣化しない**。
 *
 * rev16: 折り畳みパネルから**ダイアログ**へ。結果ペインの中に畳むと絵が列幅 (420px) に縛られ、
 * 焼き上がりを確かめる用途に足りなかった (ユーザー判断 2026-09-09「大きい画面で確認できるようにしたい」)。
 * ダイアログは左が絵・右がつまみの 2 段組で、絵は画面の高さまで使う。
 * `Teleport` で body に出すのは、この部品が `overflow: auto` の列の中に居るため。
 */
import { computed, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { FontEntry } from "../types";
import { baseName as fileName, snapshotChoices } from "../snapshots";
import { tiltLabel, tiltValue } from "../plate";
import { ask, isMessageBoxOpen } from "../dialog";
import { t } from "../i18n";
import Icon from "./Icon.vue";
import Rich from "./Rich.vue";

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
/** 見出しの縦位置 (canvas 高さ比)。null = position + 余白の既定どおり。rev14。 */
const capY = ref<number | null>(null);
const fontKey = ref(`${base.value.fontPath}#${base.value.fontIndex}`);

/**
 * はめ込み (rev11)。空欄 = 既定に従う。rev24 から **mood にも足せる** — その場合の既定は
 * 「はめ込みなし」で、スナップショットを選んだ時だけ面が乗る (`cut_kind` は書き換えない)。
 * 傾きの既定は LLM が書いた `scene.plate_tilt`、大きさとずらしの既定は見出しの帯が決める。
 */
/** コピー文 (rev12)。画像に焼かれ、scenes.md とクリップボードにも出る文そのもの。 */
const copy = ref(props.copyText);
watch(
  () => props.copyText,
  (v) => (copy.value = v),
);
const copyChanged = computed(() => copy.value !== props.copyText);

/** 使うスナップショット (rev13)。null = product なら LLM の選択のまま、mood ならはめ込みなし。 */
const snapIndex = ref<number | null>(null);

/**
 * **面が乗るか** (rev24)。product は常に乗る。mood は**人が足した時だけ**。
 * `isProduct` (LLM が決めた種別) と分けているのは、種別を書き換えずに面だけ足せるようにするため —
 * 書き換えると `image_prompt` が背景の記述でなくなり、再生成したときに mood の絵を失う。
 */
const hasPlate = computed(() => props.isProduct || snapIndex.value !== null);

/**
 * 選べる一覧 (rev20) = run に写してあるもの + **入力ペインで後から足したもの**。
 * 取り込み口は入力ペインの 1 つだけ (ドロップ / 貼り付け / ファイル選択)。ここは選ぶだけ。
 *
 * rev23: **撮り直し** (同じパスで中身が変わったもの) も下に出す。run の写しは古いまま残るので、
 * 同じファイル名が 2 行並ぶ — 上が run の中の古い写し、下が今のファイル。
 */
const choices = computed(() =>
  snapshotChoices(props.snapshots, store.project.snapshots, store.staleSnapshots),
);
const copying = ref(false);

/**
 * まだ run に写していないものが選ばれたら、その場で写す。
 * **run は自己完結する** (rev10) ので、入力ペインのパスを直に指すことはできない。
 */
async function chooseSnapshot(e: Event) {
  const v = (e.target as HTMLSelectElement).value;
  if (v === "") {
    snapIndex.value = null;
    touch();
    return;
  }
  const c = choices.value[Number(v)];
  if (!c) return;
  if (c.index !== null) {
    snapIndex.value = c.index;
    touch();
    return;
  }
  copying.value = true;
  try {
    const index = await store.addRunSnapshot(c.path);
    if (index !== null) {
      snapIndex.value = index;
      touch();
    }
  } finally {
    copying.value = false;
  }
}

/**
 * 一覧に出す肩書き。run に無いものは「入力に追加」だが、**同じパスが run にも居る**なら
 * それは撮り直しなので、そう名乗る (同じファイル名が 2 行並ぶ理由を出す)。
 */
function choiceLabel(path: string): string {
  return props.snapshots.includes(path) ? t("caption.retaken") : t("caption.addedFromInput");
}

/** 選択中が一覧の何番目か (run の番号ではなく表示上の位置)。 */
const chosen = computed(() => {
  if (snapIndex.value === null) return "";
  const i = choices.value.findIndex((c) => c.index === snapIndex.value);
  return i < 0 ? "" : String(i);
});

/** 「適用」を押すまで絵は変わらない。押し忘れが分かるように印を出す。 */
const dirty = ref(false);
function touch() {
  dirty.value = true;
  refreshQuad();
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

/**
 * **予定位置** — 面 (rev14) と見出し (rev18)。座標は backend が合成・焼き込みと同じ関数から出す。
 * 射影も版組みもここに写さない (写すと必ず食い違う)。面が無いシーンでは `quad` は null
 * (判定は backend の `plate_snapshot_index`、rev24)。
 */
interface Preview {
  canvas: [number, number];
  quad: [number, number][] | null;
  /** 1 行 1 個の [x, y, w, h] (canvas 座標)。 */
  caption_lines: [number, number, number, number][];
  /** いま効いている傾き [yaw, pitch]。合成が使う `tilt_of` の値。 */
  tilt: [number, number];
}
const previewBoxes = ref<Preview | null>(null);
let pending = 0;
async function refreshQuad() {
  const runDir = store.result?.package_dir;
  if (!runDir) return;
  const my = ++pending;
  try {
    const r = await invoke<Preview>("plate_preview", {
      runDir,
      sceneId: props.sceneId,
      spec: copy.value.trim() ? currentSpec() : null,
      plate: currentPlate(),
      copy: copy.value,
    });
    if (my === pending) previewBoxes.value = r; // 古い応答で新しい枠を上書きしない
  } catch {
    /* 枠が出ないだけ。操作は妨げない */
  }
}

/**
 * 傾きスライダーの**基準** (rev21)。触っていないときは LLM が書いた傾きを指す。
 * ここを 0 にすると、絵が傾いているのにつまみが 0° を指す嘘になる (ユーザー指摘 2026-09-09)。
 */
const baseTilt = computed<[number, number]>(() => previewBoxes.value?.tilt ?? [0, 0]);

/** 表示サイズに合わせた SVG のポリゴン点列 (面)。 */
const quadPoints = computed(() => {
  const p = previewBoxes.value;
  if (!p?.quad) return "";
  const [cw, ch] = p.canvas;
  return p.quad.map(([x, y]) => `${(x / cw) * 100},${(y / ch) * 100}`).join(" ");
});

/** 見出しの予定位置 (1 行 1 枠、canvas 比の %)。 */
const captionRects = computed(() => {
  const p = previewBoxes.value;
  if (!p) return [];
  const [cw, ch] = p.canvas;
  return p.caption_lines.map(([x, y, w, h]) => ({
    x: (x / cw) * 100,
    y: (y / ch) * 100,
    w: (w / cw) * 100,
    h: (h / ch) * 100,
  }));
});
let drag: { x: number; y: number; dx: number; dy: number } | null = null;

function onDown(e: PointerEvent) {
  if (!hasPlate.value || !preview.value) return;
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
  refreshQuad();
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
  return v === null ? t("caption.default") : `${v}${unit}`;
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

/**
 * 適用していない変更を黙って捨てない。確認はアプリ内のメッセージボックス (rev26) —
 * ブラウザ標準の確認は見出しに `localhost:1421 の内容` と出る。
 */
async function askClose() {
  // 確認が出ている間の Esc / 背景クリック / 閉じるボタンで、同じ確認を二重に積まない。
  if (isMessageBoxOpen()) return;
  if (
    dirty.value &&
    !(await ask({
      title: t("caption.unsavedTitle"),
      message: t("caption.unsavedMessage"),
      ok: t("common.close"),
      cancel: t("caption.backToEdit"),
    }))
  )
    return;
  open.value = false;
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") askClose();
}
watch(open, (v) => {
  if (v) window.addEventListener("keydown", onKey);
  else window.removeEventListener("keydown", onKey);
});
onUnmounted(() => window.removeEventListener("keydown", onKey));

/**
 * 保存済みの上書きをつまみへ戻す (rev21)。**開き直したときにつまみが実効値を指すため。**
 * 従来は毎回 null (= 既定) から始まり、適用済みの run を開き直すと
 * 絵には自分の値が焼かれているのにスライダーは既定を指していた (傾きの 0° と同じ型の嘘)。
 */
function loadSaved() {
  const promo = store.result?.promo;
  const c = promo?.caption_overrides?.[props.sceneId];
  if (c) {
    if (c.position) position.value = c.position;
    if (c.size_ratio != null) sizeRatio.value = c.size_ratio;
    if (c.color) color.value = c.color;
    capY.value = c.y_ratio ?? null;
    if (c.font_path) fontKey.value = `${c.font_path}#${c.font_index ?? 0}`;
  }
  const p = promo?.plate_overrides?.[props.sceneId];
  yaw.value = p?.yaw_degrees ?? null;
  pitch.value = p?.pitch_degrees ?? null;
  ratio.value = p?.screen_ratio ?? null;
  dx.value = p?.x_offset_ratio ?? null;
  dy.value = p?.y_offset_ratio ?? null;
  snapIndex.value = p?.snapshot_index ?? null;
}

async function toggle() {
  open.value = !open.value;
  if (!open.value) return;
  loadSaved();
  dirty.value = false;
  // **毎回**取りに行く (実効の傾きが要るので、フォントを読み込み済みでも省かない)。
  refreshQuad();
  // 撮り直しはアプリの外で起きる。開く直前に数えないと古い写しが選ばれ続ける (rev23)。
  store.refreshStaleSnapshots();
  if (!fonts.value.length) {
    try {
      fonts.value = await invoke<FontEntry[]>("list_fonts");
    } catch (e) {
      store.push("error", t("caption.fontListFailed", { error: String(e) }));
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
    y_ratio: capY.value,
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
  capY.value = null;
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
    <!-- rev24: mood にも面を足せるので、どのシーンも開ける (以前は文も面も無いと押せなかった)。アプリ固有ボタンのためテキスト保持 -->
    <button class="btn small" :disabled="store.running" @click="toggle">
      <Icon name="edit" :size="13" />
      <span>{{ hasText ? t('scene.captionAndPlate') : t('scene.plateOnly') }}</span>
    </button>
  </div>

  <!-- 結果ペインは overflow: auto の列なので、ダイアログは body へ出す。 -->
  <Teleport to="body">
    <div v-if="open" class="backdrop" @click.self="askClose">
      <div class="dlg panel">
        <div class="row" style="justify-content: space-between">
          <b>
            {{ t('caption.title', { id: sceneId }) }}
            <span class="muted" style="font-weight: 400">({{ isProduct ? "product" : "mood" }})</span>
          </b>
          <button class="btn small" :title="t('common.close')" @click="askClose">
            <Icon name="x" :size="14" />
            <span>{{ t('common.close') }}</span>
          </button>
        </div>

        <div class="cols">
          <!-- 左: 絵。ここを大きく取るためにダイアログにした (rev16)。 -->
          <div class="stage-col">
            <div v-if="store.imageUrls[sceneId]" class="stage">
              <img
                ref="preview"
                class="preview"
                :class="{ draggable: hasPlate }"
                :src="store.imageUrls[sceneId]"
                :alt="`scene ${sceneId}`"
                :title="hasPlate ? t('caption.dragHint') : ''"
                @pointerdown.prevent="onDown"
                @pointermove="onMove"
                @pointerup="onUp"
                @pointercancel="onUp"
              />
              <!-- 予定位置。座標は backend の plate_quad / caption_layout (合成・焼き込みと同じ関数) から来る。 -->
              <svg v-if="dirty && (quadPoints || captionRects.length)" class="ghost" viewBox="0 0 100 100" preserveAspectRatio="none">
                <polygon v-if="quadPoints" :points="quadPoints" />
                <rect v-for="(r, i) in captionRects" :key="i" class="cap-box" :x="r.x" :y="r.y" :width="r.w" :height="r.h" />
              </svg>
            </div>
            <p v-else class="muted note">{{ t('caption.noImage') }}</p>
            <p class="muted note"><Rich :text="t(hasPlate ? 'caption.stageNote' : 'caption.stageNoteNoPlate')" /></p>
          </div>

          <!-- 右: つまみ。縦に長いのでここだけスクロールさせる。 -->
          <div class="ctl-col">
            <label class="field">
              <span>{{ t('caption.copyLabel') }}</span>
              <textarea v-model="copy" rows="2" :placeholder="t('caption.copyPlaceholder')" @input="touch()" />
            </label>
            <div v-if="originalCopy !== null && copy !== originalCopy" class="row" style="gap: 4px">
              <button class="btn small" @click="restoreCopy">
                <Icon name="undo" :size="12" />
                <span>{{ t('caption.restoreFirst') }}</span>
              </button>
              <span class="muted" style="font-size: var(--fs-sm)">{{ t('caption.originalPrefix', { text: originalCopy ?? '' }) }}</span>
            </div>

            <template v-if="copy.trim()">
              <p v-if="!fonts.length" class="muted note">{{ t('caption.fontsLoading') }}</p>
              <label class="field">
                <span>{{ t('caption.font') }}</span>
                <select v-model="fontKey" @change="touch()">
                  <option v-for="f in fonts" :key="f.path + f.index" :value="`${f.path}#${f.index}`">
                    {{ f.has_japanese ? "🇯🇵 " : "" }}{{ f.family }}
                  </option>
                </select>
              </label>
              <div class="row">
                <label class="field" style="flex: 1">
                  <span>{{ t('caption.size') }} <b class="mono">{{ sizeRatio.toFixed(3) }}</b></span>
                  <input v-model.number="sizeRatio" type="range" min="0.02" max="0.2" step="0.005" @input="touch()" />
                </label>
                <label class="field" style="flex: 1">
                  <span>{{ t('caption.color') }}</span>
                  <input v-model="color" type="color" @change="touch()" />
                </label>
              </div>
              <label class="field">
                <span>{{ t('caption.captionY') }} <b class="mono">{{ show(capY) }}</b></span>
                <input
                  :value="capY ?? (position === 'top' ? 0.06 : 0.85)"
                  type="range"
                  min="0"
                  max="0.95"
                  step="0.01"
                  @input="capY = num($event); touch()"
                />
              </label>
            </template>

            <!-- rev24: はめ込みは mood でも足せる。種別 (cut_kind) は書き換えない。 -->
            <h4 class="sub" style="margin: 4px 0 0">{{ t('caption.plate') }}</h4>
            <label class="field">
              <span>{{ t('caption.snapshot') }}{{ copying ? t('caption.importing') : "" }}</span>
              <select :value="chosen" :disabled="copying" @change="chooseSnapshot">
                <option value="">
                  {{ isProduct ? t('caption.llmChoice', { n: (llmSnapshot ?? 0) + 1 }) : t('caption.noPlate') }}
                </option>
                <option v-for="(c, i) in choices" :key="c.path + i" :value="i">
                  {{ c.inRun ? t('caption.nth', { n: (c.index ?? 0) + 1 }) : choiceLabel(c.path) }} — {{ fileName(c.path) }}
                </option>
              </select>
            </label>
            <p class="muted note"><Rich :text="t('caption.snapshotNote')" /></p>
            <p v-if="!isProduct" class="muted note"><Rich :text="t('caption.moodNote')" /></p>

            <template v-if="hasPlate">
              <label class="field">
                <span>{{ t('caption.yaw') }} <b class="mono">{{ tiltLabel(yaw, baseTilt[0]) }}</b></span>
                <input :value="tiltValue(yaw, baseTilt[0])" type="range" min="-35" max="35" step="1"
                       @input="yaw = num($event); touch()" />
              </label>
              <label class="field">
                <span>{{ t('caption.pitch') }} <b class="mono">{{ tiltLabel(pitch, baseTilt[1]) }}</b></span>
                <input :value="tiltValue(pitch, baseTilt[1])" type="range" min="-35" max="35" step="1"
                       @input="pitch = num($event); touch()" />
              </label>
              <label class="field">
                <span>{{ t('caption.size') }} <b class="mono">{{ show(ratio) }}</b></span>
                <input :value="ratio ?? 0.78" type="range" min="0.2" max="0.95" step="0.01" @input="ratio = num($event); touch()" />
              </label>
              <label class="field">
                <span>{{ t('caption.x') }} <b class="mono">{{ show(dx) }}</b></span>
                <input :value="dx ?? 0" type="range" min="-0.4" max="0.4" step="0.01" @input="dx = num($event); touch()" />
              </label>
              <label class="field">
                <span>{{ t('caption.y') }} <b class="mono">{{ show(dy) }}</b></span>
                <input :value="dy ?? 0" type="range" min="-0.4" max="0.4" step="0.01" @input="dy = num($event); touch()" />
              </label>
              <p class="muted note"><Rich :text="t('caption.yNote')" /></p>
            </template>
          </div>
        </div>

        <div class="row foot">
          <button class="btn small primary" :class="{ on: dirty }" :disabled="busy" @click="apply">
            <Icon :name="busy ? 'refresh' : 'check'" :size="13" />
            <span>{{ busy ? "..." : dirty ? `${t('caption.apply')} (*)` : t('caption.apply') }}</span>
          </button>
          <button class="btn small" :disabled="busy" @click="reset">
            <Icon name="undo" :size="13" />
            <span>{{ t('caption.reset') }}</span>
          </button>
          <button v-if="hasText" class="btn small danger" :disabled="busy" @click="clear">
            <Icon name="trash" :size="13" />
            <span>{{ t('caption.removeCaption') }}</span>
          </button>
          <span class="muted note" style="margin-left: auto">{{ t('caption.footNote') }}</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.cap {
  margin-top: 4px;
}
.backdrop {
  position: fixed;
  inset: 0;
  background: var(--backdrop, rgb(0 0 0 / 0.65));
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  /* 履歴・設定 (40) の上。この画面からしか開かない。 */
  z-index: 45;
}
.dlg {
  width: min(1400px, 96vw);
  max-height: 92vh;
  padding: 20px;
  border-radius: var(--radius-dialog);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.cols {
  flex: 1;
  min-height: 0;
  display: grid;
  /* 絵に幅を寄せる。つまみは読める最小幅で足りる。 */
  grid-template-columns: minmax(0, 1fr) 340px;
  gap: 12px;
}
.stage-col {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
  justify-content: flex-start;
}
.ctl-col {
  min-height: 0;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-right: 4px;
}
.foot {
  gap: 4px;
  align-items: center;
}
.stage {
  position: relative;
  line-height: 0;
  /* 枠を画像にぴったり重ねるため、箱を画像そのものに縮める
     (object-fit: contain だとレターボックスのぶんだけ枠がズレる)。 */
  display: inline-block;
  max-width: 100%;
}
.ghost {
  position: absolute;
  top: 0;
  left: 0;
  /* SVG は置換要素なので inset: 0 だけでは伸びず、固有サイズ (既定 300x150) で描かれる。
     幅と高さを明示しないとポリゴンの座標が画像の箱に乗らない (実測 2026-09-09、failures #17)。 */
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.ghost polygon {
  fill: rgb(120 170 255 / 0.12);
  stroke: #78aaff;
  stroke-width: 0.4;
  vector-effect: non-scaling-stroke;
}
/* 見出しは面と別の色にする (どちらの枠を見ているか迷わせない)。 */
.ghost .cap-box {
  fill: rgb(255 210 120 / 0.14);
  stroke: #ffd278;
  stroke-width: 0.4;
  vector-effect: non-scaling-stroke;
}
.preview {
  display: block;
  max-width: 100%;
  /* ダイアログの高さから見出し・注記・操作列のぶんを引いた残り。 */
  max-height: calc(92vh - 190px);
  width: auto;
  height: auto;
  border-radius: 6px;
  touch-action: none;
  user-select: none;
}
.preview.draggable {
  cursor: grab;
}
.preview.draggable:active {
  cursor: grabbing;
}

/* 幅が足りない画面では 1 列に落として、絵を上に置く。 */
@media (max-width: 1000px) {
  .cols {
    grid-template-columns: minmax(0, 1fr);
    overflow: auto;
  }
  .ctl-col {
    overflow: visible;
  }
  .preview {
    max-height: 50vh;
  }
}
</style>
