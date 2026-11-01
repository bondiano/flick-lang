// The tags Intl.Collator has data for, by the collation each takes, and
// a corpus of pairs of strings as each collation orders them.
import { cps, hex } from "./root.mjs";

// the languages of the 52 common locales (tools/go-intl/gen.mjs)
const LANGUAGES = [
  "en", "de", "fr", "it", "es", "pt", "nl", "sv", "da", "nb", "fi", "is", "pl", "cs", "sk", "hu", "ro", "bg", "hr",
  "sl", "el", "ru", "uk", "tr", "he", "ja", "ko", "zh", "id", "vi",
];
const SCRIPTS = ["Latn", "Cyrl", "Grek", "Hebr", "Arab", "Hans", "Hant", "Jpan", "Kore", "Deva"];
const LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const REGIONS = [
  ...Array.from(LETTERS).flatMap((a) => Array.from(LETTERS, (b) => a + b)),
  ...Array.from({ length: 1000 }, (_, i) => String(i).padStart(3, "0")),
];

// strings of many scripts and the letters tailorings move
const BASE = [
  ..."abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
  " ", "-", "'", ",", "!", "$", "€", "ä", "ö", "ü", "ß", "æ", "ø", "å", "é", "è", "ê", "ë", "ñ", "ç", "č", "ř", "š", "ž",
  "ł", "đ", "ð", "þ", "ı", "İ", "ğ", "ş", "ő", "ű", "ă", "â", "ơ", "ư", "ch", "cs", "dz", "ll", "aa", "ae", "oe", "th",
  "α", "β", "γ", "Α", "ά", "а", "б", "в", "г", "ґ", "ї", "й", "ё", "Ж", "א", "ב", "ש", "あ", "ア", "ぁ", "ー", "ゝ", "か",
  "が", "가", "각", "ㄱ", "한", "一", "丁", "七", "阿", "啊", "吖", "中", "\u{20000}", "\u{2EBF0}", "\u0378", "\uFFFD",
  "a\u0301", "a\u0308", "a\u0323\u0308", "o\u0302\u0301", "e\u0300", "\u0301", "\u0308",
];

const collator = (tag) => new Intl.Collator(tag).compare;

let seed = 7;
const random = (n) => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed % n;
};
const sample = (list, n) => (list.length <= n ? list : Array.from({ length: n }, (_, i) => list[Math.floor((i * list.length) / n)]));

const keysOf = (mappings, n) => sample(mappings.map(([key, prefix]) => (prefix + key).normalize("NFC")), n);

/**
 * `[tag, id]` for each tag of the languages Intl.Collator has data for: the
 * collation whose representative tag sorts a probe as the tag does.
 */
export const localeMap = (collations) => {
  const representatives = collations.map(({ id }) => (id === "root" ? "en" : id));
  // a key, and it and its letters without marks before another letter
  const probe = [...new Set([...BASE, ...collations.flatMap(({ mappings }) => keysOf(mappings, 30)).flatMap((k) => [k, `${k}z`, `${k.normalize("NFD").replace(/\p{M}/gu, "")}z`])])];
  const available = (tag) => new Intl.Collator(tag).resolvedOptions().locale === tag;
  const tags = LANGUAGES.flatMap((l) => [l, ...SCRIPTS.map((s) => `${l}-${s}`).filter(available)])
    .flatMap((base) => [base, ...REGIONS.map((r) => `${base}-${r}`).filter(available)])
    .filter(available);
  const fingerprint = (tag) => [...probe].sort(collator(tag)).join("\u0001");
  const prints = representatives.map(fingerprint);
  prints.forEach((p, i) => {
    if (prints.indexOf(p) !== i) throw new Error(`${representatives[i]} sorts the probe as ${representatives[prints.indexOf(p)]}`);
  });
  return tags.map((tag) => {
    const i = prints.indexOf(fingerprint(tag));
    if (i < 0) throw new Error(`${tag} collates as none of the collations`);
    return [tag, collations[i].id];
  });
};

/** Lines `tag | a | b | sign`: per collation, neighbours in Intl's order and random pairs. */
export const corpus = (collations, tags) => {
  const out = [];
  for (const { id, mappings } of [{ id: "root", mappings: [] }, ...collations]) {
    const tagsOf = tags.filter(([, c]) => c === id).map(([t]) => t);
    const rep = id === "root" ? "en" : id;
    const keys = keysOf(mappings, 150);
    const pool = [...new Set([...keys, ...BASE, ...keys.flatMap((k) => [k + "a", k.toUpperCase(), k + "\u0301"])])];
    const strings = sample(pool, 300);
    for (let i = 0; i < 40; i++) strings.push(pool[random(pool.length)] + pool[random(pool.length)]);
    const compare = collator(rep);
    const sorted = [...new Set(strings)].sort(compare);
    const pairs = [
      ...sorted.slice(1).map((b, i) => [sorted[i], b]),
      ...Array.from({ length: 80 }, () => [sorted[random(sorted.length)], sorted[random(sorted.length)]]),
    ];
    pairs.forEach(([a, b], i) => {
      const tag = tagsOf[i % tagsOf.length] ?? rep;
      out.push(`${tag} | ${cps(a).map(hex).join(" ")} | ${cps(b).map(hex).join(" ")} | ${Math.sign(compare(a, b))}`);
    });
  }
  return out;
};
