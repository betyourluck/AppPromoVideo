<script setup lang="ts">
/**
 * 初回起動のナビゲーション (rev44、ユーザー要望)。移植元は Lorekeel の `FirstRunTour.vue`。
 *
 * **手順を教えるだけ** — 案内の中から設定や実行はさせない (ユーザー決定 2026-09-12。
 * Lorekeel と同じ流儀)。挨拶の幕のあと、**実物の UI 要素**をスポットライトが順に照らし、
 * 横に番号つきのカードが出る。対象は `data-tour="…"` 属性で印を付けた要素で、この部品は
 * `querySelectorAll` で測るだけ = 対象側の部品はこの部品を知らない。
 *
 * 1 歩目は印が 2 つある (入力欄の先頭と末尾) — **実行ボタン以外をまとめて**囲むため、
 * 矩形を束ねる (`spotlightUnion`)。包む div を足すと余白の出方が変わるので足さない。
 *
 * 依存ゼロ・CSS のトランジションだけ・動的 import で初回にしか読まれない (App.vue)。
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";

import { t } from "../i18n";
import type { MessageKey } from "../i18n";
import { TOUR_DONE_KEY, TOUR_STEPS, type Box, type CardPlacement, type TourStep, placeCard, spotlightBox, spotlightUnion } from "../tour";

/**
 * 歩ごとの文言。**キーは字面で書く** — `t(`tour.${step}.title`)` と組み立てると、
 * 型がキーを検めず、未使用キーの網 (`i18nUnusedKeys.test.ts`) からも見えなくなる。
 */
const STEP_TEXT: Record<TourStep, { title: MessageKey; body: MessageKey }> = {
  input: { title: "tour.input.title", body: "tour.input.body" },
  settings: { title: "tour.settings.title", body: "tour.settings.body" },
  run: { title: "tour.run.title", body: "tour.run.body" },
  result: { title: "tour.result.title", body: "tour.result.body" },
};

const emit = defineEmits<{ (e: "close"): void }>();

const phase = ref<"welcome" | "tour">("welcome");
const idx = ref(0);
const step = computed(() => TOUR_STEPS[idx.value]);
const total = TOUR_STEPS.length;

const spot = ref<Box>({ left: 0, top: 0, width: 0, height: 0 });
const card = ref<CardPlacement>({ left: 0, top: 0, side: "below", arrow: 18 });
const cardEl = ref<HTMLElement | null>(null);
const measured = ref(false);

/** カードの幅は CSS (22rem / 最大 92vw) から決まる。実寸を測れない瞬間もこの値で置ける。 */
function cardWidthFromCss(vpWidth: number): number {
  const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  return Math.min(22 * rem, vpWidth * 0.92);
}

/** `sizeEl` = 寸法を測るカード要素。Transition の enter 中は ref がまだ新しい要素を指していない。 */
function measure(sizeEl?: HTMLElement | null) {
  const els = Array.from(document.querySelectorAll<HTMLElement>(`[data-tour="${step.value}"]`));
  const vp = { width: window.innerWidth, height: window.innerHeight };
  const union = spotlightUnion(els.map((el) => el.getBoundingClientRect()));
  // 対象が見つからない = レイアウトが変わった。それでも案内は続けられるよう画面中央に小さく置く。
  spot.value = union
    ? spotlightBox(union, 8)
    : { left: vp.width / 2 - 24, top: vp.height / 2 - 24, width: 48, height: 48 };

  const src = sizeEl ?? cardEl.value;
  const size =
    src && src.offsetWidth > 0
      ? { width: src.offsetWidth, height: src.offsetHeight }
      : { width: cardWidthFromCss(vp.width), height: 190 };
  card.value = placeCard(spot.value, vp, size, 14);
  measured.value = true;
}

async function remeasure() {
  await nextTick();
  measure();
  // カードの中身 (文言の長さ) で高さが変わるので、描画後にもう一度置き直す。
  await nextTick();
  measure();
}

function begin() {
  phase.value = "tour";
  idx.value = 0;
  void remeasure();
}
function next() {
  if (idx.value + 1 >= total) return finish();
  idx.value += 1;
  void remeasure();
}
function back() {
  if (idx.value === 0) return;
  idx.value -= 1;
  void remeasure();
}
function finish() {
  try {
    localStorage.setItem(TOUR_DONE_KEY, "1");
  } catch {
    /* storage が書けない環境でも案内自体は閉じる */
  }
  emit("close");
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    finish();
  } else if (e.key === "Enter" || e.key === "ArrowRight") {
    e.preventDefault();
    if (phase.value === "welcome") begin();
    else next();
  } else if (e.key === "ArrowLeft" && phase.value === "tour") {
    e.preventDefault();
    back();
  }
}
function onResize() {
  measure();
}
onMounted(() => {
  window.addEventListener("keydown", onKey);
  window.addEventListener("resize", onResize);
});
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  window.removeEventListener("resize", onResize);
});
watch(phase, () => void remeasure());

const spotStyle = computed(() => ({
  left: `${spot.value.left}px`,
  top: `${spot.value.top}px`,
  width: `${spot.value.width}px`,
  height: `${spot.value.height}px`,
}));
const cardStyle = computed(() => ({
  left: `${card.value.left}px`,
  top: `${card.value.top}px`,
  "--arrow": `${card.value.arrow}px`,
}));
</script>

<template>
  <div class="tour-root" role="dialog" aria-modal="true" :aria-label="t('tour.welcomeTitle')">
    <!-- ===== 挨拶の幕 ===== -->
    <Transition name="tour-fade">
      <div v-if="phase === 'welcome'" class="tour-welcome" @click.self="begin">
        <div class="tour-welcome-card">
          <p class="tour-brand brand-line"><span class="outcasts">Outcasts</span> AppPromoVideo</p>
          <h2 class="tour-title">{{ t('tour.welcomeTitle') }}</h2>
          <p class="tour-lead">{{ t('tour.welcomeLead') }}</p>
          <ol class="tour-minis">
            <li v-for="(s, i) in TOUR_STEPS" :key="s" class="tour-mini" :style="{ '--d': `${0.5 + i * 0.14}s` }">
              <span class="tour-num">{{ i + 1 }}</span>
              <span class="tour-mini-text">{{ t(STEP_TEXT[s].title) }}</span>
            </li>
          </ol>
          <div class="tour-actions">
            <button class="btn primary" autofocus @click="begin">{{ t('tour.start') }}</button>
            <button class="btn" @click="finish">{{ t('tour.skip') }}</button>
          </div>
          <p class="tour-keys">{{ t('tour.keys') }}</p>
        </div>
      </div>
    </Transition>

    <!-- ===== コーチマーク ===== -->
    <template v-if="phase === 'tour'">
      <!-- スポットライト: 巨大な box-shadow で周囲を暗くし、対象だけ素の UI が見える。
           left/top/width/height に遷移を掛ける = 対象の間を滑って移動する。 -->
      <div class="tour-spot" :class="{ ready: measured }" :style="spotStyle" @click.stop="next" />
      <!-- 暗幕のどこを押しても次へ (対象以外は操作させない) -->
      <div class="tour-veil" @click="next" />

      <!-- out-in なので新しいカードは古いのが消えてから入る = 入った要素の実寸で置き直す。 -->
      <Transition name="tour-card" mode="out-in" @enter="(el) => measure(el as HTMLElement)">
        <div :key="step" ref="cardEl" class="tour-card" :class="`side-${card.side}`" :style="cardStyle" @click.stop>
          <div class="tour-card-head">
            <span class="tour-num lg">{{ idx + 1 }}</span>
            <h3>{{ t(STEP_TEXT[step].title) }}</h3>
            <span class="tour-count">{{ idx + 1 }} / {{ total }}</span>
          </div>
          <p class="tour-body">{{ t(STEP_TEXT[step].body) }}</p>
          <div class="tour-actions">
            <button v-if="idx + 1 < total" class="btn primary small" @click="next">{{ t('tour.next') }}</button>
            <button v-else class="btn primary small" @click="finish">{{ t('tour.finish') }}</button>
            <button v-if="idx > 0" class="btn small" @click="back">{{ t('tour.back') }}</button>
            <button class="btn small tour-skip" @click="finish">{{ t('tour.skip') }}</button>
          </div>
          <div class="tour-dots">
            <span v-for="(s, i) in TOUR_STEPS" :key="s" class="tour-dot" :class="{ on: i <= idx }" />
          </div>
        </div>
      </Transition>
    </template>
  </div>
</template>

<style scoped>
.tour-root {
  position: fixed;
  inset: 0;
  z-index: 70;
}

/* ===== 挨拶の幕 ===== */
.tour-welcome {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background:
    radial-gradient(ellipse 70% 55% at 50% 110%, rgb(var(--accent) / 0.2), transparent 70%),
    rgb(var(--bg) / 0.94);
}
.tour-welcome-card {
  width: 34rem;
  max-width: 92vw;
  padding: 32px 32px 26px;
  text-align: center;
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-dialog);
  background: rgb(var(--panel));
  box-shadow: var(--shadow-lg);
  animation: tour-in 0.5s cubic-bezier(0.2, 0.8, 0.2, 1) both;
}
/* 色・書体・グローは main.css の .brand-line / .outcasts と共有 (rev46、ユーザー指摘)。
   **大きさだけ**ここで決める — 幕の見出しとして読める寸法が要る。 */
.tour-brand {
  margin: 0;
  font-size: calc(var(--fs-chrome) * 1.7);
}
.tour-title {
  margin: 14px 0 0;
  font-size: var(--fs-h-lg);
  font-weight: var(--fw-bold);
}
.tour-lead {
  margin: 8px 0 0;
  color: rgb(var(--muted));
  font-size: var(--fs-sm);
  line-height: 1.7;
}
.tour-minis {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 8px;
  margin: 22px 0 0;
  padding: 0;
  list-style: none;
  text-align: left;
}
.tour-mini {
  padding: 10px;
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-md);
  background: rgb(var(--bg) / 0.6);
  animation: tour-rise 0.5s cubic-bezier(0.2, 0.8, 0.2, 1) both;
  animation-delay: var(--d, 0s);
}
.tour-mini-text {
  display: block;
  margin-top: 6px;
  font-size: var(--fs-sm);
  line-height: 1.5;
  color: rgb(var(--text) / 0.85);
}
.tour-num {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-full);
  background: var(--accent-gradient);
  color: #fff;
  font-size: var(--fs-sm);
  font-weight: var(--fw-bold);
}
.tour-num.lg {
  width: 26px;
  height: 26px;
}
.tour-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 18px;
  justify-content: center;
}
.tour-keys {
  margin: 10px 0 0;
  font-size: var(--fs-sm);
  color: rgb(var(--muted) / 0.7);
}

/* ===== コーチマーク ===== */
.tour-veil {
  position: absolute;
  inset: 0;
}
.tour-spot {
  position: fixed;
  border-radius: var(--radius-md);
  /* 巨大な box-shadow で周囲だけ暗くする = 対象は素の UI が見える。 */
  box-shadow:
    0 0 0 200vmax rgb(6 5 10 / 0.72),
    0 0 0 2px rgb(var(--accent) / 0.9),
    0 0 22px 4px rgb(var(--accent) / 0.4);
  opacity: 0;
  cursor: pointer;
  transition:
    opacity 0.3s ease,
    left 0.5s cubic-bezier(0.2, 0.8, 0.2, 1),
    top 0.5s cubic-bezier(0.2, 0.8, 0.2, 1),
    width 0.5s cubic-bezier(0.2, 0.8, 0.2, 1),
    height 0.5s cubic-bezier(0.2, 0.8, 0.2, 1);
}
.tour-spot.ready {
  opacity: 1;
}
.tour-card {
  position: fixed;
  width: 22rem;
  max-width: 92vw;
  padding: 14px 16px;
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-lg);
  background: rgb(var(--panel));
  box-shadow: var(--shadow-lg);
}
/* 対象を指す三角。カードを画面内へ寄せても --arrow が対象の中心を指し続ける。 */
.tour-card::before {
  content: "";
  position: absolute;
  width: 10px;
  height: 10px;
  background: rgb(var(--panel));
  border: 1px solid rgb(var(--line));
  transform: rotate(45deg);
}
.tour-card.side-below::before {
  top: -6px;
  left: var(--arrow, 18px);
  border-right: none;
  border-bottom: none;
}
.tour-card.side-above::before {
  bottom: -6px;
  left: var(--arrow, 18px);
  border-left: none;
  border-top: none;
}
.tour-card.side-right::before {
  left: -6px;
  top: var(--arrow, 18px);
  border-right: none;
  border-top: none;
}
.tour-card.side-left::before {
  right: -6px;
  top: var(--arrow, 18px);
  border-left: none;
  border-bottom: none;
}
.tour-card-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.tour-card-head h3 {
  margin: 0;
  font-size: var(--fs-lg);
  font-weight: var(--fw-bold);
  line-height: 1.3;
}
.tour-count {
  margin-left: auto;
  font-size: var(--fs-sm);
  color: rgb(var(--muted));
  font-variant-numeric: tabular-nums;
}
.tour-body {
  margin: 10px 0 0;
  font-size: var(--fs-sm);
  line-height: 1.7;
  color: rgb(var(--text) / 0.85);
}
.tour-card .tour-actions {
  justify-content: flex-start;
  margin-top: 14px;
}
.tour-skip {
  margin-left: auto;
}
.tour-dots {
  display: flex;
  gap: 6px;
  margin-top: 12px;
}
.tour-dot {
  width: 18px;
  height: 3px;
  border-radius: var(--radius-full);
  background: rgb(var(--line));
}
.tour-dot.on {
  background: rgb(var(--accent));
}

/* ===== 出入り ===== */
@keyframes tour-in {
  from {
    opacity: 0;
    transform: translateY(12px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
@keyframes tour-rise {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
.tour-fade-leave-active {
  transition: opacity 0.3s ease;
}
.tour-fade-leave-to {
  opacity: 0;
}
.tour-card-enter-active,
.tour-card-leave-active {
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}
.tour-card-enter-from,
.tour-card-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
</style>
