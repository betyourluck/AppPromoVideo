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
import { t } from "../i18n";
import Icon from "./Icon.vue";
import Rich from "./Rich.vue";
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
    captionMsg.value = t("settings.pickFont");
    return;
  }
  try {
    captionPreview.value = await invoke<string>("caption_preview", {
      req: {
        image_path: store.project.snapshots[0] ?? null,
        text: store.result?.promo.summary.hook_copy || t("settings.captionSample"),
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
    store.showToast(t("settings.keySaved"));
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
  if (!wf.trim()) return t("settings.workflowEmpty");
  if (store.project.snapshots.length > 0 && !workflowAcceptsRefs(wf)) return t("settings.workflowNoRefs");
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
          <button class="btn small" :class="{ on: tab === 'llm' }" @click="tab = 'llm'">
            <Icon name="terminal" :size="13" />
            <span>LLM (CLI)</span>
          </button>
          <button class="btn small" :class="{ on: tab === 'image' }" @click="tab = 'image'">
            <Icon name="sparkles" :size="13" />
            <span>{{ t('settings.imageTabLabel') }}</span>
          </button>
        </div>
        <button class="btn small" :title="t('common.close')" @click="close">
          <Icon name="x" :size="14" />
          <span>{{ t('common.close') }}</span>
        </button>
      </div>

      <!-- ===== LLM ===== -->
      <section v-if="tab === 'llm'">
        <p class="muted note"><Rich :text="t('settings.llmNote')" /></p>
        <label class="field">
          <span>{{ t('settings.cliKind') }}</span>
          <select :value="store.cli.kind" @change="onKindChange(($event.target as HTMLSelectElement).value as CliKind)">
            <option value="claude">{{ t('settings.cliClaude') }}</option>
            <option value="aider">{{ t('settings.cliAider') }}</option>
            <option value="custom">{{ t('settings.cliCustom') }}</option>
          </select>
        </label>
        <label class="field">
          <span>{{ t('settings.executable') }}</span>
          <div class="row">
            <input v-model="store.cli.executable" @change="store.persist(); store.checkCli()" />
            <span class="chip" :class="store.cliCheck ? (store.cliCheck.found ? 'ok' : 'warn') : ''">
              {{ store.cliCheck ? (store.cliCheck.found ? store.cliCheck.version || 'OK' : t('input.llmNotFound')) : '…' }}
            </span>
          </div>
        </label>
        <label class="field">
          <span>{{ t('settings.cliModel') }}</span>
          <input v-model="store.cli.model" placeholder="sonnet / haiku / opus" @change="store.persist()" />
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>{{ t('settings.timeout') }}</span>
            <input v-model.number="store.cli.timeoutSecs" type="number" min="30" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>{{ t('settings.maxTurns') }}</span>
            <input v-model.number="store.cli.maxTurns" type="number" min="1" @change="store.persist()" />
          </label>
        </div>
        <label class="field row" style="gap: 8px">
          <input v-model="store.cli.oauthOnly" type="checkbox" @change="store.persist()" />
          <span style="margin: 0">{{ t('settings.oauthOnly') }}</span>
        </label>
        <label class="field">
          <span>{{ t('settings.extraArgs') }}</span>
          <input v-model="store.cli.extraArgs" class="mono" @change="store.persist()" />
        </label>
        <div v-if="store.cliCheck" class="authbox mono">
          <div><b>{{ t('settings.authTitle') }}</b></div>
          <div>
            ANTHROPIC_API_KEY:
            <span :class="store.cliCheck.auth.api_key_present ? 'ok' : 'muted'">
              {{ store.cliCheck.auth.api_key_present ? t('settings.presentKey', { len: store.cliCheck.auth.api_key_len, fp: store.cliCheck.auth.api_key_fingerprint }) : t('settings.absent') }}
            </span>
            <span class="muted"> {{ t('settings.apiKeyPrecedence') }}</span>
          </div>
          <div>ANTHROPIC_AUTH_TOKEN: {{ store.cliCheck.auth.auth_token_present ? t('settings.present') : t('settings.absent') }} · base_url: {{ store.cliCheck.auth.base_url || t('settings.default') }}</div>
          <div>
            claude auth status:
            <span v-if="store.cliCheck.auth.oauth_logged_in === null" class="muted">{{ t('settings.unknown') }}</span>
            <span v-else :class="store.cliCheck.auth.oauth_logged_in ? 'ok' : 'warn'">{{ store.cliCheck.auth.oauth_logged_in ? t('settings.loggedIn') : t('settings.notLoggedIn') }} ({{ store.cliCheck.auth.oauth_method || '-' }})</span>
          </div>
          <div class="muted">{{ t('settings.scrubbed', { vars: store.cliCheck.auth.scrubbed.length ? store.cliCheck.auth.scrubbed.join(', ') : t('settings.absent') }) }}</div>
          <button class="btn small" style="margin-top: 6px" @click="store.checkCli()">
            <Icon name="refresh" :size="13" />
            <span>{{ t('settings.recheck') }}</span>
          </button>
        </div>
        <p class="muted note">{{ t('settings.toolsNote') }}</p>
      </section>

      <!-- ===== 画像 ===== -->
      <section v-else>
        <p class="muted note"><Rich :text="t('settings.imageNote')" /></p>
        <label class="field row" style="gap: 8px">
          <input v-model="store.image.enabled" type="checkbox" @change="store.persist()" />
          <span style="margin: 0">{{ t('settings.autoImages') }}</span>
        </label>
        <label class="field">
          <span>{{ t('settings.provider') }}</span>
          <select :value="provider" @change="setProvider(($event.target as HTMLSelectElement).value as ImageProvider)">
            <option value="gemini">{{ t('settings.providerGemini') }}</option>
            <option value="openai">{{ t('settings.providerOpenai') }}</option>
            <option value="comfy">{{ t('settings.providerComfy') }}</option>
          </select>
        </label>

        <div v-if="provider !== 'comfy'" class="keybox">
          <span class="chip" :class="keys[provider] ? 'ok' : 'warn'">{{ keys[provider] ? t('settings.keySet') : t('settings.keyNotSet') }}</span>
          <input v-model="keyInput" type="password" :placeholder="t('settings.keyPlaceholder')" style="flex: 1" />
          <button class="btn small" :disabled="!keyInput.trim()" @click="saveKey">
            <Icon name="check" :size="13" />
            <span>{{ t('settings.save') }}</span>
          </button>
        </div>

        <label class="field">
          <span>{{ t('settings.serverUrl') }}</span>
          <input v-model="slot.baseUrl" :placeholder="DEFAULT_BASE_URL[provider]" @change="store.persist()" />
        </label>
        <label class="field">
          <span>{{ t('settings.model') }} {{ provider === 'comfy' ? t('settings.modelComfy') : '' }}</span>
          <input v-model="slot.model" :placeholder="DEFAULT_MODEL[provider]" :disabled="provider === 'comfy'" @change="store.persist()" />
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>{{ t('settings.detail') }}</span>
            <select v-model="store.image.detail" @change="store.persist()">
              <option value="standard">{{ t('settings.detailStandard') }}</option>
              <option value="high">{{ t('settings.detailHigh') }}</option>
              <option value="highest">{{ t('settings.detailHighest') }}</option>
            </select>
          </label>
          <label class="field" style="flex: 1">
            <span>{{ t('settings.maxScenes') }}</span>
            <input v-model.number="store.image.maxScenes" type="number" min="0" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>{{ t('settings.requestedRefs') }}</span>
            <input v-model.number="store.image.requestedRefs" type="number" min="0" max="3" @change="store.persist()" />
          </label>
        </div>
        <label class="field">
          <span>{{ t('settings.userPrefix') }}</span>
          <textarea v-model="store.image.userPrefix" rows="2" @change="store.persist()"></textarea>
        </label>
        <template v-if="provider === 'comfy'">
          <label class="field">
            <span>{{ t('settings.negative') }}</span>
            <input v-model="slot.negative" :disabled="!supportsNegative(provider)" @change="store.persist()" />
          </label>
          <label class="field">
            <span>{{ t('settings.workflowJson') }}</span>
            <textarea v-model="slot.workflowJson" rows="8" class="mono" @change="store.persist()"></textarea>
          </label>
          <div v-if="workflowWarn" class="warn" style="font-size: var(--fs-sm)">{{ workflowWarn }}</div>
          <div class="row">
            <label class="field row" style="gap: 6px; flex: 1">
              <input v-model="store.image.lockSeed" type="checkbox" @change="store.persist()" />
              <span style="margin: 0">{{ t('settings.lockSeed') }}</span>
            </label>
            <label class="field" style="flex: 1">
              <span>seed</span>
              <input v-model.number="store.image.seed" type="number" min="0" :disabled="!store.image.lockSeed" @change="store.persist()" />
            </label>
          </div>
        </template>
        <div class="row" style="margin-top: 6px">
          <button class="btn small" :disabled="probing" @click="probe">
            <Icon :name="probing ? 'refresh' : 'sparkles'" :size="13" />
            <span>{{ probing ? t('settings.testing') : t('settings.test') }}</span>
          </button>
          <span class="muted" style="font-size: var(--fs-sm); white-space: pre-wrap">{{ probeMsg }}</span>
        </div>

        <h3 class="sub">{{ t('settings.plateHeading') }}</h3>
        <p class="muted note">{{ t('settings.plateNote') }}</p>
        <label class="field">
          <span>{{ t('settings.plateMode') }}</span>
          <select v-model="store.image.plateMode" @change="store.persist()">
            <option value="frontal">{{ t('settings.plateFrontal') }}</option>
            <option value="perspective">{{ t('settings.platePerspective') }}</option>
          </select>
        </label>
        <p class="muted note">
          <template v-if="store.image.plateMode === 'perspective'">{{ t('settings.perspectiveNote') }}</template>
          <template v-else><Rich :text="t('settings.frontalNote')" /></template>
        </p>

        <h3 class="sub">{{ t('settings.captionHeading') }}</h3>
        <p class="muted note"><Rich :text="t('settings.captionNote')" /></p>
        <label class="field row" style="gap: 8px">
          <input v-model="store.image.caption.enabled" type="checkbox" @change="store.persist()" />
          <span style="margin: 0">{{ t('settings.captionEnable') }}</span>
        </label>
        <label class="field">
          <span>{{ t('settings.fontCount', { n: fonts.length }) }}</span>
          <div class="row">
            <select v-model="fontKey" :disabled="fontsLoading">
              <option value="#0" disabled>{{ t('settings.selectPlaceholder') }}</option>
              <option v-for="f in fonts" :key="f.path + '#' + f.index" :value="f.path + '#' + f.index">
                {{ f.has_japanese ? 'JP ' : '   ' }}{{ f.family }}{{ f.source === 'user' ? t('settings.userFont') : '' }}
              </option>
            </select>
            <button class="btn small" :disabled="fontsLoading" @click="loadFonts">
              <Icon :name="fontsLoading ? 'refresh' : 'refresh'" :size="13" />
              <span>{{ t('settings.reload') }}</span>
            </button>
            <button class="btn small" @click="openFontsFolder">
              <Icon name="folder" :size="13" />
              <span>{{ t('settings.fontsFolder') }}</span>
            </button>
          </div>
        </label>
        <div class="row">
          <label class="field" style="flex: 1">
            <span>{{ t('settings.captionSize') }}</span>
            <input v-model.number="store.image.caption.sizeRatio" type="number" min="0.02" max="0.2" step="0.005" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>{{ t('settings.captionColor') }}</span>
            <input v-model="store.image.caption.color" type="color" @change="store.persist()" />
          </label>
          <label class="field" style="flex: 1">
            <span>{{ t('settings.captionPosition') }}</span>
            <select v-model="store.image.caption.position" @change="store.persist()">
              <option value="bottom">{{ t('settings.bottom') }}</option>
              <option value="top">{{ t('settings.top') }}</option>
            </select>
          </label>
          <button class="btn small" style="align-self: flex-end; margin-bottom: 8px" @click="previewCaption">
            <Icon name="image" :size="13" />
            <span>{{ t('settings.preview') }}</span>
          </button>
        </div>
        <div v-if="captionMsg" class="warn" style="font-size: var(--fs-sm)">{{ captionMsg }}</div>
        <img v-if="captionPreview" :src="captionPreview" alt="caption preview" class="preview" />
      </section>
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
  width: min(760px, 94vw);
  max-height: 88vh;
  overflow: auto;
  padding: 24px;
  border-radius: var(--radius-dialog);
  box-shadow: var(--shadow-lg);
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
  font-size: var(--fs-sm);
  line-height: 1.5;
}
.authbox {
  border: 1px solid rgb(var(--line));
  border-radius: 6px;
  padding: 8px;
  font-size: var(--fs-sm);
  line-height: 1.6;
  margin: 6px 0;
  word-break: break-all;
}
.sub {
  margin: 14px 0 4px;
  font-size: var(--fs-h-md);
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
