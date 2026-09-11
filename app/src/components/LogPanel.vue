<script setup lang="ts">
/** 右ペイン: 進捗ログ (event promo-progress)。stderr と error は色を変える。 */
import { nextTick, ref, watch } from "vue";
import { useStore } from "../store";

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
      <h2 style="margin: 0">進捗</h2>
      <button class="btn small" @click="store.log = []">消去</button>
    </div>
    <!-- rev29: 等幅 (Consolas) をやめて UI の書体の Medium に。Consolas には Medium が無い。 -->
    <div ref="box" class="log">
      <div v-for="(l, i) in store.log" :key="i" :class="cls(l.stage)">
        <span class="muted">{{ fmt(l.ts) }}</span> <span class="stage">[{{ l.stage }}]</span> {{ l.text }}
      </div>
      <div v-if="store.log.length === 0" class="muted">まだ何も起きていません。</div>
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
  /* rev29: ユーザー「進捗のログも medium で普通サイズ」。等幅をやめたので時刻の桁は等幅数字で揃える。 */
  font-size: var(--fs-base);
  font-weight: var(--fw-medium);
  font-variant-numeric: tabular-nums;
  line-height: 1.5;
  margin-top: 8px;
  white-space: pre-wrap;
  word-break: break-word;
}
.stage {
  color: rgb(var(--accent2));
}
.stderr {
  color: rgb(var(--muted));
}
</style>
