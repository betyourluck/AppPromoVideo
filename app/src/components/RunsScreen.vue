<script setup lang="ts">
/**
 * run の履歴 (契約 RunIndex / RunRecord、rev7)。rev31 から**全画面** — メインの 3 ペインと入れ替えて出す。
 *
 * rev6 まで、同じアプリに 2 回実行すると出力が上書きされて過去の生成物が消えていた。
 * 生成は「新しいものを過去と比べて選ぶ」作業なので、一覧して開けること、そして
 * **同じ scene を 2 つの run で左右に並べられること**が要件。
 *
 * rev31 (ユーザー「ダイアログではなく全画面に。開くを押したら画面を閉じ、開いた run でメイン画面に戻る」):
 * 「開く」は**読めた時だけ**戻る。読めなければこの画面に残って理由を出す — ここではログのペインも
 * 入力ペインのエラー表示も見えないので、戻らずに黙ると何が起きたか分からない。
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { ask } from "../dialog";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { describeAttempts, pinCompared } from "../runs";
import { t } from "../i18n";
import Icon from "./Icon.vue";
import Rich from "./Rich.vue";

const store = useStore();
const emit = defineEmits<{ (e: "close"): void }>();
const scene = ref(1);

onMounted(() => store.loadRuns());

/** 「開く」に失敗した理由 (この画面の上に出す)。 */
const openError = ref("");
/** 読み込み中の run_dir。二重に押させない。 */
const opening = ref<string | null>(null);

async function open(dir: string) {
  openError.value = "";
  opening.value = dir;
  try {
    if (await store.openRun(dir)) emit("close");
    else openError.value = store.error;
  } finally {
    opening.value = null;
  }
}

/** 比較に選ばれた 2 つ (順序は選んだ順)。 */
const pair = computed(() => store.compare.map((dir) => store.runs.find((r) => r.run_dir === dir)).filter(Boolean));
const canCompare = computed(() => store.compare.length === 2);

/** 2 つの run が共通して持つ scene_id。 */
const sharedScenes = computed(() => {
  if (!canCompare.value) return [];
  const [a, b] = store.compare.map((d) => Object.keys(store.compareUrls[d] ?? {}).map(Number));
  return a.filter((id) => b.includes(id)).sort((x, y) => x - y);
});

/**
 * 表の並び (rev32)。比較中は比較対象を A → B で先頭に出す — 比較中の表は高さを抑えるので、
 * 下の方の行を選ぶと見えなくなって外せなかった (ユーザー報告 2026-09-11、スクリーンショットつき)。
 */
const rows = computed(() => pinCompared(store.runs, store.compare));

/** 比較対象の行に付ける印。下の画像の A / B と揃える。比較していない時は空。 */
function pinLabel(dir: string): string {
  if (!canCompare.value) return "";
  const i = store.compare.indexOf(dir);
  return i === 0 ? "A" : i === 1 ? "B" : "";
}

const screenEl = ref<HTMLElement | null>(null);
const tableEl = ref<HTMLElement | null>(null);
/** 比較の組が変わったら先頭へ戻す。下の方で選んだ直後は、先頭に出た 2 行と画像が画面の外に居るため。 */
watch(
  () => (canCompare.value ? store.compare.join("\n") : ""),
  async (key) => {
    if (!key) return;
    await nextTick();
    screenEl.value?.scrollTo({ top: 0 });
    tableEl.value?.scrollTo({ top: 0 });
  },
);

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
  <section ref="screenEl" class="screen">
    <header class="head">
      <button class="btn small" :title="t('runs.backTitle')" @click="emit('close')">
        <Icon name="chevron-left" :size="14" />
        <span>{{ t('runs.back') }}</span>
      </button>
      <h2 class="title">{{ t('runs.title') }}</h2>
    </header>
    <p class="muted note"><Rich :text="t('runs.note')" /></p>
    <div v-if="openError" class="warn open-error">{{ openError }}</div>

    <div v-if="!store.runs.length" class="muted note">{{ t('runs.none') }}</div>

    <div v-else ref="tableEl" class="table-wrap" :class="{ comparing: canCompare }">
      <table class="runs">
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
          <tr v-for="r in rows" :key="r.run_dir" :class="{ gone: !r.exists, pinned: pinLabel(r.run_dir) !== '' }">
            <!-- rev33: td 自体を flex にしない (表のセルとして並ばなくなり、行の上端と色付けがずれた)。中の div を flex にする。 -->
            <td>
              <div class="pick">
                <input
                  type="checkbox"
                  :checked="store.compare.includes(r.run_dir)"
                  :disabled="!r.exists"
                  :title="t('runs.compareCheck')"
                  @change="store.toggleCompare(r.run_dir)"
                />
                <span v-if="pinLabel(r.run_dir)" class="chip accent pin">{{ pinLabel(r.run_dir) }}</span>
              </div>
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
            <td>
              <div class="row" style="gap: 4px; justify-content: flex-end">
                <button class="btn small" :disabled="!r.exists || opening !== null" :title="t('runs.openTitle')" @click="open(r.run_dir)">
                  <Icon :name="opening === r.run_dir ? 'refresh' : 'play'" :size="12" />
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
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <p v-if="store.runs.some((r) => !r.exists)" class="muted note"><Rich :text="t('runs.missingNote')" /></p>

    <!-- ===== 比較 ===== -->
    <section v-if="canCompare" class="compare">
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
  </section>
</template>

<style scoped>
/* rev31: 全画面。App.vue の .shell (縦の flex) の中で、タイトルバーの下を全部使う。 */
.screen {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 16px 24px 24px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.head {
  display: flex;
  align-items: center;
  gap: 12px;
}
.title {
  margin: 0;
  font-size: var(--fs-h-lg);
  font-weight: var(--fw-bold);
}
.open-error {
  white-space: pre-wrap;
}
/*
 * 英語などで列が長くなっても、画面ではなく表だけを横に流す。
 * rev32: overflow を付けた箱は縦の flex の中で**勝手に縮む** (min-height が 0 扱いになる)。rev31 はこれで、
 * 比較中に下の画像へ場所を譲って表が 2 行ぶんまで潰れていた (中身 622px のうち 156px)。縮ませない。
 */
.table-wrap {
  flex-shrink: 0;
  overflow-x: auto;
}
/* 比較中だけ高さを決めて中でスクロールする。先頭の比較対象 2 行と見出しが必ず入る高さ。 */
.table-wrap.comparing {
  max-height: 30vh;
  overflow-y: auto;
}
.table-wrap.comparing thead th {
  position: sticky;
  top: 0;
  z-index: 1;
  background: rgb(var(--bg));
}
.pick {
  display: flex;
  align-items: center;
  gap: 6px;
}
.pin {
  min-height: 0;
  padding: 0 8px;
  line-height: 1.6;
}
.runs tr.pinned td {
  background: rgb(var(--accent) / 0.06);
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
/*
 * rev32: 縮むのは表ではなく画像の側。比較の区画が残りの高さを取り、画像はその中に収まる (object-fit: contain)。
 * 窓が低すぎる時は**画像の枠**の最小の高さで止め、画面全体をスクロールさせる。区画 (見出し・選択欄を含む) に
 * 最小を持たせると、1280x720 で画像が 139px まで潰れた (実測) — 見比べられない大きさを許さない。
 */
.compare {
  flex: 1 0 auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cmp {
  flex: 1;
  min-height: 260px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  grid-template-rows: minmax(0, 1fr);
  gap: 10px;
}
.cmp figure {
  margin: 0;
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.cmp img {
  flex: 1 1 0;
  min-height: 0;
  width: 100%;
  object-fit: contain;
  border-radius: 6px;
  display: block;
}
.cmp .missing {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 1px dashed var(--line, rgb(255 255 255 / 0.2));
  border-radius: 6px;
  font-size: var(--fs-md);
}
.btn.danger {
  color: #e06c6c;
}
</style>
