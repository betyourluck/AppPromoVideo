<script setup lang="ts">
/**
 * 入り切りのスイッチ (rev34、ユーザー判断 2026-09-12「設定画面のチェックボックスはスイッチにして下さい」)。
 *
 * 中身は `input[type="checkbox"]` のまま — v-model も Tab の移動も、読み上げ上の役割も変えない。
 * **つまみは実体のある要素**で、`::after` (疑似要素) は使わない: `<input>` に疑似要素を描くかはブラウザ次第で、
 * 描かれたかどうかを外から確かめられない (位置を測る API が無い)。実体なら位置を数値で測れる。
 */
const model = defineModel<boolean>({ required: true });
defineProps<{ disabled?: boolean }>();
const emit = defineEmits<{ (e: "change"): void }>();
</script>

<template>
  <span class="sw">
    <input v-model="model" type="checkbox" class="track" :disabled="disabled" @change="emit('change')" />
    <span class="knob" aria-hidden="true"></span>
  </span>
</template>

<style scoped>
.sw {
  position: relative;
  display: inline-flex;
  flex: none;
  width: 38px;
  height: 22px;
}
/* 枠。main.css の input 規則 (16px 四方) より詳しいので、こちらが勝つ。 */
.track {
  appearance: none;
  -webkit-appearance: none;
  width: 38px;
  height: 22px;
  min-height: 0;
  margin: 0;
  padding: 0;
  border: 1px solid rgb(var(--line));
  border-radius: var(--radius-full);
  background: rgb(var(--muted) / 0.3);
  cursor: pointer;
  transition: background var(--trans-fast), border-color var(--trans-fast);
}
.track:checked {
  background: rgb(var(--accent));
  border-color: transparent;
}
.track:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
/* つまみ。枠の左右・上下に 4px ずつ余らせ、入った時は 16px 動いて右端に揃う。 */
.knob {
  position: absolute;
  top: 4px;
  left: 4px;
  width: 14px;
  height: 14px;
  border-radius: var(--radius-full);
  background: rgb(var(--panel));
  box-shadow: var(--shadow-sm);
  pointer-events: none;
  transition: transform var(--trans-fast);
}
.track:checked + .knob {
  transform: translateX(16px);
}
</style>
