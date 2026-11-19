// The compiler of a schema's generator: a node per AST node and inherited
// constraint, settled together so a recursive type pays for each step into
// itself and prefers its base cases when the budget runs out.
import { err, none, ok, some } from "../../core/data.ts";
import { item } from "../../internal.ts";
import { redactedMake } from "../redacted.ts";
import { decimalNode } from "./decimal.ts";
import {
  type Gen, type Retained, type Sample, arraySample, compiledGen, concatPulls, filterSample, generateProduct,
  generateUnion, lazyShrinks, mapRetained, mapSample, replaceAt, reserving, retain, sampleOf, sumCosts,
} from "./model.ts";
import { type GenState, randomLength, shuffle, withSize } from "./random.ts";
import { compileRecord } from "./records.ts";
import { type NumberKind, bigintNode, booleanNode, numberNode, stringNode } from "./scalar.ts";

/** A check: what generation reads of it, and its test. */
export interface Filter { readonly number?: "finite" | "integer"; readonly test: (value: unknown) => boolean }

/** A node of the AST a schema has as the JS target's Effect schemas do, as far as Arbitrary reads it. */
export type Ast = { readonly checks: ReadonlyArray<Filter> } & (
  | { readonly tag: "never"; readonly what: string }
  | { readonly tag: "literal"; readonly literal: unknown }
  | { readonly tag: "boolean" | "string" | "number" | "bigint" | "decimal" }
  | { readonly tag: "arrays"; readonly elements: ReadonlyArray<Ast>; readonly rest: Ast | undefined }
  | { readonly tag: "objects"; readonly props: ReadonlyArray<Ast>; readonly names: ReadonlyArray<string>;
    readonly make: (values: Array<unknown>) => unknown }
  | { readonly tag: "union"; readonly members: ReadonlyArray<Ast> }
  | { readonly tag: "suspend"; readonly thunk: () => Ast }
  // a declaration: its parameters, through the union of tagged structs or the link it has
  | { readonly tag: "option" | "redacted"; readonly item: Ast }
  | { readonly tag: "result"; readonly value: Ast; readonly error: Ast }
  | { readonly tag: "link"; readonly target: Ast; readonly params: ReadonlyArray<Ast>; readonly decode: (value: unknown) => unknown }
  // an object of one index signature
  | { readonly tag: "record"; readonly key: Ast; readonly value: Ast }
);

export const withChecks = (ast: Ast, ...checks: ReadonlyArray<Filter>): Ast => ({ ...ast, checks: [...ast.checks, ...checks] });

/** A struct of `_tag` and fields named as a variant's, made by `make` of the fields' values. */
export const structNode = (tag: string, fields: ReadonlyArray<Ast>, make: (values: Array<unknown>) => unknown): Ast => ({
  tag: "objects", checks: [],
  props: [{ tag: "literal", checks: [], literal: tag }, ...fields],
  names: ["_tag", ...fields.map((_, i) => `_${i}`)],
  make: (values) => make(values.slice(1)),
});

type Key = string | number;
/** The error of a schema that cannot be generated, at the path of the node in it. */
const arbError = (what: string, path: ReadonlyArray<Key>): Error =>
  new Error(`Unable to derive an arbitrary for ${what}${path.length > 0
    ? `\n  at ${path.map((k) => `[${typeof k === "number" ? k : JSON.stringify(k)}]`).join("")}` : ""}`);

/** What a node being compiled cannot be generated for; the compiler adds the node's path. */
class Failure {
  readonly what: string;
  constructor(what: string) { this.what = what; }
}
// oxlint-disable-next-line typescript/only-throw-error -- caught by the node's compilation, which throws an Error at its path
const fail = (what: string): never => { throw new Failure(what); };

const at = (path: ReadonlyArray<Key>, key: Key): Array<Key> => [...path, key];
const infinite = (): number => Infinity;
const discardAll = (): null => null;

const mergeNumber = (self: NumberKind | undefined, that: "finite" | "integer"): NumberKind =>
  self === "integer" || that === "integer" ? "integer" : "finite";

/** The generator of the values of an AST. */
export const compileAst = (root: Ast): Gen => {
  const cache = new Map<Ast, Map<NumberKind | undefined, Gen>>();
  const nodes: Array<Gen> = [];
  const bodies = new Map<Gen, Gen>();
  const pending: Array<() => void> = [];

  const recur = (ast: Ast, path: ReadonlyArray<Key>, inherited: NumberKind | undefined): Gen => {
    let entries = cache.get(ast);
    const cached = entries?.get(inherited);
    if (cached !== undefined) return cached;
    if (entries === undefined) cache.set(ast, entries = new Map());
    const node = compiledGen([], infinite, discardAll);
    entries.set(inherited, node);
    nodes.push(node);
    pending.push(() => {
      try {
        let constraint = inherited;
        for (const check of ast.checks) if (check.number !== undefined) constraint = mergeNumber(constraint, check.number);
        const base = compileBase(ast, path, constraint);
        if (ast.tag === "suspend") {
          const body = item(base.deps, 0);
          node.deps = [body];
          node.computeMinCost = () => (body.minCost === Infinity ? Infinity : body.minCost + (node.recursive ? 1 : 0));
          node.generate = (st) => {
            if (node.recursive) {
              if (st.budget.remaining <= 0) return null;
              st.budget.remaining--;
            }
            return body.generate(st);
          };
          bodies.set(node, body);
        } else {
          node.deps = base.deps;
          node.computeMinCost = base.computeMinCost;
          node.generate = base.generate;
        }
        applyFilters(node, ast.checks);
      } catch (thrown) {
        if (thrown instanceof Failure) throw arbError(thrown.what, path);
        throw thrown;
      }
    });
    return node;
  };

  const compileBase = (ast: Ast, path: ReadonlyArray<Key>, constraint: NumberKind | undefined): Gen => {
    switch (ast.tag) {
      case "never":
        if (ast.what !== "") fail(ast.what);
        return compiledGen([], infinite, discardAll);
      case "literal": {
        const s = sampleOf(ast.literal);
        return compiledGen([], () => 0, () => s);
      }
      case "boolean": return booleanNode();
      case "string": return stringNode();
      case "number": return numberNode(constraint ?? "any");
      case "bigint": return bigintNode();
      case "decimal": return decimalNode();
      case "arrays": return compileArrays(ast.elements.map((e, i) => recur(e, at(path, i), undefined)),
        ast.rest === undefined ? undefined : recur(ast.rest, at(path, ast.elements.length), undefined));
      case "objects": {
        const props = ast.props.map((p, i) => recur(p, at(path, item(ast.names, i)), undefined));
        const make = ast.make;
        return compiledGen(props, () => sumCosts(props.map((p) => p.minCost)), (st) => {
          const samples = generateSamples(props, st, 0);
          if (samples === undefined) return null;
          return st.shrinks ? objectSample(samples, make) : sampleOf(make(samples.map((s) => s.value)));
        });
      }
      case "union": {
        const members = ast.members.map((m) => recur(m, path, constraint));
        if (members.length === 0) fail("a union with no members");
        return compiledGen(members, () => Math.min(...members.map((m) => m.minCost)), (st) => generateUnion(members, st));
      }
      case "suspend": {
        const body = recur(ast.thunk(), path, undefined);
        return compiledGen([body], () => body.minCost, (st) => body.generate(st));
      }
      case "record": return compileRecord(recur(ast.value, at(path, "index-0-value"), undefined),
        recur(ast.key, at(path, "index-0-key"), undefined));
      case "option":
      case "redacted":
      case "result":
      case "link": {
        // the parameters first, so the target's structs find them compiled at their paths
        const params = (ast.tag === "link" ? ast.params : ast.tag === "result" ? [ast.value, ast.error] : [ast.item])
          .map((p, i) => recur(p, at(path, i), undefined));
        let target: Ast;
        let wrap = (value: unknown): unknown => value;
        switch (ast.tag) {
          case "link":
            target = ast.target;
            wrap = ast.decode;
            break;
          case "option":
            target = { tag: "union", checks: [], members: [structNode("Some", [ast.item], (vs) => some(vs[0])),
              structNode("None", [], () => none)] };
            break;
          case "result":
            target = { tag: "union", checks: [], members: [structNode("Success", [ast.value], (vs) => ok(vs[0])),
              structNode("Failure", [ast.error], (vs) => err(vs[0]))] };
            break;
          case "redacted":
            target = ast.item;
            wrap = redactedMake;
            break;
        }
        const compiled = recur(target, path, undefined);
        return compiledGen([compiled, ...params], () => compiled.minCost, (st) => {
          const s = compiled.generate(st);
          return s === null ? null : mapSample(s, wrap);
        });
      }
    }
  };

  /** A list, `head` repeated, or a tuple of `elements`. */
  const compileArrays = (elements: ReadonlyArray<Gen>, head: Gen | undefined): Gen => {
    const required = elements.length;
    // the repeat counts possible within a size, with their costs: a tuple has only 0
    const combinations = (limit: number): Array<readonly [number, number]> => {
      const requiredCost = sumCosts(elements.map((e) => e.minCost));
      const upper = Math.max(required, limit);
      const maximumRepeat = head === undefined ? 0 : Math.max(0, upper - required);
      const out: Array<readonly [number, number]> = [];
      for (let repeat = 0; repeat <= maximumRepeat; repeat++)
        out.push([repeat, requiredCost + (repeat > 0 && head !== undefined ? repeat * head.minCost : 0)]);
      return out;
    };
    let cachedLimit = Number.NaN;
    let cachedBudget = Number.NaN;
    let cached: Array<readonly [number, number]> = [];
    let possible: Array<readonly [number, number]> = [];
    return compiledGen(head === undefined ? elements : [...elements, head],
      () => Math.min(...combinations(0).map(([, cost]) => cost)), (st) => {
        if (st.size !== cachedLimit) {
          cachedLimit = st.size;
          cachedBudget = Number.NaN;
          cached = combinations(st.size);
        }
        if (st.budget.remaining !== cachedBudget) {
          cachedBudget = st.budget.remaining;
          possible = cached.filter(([, cost]) => cost <= cachedBudget);
        }
        if (possible.length === 0) return null;
        const repeat = item(possible, randomLength(st, 0, possible.length - 1))[0];
        const itemState = st.size < repeat ? withSize(st, repeat) : st;
        if (!st.shrinks && required === 0 && head !== undefined) return generateRepeated(head, repeat, itemState);
        const selected = [...elements];
        if (head !== undefined) for (let i = 0; i < repeat; i++) selected.push(head);
        const samples = generateSamples(selected, itemState, 0);
        if (samples === undefined) return null;
        return arraySample(samples, { fixed: required, optional: 0, repeat, tail: 0, minimum: 0 }, st.shrinks);
      });
  };

  const rootNode = recur(root, [], undefined);
  for (let i = 0; i < pending.length; i++) item(pending, i)();
  const dependents = analyzeDependencies(nodes, bodies);
  const queue = nodes.filter((n) => n.recursive);
  for (let i = 0; i < queue.length; i++) {
    const n = item(queue, i);
    if (!n.mayRecurse) {
      n.mayRecurse = true;
      queue.push(...(dependents.get(n) ?? []));
    }
  }
  const settle = [...nodes];
  const queued = new Set(settle);
  for (let i = 0; i < settle.length; i++) {
    const n = item(settle, i);
    queued.delete(n);
    const next = n.computeMinCost();
    if (next >= n.minCost) continue;
    n.minCost = next;
    for (const d of dependents.get(n) ?? []) {
      if (!queued.has(d)) {
        queued.add(d);
        settle.push(d);
      }
    }
  }
  if (rootNode.minCost === Infinity) throw arbError("a schema without a finite generation path", []);
  return rootNode;
};

/** Checks as a filter of the node's values. */
const applyFilters = (node: Gen, checks: ReadonlyArray<Filter>): void => {
  if (checks.length === 0) return;
  const generate = node.generate;
  const passes = (value: unknown): boolean => checks.every((c) => c.test(value));
  node.generate = (st) => {
    const s = generate(st);
    if (s === null) return null;
    if (!st.shrinks) return passes(s.value) ? s : null;
    return filterSample(s, passes);
  };
};

/** The children, the ones that may recurse in a random order among themselves. */
const generateSamples = (children: ReadonlyArray<Gen>, st: GenState, reserved: number): Array<Sample> | undefined => {
  const recursive: Array<number> = [];
  for (const [i, child] of children.entries()) {
    if (reserved !== Infinity) reserved += child.minCost;
    if (child.mayRecurse) recursive.push(i);
  }
  if (reserved > st.budget.remaining) return undefined;
  let order: Array<number> | undefined;
  if (recursive.length > 1) {
    const shuffled = shuffle(st, recursive);
    let next = 0;
    order = children.map((child, i) => (child.mayRecurse ? item(shuffled, next++) : i));
  }
  return generateProduct(children, st, order, reserved);
};

/** A list of `count` values of `child`; a recursive one fills its places in a random order. */
const generateRepeated = (child: Gen, count: number, st: GenState): Sample | null => {
  const out = new Array<unknown>(count);
  const order = child.mayRecurse ? shuffle(st, [...out.keys()]) : undefined;
  let reserved = count * child.minCost;
  for (let i = 0; i < count; i++) {
    reserved -= child.minCost;
    const s = reserving(child, st, reserved);
    if (s === null) return null;
    out[order === undefined ? i : item(order, i)] = s.value;
  }
  return sampleOf(out);
};

/** Shrinks each property in turn: none of them can be removed. */
const objectSample = (children: ReadonlyArray<Sample>, make: (values: Array<unknown>) => unknown): Sample => {
  const rebuild = (entries: ReadonlyArray<Retained>, start: number): Sample => {
    const pulls = [];
    for (let i = start; i < entries.length; i++) {
      const shrinks = item(entries, i).shrinks;
      if (shrinks !== undefined) pulls.push(mapRetained(shrinks(), (r) => rebuild(replaceAt(entries, i, r), i)));
    }
    return sampleOf(make(entries.map((e) => e.value)), pulls.length > 0 ? concatPulls(pulls) : undefined);
  };
  return lazyShrinks(make(children.map((c) => c.value)), () => rebuild(children.map(retain), 0).shrinks);
};

/** Finds the strongly connected nodes, Kosaraju's way, marks the recursive suspensions and gives each node's dependents. */
const analyzeDependencies = (nodes: ReadonlyArray<Gen>, bodies: ReadonlyMap<Gen, Gen>): Map<Gen, Array<Gen>> => {
  const visited = new Set<Gen>();
  const order: Array<Gen> = [];
  const finished: Array<Gen> = [];
  for (const root of nodes) {
    if (visited.has(root)) continue;
    visited.add(root);
    order.push(root);
    const stack: Array<{ readonly node: Gen; index: number }> = [{ node: root, index: 0 }];
    while (stack.length > 0) {
      const frame = item(stack, stack.length - 1);
      if (frame.index < frame.node.deps.length) {
        const d = item(frame.node.deps, frame.index++);
        if (!visited.has(d)) {
          visited.add(d);
          order.push(d);
          stack.push({ node: d, index: 0 });
        }
      } else {
        finished.push(frame.node);
        stack.pop();
      }
    }
  }
  const reverse = new Map<Gen, Array<Gen>>(order.map((n) => [n, []]));
  for (const n of order) for (const d of n.deps) reverse.get(d)?.push(n);
  const component = new Map<Gen, number>();
  let id = 0;
  for (let i = finished.length - 1; i >= 0; i--) {
    const root = item(finished, i);
    if (component.has(root)) continue;
    component.set(root, id);
    const stack = [root];
    while (stack.length > 0) {
      const n = stack.pop();
      for (const d of (n === undefined ? undefined : reverse.get(n)) ?? []) {
        if (!component.has(d)) {
          component.set(d, id);
          stack.push(d);
        }
      }
    }
    id++;
  }
  for (const [s, body] of bodies) s.recursive = component.get(s) === component.get(body);
  return reverse;
};
