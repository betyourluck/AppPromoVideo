<script setup lang="ts">
/** 右ペイン: 進捗ログ (event promo-progress)。stderr と error は色を変える。 */
import { nextTick, ref, watch } from "vue";
import { useStore } from "../store";
import { t } from "../i18n";
import Icon from "./Icon.vue";

const store = useStore();
const box = ref<HTMLElement | null>(null);

watch(
  () => store.log.length,
  async () => {
    await nextTick();
    if (box.value) box.value.scrollTop = box.value.scrollHeight;
  },
);

function fmt(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function cls(stage: string): string {
  if (stage === "error") return "warn";
  if (stage === "cli-stderr") return "stderr";
  if (stage === "done") return "ok";
  return "";
}
</script>

<template>
  <div class="panel wrap">
    <div class="row" style="justify-content: space-between">
      <h2 style="margin: 0">
        <span>{{ t('log.title') }}</span>
      </h2>
      <button class="btn small" :title="t('log.clearTitle')" :disabled="store.log.length === 0" @click="store.log = []">
        <Icon name="trash" :size="13" />
        <span>{{ t('log.clear') }}</span>
      </button>
    </div>
    <!-- rev29: 等幅 (Consolas) をやめて UI の書体の Medium に。Consolas には Medium が無い。 -->
    <!-- 進捗ログは配布ビルドでも選べるまま (rev47) — 報告のために貼れなくなるのは実害。 -->
    <div ref="box" class="log selectable">
      <div v-for="(l, i) in store.log" :key="i" class="log-entry" :class="cls(l.stage)">
        <span class="muted time">{{ fmt(l.ts) }}</span>
        <span class="stage-tag">[{{ l.stage }}]</span>
        <span class="log-text">{{ l.text }}</span>
      </div>
      <div v-if="store.log.length === 0" class="muted empty-log">{{ t('log.empty') }}</div>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.log {
  flex: 1;
  min-height: 0;
  overflow: auto;
  /* ユーザー要望: 入力やpromptと揃えてvar(--fs-xs)でコンパクトに */
  font-size: var(--fs-xs);
  font-weight: var(--fw-medium);
  font-variant-numeric: tabular-nums;
  line-height: 1.55;
  margin-top: 10px;
  white-space: pre-wrap;
  word-break: break-word;
}
.log-entry {
  padding: 3px 0;
  border-bottom: 1px solid rgb(var(--line-subtle, var(--line)) / 0.4);
}
.time {
  font-size: var(--fs-xs);
  margin-right: 4px;
}
.stage-tag {
  color: rgb(var(--accent2));
  font-weight: var(--fw-semi);
  margin-right: 6px;
}
.log-text {
  color: rgb(var(--text));
}
.stderr {
  color: rgb(var(--muted));
}
.empty-log {
  padding: 24px 0;
  text-align: center;
}
</style>
