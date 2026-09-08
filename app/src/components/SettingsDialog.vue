<script setup lang="ts">
/**
 * 設定。**2 セクションを分ける** (契約 決定 10): LLM は CLI の認証に委ねるのでキー欄なし、
 * 画像生成は OpenAI / Gemini がキー必須で ComfyUI だけ無キー。「完全無キー」と読める文言は置かない。
 * 画像の設定はプロバイダ別スロット (切替で値が漏れない)。キーは backend の .env へ (値は WebView に残さない)。
 */
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { FontEntry } from "../types";
import {
  DEFAULT_BASE_URL,
  DEFAULT_CLI_EXE,
  DEFAULT_MODEL,
  currentSlot,
  supportsNegative,
  toBackendCaption,
  toBackendConfig,
  workflowAcceptsRefs,
  type CliKind,
  type ImageProvider,
} from "../settings";

const store = useStore();
const emit = defineEmits<{ (e: "close"): void }>();
const tab = ref<"llm" | "image">("llm");

const keys = ref<{ openai: boolean; gemini: boolean }>({ openai: false, gemini: false });
const keyInput = ref("");
const probeMsg = ref("");
const probing = ref(false);

onMounted(() => {
  refreshKeys();
  loadFonts();
});

// --- 見出し (opt-in) ---
const fonts = ref<FontEntry[]>([]);
const fontsLoading = ref(false);
const captionPreview = ref("");
const captionMsg = ref("");
async function loadFonts() {
  fontsLoading.value = true;
  try {
    fonts.value = await invoke<FontEntry[]>("list_fonts");
  } catch (e) {
    captionMsg.value = String(e);
  } finally {
    fontsLoading.value = false;
  }
}
const fontKey = computed({
  get: () => `${store.image.caption.fontPath}#${store.image.caption.fontIndex}`,
  set: (v: string) => {
    const i = v.lastIndexOf("#");
    store.image.caption.fontPath = i >= 0 ? v.slice(0, i) : v;
    store.image.caption.fontIndex = i >= 0 ? Number(v.slice(i + 1)) || 0 : 0;
    store.persist();
  },
});
async function openFontsFolder() {
  try {
    await invoke("open_fonts_folder");
  } catch (e) {
    captionMsg.value = String(e);
  }
}
async function previewCaption() {
  captionMsg.value = "";
  const c = toBackendCaption({ ...store.image.caption, enabled: true });
  if (!c) {
    captionMsg.value = "フォントを選んでください";
    return;
  }
  try {
    captionPreview.value = await invoke<string>("caption_preview", {
      req: {
        image_path: store.project.snapshots[0] ?? null,
        text: store.result?.promo.summary.hook_copy || "整理するほど、時間は増える。",
        caption: c,
      },
    });
  } catch (e) {
    captionMsg.value = String(e);
  }
}

async function refreshKeys() {
  try {
    keys.value = await invoke<{ openai: boolean; gemini: boolean }>("get_image_api_keys");
  } catch {
    /* Tauri 外 */
  }
}

function onKindChange(k: CliKind) {
  store.cli.kind = k;
  if (!store.cli.executable.trim() || Object.values(DEFAULT_CLI_EXE).includes(store.cli.executable.trim())) {
    store.cli.executable = DEFAULT_CLI_EXE[k];
  }
  store.persist();
  store.checkCli();
}

const slot = computed(() => currentSlot(store.image));
const provider = computed(() => store.image.provider);

function setProvider(p: ImageProvider) {
  store.image.provider = p;
  store.persist();
  probeMsg.value = "";
}

async function saveKey() {
  if (provider.value === "comfy") return;
  try {
    await invoke("set_image_api_key", { provider: provider.value, key: keyInput.value });
    keyInput.value = "";
    await refreshKeys();
    store.showToast("API キーを保存しました (app_data/.env)");
  } catch (e) {
    probeMsg.value = String(e);
  }
}

async function probe() {
  probing.value = true;
  probeMsg.value = "";
  try {
    probeMsg.value = await invoke<string>("probe_image", { image: toBackendConfig(store.image, store.project.aspect) });
  } catch (e) {
    probeMsg.value = String(e);
  } finally {
    probing.value = false;
  }
}

const workflowWarn = computed(() => {
  if (provider.value !== "comfy") return "";
  const wf = slot.value.workflowJson;
  if (!wf.trim()) return "ワークフロー JSON (API 形式) が空です。ComfyUI で『Save (API Format)』したものを貼ってください。";
  if (store.project.snapshots.length > 0 && !workflowAcceptsRefs(wf)) return "%ref_1% が無いので、スナップショットは送っても使われません。";
  return "";
});

function close() {
  store.persist();
  emit("close");
}
</script>

<template>
  <div class="backdrop" @click.self="close">
    <div class="dlg panel">
      <div class="row" style="justify-content: space-between">
        <div class="tabs">
          <button class="btn small" :class="{ on: tab === 'llm' }" @click="tab = 'llm'">LLM (CLI)</button>
          <button class="btn small" :class="{ on: tab === 'image' }" @click="tab = 'image'">画像生成 (API キー)</button>
        </div>
        <button class="btn small" @click="close">閉じる</button>
      </div>

      <!-- ===== LLM ===== -->
      <section v-if="tab === 'llm'">
        <p class="muted note">
          テキスト解析はローカルの CLI をサブプロセスで実行します。<b>キーはこのアプリでは持ちません</b> — CLI 側のログイン
          (claude は <span class="mono">claude auth login</span>、または環境変数 ANTHROPIC_API_KEY) に委ねます。
          対象リポジトリの hook や MCP 設定は読み込みません (作業ディレクトリは app 側)。
        </p>
        <label class="field">
          <span>CLI の種類</span>
          <select :value="store.cli.kind" @change="onKindChange(($event.target as HTMLSelectElement).value as CliKind)">
            <option value="claude">Claude Code (claude -p、構造化出力・リポジトリ走査あり)</option>
            <option value="aider">aider (--message-file、走査なし)</option>
            <option value="custom">カスタム (stdin に本文、stdout を読む)</option>
          </select>
        </label>
        <label class="field">
          <span>実行ファイル (パス or PATH 上の名前)</span>
          <div class="row">
            <input v-model="store.cli.executable" @change="store.persist(); store.checkCli()" />
            <span class="chip" :class="store.cliCheck ? (store.cliCheck.found ? 'ok' : 'warn') : ''">
              {{ store.cliCheck ? (store.cliCheck.found ? store.cliCheck.version || 'OK' : '見つかりません') : '…' }}
            </span>
          </div>
        </label>
        <label class="field">
          <span>モデル (空 = CLI の既定)</span>
          <input v-model="store.cli.model" placeholder="sonnet / haiku / opus" @change="store.persist()" />
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>タイムアウト (秒、最小 30)</span>
            <input v-model.number="store.cli.timeoutSecs" type="number" min="30" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>最大ターン (走査の深掘り回数)</span>
            <input v-model.number="store.cli.maxTurns" type="number" min="1" @change="store.persist()" />
          </label>
        </div>
        <label class="field row" style="gap: 8px">
          <input v-model="store.cli.oauthOnly" type="checkbox" style="width: auto" @change="store.persist()" />
          <span style="margin: 0">OAuth ログインを使う — 環境変数の ANTHROPIC_API_KEY / ANTHROPIC_AUTH_TOKEN を子 CLI に渡さない (端末の鍵が古い・無効なときの回避)</span>
        </label>
        <label class="field">
          <span>追加引数 (空白区切り。既定では付けない: --dangerously-skip-permissions 等)</span>
          <input v-model="store.cli.extraArgs" class="mono" @change="store.persist()" />
        </label>
        <div v-if="store.cliCheck" class="authbox mono">
          <div><b>子 CLI が使う認証 (このアプリのプロセス環境)</b></div>
          <div>
            ANTHROPIC_API_KEY:
            <span :class="store.cliCheck.auth.api_key_present ? 'ok' : 'muted'">
              {{ store.cliCheck.auth.api_key_present ? `あり (len ${store.cliCheck.auth.api_key_len}, fp ${store.cliCheck.auth.api_key_fingerprint})` : 'なし' }}
            </span>
            <span class="muted"> — あれば OAuth ログインより優先されます</span>
          </div>
          <div>ANTHROPIC_AUTH_TOKEN: {{ store.cliCheck.auth.auth_token_present ? 'あり' : 'なし' }} · base_url: {{ store.cliCheck.auth.base_url || '既定' }}</div>
          <div>
            claude auth status:
            <span v-if="store.cliCheck.auth.oauth_logged_in === null" class="muted">不明</span>
            <span v-else :class="store.cliCheck.auth.oauth_logged_in ? 'ok' : 'warn'">{{ store.cliCheck.auth.oauth_logged_in ? 'ログイン済み' : '未ログイン' }} ({{ store.cliCheck.auth.oauth_method || '-' }})</span>
          </div>
          <div class="muted">子に渡さない変数: {{ store.cliCheck.auth.scrubbed.length ? store.cliCheck.auth.scrubbed.join(', ') : 'なし' }}</div>
          <button class="btn small" style="margin-top: 4px" @click="store.checkCli()">再検査</button>
        </div>
        <p class="muted note">
          claude には Read / Glob / Grep だけを許可し、対象リポジトリは --add-dir で読み取り専用に渡します。Write / Edit / Bash は許可しません。
        </p>
      </section>

      <!-- ===== 画像 ===== -->
      <section v-else>
        <p class="muted note">
          参照画像の生成は HTTP で画像 API を呼びます。<b>OpenAI / Gemini は API キーが必要</b>、ComfyUI (ローカル) はキー不要。
          キーは app_data/.env に保存され、画面には有無だけ表示します。
        </p>
        <label class="field row" style="gap: 8px">
          <input v-model="store.image.enabled" type="checkbox" style="width: auto" @change="store.persist()" />
          <span style="margin: 0">解析のあと参照画像も自動で作る</span>
        </label>
        <label class="field">
          <span>プロバイダ (設定はプロバイダごとに保持)</span>
          <select :value="provider" @change="setProvider(($event.target as HTMLSelectElement).value as ImageProvider)">
            <option value="gemini">Gemini (Nano Banana) — キー必要</option>
            <option value="openai">OpenAI Images — キー必要</option>
            <option value="comfy">ComfyUI — ローカル、キー不要</option>
          </select>
        </label>

        <div v-if="provider !== 'comfy'" class="keybox">
          <span class="chip" :class="keys[provider] ? 'ok' : 'warn'">{{ keys[provider] ? 'キー設定済み' : 'キー未設定' }}</span>
          <input v-model="keyInput" type="password" placeholder="API キーを貼って保存 (値は再表示されません)" style="flex: 1" />
          <button class="btn small" :disabled="!keyInput.trim()" @click="saveKey">保存</button>
        </div>

        <label class="field">
          <span>サーバー URL</span>
          <input v-model="slot.baseUrl" :placeholder="DEFAULT_BASE_URL[provider]" @change="store.persist()" />
        </label>
        <label class="field">
          <span>モデル {{ provider === 'comfy' ? '(ComfyUI はワークフロー側で決まる)' : '' }}</span>
          <input v-model="slot.model" :placeholder="DEFAULT_MODEL[provider]" :disabled="provider === 'comfy'" @change="store.persist()" />
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>解像度段</span>
            <select v-model="store.image.detail" @change="store.persist()">
              <option value="standard">標準</option>
              <option value="high">高</option>
              <option value="highest">最高 (OpenAI のみ、高コスト)</option>
            </select>
          </label>
          <label class="field" style="flex: 1">
            <span>先頭から何シーン (0 = 全部)</span>
            <input v-model.number="store.image.maxScenes" type="number" min="0" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>参照枚数 (0 = 既定: OpenAI 1 / 他 3)</span>
            <input v-model.number="store.image.requestedRefs" type="number" min="0" max="3" @change="store.persist()" />
          </label>
        </div>
        <label class="field">
          <span>スタイル接頭辞 (空 = スナップショットの palette と解析結果から自動合成)</span>
          <textarea v-model="store.image.userPrefix" rows="2" @change="store.persist()"></textarea>
        </label>
        <template v-if="provider === 'comfy'">
          <label class="field">
            <span>ネガティブプロンプト</span>
            <input v-model="slot.negative" :disabled="!supportsNegative(provider)" @change="store.persist()" />
          </label>
          <label class="field">
            <span>ワークフロー JSON (API 形式。%prompt% %negative% %seed% %width% %height% %ref_1..3% を差し替えます)</span>
            <textarea v-model="slot.workflowJson" rows="8" class="mono" @change="store.persist()"></textarea>
          </label>
          <div v-if="workflowWarn" class="warn" style="font-size: 11px">{{ workflowWarn }}</div>
          <div class="row">
            <label class="field row" style="gap: 6px; flex: 1">
              <input v-model="store.image.lockSeed" type="checkbox" style="width: auto" @change="store.persist()" />
              <span style="margin: 0">seed 固定</span>
            </label>
            <label class="field" style="flex: 1">
              <span>seed</span>
              <input v-model.number="store.image.seed" type="number" min="0" :disabled="!store.image.lockSeed" @change="store.persist()" />
            </label>
          </div>
        </template>
        <div class="row" style="margin-top: 6px">
          <button class="btn small" :disabled="probing" @click="probe">{{ probing ? '接続テスト中…' : '接続テスト' }}</button>
          <span class="muted" style="font-size: 11px; white-space: pre-wrap">{{ probeMsg }}</span>
        </div>

        <h3 class="sub">見出し (copy) の焼き込み — 任意</h3>
        <p class="muted note">
          各カット画像に copy_text を焼き込みます。既定は OFF (動画側でテロップを載せる運用)。フォントはシステムにインストール済みのものと、
          <span class="mono">app_data/fonts</span> に置いたファイルから選べます。
        </p>
        <label class="field row" style="gap: 8px">
          <input v-model="store.image.caption.enabled" type="checkbox" style="width: auto" @change="store.persist()" />
          <span style="margin: 0">カット画像に見出しを焼き込む</span>
        </label>
        <label class="field">
          <span>フォント ({{ fonts.length }} 件。JP = 日本語グリフあり)</span>
          <div class="row">
            <select v-model="fontKey" :disabled="fontsLoading">
              <option value="#0" disabled>— 選択 —</option>
              <option v-for="f in fonts" :key="f.path + '#' + f.index" :value="f.path + '#' + f.index">
                {{ f.has_japanese ? 'JP ' : '   ' }}{{ f.family }}{{ f.source === 'user' ? ' (自分のフォント)' : '' }}
              </option>
            </select>
            <button class="btn small" :disabled="fontsLoading" @click="loadFonts">再読込</button>
            <button class="btn small" @click="openFontsFolder">フォントフォルダ</button>
          </div>
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>文字の高さ (canvas 比)</span>
            <input v-model.number="store.image.caption.sizeRatio" type="number" min="0.02" max="0.2" step="0.005" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>位置</span>
            <select v-model="store.image.caption.position" @change="store.persist()">
              <option value="bottom">下</option>
              <option value="top">上</option>
            </select>
          </label>
          <button class="btn small" style="align-self: flex-end; margin-bottom: 8px" @click="previewCaption">プレビュー</button>
        </div>
        <div v-if="captionMsg" class="warn" style="font-size: 11px">{{ captionMsg }}</div>
        <img v-if="captionPreview" :src="captionPreview" alt="caption preview" class="preview" />
      </section>
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
  width: min(720px, 92vw);
  max-height: 88vh;
  overflow: auto;
}
.tabs {
  display: flex;
  gap: 6px;
}
.tabs .on {
  border-color: rgb(var(--accent));
  color: rgb(var(--accent));
}
.note {
  font-size: 12px;
  line-height: 1.5;
}
.authbox {
  border: 1px solid rgb(var(--line));
  border-radius: 6px;
  padding: 8px;
  font-size: 11px;
  line-height: 1.6;
  margin: 6px 0;
  word-break: break-all;
}
.sub {
  margin: 14px 0 4px;
  font-size: 12px;
  letter-spacing: 0.06em;
  color: rgb(var(--muted));
  border-top: 1px solid rgb(var(--line));
  padding-top: 10px;
}
.preview {
  width: 100%;
  border-radius: 6px;
  border: 1px solid rgb(var(--line));
  margin-top: 6px;
}
.keybox {
  display: flex;
  gap: 8px;
  align-items: center;
  margin: 6px 0;
}
</style>
