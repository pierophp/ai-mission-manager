export type Theme = "light" | "dark";
export type ThemePreference = Theme | "system";

const themeStorageKey = "ai-mission-manager.theme";
const darkSchemeQuery = "(prefers-color-scheme: dark)";

function isTheme(value: string | null): value is Theme {
  return value === "light" || value === "dark";
}

export function loadThemePreference(): ThemePreference {
  if (typeof window === "undefined") return "system";

  try {
    const storedTheme = window.localStorage.getItem(themeStorageKey);
    if (isTheme(storedTheme)) return storedTheme;
  } catch {
    // Use the system preference when local persistence is unavailable.
  }

  return "system";
}

export function systemTheme(): Theme {
  if (typeof window === "undefined") return "light";

  return window.matchMedia?.(darkSchemeQuery)?.matches ? "dark" : "light";
}

export function resolveTheme(preference: ThemePreference, system: Theme): Theme {
  return preference === "system" ? system : preference;
}

/** Calls `onChange` whenever the operating system switches color scheme. */
export function watchSystemTheme(onChange: (theme: Theme) => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => {};

  const query = window.matchMedia(darkSchemeQuery);
  const listener = (event: MediaQueryListEvent) => onChange(event.matches ? "dark" : "light");
  query.addEventListener("change", listener);
  return () => query.removeEventListener("change", listener);
}

export function applyTheme(theme: Theme) {
  if (typeof document === "undefined") return;

  document.documentElement.dataset.theme = theme;
  document
    .querySelector('meta[name="theme-color"]')
    ?.setAttribute("content", theme === "dark" ? "#171411" : "#f7f4ee");
}

export function saveThemePreference(preference: ThemePreference) {
  if (typeof window === "undefined") return;

  try {
    if (preference === "system") window.localStorage.removeItem(themeStorageKey);
    else window.localStorage.setItem(themeStorageKey, preference);
  } catch {
    // The theme still applies for this session when local persistence is unavailable.
  }
}

export function initializeTheme(): Theme {
  const theme = resolveTheme(loadThemePreference(), systemTheme());
  applyTheme(theme);
  return theme;
}
