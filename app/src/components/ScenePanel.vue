<script setup lang="ts">
/**
 * 中央ペイン: 要約 / シーン表 / 参照画像。コピーはプロバイダ別 (契約 ExportPackage.clipboard):
 * Veo / Sora はプロンプトのみ、汎用は別行にメタ。`--ar` はどこにも出さない。
 */
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { Scene } from "../types";
import Lightbox from "./Lightbox.vue";
import CaptionEditor from "./CaptionEditor.vue";

const store = useStore();
type Target = "minimax" | "generic";
const target = ref<Target>("minimax");

const promo = computed(() => store.result?.promo ?? null);

/** MiniMax (画像→動画) は画像 + 動きの短文。汎用 t2v は全文 + 別行のメタ (Veo / Sora は指示に従わないので外した)。 */
function clipboardText(s: Scene): string {
  if (target.value === "minimax") return (s.motion_prompt || s.video_prompt).trim();
  const prompt = s.video_prompt.trim();
  return promo.value ? `${prompt}\n[aspect ${promo.value.plan.aspect} | ${s.duration_seconds}s]` : prompt;
}

async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text);
    store.showToast(`${what} をコピーしました`);
  } catch (e) {
    store.push("error", `クリップボードに書けません: ${e}`);
  }
}

function copyAll() {
  if (!promo.value) return;
  const all = promo.value.plan.scenes.map((s) => `# Scene ${s.scene_id} (${s.duration_seconds}s)\n${clipboardText(s)}`).join("\n\n");
  copy(all, "全シーン");
}

async function openPackage() {
  if (!store.result) return;
  try {
    await invoke("open_folder", { path: store.result.package_dir });
  } catch (e) {
    store.push("error", String(e));
  }
}

async function copyPackage() {
  if (!store.result) return;
  const dst = await invoke<string | null>("pick_directory");
  if (!dst) return;
  try {
    const p = await invoke<string>("copy_package", { srcDir: store.result.package_dir, dstRoot: dst });
    store.showToast(`書き出しました: ${p}`);
  } catch (e) {
    store.push("error", String(e));
  }
}

const cost = computed(() => (store.result ? store.result.analyze.cost_usd + store.result.plan.cost_usd : 0));

// 参照画像の拡大表示 (生成済みのものだけを並べる)。
const refItems = computed(() =>
  (promo.value?.plan.scenes ?? [])
    .filter((s) => store.imageUrls[s.scene_id])
    .map((s) => ({ key: String(s.scene_id), url: store.imageUrls[s.scene_id], title: `Scene ${s.scene_id}`, subtitle: s.reference_image ?? "" })),
);
const refLightbox = ref<number | null>(null);
function openRef(sceneId: number) {
  const i = refItems.value.findIndex((it) => it.key === String(sceneId));
  if (i >= 0) refLightbox.value = i;
}
</script>

<template>
  <div class="panel wrap">
    <template v-if="!promo">
      <h2>結果</h2>
      <p class="muted">左でリポジトリと動画イメージを入れて実行すると、要約・シーン構成・参照画像がここに出ます。</p>
    </template>

    <template v-else>
      <div class="head">
        <div>
          <div class="app">{{ promo.summary.app_name }}</div>
          <div class="hook">{{ promo.summary.hook_copy }}</div>
          <div class="muted" style="font-size: 11px">{{ promo.summary.one_liner }} · {{ promo.summary.target_audience }}</div>
        </div>
        <div class="stats">
          <span class="chip">{{ promo.plan.total_seconds }}s · {{ promo.plan.aspect }}</span>
          <span class="chip" :title="'解析 ' + store.result!.analyze.duration_ms + ' ms / 構成 ' + store.result!.plan.duration_ms + ' ms'">{{ cost.toFixed(3) }} USD</span>
          <span class="chip" :class="store.result!.plan.attempts > 1 ? 'accent' : ''">構成 {{ store.result!.plan.attempts }} 回目で通過</span>
        </div>
      </div>

      <details class="sum">
        <summary>差別化ポイントと視覚アイデンティティ</summary>
        <ul>
          <li v-for="d in promo.summary.differentiators" :key="d">{{ d }}</li>
        </ul>
        <div class="muted" style="font-size: 12px">
          mood: {{ promo.summary.visual_identity.mood }}<br />
          palette:
          <span v-for="c in promo.summary.visual_identity.palette" :key="c" class="swatch" :style="{ background: c.match(/#[0-9a-fA-F]{6}/)?.[0] ?? 'transparent' }" :title="c"></span>
          <span v-if="store.images?.anchor"><br />anchor: {{ store.images.anchor }}</span>
        </div>
      </details>

      <div class="toolbar">
        <label class="row" style="gap: 6px">
          <span class="muted">コピー先</span>
          <select v-model="target" style="width: auto">
            <option value="minimax">MiniMax (画像→動画: 動きの短文だけ)</option>
            <option value="generic">汎用 text-to-video (全文 + 比率・尺を別行)</option>
          </select>
        </label>
        <button class="btn small" @click="copyAll">全シーンをコピー</button>
        <button class="btn small" @click="openPackage">フォルダを開く</button>
        <button class="btn small" @click="copyPackage">別フォルダへ書き出し…</button>
        <button class="btn small" :disabled="store.imaging || store.running" @click="store.makeImages()">
          {{ store.imaging ? '参照画像 生成中…' : '参照画像を生成' }}
        </button>
      </div>
      <div v-if="store.images?.truncated" class="muted" style="font-size: 11px">参照: {{ store.images.truncated }}</div>

      <div v-for="s in promo.plan.scenes" :key="s.scene_id" class="scene">
        <div class="scene-head">
          <span class="num">#{{ s.scene_id }}</span>
          <span class="chip" :class="s.cut_kind === 'product' ? 'accent' : ''">{{ s.cut_kind === 'product' ? `product · snap ${s.snapshot_index ?? '?'}` : 'mood' }}</span>
          <span class="chip">{{ s.duration_seconds }}s</span>
          <span class="chip">{{ s.shot_type }}</span>
          <span class="copytext">{{ s.copy_text }}</span>
          <button class="btn small" @click="copy(clipboardText(s), 'シーン ' + s.scene_id)">コピー</button>
        </div>
        <div class="scene-body">
          <div>
            <div class="muted" style="font-size: 11px">motion (image-to-video)</div>
            <pre class="block mono">{{ s.motion_prompt }}</pre>
            <details class="muted" style="font-size: 11px">
              <summary>video prompt (text-to-video fallback)</summary>
              <pre class="block mono">{{ s.video_prompt }}</pre>
            </details>
          </div>
          <div class="ref">
            <img v-if="store.imageUrls[s.scene_id]" :src="store.imageUrls[s.scene_id]" :alt="'scene ' + s.scene_id" class="zoom" title="クリックで拡大" @click="openRef(s.scene_id)" />
            <div v-else class="noimg muted">
              <template v-if="store.images?.results.find((r) => r.scene_id === s.scene_id && !r.ok)">
                <span class="warn">{{ store.images!.results.find((r) => r.scene_id === s.scene_id)!.error }}</span>
              </template>
              <template v-else>参照画像なし</template>
            </div>
            <CaptionEditor v-if="store.imageUrls[s.scene_id]" :scene-id="s.scene_id" :has-text="!!s.copy_text.trim()" :is-product="s.cut_kind === 'product'"
              :copy-text="s.copy_text" :original-copy="promo.original_copy?.[s.scene_id] ?? null" />
            <details class="muted" style="font-size: 11px">
              <summary>{{ s.cut_kind === 'product' ? 'backdrop prompt' : 'image prompt' }}</summary>
              <pre class="block mono">{{ s.image_prompt }}</pre>
            </details>
          </div>
        </div>
      </div>
      <Lightbox v-if="refLightbox !== null && refItems.length" :items="refItems" :index="Math.min(refLightbox, refItems.length - 1)" @update:index="refLightbox = $event" @close="refLightbox = null" />
    </template>
  </div>
</template>

<style scoped>
.wrap {
  min-height: 100%;
}
.head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}
.app {
  font-size: 18px;
  font-weight: 700;
}
.hook {
  font-size: 14px;
  color: rgb(var(--accent));
  margin: 2px 0 4px;
}
.stats {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.sum {
  margin: 10px 0;
}
.sum summary {
  cursor: pointer;
  color: rgb(var(--muted));
  font-size: 12px;
}
.swatch {
  display: inline-block;
  width: 14px;
  height: 14px;
  border-radius: 3px;
  border: 1px solid rgb(var(--line));
  margin: 0 2px;
  vertical-align: middle;
}
.toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  padding: 8px 0;
  border-top: 1px solid rgb(var(--line));
  border-bottom: 1px solid rgb(var(--line));
}
.scene {
  padding: 10px 0;
  border-bottom: 1px solid rgb(var(--line));
}
.scene-head {
  display: flex;
  gap: 8px;
  align-items: center;
}
.num {
  font-weight: 700;
  color: rgb(var(--accent2));
}
.copytext {
  flex: 1;
  font-size: 12px;
}
.scene-body {
  display: grid;
  grid-template-columns: 1fr 260px;
  gap: 10px;
  margin-top: 6px;
}
.ref img {
  width: 100%;
  border-radius: 6px;
  border: 1px solid rgb(var(--line));
}
.ref img.zoom {
  cursor: zoom-in;
}
.ref img.zoom:hover {
  border-color: rgb(var(--accent2));
}
.noimg {
  height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px dashed rgb(var(--line));
  border-radius: 6px;
  font-size: 11px;
  text-align: center;
  padding: 6px;
}
</style>
