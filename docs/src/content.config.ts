import { docsLoader, i18nLoader } from "@astrojs/starlight/loaders";
import { docsSchema, i18nSchema } from "@astrojs/starlight/schema";
import { defineCollection } from "astro:content";
import { z } from "astro/zod";

export const collections = {
  // `source_hash`: the hash of the original a translation was made from (`docs/scripts/parity.mjs`)
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema({ extend: z.object({ source_hash: z.string().optional() }) }) }),
  i18n: defineCollection({ loader: i18nLoader(), schema: i18nSchema() }),
};
