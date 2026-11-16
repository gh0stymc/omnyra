import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import es from "./es/common.json";
import en from "./en/common.json";

const LANG_KEY = "omnyra.lang";

export const SUPPORTED_LANGS = ["es", "en"] as const;
export type Lang = (typeof SUPPORTED_LANGS)[number];

function initialLang(): Lang {
  const saved = localStorage.getItem(LANG_KEY);
  return saved === "en" || saved === "es" ? saved : "es"; // Spanish by default
}

void i18n.use(initReactI18next).init({
  resources: { es: { common: es }, en: { common: en } },
  lng: initialLang(),
  fallbackLng: "es",
  defaultNS: "common",
  interpolation: { escapeValue: false },
});

export function setLang(lang: Lang) {
  localStorage.setItem(LANG_KEY, lang);
  void i18n.changeLanguage(lang);
}

export default i18n;
