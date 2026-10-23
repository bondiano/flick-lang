// The sidebar: guides first, then the reference by topic.
export const sidebar = [
  { label: "Start", translations: { ru: "Начало" }, items: [{ slug: "start/getting-started" }, { slug: "playground" }] },
  {
    label: "Language",
    translations: { ru: "Язык" },
    items: ["types", "effects", "classes", "regions", "modules", "grammar", "keywords"].map((f) => ({ slug: `reference/${f}` })),
  },
  {
    label: "Libraries",
    translations: { ru: "Библиотеки" },
    items: ["stdlib", "effect-surface", "schema", "sql", "ui", "http-api", "cli-dsl", "ffi"].map((f) => ({ slug: `reference/${f}` })),
  },
  {
    label: "Tools",
    translations: { ru: "Инструменты" },
    items: ["cli", "project", "editor", "codegen", "runtime", "iface"].map((f) => ({ slug: `reference/${f}` })),
  },
  { label: "Contributing", translations: { ru: "Участие в разработке" }, items: [{ slug: "contributing/testing" }] },
];
