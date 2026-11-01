// The CLDR root collation, as ICU builds it from FractionalUCA.txt, and
// the CLDR data it comes with: fetched once into a cache outside the
// repository.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export const CLDR = "48";
const ZIP = `https://unicode.org/Public/cldr/${CLDR}/cldr-common-${CLDR}.zip`;

/** The directory of CLDR's `common` data, fetched on first use. */
export const cldrCommon = async () => {
  const cache = join(process.env.XDG_CACHE_HOME ?? join(homedir(), ".cache"), "flick-cldr", CLDR);
  const common = join(cache, "common");
  if (!existsSync(join(common, "uca", "FractionalUCA.txt"))) {
    mkdirSync(cache, { recursive: true });
    const zip = join(cache, "cldr-common.zip");
    const response = await fetch(ZIP);
    if (!response.ok) throw new Error(`${ZIP}: ${response.status}`);
    writeFileSync(zip, Buffer.from(await response.arrayBuffer()));
    execFileSync("unzip", ["-qo", zip, "common/uca/*", "common/collation/*", "-d", cache]);
  }
  return common;
};

export const hex = (n) => n.toString(16);
export const cps = (s) => Array.from(s, (c) => c.codePointAt(0));
export const nfd = (s) => s.normalize("NFD");

// A weight is ICU's bytes as a number, left-aligned: a primary in 32
// bits, a secondary or tertiary (with the case bits) in 16.
const bytes = (text, width) => {
  const b = text.trim().split(/\s+/).filter(Boolean).map((x) => parseInt(x, 16));
  if (b.length > width) throw new Error(`weight ${text}`);
  return b.reduce((n, x, i) => n + x * 256 ** (width - 1 - i), 0);
};
export const COMMON = 0x0500;

/**
 * The root: `entries` maps an NFD string to its collation elements
 * `[p, s, t]`, `prefixed` one to `[prefix, elements]` pairs for it after
 * a prefix; `han` is Unified_Ideograph in the order of their implicit
 * weights, radical-stroke order: characters and ranges `a-b`; `groups` the
 * reordering group of each lead byte; `lastRegular` the primary ICU resets
 * `[last regular]` to, the first of the Hani group.
 */
export const readRoot = (common) => {
  const text = readFileSync(join(common, "uca", "FractionalUCA.txt"), "utf8");
  const han = Array.from(text.matchAll(/^\[radical [^:]*:([^\]]*)\]/gm), (m) => m[1]).join("");
  const hanRank = new Map();
  const chars = Array.from(han);
  chars.forEach((c, i) => {
    if (c === "-") {
      for (let cp = chars[i - 1].codePointAt(0) + 1; cp < chars[i + 1].codePointAt(0); cp++) hanRank.set(cp, hanRank.size);
    } else hanRank.set(c.codePointAt(0), hanRank.size);
  });
  for (const r of /^\[Unified_Ideograph ([^\]]*)\]/m.exec(text)[1].split(" ")) {
    const [lo, hi = lo] = r.split("..").map((x) => parseInt(x, 16));
    for (let c = lo; c <= hi; c++) if (!hanRank.has(c)) throw new Error(`U+${hex(c)} has no radical`);
  }
  const implicitPrimary = (cp) => (hanRank.has(cp) ? 0xe0000000 + hanRank.get(cp) : 0xe4000000 + cp);

  const groups = new Array(256).fill("");
  for (const [, byte, label] of text.matchAll(/^\[top_byte\t([0-9A-F]{2})\t([^\]]*?)\s*\]/gm)) {
    groups[parseInt(byte, 16)] = label.replace(/\s+COMPRESS/, "").replace(/\s+/g, " ");
  }
  // implicit Han goes with the Hani group, unassigned code points not
  for (let b = 0xe0; b < 0xe4; b++) groups[b] = groups[0x81];

  const entries = new Map();
  const prefixed = new Map();
  let lastRegular;
  for (const line of text.split("\n")) {
    const m = /^([0-9A-F ]+?)\s*(?:\|\s*([0-9A-F ]+?))?\s*;\s*((?:\[[^\]]*\])+)/.exec(line);
    if (m === null) continue;
    const str = (h) => String.fromCodePoint(...h.trim().split(/\s+/).map((x) => parseInt(x, 16)));
    // `a | b` is b after a
    const [key, prefix] = m[2] === undefined ? [str(m[1]), ""] : [str(m[2]), str(m[1])];
    const ces = Array.from(m[3].matchAll(/\[([^\]]*)\]/g), ([, ce]) => {
      const parts = ce.split(",");
      const implicit = /^U\+([0-9A-F]+)/.exec(parts[0].trim());
      if (implicit) {
        const p = implicitPrimary(parseInt(implicit[1], 16));
        if (parts.length === 1) return [p, COMMON, COMMON];
        if (parts.length === 2) return [p, COMMON, bytes(parts[1], 2)];
        return [p, bytes(parts[1], 2), bytes(parts[2], 2)];
      }
      return [bytes(parts[0], 4), bytes(parts[1], 2), bytes(parts[2], 2)];
    });
    if (line.includes("# HAN first primary")) lastRegular = ces[0][0];
    if (nfd(key) !== key || nfd(prefix) !== prefix) continue; // NFD input never reaches it
    if (prefix === "") entries.set(key, ces);
    else prefixed.set(key, [...(prefixed.get(key) ?? []), [prefix, ces]]);
  }
  return { entries, prefixed, han, groups, lastRegular, implicitPrimary };
};
