<script setup lang="ts">
/** 左ペイン: リポジトリ / スナップショット (SnapshotStrip) / 世界観 / 尺・比率・言語 / 出力先 / 実行。 */
import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import SnapshotStrip from "./SnapshotStrip.vue";

const store = useStore();
const emit = defineEmits<{ (e: "open-settings"): void }>();

async function pickRepo() {
  const p = await invoke<string | null>("pick_directory");
  if (p) {
    store.project.projectPath = p;
    store.persist();
    store.previewBrief();
  }
}
async function pickExport() {
  const p = await invoke<string | null>("pick_directory");
  if (p) {
    store.project.exportDir = p;
    store.persist();
  }
}
const canRun = computed(
  () => !store.running && store.project.projectPath.trim() !== "" && store.project.concept.trim() !== "" && store.cliCheck?.found !== false,
);
</script>

<template>
  <div class="panel">
    <h2>入力</h2>

    <label class="field">
      <span>リポジトリ</span>
      <div class="row">
        <input v-model="store.project.projectPath" placeholder="D:\Github\my-app" @change="store.persist(); store.previewBrief()" />
        <button class="btn" @click="pickRepo">参照</button>
      </div>
    </label>
    <div v-if="store.brief" class="muted" style="font-size: var(--fs-sm)">brief {{ store.brief.chars }} 字 / tree {{ store.brief.tree_lines }} 行</div>

    <SnapshotStrip />

    <label class="field">
      <span>動画イメージ / 世界観</span>
      <textarea v-model="store.project.concept" rows="5" placeholder="ミニマリスト向けの生産性ツール。落ち着いたトーン、シネマティックなライティング、4K。" @change="store.persist()"></textarea>
    </label>

    <div class="row">
      <label class="field" style="flex: 1">
        <span>尺</span>
        <select v-model.number="store.project.seconds" @change="store.persist()">
          <option :value="15">15 秒</option>
          <option :value="30">30 秒</option>
          <option :value="60">60 秒</option>
        </select>
      </label>
      <label class="field" style="flex: 1">
        <span>比率</span>
        <select v-model="store.project.aspect" @change="store.persist()">
          <option value="16:9">16:9</option>
          <option value="9:16">9:16</option>
          <option value="1:1">1:1</option>
        </select>
      </label>
      <label class="field" style="flex: 1">
        <span>コピー言語</span>
        <select v-model="store.project.lang" @change="store.persist()">
          <option value="ja">日本語</option>
          <option value="en">English</option>
        </select>
      </label>
    </div>

    <label class="field">
      <span>出力先フォルダ</span>
      <div class="row">
        <input v-model="store.project.exportDir" placeholder="(未指定なら作業フォルダ)" @change="store.persist()" />
        <button class="btn" @click="pickExport">参照</button>
      </div>
    </label>

    <div class="cli-line">
      <span class="chip" :class="store.cliCheck ? (store.cliCheck.found ? 'ok' : 'warn') : ''">
        LLM: {{ store.cli.kind }} {{ store.cliCheck?.found ? store.cliCheck.version : store.cliCheck ? '見つかりません' : '検査中…' }}
      </span>
      <span class="chip" :class="store.image.enabled ? 'accent' : ''">画像: {{ store.image.enabled ? store.image.provider : 'off' }}</span>
      <button class="btn small" @click="emit('open-settings')">設定</button>
    </div>
    <div v-if="store.cliCheck && !store.cliCheck.found" class="warn" style="font-size: var(--fs-sm); margin-top: 4px">{{ store.cliCheck.error }}</div>

    <div class="row" style="margin-top: 12px">
      <button class="btn primary" :disabled="!canRun" @click="store.run()">
        {{ store.running ? '実行中…' : '解析 → シーン構成' }}
      </button>
      <button v-if="store.running" class="btn danger" @click="store.cancel()">中断</button>
    </div>
    <div v-if="store.error" class="warn" style="margin-top: 8px; white-space: pre-wrap">{{ store.error }}</div>
  </div>
</template>

<style scoped>
.cli-line {
  display: flex;
  gap: 6px;
  align-items: center;
  flex-wrap: wrap;
  margin-top: 8px;
}
</style>
