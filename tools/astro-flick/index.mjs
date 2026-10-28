// The Astro integration: adds the Vite plugin that compiles `.fl` imports,
// and writes the declarations editors read before the first build.
import { fileURLToPath } from "node:url";

import { declare, flickVite } from "./vite.mjs";

export { flickVite };

/** @param {Parameters<typeof flickVite>[0]} options */
export default function flick(options = {}) {
  return {
    name: "@flick-lang/astro",
    hooks: {
      "astro:config:setup": async ({ config, updateConfig, logger }) => {
        updateConfig({ vite: { plugins: [flickVite(options)] } });
        if (options.declarations !== false) {
          const binary = options.binary ?? process.env.FLICK_BIN ?? "flick";
          await declare(binary, [fileURLToPath(config.srcDir)], (message) => logger.warn(message));
        }
      },
    },
  };
}
