import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

const STORAGE_KEY = "i18nextLng";

/** 语言包按需加载：静态 import 会让每个常驻窗口都解析并常驻全部 6 份 JSON */
const LOADERS: Record<string, () => Promise<{ default: object }>> = {
  zh: () => import("@/locales/zh.json"),
  en: () => import("@/locales/en.json"),
  "zh-TW": () => import("@/locales/zh-TW.json"),
  fr: () => import("@/locales/fr.json"),
  ja: () => import("@/locales/ja.json"),
  de: () => import("@/locales/de.json"),
};

const FALLBACK_LNG = "zh";

/** localStorage 里可能是 fr-CA 这类未收录的变体，未收录的一律取回退语言包 */
async function loadMessages(lng: string): Promise<object> {
  const mod = await (LOADERS[lng] ?? LOADERS[FALLBACK_LNG])();
  return mod.default;
}

async function getSystemLocale(): Promise<string> {
  try {
    const locale = await invoke<string>("get_system_locale");
    const localeMap: Record<string, string> = {
      "zh-Hans": "zh",
      "zh-Hant": "zh-TW",
      "fr": "fr",
      "ja": "ja",
      "de": "de",
    };
    return localeMap[locale] || "zh";
  } catch {
    return "zh";
  }
}

async function initI18n() {
  let initialLang = localStorage.getItem(STORAGE_KEY);

  if (!initialLang) {
    initialLang = await getSystemLocale();
    localStorage.setItem(STORAGE_KEY, initialLang);
  }

  const lang = initialLang && LOADERS[initialLang] ? initialLang : FALLBACK_LNG;

  const resources: Record<string, { translation: object }> = {
    [lang]: { translation: await loadMessages(lang) },
  };
  if (lang !== FALLBACK_LNG) {
    resources[FALLBACK_LNG] = { translation: await loadMessages(FALLBACK_LNG) };
  }

  await i18n.use(initReactI18next).init({
    resources,
    lng: lang,
    fallbackLng: FALLBACK_LNG,
    supportedLngs: ["zh", "en", "zh-TW", "fr", "ja", "de"],
    interpolation: {
      escapeValue: false,
    },
  });

  i18n.on('languageChanged', (lng) => {
    localStorage.setItem(STORAGE_KEY, lng);
    // 切到尚未加载的语言时补载，addResourceBundle 会驱动订阅组件重渲染
    if (lng && !i18n.hasResourceBundle(lng, "translation")) {
      loadMessages(lng).then((messages) => {
        i18n.addResourceBundle(lng, "translation", messages, true, true);
      });
    }
  });

  return i18n;
}

initI18n();

export default i18n;
