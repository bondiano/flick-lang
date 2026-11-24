// The landing page's two languages, written side by side.
export const texts = (locale: string) => (en: string, russian: string): string => locale === "ru" ? russian : en;

/** Backticks in the page's prose become `<code>`. */
export const inlineCode = (value: string): string => value.replace(/`([^`]+)`/g, "<code>$1</code>");
