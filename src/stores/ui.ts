import { create } from "zustand";
import { isRange, type Range } from "../lib/range";

export type Theme = "light" | "dark";

const THEME_KEY = "omnyra.theme";
const RANGE_KEY = "omnyra.range";

function initialRange(): Range {
  try {
    const saved = localStorage.getItem(RANGE_KEY);
    if (isRange(saved)) return saved;
  } catch {
    // storage unavailable: use the default
  }
  return "30d";
}

export function initialTheme(): Theme {
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "light" || saved === "dark") return saved;
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
}

interface UiState {
  theme: Theme;
  setTheme: (t: Theme) => void;
  toggleTheme: () => void;
  /** Period shown by the screens (shared, remembered). */
  range: Range;
  setRange: (r: Range) => void;
}

export const useUi = create<UiState>((set, get) => ({
  theme: initialTheme(),
  setTheme: (theme) => {
    localStorage.setItem(THEME_KEY, theme);
    applyTheme(theme);
    set({ theme });
  },
  toggleTheme: () => get().setTheme(get().theme === "dark" ? "light" : "dark"),
  range: initialRange(),
  setRange: (range) => {
    try {
      localStorage.setItem(RANGE_KEY, range);
    } catch {
      // not persisted; still applied
    }
    set({ range });
  },
}));
