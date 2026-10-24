import { existsSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";

import flick from "@flick-lang/astro";
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

import { sidebar } from "./astro.sidebar.mjs";

// the newest compiler of this checkout, unless FLICK_BIN names another
const local = ["release", "debug"]
  .map((p) => fileURLToPath(new URL(`../target/${p}/flick`, import.meta.url)))
  .filter((p) => existsSync(p))
  .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];

/** The TextMate grammar of the VS Code extension highlights `flick` blocks. */
const grammar = JSON.parse(readFileSync(new URL("../editors/vscode/syntaxes/flick.tmLanguage.json", import.meta.url), "utf8"));

const output = {
  // Effect's cyclic modules must keep their execution order across chunks.
  strictExecutionOrder: true,
  codeSplitting: {
    groups: [{ name: "vendor", test: /node_modules/, maxSize: 400_000 }],
  },
};

export default defineConfig({
  site: "https://flick-lang.dev",
  output: "static",
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
    starlight({
      title: "Flick",
      description: "An ML language with higher-kinded types, typed effects and regions, on Effect",
      favicon: "/flick-logo.svg",
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
        frames: { showCopyToClipboardButton: false },
        shiki: { langs: [{ ...grammar, name: "flick" }] },
      },
    }),
  ],
});
