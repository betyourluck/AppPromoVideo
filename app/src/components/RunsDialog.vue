<script setup lang="ts">
/**
 * run の履歴 (契約 RunIndex / RunRecord、rev7)。
 *
 * rev6 まで、同じアプリに 2 回実行すると出力が上書きされて過去の生成物が消えていた。
 * 生成は「新しいものを過去と比べて選ぶ」作業なので、一覧して開けること、そして
 * **同じ scene を 2 つの run で左右に並べられること**が要件。
 */
import { computed, onMounted, ref } from "vue";
import { ask } from "../dialog";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { describeAttempts } from "../runs";
import { t } from "../i18n";
import Icon from "./Icon.vue";
import Rich from "./Rich.vue";

const store = useStore();
const emit = defineEmits<{ (e: "close"): void }>();
const scene = ref(1);

onMounted(() => store.loadRuns());

/** 比較に選ばれた 2 つ (順序は選んだ順)。 */
const pair = computed(() => store.compare.map((dir) => store.runs.find((r) => r.run_dir === dir)).filter(Boolean));
const canCompare = computed(() => store.compare.length === 2);

/** 2 つの run が共通して持つ scene_id。 */
const sharedScenes = computed(() => {
  if (!canCompare.value) return [];
  const [a, b] = store.compare.map((d) => Object.keys(store.compareUrls[d] ?? {}).map(Number));
  return a.filter((id) => b.includes(id)).sort((x, y) => x - y);
});

function when(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

async function reveal(dir: string) {
  try {
    await invoke("open_folder", { path: dir });
  } catch (e) {
    store.push("error", String(e));
  }
}

async function drop(dir: string, files: boolean) {
  // 取り返しがつかないので danger (最初の焦点は「キャンセル」)。rev26: ブラウザ標準の確認は使わない。
  if (
    files &&
    !(await ask({
      title: t("runs.deleteTitle"),
      message: t("runs.deleteMessage"),
      ok: t("runs.deleteOk"),
      danger: true,
    }))
  )
    return;
  await store.forgetRun(dir, files);
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="dlg panel">
      <div class="row" style="justify-content: space-between">
        <b>{{ t('runs.title') }}</b>
        <button class="btn small" :title="t('common.close')" @click="emit('close')">
          <Icon name="x" :size="14" />
          <span>{{ t('common.close') }}</span>
        </button>
      </div>
      <p class="muted note"><Rich :text="t('runs.note')" /></p>

      <div v-if="!store.runs.length" class="muted note">{{ t('runs.none') }}</div>

      <table v-else class="runs">
        <thead>
          <tr>
            <th></th>
            <th>{{ t('runs.time') }}</th>
            <th>{{ t('runs.colApp') }}</th>
            <th>{{ t('runs.colScenes') }}</th>
            <th>{{ t('runs.colImages') }}</th>
            <th>{{ t('runs.colCost') }}</th>
            <th>{{ t('runs.colPlate') }}</th>
            <th :title="t('runs.colRetriesTitle')">{{ t('runs.colRetries') }}</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in store.runs" :key="r.run_dir" :class="{ gone: !r.exists }">
            <td>
              <input
                type="checkbox"
                :checked="store.compare.includes(r.run_dir)"
                :disabled="!r.exists"
                :title="t('runs.compareCheck')"
                @change="store.toggleCompare(r.run_dir)"
              />
            </td>
            <td class="mono">{{ when(r.created_at) }}</td>
            <td>{{ r.app_name }}</td>
            <td>{{ r.scene_count }}</td>
            <td>{{ r.image_count }}<span v-if="r.image_provider" class="muted"> / {{ r.image_provider }}</span></td>
            <td class="mono">{{ r.cost_usd ? r.cost_usd.toFixed(3) : "—" }}</td>
            <td class="muted">{{ r.plate_mode }}</td>
            <td class="mono" :class="{ retried: describeAttempts(r.plan_attempts, r.violation_kinds).retried }" :title="describeAttempts(r.plan_attempts, r.violation_kinds).title">
              {{ describeAttempts(r.plan_attempts, r.violation_kinds).text }}
            </td>
            <td class="row" style="gap: 4px; justify-content: flex-end">
              <button class="btn small" :disabled="!r.exists" :title="t('runs.openTitle')" @click="store.openRun(r.run_dir)">
                <Icon name="play" :size="12" />
                <span>{{ t('runs.open') }}</span>
              </button>
              <button class="btn small" :disabled="!r.exists" :title="t('runs.folderTitle')" @click="reveal(r.run_dir)">
                <Icon name="folder" :size="12" />
                <span>{{ t('runs.folder') }}</span>
              </button>
              <button class="btn small" :title="t('runs.forgetTitle')" @click="drop(r.run_dir, false)">
                <Icon name="x" :size="12" />
                <span>{{ t('common.remove') }}</span>
              </button>
              <button class="btn small danger" :disabled="!r.exists" :title="t('runs.deleteRunTitle')" @click="drop(r.run_dir, true)">
                <Icon name="trash" :size="12" />
                <span>{{ t('common.delete') }}</span>
              </button>
            </td>
          </tr>
        </tbody>
      </table>

      <p v-if="store.runs.some((r) => !r.exists)" class="muted note"><Rich :text="t('runs.missingNote')" /></p>

      <!-- ===== 比較 ===== -->
      <section v-if="canCompare">
        <h3 class="sub">{{ t('runs.compare') }}</h3>
        <div class="row" style="gap: 8px; align-items: center">
          <label class="field" style="margin: 0">
            <span>{{ t('runs.scene') }}</span>
            <select v-model.number="scene">
              <option v-for="id in sharedScenes" :key="id" :value="id">scene {{ id }}</option>
            </select>
          </label>
          <span v-if="!sharedScenes.length" class="muted">{{ t('runs.noShared') }}</span>
        </div>
        <div class="cmp">
          <figure v-for="(r, i) in pair" :key="r!.run_dir">
            <img v-if="store.compareUrls[r!.run_dir]?.[scene]" :src="store.compareUrls[r!.run_dir][scene]" :alt="`scene ${scene}`" />
            <div v-else class="missing muted">{{ t('runs.noSceneImage', { scene }) }}</div>
            <figcaption class="muted">
              {{ i === 0 ? "A" : "B" }} — {{ when(r!.created_at) }} / {{ r!.plate_mode }}
              <span v-if="r!.image_provider"> / {{ r!.image_provider }}</span>
            </figcaption>
          </figure>
        </div>
      </section>
      <p v-else class="muted note">{{ t('runs.selectTwo') }}</p>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: var(--backdrop, rgb(0 0 0 / 0.65));
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 40;
}
.dlg {
  width: min(960px, 94vw);
  max-height: 88vh;
  overflow: auto;
  padding: 24px;
  border-radius: var(--radius-dialog);
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.runs {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-md);
}
.runs th,
.runs td {
  padding: 4px 6px;
  border-bottom: 1px solid var(--line, rgb(255 255 255 / 0.08));
  text-align: left;
  white-space: nowrap;
}
.runs tr.gone {
  opacity: 0.45;
}
/* 2 回以上かかった run。ここを見るために列を足したので、目に入る強さにする。 */
.runs td.retried {
  color: var(--warn, #e0a33e);
  font-weight: var(--fw-semi);
}
.cmp {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.cmp figure {
  margin: 0;
  min-width: 0;
}
.cmp img {
  width: 100%;
  /* ダイアログ (88vh) の中に表・見出しと一緒に収める。はみ出すと左右を見比べられない。 */
  max-height: 46vh;
  object-fit: contain;
  border-radius: 6px;
  display: block;
}
.cmp .missing {
  display: flex;
  align-items: center;
  justify-content: center;
  aspect-ratio: 16 / 9;
  border: 1px dashed var(--line, rgb(255 255 255 / 0.2));
  border-radius: 6px;
  font-size: var(--fs-md);
}
.btn.danger {
  color: #e06c6c;
}
</style>
