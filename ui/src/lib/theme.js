import { ref, watchEffect } from "vue";

const KEY = "hunter.runtimes.theme";
const media = window.matchMedia("(prefers-color-scheme: light)");

// light | dark | system
export const theme = ref(localStorage.getItem(KEY) || "system");

// 实际生效的那个主题（system 会解析成 light / dark）。画布取色要按它。
export const resolvedTheme = ref("dark");

const systemLight = ref(media.matches);
media.addEventListener("change", (e) => {
  systemLight.value = e.matches;
});

watchEffect(() => {
  const next =
    theme.value === "system" ? (systemLight.value ? "light" : "dark") : theme.value;
  resolvedTheme.value = next;
  document.documentElement.dataset.theme = next;
});

export function setTheme(value) {
  theme.value = value;
  if (value === "system") localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, value);
}
