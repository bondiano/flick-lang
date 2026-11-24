import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

import { createHighlighterCore } from "shiki/core";
import type { LanguageRegistration } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import toml from "shiki/langs/toml.mjs";
import typescript from "shiki/langs/typescript.mjs";

import grammar from "../../../editors/vscode/syntaxes/flick.tmLanguage.json";
import { flickDark, flickLight } from "../styles/code-themes.mjs";

const highlighter = await createHighlighterCore({
  engine: createJavaScriptRegexEngine(),
  themes: [flickLight, flickDark],
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the editor grammar is a TextMate grammar
  langs: [{ ...grammar, name: "flick" } as LanguageRegistration, typescript, toml],
});

/** Highlighted markup in both themes; `focus` is the one line under discussion, counted from 1. */
export const highlight = (code: string, lang: "flick" | "typescript" | "toml", focus?: number): string =>
  highlighter.codeToHtml(code.trimEnd(), {
    lang,
    themes: { light: flickLight.name, dark: flickDark.name },
    defaultColor: false,
    transformers: [{
      line(node, line) {
        if (line === focus) this.addClassToHast(node, "is-focus");
      },
    }],
  });

// From the site's root: the Flick plugin compiles an imported `.fl`, even with `?raw`.
const samples = path.join(process.cwd(), "src/widgets/samples");
const MARKER = "// show\n";

/** The parts of a checked sample that a page shows, what follows each `// show` marker, with where each starts in the file. */
export const sample = (name: string): ReadonlyArray<{ readonly text: string; readonly offset: number }> => {
  const [hidden = "", ...pieces] = readFileSync(path.join(samples, `${name}.fl`), "utf8").split(MARKER);
  let at = hidden.length;
  return pieces.map((piece) => {
    const offset = at + MARKER.length + piece.length - piece.trimStart().length;
    at += MARKER.length + piece.length;
    return { text: piece.trim(), offset };
  });
};

export const shown = (name: string): readonly string[] => sample(name).map((part) => part.text);

/** Every sample's whole source by name, as JSON safe inside a `<script>`: the browser compiler answers hovers on it. */
export const sampleSources = (): string =>
  JSON.stringify(Object.fromEntries(readdirSync(samples).filter((file) => file.endsWith(".fl")).map((file) => [file.slice(0, -3), readFileSync(path.join(samples, file), "utf8")]))).replaceAll("<", "\\u003c");
