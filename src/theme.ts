export type Theme = "light" | "dark";

const themeStorageKey = "ai-mission-manager.theme";

function isTheme(value: string | null): value is Theme {
  return value === "light" || value === "dark";
}

export function loadStoredTheme(): Theme | undefined {
  if (typeof window === "undefined") return undefined;

  try {
    const storedTheme = window.localStorage.getItem(themeStorageKey);
    if (isTheme(storedTheme)) return storedTheme;
  } catch {
    // Use the system preference when local persistence is unavailable.
  }

  return undefined;
}

export function loadTheme(): Theme {
  const storedTheme = loadStoredTheme();
  if (storedTheme) return storedTheme;

  if (typeof window === "undefined") return "light";

  return window.matchMedia?.("(prefers-color-scheme: dark)")?.matches ? "dark" : "light";
}

export function applyTheme(theme: Theme) {
  if (typeof document === "undefined") return;

  document.documentElement.dataset.theme = theme;
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", theme === "dark" ? "#171411" : "#f7f4ee");
}

export function saveTheme(theme: Theme) {
  if (typeof window === "undefined") return;

  try {
    window.localStorage.setItem(themeStorageKey, theme);
  } catch {
    // The theme still applies for this session when local persistence is unavailable.
  }
}

export function initializeTheme(): Theme {
  const theme = loadTheme();
  applyTheme(theme);
  return theme;
}
