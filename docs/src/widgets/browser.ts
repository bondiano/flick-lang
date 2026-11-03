// Platform/compiler interop only. Widget state and event policies live in Flick.
export const isRussian = (): boolean => document.documentElement.lang === "ru";

// The site path of the locale, under the deployment base.
export const localePrefix = (ru: boolean): string => `${import.meta.env.BASE_URL.replace(/\/$/, "")}${ru ? "/ru" : ""}`;

export const preferences = () => ({
  ru: isRussian(),
  dark: document.documentElement.dataset.theme === "dark",
});

const remember = (key: string, value: string): void => {
  try { localStorage.setItem(key, value); } catch { /* Private browsing may disable preference storage. */ }
};

export const setDark = (dark: boolean): void => {
  const theme = dark ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  remember("starlight-theme", theme);
};

export const openSearch = (): void => {
  const dialog = document.querySelector<HTMLDialogElement>("#flick-search-dialog");
  dialog?.showModal();
  dialog?.querySelector<HTMLInputElement>("input")?.focus();
};
export const closeSearch = (): void => document.querySelector<HTMLDialogElement>("#flick-search-dialog")?.close();
export const copyText = (source: string): Promise<void> => navigator.clipboard.writeText(source);

interface SearchData { meta: { title?: string }; excerpt: string; url: string }
interface SearchIndex {
  search(query: string): Promise<{ results: Array<{ data(): Promise<SearchData> }> }>;
}
interface SearchModule {
  createInstance(settings: { noWorker: boolean }): SearchIndex;
}
let index: Promise<SearchIndex> | undefined;

export const search = async (query: string) => {
  // The production-only Pagefind module does not exist in source, so a static import cannot resolve it.
  const path = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/pagefind/pagefind.js`;
  const pagefind = await (index ??= import(/* @vite-ignore */ path).then((module: SearchModule) => {
    // shortcut: Pagefind 1.5 gives blob workers relative URLs; re-enable workers when its initializer preserves absolute paths.
    return module.createInstance({ noWorker: true });
  }).catch((error: unknown) => {
    index = undefined;
    throw error;
  }));
  const results = await pagefind.search(query);
  return Promise.all(results.results.slice(0, 12).map(async (hit) => {
    const data = await hit.data();
    return {
      title: data.meta.title ?? data.url,
      excerpt: new DOMParser().parseFromString(data.excerpt, "text/html").body.textContent ?? "",
      url: data.url,
    };
  }));
};

