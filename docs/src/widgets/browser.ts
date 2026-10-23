// Platform/compiler interop only. Widget state and event policies live in Flick.
export const isRussian = (): boolean => document.documentElement.lang === "ru";

export const preferences = () => ({
  ru: isRussian(),
  dark: document.documentElement.dataset.theme === "dark",
  paused: document.documentElement.dataset.motion === "paused" || matchMedia("(prefers-reduced-motion: reduce)").matches,
  home: document.body.classList.contains("flick-home"),
});

const remember = (key: string, value: string): void => {
  try { localStorage.setItem(key, value); } catch { /* Private browsing may disable preference storage. */ }
};

export const setDark = (dark: boolean): void => {
  const theme = dark ? "dark" : "light";
  document.documentElement.dataset.theme = theme;
  remember("starlight-theme", theme);
};

export const setPaused = (paused: boolean): void => {
  document.documentElement.dataset.motion = paused ? "paused" : "running";
  remember("flick-motion", paused ? "paused" : "running");
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
  options(settings: { basePath: string }): Promise<void>;
  search(query: string): Promise<{ results: Array<{ data(): Promise<SearchData> }> }>;
}
let index: Promise<SearchIndex> | undefined;

export const search = async (query: string) => {
  // The production-only Pagefind module does not exist in source, so a static import cannot resolve it.
  const path = "/pagefind/pagefind.js";
  const pagefind = await (index ??= import(/* @vite-ignore */ path).then(async (module: SearchIndex) => {
    // Blob workers need an absolute URL for the index and metadata fetches.
    await module.options({ basePath: new URL("/pagefind/", location.href).href });
    return module;
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

