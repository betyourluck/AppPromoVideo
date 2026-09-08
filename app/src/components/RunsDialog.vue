<script setup lang="ts">
/**
 * run の履歴 (契約 RunIndex / RunRecord、rev7)。
 *
 * rev6 まで、同じアプリに 2 回実行すると出力が上書きされて過去の生成物が消えていた。
 * 生成は「新しいものを過去と比べて選ぶ」作業なので、一覧して開けること、そして
 * **同じ scene を 2 つの run で左右に並べられること**が要件。
 */
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";

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
  if (files && !confirm("この run のフォルダごと削除します。元に戻せません。よろしいですか？")) return;
  await store.forgetRun(dir, files);
}
</script>

<template>
  <div class="backdrop" @click.self="emit('close')">
    <div class="dlg panel">
      <div class="row" style="justify-content: space-between">
        <b>履歴 — 過去の実行</b>
        <button class="btn small" @click="emit('close')">閉じる</button>
      </div>
      <p class="muted note">
        実行ごとに <span class="mono">&lt;パッケージ&gt;/runs/&lt;日時&gt;/</span> に分けて保存しています。
        <b>比較</b>を 2 つ選ぶと、同じシーンの画像を左右に並べられます。
      </p>

      <div v-if="!store.runs.length" class="muted note">まだ履歴がありません。</div>

      <table v-else class="runs">
        <thead>
          <tr>
            <th></th>
            <th>日時</th>
            <th>アプリ</th>
            <th>シーン</th>
            <th>画像</th>
            <th>費用</th>
            <th>面</th>
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
                title="比較に使う (2 つまで)"
                @change="store.toggleCompare(r.run_dir)"
              />
            </td>
            <td class="mono">{{ when(r.created_at) }}</td>
            <td>{{ r.app_name }}</td>
            <td>{{ r.scene_count }}</td>
            <td>{{ r.image_count }}<span v-if="r.image_provider" class="muted"> / {{ r.image_provider }}</span></td>
            <td class="mono">{{ r.cost_usd ? r.cost_usd.toFixed(3) : "—" }}</td>
            <td class="muted">{{ r.plate_mode }}</td>
            <td class="row" style="gap: 4px; justify-content: flex-end">
              <button class="btn small" :disabled="!r.exists" @click="store.openRun(r.run_dir)">開く</button>
              <button class="btn small" :disabled="!r.exists" @click="reveal(r.run_dir)">フォルダ</button>
              <button class="btn small" @click="drop(r.run_dir, false)" title="索引から外すだけ (ファイルは残る)">外す</button>
              <button class="btn small danger" :disabled="!r.exists" @click="drop(r.run_dir, true)">削除</button>
            </td>
          </tr>
        </tbody>
      </table>

      <p v-if="store.runs.some((r) => !r.exists)" class="muted note">
        薄い行はフォルダが見つからないものです。<b>索引からは消していません</b> — 移動しただけかもしれないので、
        戻せば再び開けます。
      </p>

      <!-- ===== 比較 ===== -->
      <section v-if="canCompare">
        <h3 class="sub">比較</h3>
        <div class="row" style="gap: 8px; align-items: center">
          <label class="field" style="margin: 0">
            <span>シーン</span>
            <select v-model.number="scene">
              <option v-for="id in sharedScenes" :key="id" :value="id">scene {{ id }}</option>
            </select>
          </label>
          <span v-if="!sharedScenes.length" class="muted">2 つに共通する画像がありません。</span>
        </div>
        <div class="cmp">
          <figure v-for="(r, i) in pair" :key="r!.run_dir">
            <img v-if="store.compareUrls[r!.run_dir]?.[scene]" :src="store.compareUrls[r!.run_dir][scene]" :alt="`scene ${scene}`" />
            <div v-else class="missing muted">この run に scene {{ scene }} の画像はありません</div>
            <figcaption class="muted">
              {{ i === 0 ? "A" : "B" }} — {{ when(r!.created_at) }} / {{ r!.plate_mode }}
              <span v-if="r!.image_provider"> / {{ r!.image_provider }}</span>
            </figcaption>
          </figure>
        </div>
      </section>
      <p v-else class="muted note">比較するには、左端のチェックで run を 2 つ選んでください。</p>
    </div>
  </div>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  background: rgb(0 0 0 / 0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 40;
}
.dlg {
  width: min(960px, 94vw);
  max-height: 88vh;
  overflow: auto;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.runs {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
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
  font-size: 12px;
}
.btn.danger {
  color: #e06c6c;
}
</style>
