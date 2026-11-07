import { readFileSync } from "node:fs";
import path from "node:path";

import { createHighlighterCore } from "shiki/core";
import type { LanguageRegistration } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import typescript from "shiki/langs/typescript.mjs";

import grammar from "../../../editors/vscode/syntaxes/flick.tmLanguage.json";
import { flickDark, flickLight } from "../styles/code-themes.mjs";

const highlighter = await createHighlighterCore({
  engine: createJavaScriptRegexEngine(),
  themes: [flickLight, flickDark],
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the editor grammar is a TextMate grammar
  langs: [{ ...grammar, name: "flick" } as LanguageRegistration, typescript],
});

/** Highlighted markup in both themes; `focus` is the one line under discussion, counted from 1. */
export const highlight = (code: string, lang: "flick" | "typescript", focus?: number): string =>
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

/** The parts of a checked sample that a page shows: what follows each `// show` marker. */
export const shown = (name: string): readonly string[] =>
  // From the site's root: the Flick plugin compiles an imported `.fl`, even with `?raw`.
  readFileSync(path.join(process.cwd(), `src/widgets/samples/${name}.fl`), "utf8").split("// show\n").slice(1).map((part) => part.trim());
