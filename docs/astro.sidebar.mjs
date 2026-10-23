// The sidebar: guides first, then the reference by topic.
export const sidebar = [
  { label: "Start", items: [{ slug: "start/getting-started" }, { slug: "playground" }] },
  {
    label: "Language",
    items: ["types", "effects", "classes", "regions", "modules", "grammar"].map((f) => ({ slug: `reference/${f}` })),
  },
  {
    label: "Libraries",
    items: ["stdlib", "effect-surface", "schema", "sql", "ui", "http-api", "cli-dsl", "ffi"].map((f) => ({ slug: `reference/${f}` })),
  },
  {
    label: "Tools",
    items: ["cli", "project", "editor", "codegen", "runtime", "iface"].map((f) => ({ slug: `reference/${f}` })),
  },
  { label: "Contributing", items: [{ slug: "contributing/testing" }] },
];
