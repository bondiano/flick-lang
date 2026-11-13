import { i18nLoader } from "@astrojs/starlight/loaders";
import { docsSchema, i18nSchema } from "@astrojs/starlight/schema";
import { glob } from "astro/loaders";
import { contentId } from "../scripts/api.mjs";
import { defineCollection } from "astro:content";
import { z } from "astro/zod";

export const collections = {
  // `source_hash`: the hash of the original a translation was made from (`docs/scripts/parity.mjs`)
  docs: defineCollection({ loader: glob({ base: "./", pattern: ["src/content/docs/**/[^_]*.{md,mdx,markdown,mdown,mkdn,mkd,mdwn}", ".generated/**/*.md"], generateId: contentId }), schema: docsSchema({ extend: z.object({ source_hash: z.string().optional() }) }) }),
  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};
