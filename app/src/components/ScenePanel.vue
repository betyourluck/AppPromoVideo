<script setup lang="ts">
/**
 * 中央ペイン: 要約 / シーン表 / 参照画像。コピーはプロバイダ別 (契約 ExportPackage.clipboard):
 * Veo / Sora はプロンプトのみ、汎用は別行にメタ。`--ar` はどこにも出さない。
 */
import { computed, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { runHeaderStats } from "../runs";
import type { Scene, SceneOp } from "../types";
import { t } from "../i18n";
import Lightbox from "./Lightbox.vue";
import CaptionEditor from "./CaptionEditor.vue";
import Icon from "./Icon.vue";
import Rich from "./Rich.vue";
import { ask } from "../dialog";

const store = useStore();
type Target = "minimax" | "generic";
const target = ref<Target>("minimax");
const copiedSceneId = ref<number | null>(null);

const promo = computed(() => store.result?.promo ?? null);

/**
 * プロンプトの編集モード (rev37、ユーザー「鉛筆を押してシーンの編集モードにしてから書き換えたい」)。
 * **常に書き換えられる欄にしない** — 読むつもりの操作で壊れるため。入れるのは 1 シーンずつ。
 * 保存は backend (promo.json と scenes.md を書き直す)。破棄は入る前の値に戻すだけ。
 */
const editingSceneId = ref<number | null>(null);
const draft = ref({ motion: "", video: "" });
const savingPrompts = ref(false);
const busyScene = ref(false);

const scenes = computed(() => promo.value?.plan.scenes ?? []);

/** 尺の合計と `total_seconds` のずれ (秒)。**直さない、出すだけ** (契約 `SceneEdit.duration`)。 */
const durationDrift = computed(() => {
  const p = promo.value;
  if (!p) return 0;
  return scenes.value.reduce((n, s) => n + s.duration_seconds, 0) - p.plan.total_seconds;
});

/** 並び替え / 複製 / 削除 (rev43)。**backend が返した promo に差し替わる** — 手元で並べ替えない。 */
async function edit(op: SceneOp, sceneId: number) {
  busyScene.value = true;
  try {
    await store.editScenes(op, sceneId);
  } finally {
    busyScene.value = false;
  }
}

/** 削除は取り消せない (参照画像も消える) ので確認する。ブラウザ標準は使わない (rev26)。 */
async function askRemove(sceneId: number) {
  // 取り返しがつかない (参照画像も消える) ので danger。
  const ok = await ask({
    title: t("scene.removeTitle"),
    message: t("scene.removeMessage", { id: sceneId }),
    ok: t("scene.remove"),
    danger: true,
  });
  if (ok) await edit("remove", sceneId);
}

function startEdit(s: Scene) {
  editingSceneId.value = s.scene_id;
  draft.value = { motion: s.motion_prompt, video: s.video_prompt };
}

async function savePrompts(sceneId: number) {
  savingPrompts.value = true;
  try {
    if (await store.updateScenePrompts(sceneId, draft.value.motion, draft.value.video)) editingSceneId.value = null;
  } finally {
    savingPrompts.value = false;
  }
}

/** MiniMax (画像→動画) は画像 + 動きの短文。汎用 t2v は全文 + 別行のメタ (Veo / Sora は指示に従わないので外した)。 */
function clipboardText(s: Scene): string {
  if (target.value === "minimax") return (s.motion_prompt || s.video_prompt).trim();
  const prompt = s.video_prompt.trim();
  return promo.value ? `${prompt}\n[aspect ${promo.value.plan.aspect} | ${s.duration_seconds}s]` : prompt;
}

async function copy(text: string, msg: string) {
  try {
    await navigator.clipboard.writeText(text);
    store.showToast(msg);
  } catch (e) {
    store.push("error", t("scene.clipboardFailed", { error: String(e) }));
  }
}

function copyAll() {
  if (!promo.value) return;
  const all = promo.value.plan.scenes.map((s) => `# Scene ${s.scene_id} (${s.duration_seconds}s)\n${clipboardText(s)}`).join("\n\n");
  copy(all, t("scene.toastCopyAll"));
}

async function copyScene(s: Scene) {
  await copy(clipboardText(s), t("scene.toastCopyScene", { id: s.scene_id }));
  copiedSceneId.value = s.scene_id;
  setTimeout(() => {
    if (copiedSceneId.value === s.scene_id) {
      copiedSceneId.value = null;
    }
  }, 1600);
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
    store.showToast(t("scene.toastExport", { path: p }));
  } catch (e) {
    store.push("error", String(e));
  }
}

/**
 * 見出しの数値 (rev36)。正本は promo.json の `run_stats` で、実行直後の stage は経過時間のためだけに使う。
 * 履歴から開いた run は stage を持たないので、そこを描くと無い記録 (0 回 / 0.000 USD) が出る。
 */
const header = computed(() => {
  const empty = { attempts: 0, cost_usd: null, duration_ms: 0, violations: [] };
  return runHeaderStats(store.result?.promo.run_stats, store.result?.analyze ?? empty, store.result?.plan ?? empty);
});

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
  <!-- 案内 (rev44) の 4 歩目: 出力エリア全体。 -->
  <div class="panel wrap" data-tour="result">
    <template v-if="!promo">
      <h2>{{ t('scene.result') }}</h2>
      <div class="empty-guide">
        <Icon name="sparkles" :size="32" class="guide-icon" />
        <p class="muted">{{ t('scene.emptyGuide') }}</p>
      </div>
    </template>

    <template v-else>
      <div class="head">
        <div>
          <div class="app">{{ promo.summary.app_name }}</div>
          <div class="hook">{{ promo.summary.hook_copy }}</div>
          <div class="muted" style="font-size: var(--fs-sm)">{{ promo.summary.one_liner }} · {{ promo.summary.target_audience }}</div>
        </div>
        <div class="stats">
          <span class="chip">{{ promo.plan.total_seconds }}s · {{ promo.plan.aspect }}</span>
          <span
            v-if="header.costKnown"
            class="chip"
            :title="header.durationsKnown ? t('scene.durationsTitle', { analyze: store.result!.analyze.duration_ms, plan: store.result!.plan.duration_ms }) : ''"
          >{{ header.cost }} USD</span>
          <span v-if="header.attemptsKnown" class="chip" :class="header.attempts.retried ? 'accent' : ''" :title="header.attempts.title">
            {{ t('scene.attempts', { attempts: header.attempts.text }) }}
          </span>
        </div>
      </div>

      <details class="sum">
        <summary>{{ t('scene.differentiators') }}</summary>
        <div class="sum-body">
          <ul class="differentiators">
            <li v-for="d in promo.summary.differentiators" :key="d">{{ d }}</li>
          </ul>
          <div class="muted visual-meta" style="font-size: var(--fs-md)">
            <span>mood: {{ promo.summary.visual_identity.mood }}</span>
            <div class="palette-row">
              <span>palette:</span>
              <span v-for="c in promo.summary.visual_identity.palette" :key="c" class="swatch" :style="{ background: c.match(/#[0-9a-fA-F]{6}/)?.[0] ?? 'transparent' }" :title="c"></span>
            </div>
            <span v-if="store.images?.anchor">anchor: {{ store.images.anchor }}</span>
          </div>
        </div>
      </details>

      <div class="toolbar">
        <label class="row" style="gap: 6px">
          <span class="muted" style="font-size: var(--fs-sm)">{{ t('scene.copyTarget') }}</span>
          <select v-model="target" :disabled="store.running" style="width: auto">
            <option value="minimax">{{ t('scene.targetMinimax') }}</option>
            <option value="generic">{{ t('scene.targetGeneric') }}</option>
          </select>
        </label>
        <div class="spacer"></div>
        <button class="btn small" :disabled="store.running" @click="copyAll">
          <Icon name="copy" :size="13" />
          <span>{{ t('scene.copyAll') }}</span>
        </button>
        <button class="btn small" :disabled="store.running" @click="openPackage">
          <Icon name="folder" :size="13" />
          <span>{{ t('scene.openPackage') }}</span>
        </button>
        <button class="btn small" :disabled="store.running" @click="copyPackage">
          <Icon name="download" :size="13" />
          <span>{{ t('scene.exportPackage') }}</span>
        </button>
        <!-- アプリ固有の重要アクション: テキスト「参照画像を生成」は維持 -->
        <button class="btn small primary" :disabled="store.imaging || store.running" @click="store.makeImages()">
          <Icon :name="store.imaging ? 'refresh' : 'image'" :size="13" />
          <span>{{ store.imaging ? t('scene.makingImages') : t('scene.makeImages') }}</span>
        </button>
      </div>
      <div v-if="store.images?.truncated" class="muted" style="font-size: var(--fs-sm); margin: 4px 0">{{ t('scene.refsTruncated', { detail: store.images?.truncated ?? '' }) }}</div>

      <div class="scene-list">
        <!-- 尺の合計と指定のずれ。**直さない、出すだけ** (契約 SceneEdit.duration)。 -->
        <p v-if="durationDrift !== 0" class="muted note drift">
          <Rich :text="t(durationDrift > 0 ? 'scene.driftOver' : 'scene.driftUnder', { n: Math.abs(durationDrift), total: promo.plan.total_seconds })" />
        </p>
        <div v-for="(s, i) in scenes" :key="s.scene_id" class="scene-card">
          <div class="scene-head">
            <span class="num">#{{ s.scene_id }}</span>
            <span class="chip" :class="s.cut_kind === 'product' ? 'accent' : ''">{{ s.cut_kind === 'product' ? `product · snap ${s.snapshot_index ?? '?'}` : 'mood' }}</span>
            <span class="chip">{{ s.duration_seconds }}s</span>
            <span class="chip">{{ s.shot_type }}</span>
            <span class="copytext">{{ s.copy_text }}</span>
            <template v-if="editingSceneId === s.scene_id">
              <button class="btn small primary" :disabled="savingPrompts" :title="t('common.save')" @click="savePrompts(s.scene_id)">
                <Icon :name="savingPrompts ? 'refresh' : 'check'" :size="13" />
                <span>{{ t('common.save') }}</span>
              </button>
              <button class="btn small" :disabled="savingPrompts" :title="t('common.cancel')" @click="editingSceneId = null">
                <Icon name="x" :size="13" />
              </button>
            </template>
            <button v-else class="btn small" :disabled="store.running" :title="t('scene.editPrompts')" @click="startEdit(s)">
              <Icon name="edit" :size="13" />
            </button>
            <!-- 並び替え / 複製 / 削除 (rev43)。**端と限界では押せない** — 拒否は backend にもあるが、
                 押せるのに必ず失敗するボタンは出さない。 -->
            <button class="btn small" :disabled="busyScene || i === 0" :title="t('scene.moveUp')" @click="edit('move_up', s.scene_id)">
              <Icon name="chevron-up" :size="13" />
            </button>
            <button
              class="btn small"
              :disabled="busyScene || i === scenes.length - 1"
              :title="t('scene.moveDown')"
              @click="edit('move_down', s.scene_id)"
            >
              <Icon name="chevron-down" :size="13" />
            </button>
            <button class="btn small" :disabled="busyScene || scenes.length >= 8" :title="t('scene.duplicate')" @click="edit('duplicate', s.scene_id)">
              <Icon name="plus" :size="13" />
            </button>
            <button class="btn small" :disabled="busyScene || scenes.length <= 3" :title="t('scene.remove')" @click="askRemove(s.scene_id)">
              <Icon name="trash" :size="13" />
            </button>
            <button
              class="btn small copy-btn"
              :disabled="store.running"
              :class="{ ok: copiedSceneId === s.scene_id }"
              :title="copiedSceneId === s.scene_id ? t('scene.copiedPrompt') : t('scene.copyPrompt')"
              @click="copyScene(s)"
            >
              <Icon :name="copiedSceneId === s.scene_id ? 'check' : 'copy'" :size="13" />
              <span>{{ copiedSceneId === s.scene_id ? t('scene.copiedPrompt') : t('common.copy') }}</span>
            </button>
          </div>
          <div class="scene-body">
            <div class="prompts-col">
              <div class="muted prompt-label">{{ t('scene.motionPrompt') }}</div>
              <textarea v-if="editingSceneId === s.scene_id" v-model="draft.motion" class="mono prompt-edit" rows="3"></textarea>
              <pre v-else class="block mono">{{ s.motion_prompt }}</pre>
              <details class="muted prompt-details" :open="editingSceneId === s.scene_id">
                <summary>{{ t('scene.videoPromptFallback') }}</summary>
                <textarea v-if="editingSceneId === s.scene_id" v-model="draft.video" class="mono prompt-edit" rows="4"></textarea>
                <pre v-else class="block mono">{{ s.video_prompt }}</pre>
              </details>
            </div>
            <div class="ref-col">
              <img v-if="store.imageUrls[s.scene_id]" :src="store.imageUrls[s.scene_id]" :alt="'scene ' + s.scene_id" class="zoom" :class="{ disabled: store.running }" :title="t('scene.zoomTitle')" @click="store.running ? undefined : openRef(s.scene_id)" />
              <div v-else class="noimg muted">
                <template v-if="store.images?.results.find((r) => r.scene_id === s.scene_id && !r.ok)">
                  <span class="warn">{{ store.images!.results.find((r) => r.scene_id === s.scene_id)!.error }}</span>
                </template>
                <template v-else>{{ t('scene.noImage') }}</template>
              </div>
              <CaptionEditor v-if="store.imageUrls[s.scene_id]" :scene-id="s.scene_id" :has-text="!!s.copy_text.trim()" :is-product="s.cut_kind === 'product'"
                :copy-text="s.copy_text" :original-copy="promo.original_copy?.[s.scene_id] ?? null"
                :snapshots="promo.snapshot_paths" :llm-snapshot="s.snapshot_index ?? null" />
              <details class="muted prompt-details">
                <summary>{{ s.cut_kind === 'product' ? t('scene.backdropPrompt') : t('scene.imagePrompt') }}</summary>
                <pre class="block mono">{{ s.image_prompt }}</pre>
              </details>
            </div>
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
.empty-guide {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 48px 24px;
  text-align: center;
}
.guide-icon {
  color: rgb(var(--accent2) / 0.5);
  margin-bottom: 12px;
}
.head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
  margin-bottom: 8px;
}
.app {
  font-size: var(--fs-h-xl);
  font-weight: var(--fw-bold);
  letter-spacing: -0.01em;
}
.hook {
  font-size: var(--fs-h-lg);
  color: rgb(var(--accent));
  margin: 3px 0 5px;
}
.stats {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.sum {
  margin: 10px 0;
  border-radius: var(--radius-md);
  border: 1px solid rgb(var(--line-subtle, var(--line)));
  background: rgb(var(--panel-hover) / 0.5);
  padding: 8px 12px;
}
.sum summary {
  cursor: pointer;
  color: rgb(var(--muted));
  font-size: var(--fs-md);
  font-weight: var(--fw-semi);
  user-select: none;
}
.sum-body {
  margin-top: 8px;
}
.differentiators {
  margin: 4px 0 8px 18px;
  padding: 0;
}
.visual-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.palette-row {
  display: flex;
  align-items: center;
  gap: 4px;
}
.swatch {
  display: inline-block;
  width: 16px;
  height: 16px;
  border-radius: 4px;
  border: 1px solid rgb(var(--line));
  vertical-align: middle;
  box-shadow: var(--shadow-sm);
}
.toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  padding: 10px 0;
  border-top: 1px solid rgb(var(--line));
  border-bottom: 1px solid rgb(var(--line));
  margin: 10px 0;
}
.spacer {
  flex: 1;
}
.scene-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.scene-card {
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-lg);
  padding: 16px;
  background: rgb(var(--panel-hover) / 0.25);
  transition: border-color var(--trans-fast), box-shadow var(--trans-fast);
}
.scene-card:hover {
  border-color: rgb(var(--accent2) / 0.4);
  box-shadow: var(--shadow-sm);
}
.scene-head {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
.num {
  font-weight: var(--fw-bold);
  font-size: var(--fs-lg);
  color: rgb(var(--accent2));
}
.copytext {
  flex: 1;
  font-size: var(--fs-md);
  min-width: 140px;
}
.copy-btn {
  margin-left: auto;
}
.copy-btn.ok {
  border-color: rgb(var(--ok));
  color: rgb(var(--ok));
}
.scene-body {
  display: grid;
  grid-template-columns: 1fr 270px;
  gap: 12px;
  margin-top: 10px;
}
.prompts-col {
  min-width: 0;
}
.prompt-label {
  font-size: var(--fs-xs);
  margin-bottom: 2px;
}
/* 編集モードの入力欄 (rev37)。読むときの `pre.block` と同じ見え方に揃える。 */
.prompt-edit {
  width: 100%;
  font-size: var(--fs-xs);
  line-height: 1.55;
  min-height: 0;
  padding: 8px 10px;
}
.prompt-details {
  margin-top: 6px;
}
.prompt-details summary {
  cursor: pointer;
  user-select: none;
}
.ref-col img {
  width: 100%;
  border-radius: var(--radius-sm);
  border: 1px solid rgb(var(--line));
  background: rgb(var(--bg));
  transition: border-color var(--trans-fast), transform var(--trans-fast);
}
.ref-col img.zoom {
  cursor: zoom-in;
}
.ref-col img.zoom:hover {
  border-color: rgb(var(--accent2));
  transform: scale(1.01);
}
.ref-col img.zoom.disabled {
  cursor: default;
  pointer-events: none;
}
.ref-col img.zoom.disabled:hover {
  border-color: rgb(var(--line));
  transform: none;
}
.noimg {
  height: 120px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1.5px dashed rgb(var(--line));
  border-radius: var(--radius-sm);
  font-size: var(--fs-sm);
  text-align: center;
  padding: 8px;
  background: rgb(var(--bg) / 0.5);
}
</style>
