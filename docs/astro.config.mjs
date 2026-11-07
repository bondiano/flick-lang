import { existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

import flick from "@flick-lang/astro";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

import { sidebar } from "./astro.sidebar.mjs";
import { flickBlocks } from "./flick-blocks.mjs";
import { flickDark, flickLight } from "./src/styles/code-themes.mjs";

// the newest compiler of this checkout, unless FLICK_BIN names another
const local = ["release", "debug"]
  .map((p) => fileURLToPath(new URL(`../target/${p}/flick`, import.meta.url)))
  .filter((p) => existsSync(p))
  .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];

/** The TextMate grammar of the VS Code extension highlights `flick` blocks. */
const grammar = JSON.parse(readFileSync(new URL("../editors/vscode/syntaxes/flick.tmLanguage.json", import.meta.url), "utf8"));

// GitHub Pages serves the site under the repository name.
const site = "https://bondiano.github.io";
const base = "/flick-lang";

const output = {
  // Effect's cyclic modules must keep their execution order across chunks.
  strictExecutionOrder: true,
  codeSplitting: {
    groups: [{ name: "vendor", test: /node_modules/, maxSize: 400_000 }],
  },
};

export default defineConfig({
  site,
  base,
  output: "static",
  redirects: {
    "/start/upgrading/": "/handbook/upgrading/",
    "/ru/start/upgrading/": "/ru/handbook/upgrading/",
  },
  vite: {
    environments: {
      client: { build: { rolldownOptions: { output } } },
    },
    worker: {
      format: "es",
      rolldownOptions: { output },
    },
  },
  integrations: [
    flick({ binary: process.env.FLICK_BIN ?? local ?? "flick" }),
    {
      name: "flick-blocks",
      hooks: {
        // Sätteri, Astro's Markdown processor, takes mdast plugins as Starlight adds its own
        "astro:config:setup": ({ config }) => config.markdown.processor.options.mdastPlugins.push(flickBlocks),
      },
    },
    starlight({
      title: "Flick",
      description: "A language for whole apps, where every function does what it says",
      favicon: "/favicon.ico",
      head: [
        { tag: "link", attrs: { rel: "icon", type: "image/svg+xml", href: `${base}/favicon.svg` } },
        { tag: "link", attrs: { rel: "apple-touch-icon", href: `${base}/apple-touch-icon.png`, sizes: "180x180" } },
        { tag: "meta", attrs: { property: "og:image", content: `${site}${base}/social.png` } },
        { tag: "meta", attrs: { property: "og:image:type", content: "image/png" } },
        { tag: "meta", attrs: { property: "og:image:width", content: "1200" } },
        { tag: "meta", attrs: { property: "og:image:height", content: "630" } },
        { tag: "meta", attrs: { property: "og:image:alt", content: "Flick" } },
        { tag: "meta", attrs: { name: "twitter:image", content: `${site}${base}/social.png` } },
        { tag: "meta", attrs: { name: "twitter:image:alt", content: "Flick" } },
      ],
      disable404Route: true,
      defaultLocale: "root",
      locales: {
        root: { label: "English", lang: "en" },
        ru: { label: "Русский" },
      },
      sidebar,
      customCss: ["./src/styles/custom.css"],
      components: {
        Header: "./src/components/Header.astro",
        PageFrame: "./src/components/DocFrame.astro",
        Sidebar: "./src/components/Sidebar.astro",
        ThemeProvider: "./src/components/ThemeInit.astro",
        TableOfContents: "./src/components/TableOfContents.astro",
        MobileTableOfContents: "./src/components/MobileTableOfContents.astro",
      },
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/bondiano/flick-lang" }],
      editLink: { baseUrl: "https://github.com/bondiano/flick-lang/edit/main/docs/" },
      expressiveCode: {
        themes: [flickDark, flickLight],
        useStarlightUiThemeColors: false,
        frames: { showCopyToClipboardButton: false },
        shiki: { langs: [{ ...grammar, name: "flick" }] },
        styleOverrides: {
          borderRadius: "0",
          borderColor: "var(--sl-color-hairline)",
          codeFontFamily: "var(--sl-font-mono)",
          uiFontFamily: "var(--sl-font)",
          codeLineHeight: "1.6",
          frames: { frameBoxShadowCssValue: "none" },
        },
      },
    }),
  ],
});
