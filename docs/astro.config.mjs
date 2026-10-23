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

export default defineConfig({
  site: "https://flick-lang.dev",
  output: "static",
  integrations: [
    flick({ binary: process.env.FLICK_BIN ?? local ?? "flick" }),
    starlight({
      title: "Flick",
      description: "An ML language with higher-kinded types, typed effects and regions, on Effect",
      sidebar,
      customCss: ["./src/styles/custom.css"],
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/bondiano/flick-lang" }],
      editLink: { baseUrl: "https://github.com/bondiano/flick-lang/edit/main/docs/" },
      expressiveCode: { shiki: { langs: [{ ...grammar, name: "flick" }] } },
    }),
  ],
});
