// A ```flick block may declare what it uses on hidden lines, `#` alone or
// starting with `# `, and say how `flick-core/tests/corpus.rs` checks it
// (`flick ignore`, `flick fail`) or that `flick check --fix` leaves it as
// written (`flick keep`). The checker reads both; the page shows neither.

// shortcut: a raw string line starting with `# ` would be hidden too; no example has one, mark such a block `ignore`
const hidden = (line) => line === "#" || line.startsWith("# ");

/** The visible code of a block. */
const shown = (value) =>
  value
    .split("\n")
    .filter((line) => !hidden(line))
    .join("\n");

/** The meta of a block without the words only the checker reads. */
const meta = (words) => (words ?? "").replace(/(^|\s)(ignore|fail|keep)(?=\s|$)/g, "").trim() || null;

/** A Sätteri mdast plugin: ```flick blocks as the reader sees them. */
export const flickBlocks = {
  name: "flick-blocks",
  code: (node) => (node.lang === "flick" ? { ...node, value: shown(node.value), meta: meta(node.meta) } : undefined),
};
