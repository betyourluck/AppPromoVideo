<script setup lang="ts">
/**
 * 辞書の文言を、強調と等幅つきで出す (rev30)。
 *
 * `v-html` は使わない。解析は `rich.ts` で、認める印は `<b>` と `<mono>` の 2 つだけ —
 * それ以外の `<` は文字として出るので、辞書にも差し込む値にも HTML を注入できない。
 */
import { computed } from "vue";
import { parseRich } from "../rich";

const props = defineProps<{ text: string }>();
const segments = computed(() => parseRich(props.text));
</script>

<template>
  <template v-for="(s, i) in segments" :key="i">
    <b v-if="s.b && s.mono"><span class="mono">{{ s.text }}</span></b>
    <b v-else-if="s.b">{{ s.text }}</b>
    <span v-else-if="s.mono" class="mono">{{ s.text }}</span>
    <template v-else>{{ s.text }}</template>
  </template>
</template>
