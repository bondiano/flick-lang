// Writes runtime-go/flickrt/collate_tables.go: the collations of
// `localeCompare` for the languages of the 52 common locales, as Node's
// ICU builds them, so that both targets order strings alike: the CLDR
// root collation (FractionalUCA.txt) and the tailorings of CLDR's
// `common/collation/*.xml`, applied as ICU's CollationBuilder applies
// them, with the locale each tag resolves to.
// It also writes runtime-go/flickrt/testdata/collation.txt, pairs of
// strings each collation orders as Intl.Collator does.
// Run from the repository root: `node tools/go-collate/gen.mjs`. It
// fetches CLDR into ~/.cache/flick-cldr on first run.
import { writeFileSync } from "node:fs";
import { cldrCommon, CLDR, COMMON, cps, hex, readRoot } from "./root.mjs";
import { collationRules, parseRules } from "./rules.mjs";
import { Tailoring } from "./tailor.mjs";
import { corpus, localeMap } from "./corpus.mjs";

const out = "runtime-go/flickrt/collate_tables.go";
const corpusOut = "runtime-go/flickrt/testdata/collation.txt";

// the collations: a representative tag, the CLDR file and type
const COLLATIONS = [
  ["fr-CA", "fr_CA", "standard"], ["es", "es", "standard"], ["sv", "sv", "standard"], ["da", "da", "standard"],
  ["nb", "no", "standard"], ["fi", "fi", "standard"], ["is", "is", "standard"], ["pl", "pl", "standard"],
  ["cs", "cs", "standard"], ["sk", "sk", "standard"], ["hu", "hu", "standard"], ["ro", "ro", "standard"],
  ["bg", "bg", "standard"], ["hr", "hr", "standard"], ["sl", "sl", "standard"], ["el", "el", "standard"],
  ["ru", "ru", "standard"], ["uk", "uk", "standard"], ["tr", "tr", "standard"], ["he", "he", "standard"],
  ["vi", "vi", "standard"], ["ja", "ja", "standard"], ["ko", "ko", "standard"], ["zh", "zh", "pinyin"],
  ["zh-Hant", "zh", "stroke"],
];

const common = await cldrCommon();
const root = readRoot(common);
const imported = (tag) => {
  const [, language, type] = /^([a-z]+)-u-co-(.+)$/.exec(tag);
  const rules = collationRules(common, language === "und" ? "root" : language, type);
  if (rules === undefined) throw new Error(`no rules ${tag}`);
  return parseRules(rules);
};

// A weight as hex: its bytes without trailing zero ones, then a tailored
// offset; an element `p.s.t` without a common tertiary and secondary.
const bytesHex = (v, width) => v.toString(16).padStart(width * 2, "0").replace(/(00)+$/, "");
const offset = (k) => (k === 0 ? "" : k > 0 ? `+${hex(k)}` : `-${hex(-k)}`);
const elementText = ([p, kp, s, ks, t, kt]) => {
  const P = bytesHex(p, 4) + offset(kp), S = bytesHex(s, 2) + offset(ks), T = bytesHex(t, 2) + offset(kt);
  return T === "05" ? (S === "05" ? P : `${P}.${S}`) : `${P}.${S}.${T}`;
};
const keyText = (s) => cps(s).map(hex).join(",");
const mappingText = ([key, prefix, ces]) =>
  `${prefix === "" ? "" : `${keyText(prefix)}|`}${keyText(key)}:${ces.map(elementText).join(" ")}`;

// Mappings in the order of their weights, with runs of single characters
// that take one tailored primary each, one after another: `@p+k:chars`.
const mappingsText = (mappings) => {
  const sorted = [...mappings].sort(([, , a], [, , b]) => {
    for (let i = 0; i < 6; i++) if (a[0][i] !== b[0][i]) return a[0][i] - b[0][i];
    return 0;
  });
  const out = [];
  let run;
  for (const m of sorted) {
    const [key, prefix, ces] = m;
    const [p, kp, s, ks, t, kt] = ces[0];
    const simple = prefix === "" && ces.length === 1 && cps(key).length === 1 && key.codePointAt(0) >= 0x3000 &&
      kp > 0 && s === COMMON && ks === 0 && t === COMMON && kt === 0;
    if (simple && run !== undefined && run.p === p && run.next === kp) {
      run.text += key;
      run.next++;
      continue;
    }
    if (run !== undefined) out.push(`@${bytesHex(run.p, 4)}+${hex(run.k)}:${run.text}`);
    run = undefined;
    if (simple) run = { p, k: kp, next: kp + 1, text: key };
    else out.push(mappingText(m));
  }
  if (run !== undefined) out.push(`@${bytesHex(run.p, 4)}+${hex(run.k)}:${run.text}`);
  return out.join(";");
};

// ICU's reordering by lead byte: the groups the codes name move, in
// order, after the space, punctuation, symbol, currency and digit groups;
// the other groups keep their order after them.
const reordering = (codes) => {
  if (codes.length === 0) return "";
  const groups = [];
  for (let b = 3; b < 0xe4; b++) {
    if (groups.length && groups.at(-1).label === root.groups[b]) groups.at(-1).bytes.push(b);
    else groups.push({ label: root.groups[b], bytes: [b] });
  }
  const special = (g) => /SPACE|PUNCTUATION|SYMBOL|CURRENCY|DIGIT/.test(g.label);
  const named = codes.map((code) => groups.find((g) => g.label.split(" ").includes(code)) ?? (() => { throw new Error(`[reorder ${code}]`); })());
  const order = [...groups.filter(special), ...named, ...groups.filter((g) => !special(g) && !named.includes(g))];
  const perm = Array.from({ length: 256 }, (_, b) => b);
  let next = 3;
  for (const g of order) for (const b of g.bytes) perm[b] = next++;
  return perm.map((b) => b.toString(16).padStart(2, "0")).join("");
};

const rootText = mappingsText([
  ...[...root.entries].map(([key, ces]) => [key, "", ces.map(([p, s, t]) => [p, 0, s, 0, t, 0])]),
  ...[...root.prefixed].flatMap(([key, list]) => list.map(([prefix, ces]) => [key, prefix, ces.map(([p, s, t]) => [p, 0, s, 0, t, 0])])),
]);

const lines = COLLATIONS.map(([id, file, type]) => {
  const t = new Tailoring(root);
  t.apply(parseRules(collationRules(common, file, type)), imported);
  const { backwards, upperFirst, reorder } = t.settings;
  const flags = `${backwards ? "b" : ""}${upperFirst ? "u" : ""}`;
  console.log(`${id}: ${t.map.size + t.prefixed.size} mappings${flags ? ` [${flags}]` : ""}${reorder.length ? ` [reorder ${reorder.join(" ")}]` : ""}`);
  const mappings = t.resolve();
  return { id, mappings, text: `${id}\t${flags}\t${reordering(reorder)}\t${mappingsText(mappings)}` };
});

// collations alike (bg and ru) are one
const distinct = lines.filter((l, i) => lines.findIndex((m) => m.text.split("\t").slice(1).join() === l.text.split("\t").slice(1).join()) === i);
const tags = localeMap([{ id: "root", mappings: [] }, ...distinct]);

const go = `// Code generated by tools/go-collate/gen.mjs; DO NOT EDIT.

package flickrt

// CLDR ${CLDR}, as Node ${process.versions.node} (ICU ${process.versions.icu}) collates.
const (
	// collationRootData is the root's mappings, apart by ";": \`key:elements\` or
	// \`prefix|key:elements\`, the key and prefix code points in hex apart by ",", the elements
	// apart by " ", each \`p.s.t\` (a secondary and tertiary of 05 left out): weights in hex
	// bytes, a primary of 4 and the others of 2, without trailing zero bytes; a tailored weight
	// adds an offset (\`+k\`, \`-k\`) after the root weight it follows or precedes. A run
	// \`@p+k:chars\` gives its characters one primary each, from \`p+k\` on.
	collationRootData = "${rootText}"
	// hanData is Unified_Ideograph in the order of their implicit weights, radical-stroke
	// order: characters, and ranges \`a-b\`.
	hanData = \`${root.han}\`
	// collationData is a line per tailoring: its id, its flags (\`b\` backwards secondaries,
	// \`u\` upper case first), the new lead byte of each lead byte of a primary if it reorders
	// scripts, and its mappings as collationRootData's, apart by tabs.
	collationData = \`${distinct.map((l) => l.text).join("\n")}\`
	// collationLocaleData is the tags Intl.Collator has data for and their collation ids:
	// \`tag=id\`, apart by ";".
	collationLocaleData = "${tags.map(([tag, id]) => `${tag}=${id}`).join(";")}"
)
`;
writeFileSync(out, go);
console.log(`${out}: ${go.length} bytes, ${tags.length} tags`);

const pairs = corpus(distinct, tags);
writeFileSync(corpusOut, `# locale | a | b | sign of Intl.Collator(locale).compare(a, b), as code points\n${pairs.join("\n")}\n`);
console.log(`${corpusOut}: ${pairs.length} pairs`);
