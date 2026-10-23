// The Astro integration: adds the Vite plugin that compiles `.fl` imports.
import { flickVite } from "./vite.mjs";

export { flickVite };

/** @param {Parameters<typeof flickVite>[0]} options */
export default function flick(options = {}) {
  return {
    name: "@flick-lang/astro",
    hooks: {
      "astro:config:setup": ({ updateConfig }) => {
        updateConfig({ vite: { plugins: [flickVite(options)] } });
      },
    },
  };
}
