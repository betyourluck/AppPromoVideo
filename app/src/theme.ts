import { ref } from "vue";

export type Theme = "dark" | "light";
const KEY = "apppromo.theme";

function load(): Theme {
  try {
    return localStorage.getItem(KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export const theme = ref<Theme>(load());

/** <html data-theme> に反映。main.ts が mount 前に呼ぶ。 */
export function applyTheme(): void {
  document.documentElement.dataset.theme = theme.value;
}

export function toggleTheme(): void {
  theme.value = theme.value === "dark" ? "light" : "dark";
  try {
    localStorage.setItem(KEY, theme.value);
  } catch {
    /* storage が塞がれた環境では永続化しない */
  }
  applyTheme();
}
