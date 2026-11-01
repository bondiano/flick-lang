// CLDR collation rules as ICU's CollationRuleParser reads them, into
// settings, resets and relations, and the collations of CLDR's XML.
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The rules of a collation type in a CLDR collation file, or undefined:
 * its `alt="short"` variant if it has one, which ICU builds its data from.
 */
export const collationRules = (common, locale, type) => {
  let xml;
  try {
    xml = readFileSync(join(common, "collation", `${locale}.xml`), "utf8");
  } catch {
    return undefined;
  }
  const found = new Map();
  for (const [, attrs, body] of xml.matchAll(/<collation\b([^>]*)>([\s\S]*?)<\/collation\s*>/g)) {
    const a = Object.fromEntries(Array.from(attrs.matchAll(/(\w+)=['"]([^'"]*)['"]/g), ([, k, v]) => [k, v]));
    if (a.type === type && (a.alt === undefined || a.alt === "short")) {
      found.set(a.alt ?? "", Array.from(body.matchAll(/<!\[CDATA\[([\s\S]*?)\]\]>/g), (m) => m[1]).join("\n"));
    }
  }
  return found.get("short") ?? found.get("");
};

/** The default collation type of a CLDR collation file. */
export const defaultType = (common, locale) => {
  try {
    const xml = readFileSync(join(common, "collation", `${locale}.xml`), "utf8");
    return /<defaultCollation>([^<]*)</.exec(xml)?.[1] ?? "standard";
  } catch {
    return "standard";
  }
};

// The ASCII characters that are syntax, not text, unless quoted.
const isSyntax = (c) => {
  const n = c.codePointAt(0);
  return n >= 0x21 && n <= 0x7e && (n <= 0x2f || (n >= 0x3a && n <= 0x40) || (n >= 0x5b && n <= 0x60) || n >= 0x7b);
};
const isSpace = (c) => /^\p{Pattern_White_Space}$/u.test(c);

const STRENGTHS = { "<": 1, "<<": 2, "<<<": 3, "<<<<": 4, "=": 15 };

/**
 * The rules as a list of `{ setting, value }`, `{ reset, special, before }`
 * (text or a special position, with the strength of a `[before n]` or 0) and
 * `{ strength, prefix, text, extension }`; a starred relation is one
 * relation for each character it lists.
 */
export const parseRules = (source) => {
  // CLDR's XML escapes as genrb unescapes it for ICU
  const rules = Array.from(
    source.replace(/\\u([0-9A-Fa-f]{4})|\\U([0-9A-Fa-f]{8})/g, (_, u, U) => String.fromCodePoint(parseInt(u ?? U, 16))),
  );
  const out = [];
  let i = 0;
  const fail = (what) => {
    throw new Error(`${what} at ${i}: ${rules.slice(Math.max(0, i - 20), i + 20).join("")}`);
  };
  const skipSpace = () => {
    for (;;) {
      while (i < rules.length && isSpace(rules[i])) i++;
      if (rules[i] !== "#") return;
      while (i < rules.length && rules[i] !== "\n" && rules[i] !== "\r") i++;
    }
  };
  // ICU's parseString: text up to unquoted syntax or space
  const string = () => {
    let s = "";
    while (i < rules.length) {
      const c = rules[i++];
      if (isSyntax(c)) {
        if (c === "'") {
          if (rules[i] === "'") {
            s += "'";
            i++;
            continue;
          }
          for (;;) {
            if (i === rules.length) fail("unclosed quote");
            const q = rules[i++];
            if (q === "'") {
              if (rules[i] !== "'") break;
              i++;
            }
            s += q;
          }
        } else if (c === "\\") {
          if (i === rules.length) fail("trailing backslash");
          s += rules[i++];
        } else {
          i--;
          break;
        }
      } else if (isSpace(c)) {
        i--;
        break;
      } else s += c;
    }
    return s;
  };
  const tailoringString = () => {
    skipSpace();
    const s = string();
    if (s === "") fail("missing text");
    return s;
  };
  const bracket = () => {
    let depth = 0, s = "";
    do {
      const c = rules[i++];
      if (c === undefined) fail("unclosed [");
      if (c === "[") depth++;
      if (c === "]") depth--;
      s += c;
    } while (depth > 0);
    return s.slice(1, -1).trim();
  };
  for (skipSpace(); i < rules.length; skipSpace()) {
    const c = rules[i];
    if (c === "[") {
      const [setting, ...value] = bracket().split(/\s+/);
      out.push({ setting, value: value.join(" ") });
    } else if (c === "&") {
      i++;
      skipSpace();
      let before = 0;
      if (rules[i] === "[") {
        const at = i;
        const m = /^before ([123])$/.exec(bracket());
        if (m) {
          before = Number(m[1]);
          skipSpace();
        } else i = at;
      }
      const special = rules[i] === "[";
      out.push({ reset: special ? bracket() : tailoringString(), special, before });
    } else if (c === "<" || c === "=") {
      let op = "";
      while (rules[i] === c && op.length < 4) op += rules[i++];
      const strength = STRENGTHS[op] ?? fail(`operator ${op}`);
      if (rules[i] === "*") {
        i++;
        skipSpace();
        let previous, range = false;
        for (;;) {
          const chars = Array.from(string());
          if (chars.length === 0) fail("empty starred list");
          let list = chars;
          if (range) {
            // `a-c` lists a to c
            list = [];
            for (let cp = previous + 1; cp <= chars[0].codePointAt(0); cp++) list.push(String.fromCodePoint(cp));
            list.push(...chars.slice(1));
          }
          for (const ch of list) out.push({ strength, prefix: "", text: ch, extension: "" });
          previous = list.at(-1).codePointAt(0);
          range = rules[i] === "-";
          if (!range) break;
          i++;
        }
      } else {
        let prefix = "", text = tailoringString(), extension = "";
        if (rules[i] === "|") {
          i++;
          prefix = text;
          text = tailoringString();
        }
        if (rules[i] === "/") {
          i++;
          extension = tailoringString();
        }
        out.push({ strength, prefix, text, extension });
      }
    } else fail("syntax");
  }
  return out;
};
