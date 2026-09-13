<script setup lang="ts">
/** 左ペイン: リポジトリ / スナップショット (SnapshotStrip) / 世界観 / 尺・比率・言語 / 出力先 / 実行。 */
import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { t } from "../i18n";
import SnapshotStrip from "./SnapshotStrip.vue";
import Icon from "./Icon.vue";

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
  <div class="panel input-pane">
    <h2>{{ t('input.title') }}</h2>

    <!-- 案内 (rev44) の 1 歩目。**先頭と末尾の 2 か所**に印を付け、矩形を束ねて
         「実行ボタン以外」を覆う (包む div を足すと余白の出方が変わるため)。 -->
    <label class="field" data-tour="input">
      <span>{{ t('input.repo') }}</span>
      <div class="row">
        <input v-model="store.project.projectPath" :disabled="store.running" :placeholder="t('input.repoPlaceholder')" @change="store.persist(); store.previewBrief()" />
        <button class="btn" :disabled="store.running" :title="t('input.pickRepo')" @click="pickRepo">
          <Icon name="folder" :size="15" />
          <span>{{ t('common.browse') }}</span>
        </button>
      </div>
    </label>
    <div v-if="store.brief" class="muted" style="font-size: var(--fs-sm)">{{ t('input.briefInfo', { chars: store.brief.chars, tree_lines: store.brief.tree_lines }) }}</div>

    <SnapshotStrip />

    <label class="field">
      <span>{{ t('input.concept') }}</span>
      <textarea v-model="store.project.concept" :disabled="store.running" rows="5" :placeholder="t('input.conceptPlaceholder')" @change="store.persist()"></textarea>
    </label>

    <div class="row select-row">
      <label class="field" style="flex: 1">
        <span>{{ t('input.duration') }}</span>
        <select v-model.number="store.project.seconds" :disabled="store.running" @change="store.persist()">
          <option :value="15">{{ t('input.seconds', { n: 15 }) }}</option>
          <option :value="30">{{ t('input.seconds', { n: 30 }) }}</option>
          <option :value="60">{{ t('input.seconds', { n: 60 }) }}</option>
        </select>
      </label>
      <label class="field" style="flex: 1">
        <span>{{ t('input.aspect') }}</span>
        <select v-model="store.project.aspect" :disabled="store.running" @change="store.persist()">
          <option value="16:9">16:9</option>
          <option value="9:16">9:16</option>
          <option value="1:1">1:1</option>
        </select>
      </label>
      <label class="field" style="flex: 1">
        <span>{{ t('input.copyLang') }}</span>
        <select v-model="store.project.lang" :disabled="store.running" @change="store.persist()">
          <option value="ja">{{ t('input.langJa') }}</option>
          <option value="en">{{ t('input.langEn') }}</option>
        </select>
      </label>
    </div>

    <label class="field">
      <span>{{ t('input.exportDir') }}</span>
      <div class="row">
        <input v-model="store.project.exportDir" :disabled="store.running" :placeholder="t('input.exportDirPlaceholder')" @change="store.persist()" />
        <button class="btn" :disabled="store.running" :title="t('input.pickExport')" @click="pickExport">
          <Icon name="folder" :size="15" />
          <span>{{ t('common.browse') }}</span>
        </button>
      </div>
    </label>

    <div class="cli-line" data-tour="input">
      <span class="chip" :class="store.cliCheck ? (store.cliCheck.found ? 'ok' : 'warn') : ''">
        <Icon name="terminal" :size="13" />
        {{ t('input.llm') }} {{ store.cli.kind }} {{ store.cliCheck?.found ? store.cliCheck.version : store.cliCheck ? t('input.llmNotFound') : t('input.llmChecking') }}
      </span>
      <span class="chip" :class="store.image.enabled ? 'accent' : ''">
        <Icon name="image" :size="13" />
        {{ t('input.image') }} {{ store.image.enabled ? store.image.provider : 'off' }}
      </span>
      <button class="btn small" data-tour="settings" :disabled="store.running" @click="emit('open-settings')">
        <Icon name="settings" :size="13" />
        <span>{{ t('common.settings') }}</span>
      </button>
    </div>
    <div v-if="store.cliCheck && !store.cliCheck.found" class="warn" style="font-size: var(--fs-sm); margin-top: 4px">{{ store.cliCheck.error }}</div>

    <div class="row action-row" style="margin-top: 14px">
      <!-- アプリ固有のコアボタン: テキスト「解析 → シーン構成」は維持 -->
      <button class="btn primary run-btn" data-tour="run" :disabled="!canRun" @click="store.run()">
        <Icon :name="store.running ? 'refresh' : 'sparkles'" :size="16" :class="{ spin: store.running }" />
        <span>{{ store.running ? t('input.running') : t('input.run') }}</span>
      </button>
      <button v-if="store.running" class="btn danger" @click="store.cancel()">
        <Icon name="stop" :size="14" />
        <span>{{ t('input.cancel') }}</span>
      </button>
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
  margin-top: 10px;
}
.select-row {
  gap: 6px;
}
.select-row .field {
  min-width: 0;
}
.select-row select {
  font-size: var(--fs-xs);
  padding: 6px 4px 6px 8px;
  min-height: 38px;
}
.action-row {
  width: 100%;
  margin-top: 18px;
}
.run-btn {
  flex: 1;
  min-height: 48px;
  padding: 0 24px;
  font-size: var(--fs-base);
  font-weight: var(--fw-bold);
  border-radius: var(--radius-full);
}
</style>
