// A ```flick block may declare what it uses on hidden lines, `#` alone or
// starting with `# `, and say how `flick-core/tests/corpus.rs` checks it
// (`flick fail T010`, `flick ignore <reason>`) or that `flick check --fix`
// leaves it as written (`flick keep`). The checker reads both; the page shows
// neither, nor the words that follow them.

// shortcut: a raw string line starting with `# ` would be hidden too; no example has one, mark such a block `ignore`
const hidden = (line) => line === "#" || line.startsWith("# ");

/** The visible code of a block. */
const shown = (value) =>
  value
    .split("\n")
    .filter((line) => !hidden(line))
    .join("\n");

/** Expressive Code's `key=value` props, such as `title="Main.fl"`. */
const props = /\b[\w-]+=(?:"[^"]*"|'[^']*'|\S+)/g;

/** The meta of a block without the words only the checker reads; frame props stay. */
const meta = (words) => {
  const rest = (words ?? "").trim();
  if (!/^(ignore|fail|keep)(\s|$)/.test(rest)) return rest || null;
  return rest.match(props)?.join(" ") || null;
};

/** A Sätteri mdast plugin: ```flick blocks as the reader sees them. */
export const flickBlocks = {
  name: "flick-blocks",
  code: (node) => (node.lang === "flick" ? { ...node, value: shown(node.value), meta: meta(node.meta) } : undefined),
};
