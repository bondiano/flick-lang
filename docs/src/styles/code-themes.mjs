// The Shiki themes of the site's code. The two parts of an arrow's row carry the colours of the
// lines on the landing page: what a function needs and how it can fail.

const rows = (needs, fails, punctuation) => [
  { scope: ["meta.effect-row.needs"], settings: { foreground: needs } },
  { scope: ["meta.effect-row.fails"], settings: { foreground: fails } },
  {
    scope: [
      "punctuation.definition.effect-row",
      "meta.effect-row.needs punctuation.separator",
      "meta.effect-row.fails punctuation.separator",
    ],
    settings: { foreground: punctuation },
  },
];

const theme = ({ name, type, fg, bg, comment, keyword, string, number, operator, needs, fails }) => ({
  name,
  type,
  colors: { "editor.foreground": fg, "editor.background": bg },
  fg,
  bg,
  settings: [
    { settings: { foreground: fg, background: bg } },
    { scope: "meta.embedded", settings: { foreground: fg } },
    { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: comment } },
    { scope: ["keyword", "storage", "keyword.other", "keyword.control"], settings: { foreground: keyword } },
    { scope: ["string", "constant.character"], settings: { foreground: string } },
    { scope: ["constant.numeric", "constant.language"], settings: { foreground: number } },
    { scope: ["keyword.operator", "punctuation"], settings: { foreground: operator } },
    { scope: ["entity.name.function"], settings: { foreground: fg, fontStyle: "bold" } },
    { scope: ["invalid"], settings: { foreground: fails } },
    ...rows(needs, fails, operator),
  ],
});

export const flickLight = theme({
  name: "flick-lines-light",
  type: "light",
  fg: "#0e1116",
  bg: "#ffffff",
  comment: "#5a6470",
  keyword: "#5b3fd6",
  string: "#0a7a4c",
  number: "#8a5a00",
  operator: "#3a424d",
  needs: "#1560d4",
  fails: "#c8283f",
});

export const flickDark = theme({
  name: "flick-lines-dark",
  type: "dark",
  fg: "#f4f6f8",
  bg: "#141c40",
  comment: "#98a2ae",
  keyword: "#b7a5ff",
  string: "#3fd08f",
  number: "#ffc20e",
  operator: "#c4cbd4",
  needs: "#6aa9ff",
  fails: "#ff7a8c",
});
