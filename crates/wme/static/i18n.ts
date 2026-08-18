(() => {
    let currentLocale = "fr";
    let translations: Record<string, string> = {};
    let fallbackTranslations: Record<string, string> = {};

    function detectLocale(): string {
        try {
            const stored = localStorage.getItem("roadwork-wme-language");
            if (stored && stored !== "auto") return stored;
        } catch (_) {}
        const wmeLang = document.documentElement?.lang;
        if (wmeLang) {
            const short = wmeLang.split("-")[0].toLowerCase();
            if (["fr", "en"].includes(short)) return short;
        }
        const nav = navigator.language?.split("-")[0]?.toLowerCase();
        if (nav && ["fr", "en"].includes(nav)) return nav;
        return "fr";
    }

    function setTranslations(locale: string, data: Record<string, string>) {
        currentLocale = locale;
        translations = data;
    }

    function setFallbackTranslations(data: Record<string, string>) {
        fallbackTranslations = data;
    }

    function getLocale(): string {
        return currentLocale;
    }

    function t(key: string, vars?: Record<string, string | number>): string {
        let text = translations[key] ?? fallbackTranslations[key] ?? key;
        if (vars) {
            for (const [k, v] of Object.entries(vars)) {
                text = text.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
            }
        }
        return text;
    }

    // Auto-initialize from build-time embedded locale data
    const allData = (window as any).__ROADWORK_LOCALE_DATA_ALL__;
    if (allData) {
        const locale = detectLocale();
        const fallbackLocale = locale === "fr" ? "en" : "fr";
        setTranslations(locale, allData[locale] || {});
        setFallbackTranslations(allData[fallbackLocale] || {});
    }

    (window as any).__rw_i18n = {
        t,
        setTranslations,
        setFallbackTranslations,
        getLocale,
        detectLocale,
    };
})();
