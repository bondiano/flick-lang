// A tailoring: CLDR rules applied to the root as ICU's CollationBuilder
// applies them. Weights are positions: a tailored primary follows a root
// primary (or another tailored one after it), a secondary is in the chain
// of its primary, a tertiary in that of its secondary. `resolve` numbers
// them: a root weight and the offset of a tailored one from the root
// weight before it (or, before any, from the first root weight after it).
import { COMMON, cps, nfd } from "./root.mjs";

const T_MASK = 0x3f3f; // a tertiary without its case bits
const START = { start: true }; // the position before a chain's first node

export class Tailoring {
  constructor(root) {
    this.root = root;
    this.map = new Map(); // NFD string → elements
    this.prefixed = new Map(); // NFD string → [[NFD prefix, elements]]
    this.settings = { backwards: false, upperFirst: false, reorder: [] };
    this.primaries = new Map(); // root primary → node
    this.maxKey = 1;
    // the secondaries of each root primary, the tertiaries of each pair
    this.rootSecondaries = new Map();
    this.rootTertiaries = new Map();
    const add = (map, key, value) => map.set(key, [...new Set([...(map.get(key) ?? []), value])].sort((a, b) => a - b));
    for (const ces of [...root.entries.values(), ...[...root.prefixed.values()].flatMap((l) => l.map(([, c]) => c))]) {
      for (const [p, s, t] of ces) {
        add(this.rootSecondaries, p, s);
        add(this.rootTertiaries, `${p}/${s}`, t & T_MASK);
      }
    }
    this.sortedPrimaries = [...this.rootSecondaries.keys()].filter((p) => p !== 0).sort((a, b) => a - b);
  }

  primaryNode(p) {
    if (!this.primaries.has(p)) this.primaries.set(p, { root: p, after: [], chain: undefined });
    return this.primaries.get(p);
  }
  // a primary's chain of secondaries, a secondary's chain of tertiaries:
  // the root's weights, or the common one under a tailored node
  secondaries(pn) {
    pn.chain ??= (pn.root === undefined ? [COMMON] : this.rootSecondaries.get(pn.root) ?? [COMMON]).map((s) => ({
      root: s,
      scope: pn,
      chain: undefined,
    }));
    return pn.chain;
  }
  tertiaries(sn) {
    const rootT = sn.root === undefined || sn.scope.root === undefined ? undefined : this.rootTertiaries.get(`${sn.scope.root}/${sn.root}`);
    sn.chain ??= (rootT ?? [COMMON]).map((t) => ({ root: t, scope: sn }));
    return sn.chain;
  }
  rootNodeIn(chain, value) {
    const found = chain.find((n) => n.root === value);
    if (found) return found;
    // a weight no root element of the scope has: after those below it
    const node = { root: value, scope: chain[0].scope, chain: undefined };
    const at = chain.findIndex((n) => n.root !== undefined && n.root > value);
    chain.splice(at < 0 ? chain.length : at, 0, node);
    return node;
  }
  rootElement([p, s, t]) {
    const pn = this.primaryNode(p);
    const sn = this.rootNodeIn(this.secondaries(pn), s);
    const tn = this.rootNodeIn(this.tertiaries(sn), t & T_MASK);
    return { p: pn, s: sn, t: tn, root: [p, s, t] };
  }

  /** The elements of a string as tailored so far: longest matches first. */
  elements(text) {
    const runes = cps(nfd(text));
    const out = [];
    for (let i = 0; i < runes.length; ) {
      let found, n;
      for (n = Math.min(runes.length - i, Math.max(this.maxKey, 4)); n > 0; n--) {
        const key = String.fromCodePoint(...runes.slice(i, i + n));
        found = this.map.get(key) ?? this.root.entries.get(key)?.map((ce) => this.rootElement(ce));
        if (found) break;
      }
      if (!found) {
        n = 1;
        found = [this.rootElement([this.root.implicitPrimary(runes[i]), COMMON, COMMON])];
      }
      out.push(...found);
      i += n;
    }
    return out;
  }

  special(position) {
    if (position === "last regular") return this.rootElement([this.root.lastRegular, COMMON, COMMON]);
    throw new Error(`reset to ${position}`);
  }

  /** The position just before an element at a level. */
  before(ce, level) {
    if (level === 1) {
      let prev;
      if (ce.p.root === undefined) {
        const i = ce.p.owner.after.indexOf(ce.p);
        prev = i > 0 ? ce.p.owner.after[i - 1] : ce.p.owner;
      } else {
        const i = this.sortedPrimaries.indexOf(ce.p.root);
        if (i <= 0) throw new Error("[before 1] the first primary");
        const owner = this.primaryNode(this.sortedPrimaries[i - 1]);
        prev = owner.after.at(-1) ?? owner;
      }
      return { p: prev };
    }
    const chain = level === 2 ? this.secondaries(ce.p) : this.tertiaries(ce.s);
    const i = chain.indexOf(level === 2 ? ce.s : ce.t);
    const prev = i > 0 ? chain[i - 1] : START;
    return level === 2 ? { p: ce.p, s: prev } : { p: ce.p, s: ce.s, t: prev };
  }

  /** A new element after `ce` at a strength, or `ce` for = and <<<<. */
  relate(ce, strength) {
    if (strength === 1) {
      const node = { owner: ce.p.root === undefined ? ce.p.owner : ce.p, after: undefined, chain: undefined };
      const list = node.owner.after;
      const at = ce.p.root === undefined ? (list.at(-1) === ce.p ? list.length - 1 : list.indexOf(ce.p)) : -1;
      list.splice(at + 1, 0, node);
      const sn = this.secondaries(node)[0];
      return { p: node, s: sn, t: this.tertiaries(sn)[0] };
    }
    if (strength === 2) {
      const chain = this.secondaries(ce.p);
      const node = { scope: ce.p, chain: undefined };
      chain.splice(ce.s === START ? 0 : chain.indexOf(ce.s) + 1, 0, node);
      return { p: ce.p, s: node, t: this.tertiaries(node)[0] };
    }
    if (strength === 3) {
      const chain = this.tertiaries(ce.s);
      const node = { scope: ce.s };
      chain.splice(ce.t === START ? 0 : chain.indexOf(ce.t) + 1, 0, node);
      return { p: ce.p, s: ce.s, t: node };
    }
    return ce;
  }

  /** Applies rules; `imported(tag)` gives the rules an `[import]` names. */
  apply(rules, imported) {
    let ce, prefixCEs = [];
    for (const rule of rules) {
      if (rule.setting !== undefined) {
        const { setting, value } = rule;
        if (setting === "import") this.apply(imported(value), imported);
        else if (setting === "reorder") this.settings.reorder = value.split(" ");
        else if (setting === "caseFirst") this.settings.upperFirst = value === "upper";
        else if (setting === "backwards") this.settings.backwards = value === "2";
        else if (!["normalization", "strength", "optimize"].includes(setting)) throw new Error(`[${setting} ${value}]`);
      } else if (rule.reset !== undefined) {
        const ces = rule.special ? [this.special(rule.reset)] : this.elements(rule.reset);
        prefixCEs = ces.slice(0, -1);
        ce = rule.before ? this.before(ces.at(-1), rule.before) : ces.at(-1);
      } else {
        ce = this.relate(ce, rule.strength);
        const ces = [...prefixCEs, ce, ...(rule.extension ? this.elements(rule.extension) : [])];
        const key = nfd(rule.text), prefix = nfd(rule.prefix);
        this.maxKey = Math.max(this.maxKey, cps(key).length);
        if (prefix === "") this.map.set(key, ces);
        else this.prefixed.set(key, [...(this.prefixed.get(key) ?? []).filter(([p]) => p !== prefix), [prefix, ces]]);
        ces.text = key; // for the case bits
      }
    }
  }

  /**
   * The mappings with numbered weights, `[key, prefix, [[p, kp, s, ks, t, kt]]]`,
   * t with the case bits ICU's setCaseBits gives a tailored string.
   */
  resolve() {
    const offsets = new Map();
    for (const pn of this.primaries.values()) pn.after.forEach((n, i) => offsets.set(n, i + 1));
    const numberChain = (chain) => {
      const roots = chain.flatMap((n, i) => (n.root === undefined ? [] : [i]));
      chain.forEach((n, i) => {
        if (n.root !== undefined) return;
        const base = roots.filter((r) => r < i).at(-1) ?? roots[0];
        offsets.set(n, [chain[base].root, i - base]);
      });
    };
    const seen = new Set();
    const weight = (ce) => {
      for (const [n, chain] of [[ce.s, ce.p.chain], [ce.t, ce.s.chain]]) {
        if (n.root === undefined && !seen.has(chain)) {
          seen.add(chain);
          numberChain(chain);
        }
      }
      const p = ce.p.root ?? ce.p.owner.root, kp = ce.p.root === undefined ? offsets.get(ce.p) : 0;
      const [s, ks] = ce.s.root === undefined ? offsets.get(ce.s) : [ce.s.root, 0];
      const [t, kt] = ce.t.root === undefined ? offsets.get(ce.t) : [ce.t.root, 0];
      return [p, kp, s, ks, t, kt];
    };
    const withCase = (ces) => {
      const out = ces.map(weight);
      const cases = caseBits(this.root, ces.text, out.filter(([p]) => p !== 0).length);
      return out.map((w, i) => {
        const [p, , s] = w;
        const bits = p !== 0 ? cases.shift() : s === 0 && w[4] !== 0 ? 2 : 0;
        return [...w.slice(0, 4), (w[4] & T_MASK) | (bits << 14), w[5]];
      });
    };
    return [
      ...[...this.map].map(([key, ces]) => [key, "", withCase(ces)]),
      ...[...this.prefixed].flatMap(([key, list]) => list.map(([prefix, ces]) => [key, prefix, withCase(ces)])),
    ];
  }
}

// ICU's setCaseBits: the case of each tailored primary from the root's
// elements of the string, mixed for a last one that covers several.
const caseBits = (root, text, primaries) => {
  const out = [];
  if (primaries === 0) return out;
  const base = new Tailoring.Base(root).elements(text).filter(([p]) => p !== 0).map(([, , t]) => t >> 14);
  let last = 0;
  base.forEach((c, i) => {
    if (i < primaries - 1) out.push(c);
    else if (i === primaries - 1) last = c;
    else if (c !== last) last = 1;
  });
  while (out.length < primaries - 1) out.push(0);
  if (base.length >= primaries) out.push(last);
  else out.push(0);
  return out;
};

// The root's elements of a string, as numbers.
Tailoring.Base = class {
  constructor(root) {
    this.root = root;
  }
  elements(text) {
    const runes = cps(nfd(text));
    const out = [];
    for (let i = 0; i < runes.length; ) {
      let n = Math.min(runes.length - i, 4);
      for (; n > 1 && !this.root.entries.has(String.fromCodePoint(...runes.slice(i, i + n))); n--);
      out.push(...(this.root.entries.get(String.fromCodePoint(...runes.slice(i, i + n))) ?? [[this.root.implicitPrimary(runes[i]), COMMON, COMMON]]));
      i += n;
    }
    return out;
  }
};
