// Documentation entry points, with existing reference URLs preserved.
export const sidebar = [
  {
    label: "Learn",
    translations: { ru: "Изучение Flick" },
    items: [{ slug: "learn" }, { slug: "start/getting-started" }, { slug: "learn/flick-and-effect" }],
  },
  {
    label: "Guides",
    translations: { ru: "Руководства" },
    items: [{ slug: "guides" }, ...["ui", "cli-dsl", "http-api", "rpc", "server-fn", "route", "form", "observability", "schema", "sql", "ffi"].map((f) => ({ slug: `reference/${f}` }))],
  },
  {
    label: "API",
    translations: { ru: "Справочник" },
    items: [
      { slug: "api" },
      {
        label: "Language",
        translations: { ru: "Язык" },
        items: ["types", "effects", "classes", "regions", "modules", "grammar", "keywords"].map((f) => ({ slug: `reference/${f}` })),
      },
      {
        label: "Runtime",
        translations: { ru: "Среда выполнения" },
        items: ["stdlib", "effect-surface", "runtime"].map((f) => ({ slug: `reference/${f}` })),
      },
    ],
  },
  {
    label: "Specification",
    translations: { ru: "Спецификация" },
    items: [{ autogenerate: { directory: "spec" } }],
  },
  {
    label: "Ecosystem",
    translations: { ru: "Экосистема" },
    items: [
      { slug: "ecosystem" },
      ...["cli", "project", "editor", "codegen", "iface"].map((f) => ({ slug: `reference/${f}` })),
      { slug: "contributing/testing" },
      { slug: "contributing/implementation-decisions" },
    ],
  },
  { label: "Play", translations: { ru: "Песочница" }, items: [{ slug: "play" }, { slug: "transpile" }] },
];
