// Effect's `Graph` for the bindings of the standard library, and the adapters:
// Flick's graphs are persistent, so each change runs Effect's mutations on a
// copy (`Graph.mutate`); a `GraphError` Effect throws becomes the `Err` of a
// `Result`; options records with `Option` fields become Effect's optional
// fields; `Map` results become lists of entries.
import { Graph, Option, Result, Schema } from "effect";

import type { Tagged } from "../data.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "@flick-lang/runtime/tags";
import { fallible } from "./Schema.ts";

export * from "effect/Graph";

const GRAPH_ERROR = /*#__PURE__*/ tag("Graph.Types.GraphError.GraphError");

/** `Graph.GraphError`. */
export type FlickGraphError = Tagged<string, string>;

type K = Graph.Kind;
type G<N, E, T extends K> = Graph.Graph<N, E, T>;
type Directed<N, E> = Graph.Graph<N, E>;
type Undirected<N, E> = Graph.Graph<N, E, "undirected">;
type Out<A> = Result.Result<A, FlickGraphError>;

/** Runs `f`; a `GraphError` it throws is the `Err`, anything else stays a defect. */
const attempt = <A>(f: () => A): Out<A> => {
  try {
    return Result.succeed(f());
  } catch (error) {
    if (error instanceof Graph.GraphError) return Result.fail({ _tag: GRAPH_ERROR, _0: error.message });
    throw error;
  }
};

/** An optional field of Effect's options from a Flick `Option`. */
const opt = <P extends string, A>(key: P, o: Option.Option<A>): Partial<Record<P, A>> => {
  const out: Partial<Record<P, A>> = {};
  if (Option.isSome(o)) out[key] = o.value;
  return out;
};

/** A copy of `g` changed by Effect's mutations. */
const edit = <N, E, T extends K>(g: G<N, E, T>, f: (m: Graph.MutableGraph<N, E, T>) => void): G<N, E, T> =>
  Graph.mutate(g, (m) => {
    f(m);
    return undefined;
  });

const entries = <A, B>(m: ReadonlyMap<A, B>): Array<[A, B]> => Array.from(m);
const nested = <A, B, C>(m: ReadonlyMap<A, ReadonlyMap<B, C>>): Array<[A, Array<[B, C]>]> =>
  Array.from(m, ([k, inner]) => [k, entries(inner)]);

// Building

export const flickMake = <N, E, T extends K>(kind: T): G<N, E, T> => Graph.make(kind)<N, E>();

export interface FlickSnapshot<N, E, T extends K> {
  readonly graph_type: T;
  readonly nodes: ReadonlyArray<Graph.IndexedNode<N>>;
  readonly edges: ReadonlyArray<Graph.IndexedEdge<E>>;
}

export const flickFromSnapshot = <N, E, T extends K>(s: FlickSnapshot<N, E, T>): Out<G<N, E, T>> =>
  attempt(() => Graph.fromSnapshot({ type: s.graph_type, nodes: s.nodes, edges: s.edges }));

export const flickToSnapshot = <N, E, T extends K>(g: G<N, E, T>): FlickSnapshot<N, E, T> => {
  const s = Graph.toSnapshot(g);
  return { graph_type: s.type, nodes: s.nodes, edges: s.edges };
};

export const flickAddNode = <N, E, T extends K>(data: N, g: G<N, E, T>): [number, G<N, E, T>] => {
  let index = -1;
  const next = edit(g, (m) => {
    index = Graph.addNode(m, data);
  });
  return [index, next];
};

export const flickAddEdge = <N, E, T extends K>(source: number, target: number, data: E, g: G<N, E, T>): Out<[number, G<N, E, T>]> =>
  attempt(() => {
    let index = -1;
    const next = edit(g, (m) => {
      index = Graph.addEdge(m, source, target, data);
    });
    return [index, next];
  });

export const flickRemoveNode = <N, E, T extends K>(i: number, g: G<N, E, T>): G<N, E, T> => edit(g, (m) => Graph.removeNode(m, i));
export const flickRemoveNodes = <N, E, T extends K>(is: ReadonlyArray<number>, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.removeNodes(m, is));
export const flickRemoveEdge = <N, E, T extends K>(i: number, g: G<N, E, T>): G<N, E, T> => edit(g, (m) => Graph.removeEdge(m, i));
export const flickRemoveEdges = <N, E, T extends K>(is: ReadonlyArray<number>, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.removeEdges(m, is));
export const flickUpdateNode = <N, E, T extends K>(i: number, f: (n: N) => N, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.updateNode(m, i, f));
export const flickUpdateEdge = <N, E, T extends K>(i: number, f: (e: E) => E, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.updateEdge(m, i, f));
export const flickMapNodes = <N, E, T extends K>(f: (n: N) => N, g: G<N, E, T>): G<N, E, T> => edit(g, (m) => Graph.mapNodes(m, f));
export const flickMapEdges = <N, E, T extends K>(f: (e: E) => E, g: G<N, E, T>): G<N, E, T> => edit(g, (m) => Graph.mapEdges(m, f));
export const flickFilterMapNodes = <N, E, T extends K>(f: (n: N) => Option.Option<N>, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.filterMapNodes(m, f));
export const flickFilterMapEdges = <N, E, T extends K>(f: (e: E) => Option.Option<E>, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.filterMapEdges(m, f));
export const flickFilterNodes = <N, E, T extends K>(f: (n: N) => boolean, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.filterNodes(m, f));
export const flickFilterEdges = <N, E, T extends K>(f: (e: E) => boolean, g: G<N, E, T>): G<N, E, T> =>
  edit(g, (m) => Graph.filterEdges(m, f));
export const flickReverse = <N, E, T extends K>(g: G<N, E, T>): G<N, E, T> => edit(g, (m) => Graph.reverse(m));

// Combining

export interface FlickIdentity<N, E, NI, EI> {
  readonly nodeIdentity: Option.Option<(n: N) => NI>;
  readonly edgeIdentity: Option.Option<(e: E) => EI>;
}

const identity = <N, E, NI, EI>(o: FlickIdentity<N, E, NI, EI>): Graph.IdentityOptions<N, E, NI, EI> => ({
  ...opt("nodeIdentity", o.nodeIdentity),
  ...opt("edgeIdentity", o.edgeIdentity),
});

type Combine = <N, E, T extends K, NI, EI>(self: G<N, E, T>, that: G<N, E, T>, o: Graph.IdentityOptions<N, E, NI, EI>) => G<N, E, T>;
const combine =
  (f: Combine) =>
  <N, E, T extends K, NI, EI>(o: FlickIdentity<N, E, NI, EI>, that: G<N, E, T>, self: G<N, E, T>): G<N, E, T> =>
    f(self, that, identity(o));

export const flickCompose = /*#__PURE__*/ combine((self, that, o) => Graph.compose(self, that, o));
export const flickIntersection = /*#__PURE__*/ combine((self, that, o) => Graph.intersection(self, that, o));
export const flickDifference = /*#__PURE__*/ combine((self, that, o) => Graph.difference(self, that, o));
export const flickSymmetricDifference = /*#__PURE__*/ combine((self, that, o) => Graph.symmetricDifference(self, that, o));

export interface FlickNeighborhood {
  readonly radius: Option.Option<number>;
  readonly direction: Option.Option<Graph.TraversalDirection>;
}

export const flickNeighborhood = <N, E, T extends K>(i: number, o: FlickNeighborhood, g: G<N, E, T>): Out<G<N, E, T>> =>
  attempt(() => Graph.neighborhood(g, i, { ...opt("radius", o.radius), ...opt("direction", o.direction) }));
export const flickInducedSubgraph = <N, E, T extends K>(is: ReadonlyArray<number>, g: G<N, E, T>): Out<G<N, E, T>> =>
  attempt(() => Graph.inducedSubgraph(g, is));

// Edges and degrees of a node

export const flickIncidentEdges = <N, E, T extends K>(i: number, g: G<N, E, T>): Out<Array<number>> =>
  attempt(() => Graph.incidentEdges(g, i));
export const flickOutgoingEdges = <N, E>(i: number, g: Directed<N, E>): Out<Array<number>> => attempt(() => Graph.outgoingEdges(g, i));
export const flickIncomingEdges = <N, E>(i: number, g: Directed<N, E>): Out<Array<number>> => attempt(() => Graph.incomingEdges(g, i));
export const flickEdgesBetween = <N, E, T extends K>(source: number, target: number, g: G<N, E, T>): Out<Array<number>> =>
  attempt(() => Graph.edgesBetween(g, source, target));
export const flickDegree = <N, E>(i: number, g: Undirected<N, E>): Out<number> => attempt(() => Graph.degree(g, i));
export const flickOutDegree = <N, E>(i: number, g: Directed<N, E>): Out<number> => attempt(() => Graph.outDegree(g, i));
export const flickInDegree = <N, E>(i: number, g: Directed<N, E>): Out<number> => attempt(() => Graph.inDegree(g, i));

// Rendering

export interface FlickGraphViz<N, E> {
  readonly nodeLabel: Option.Option<(n: N) => string>;
  readonly edgeLabel: Option.Option<(e: E) => string>;
  readonly graphName: Option.Option<string>;
}

export const flickToGraphViz = <N, E, T extends K>(o: FlickGraphViz<N, E>, g: G<N, E, T>): string =>
  Graph.toGraphViz(g, { ...opt("nodeLabel", o.nodeLabel), ...opt("edgeLabel", o.edgeLabel), ...opt("graphName", o.graphName) });

export interface FlickMermaid<N, E> {
  readonly nodeLabel: Option.Option<(n: N) => string>;
  readonly edgeLabel: Option.Option<(e: E) => string>;
  readonly diagramType: Option.Option<Graph.MermaidDiagramType>;
  readonly direction: Option.Option<Graph.MermaidDirection>;
  readonly nodeShape: Option.Option<(n: N) => Graph.MermaidNodeShape>;
}

export const flickToMermaid = <N, E, T extends K>(o: FlickMermaid<N, E>, g: G<N, E, T>): string =>
  Graph.toMermaid(g, {
    ...opt("nodeLabel", o.nodeLabel),
    ...opt("edgeLabel", o.edgeLabel),
    ...opt("diagramType", o.diagramType),
    ...opt("direction", o.direction),
    ...opt("nodeShape", o.nodeShape),
  });

// Analysis

export const flickMaximumBipartiteMatching = <N, E>(g: Undirected<N, E>): Out<Array<Graph.BipartiteMatch>> =>
  attempt(() => Graph.maximumBipartiteMatching(g));

export interface FlickReachability {
  readonly direction: Option.Option<Graph.TraversalDirection>;
}

export const flickUnweightedDistances = <N, E, T extends K>(
  source: number,
  o: FlickReachability,
  g: G<N, E, T>,
): Out<Array<[number, number]>> => attempt(() => entries(Graph.unweightedDistances(g, source, opt("direction", o.direction))));
export const flickHasPath = <N, E, T extends K>(source: number, target: number, o: FlickReachability, g: G<N, E, T>): Out<boolean> =>
  attempt(() => Graph.hasPath(g, source, target, opt("direction", o.direction)));

export const flickMaximumFlow = <N, E>(
  config: Graph.MaximumFlowConfig<E>,
  g: Directed<N, E>,
): Out<{ value: number; flows: Array<[number, number]>; cut: Array<number> }> =>
  attempt(() => {
    const r = Graph.maximumFlow(g, config);
    return { value: r.value, flows: entries(r.flows), cut: r.cut };
  });
export const flickMinimumCut = <N, E>(config: Graph.MaximumFlowConfig<E>, g: Directed<N, E>): Out<Graph.MinimumCutResult> =>
  attempt(() => Graph.minimumCut(g, config));
export const flickMinimumSpanningForest = <N, E>(cost: (e: E) => number, g: Undirected<N, E>): Out<Undirected<N, E>> =>
  attempt(() => Graph.minimumSpanningForest(g, cost));
export const flickTransitiveReduction = <N, E>(g: Directed<N, E>): Out<Directed<N, E>> => attempt(() => Graph.transitiveReduction(g));

// Paths

export const flickDijkstra = <N, E, T extends K>(config: Graph.DijkstraConfig<E>, g: G<N, E, T>): Out<Option.Option<Graph.PathResult<E>>> =>
  attempt(() => Graph.dijkstra(g, config));
export const flickBellmanFord = <N, E, T extends K>(
  config: Graph.BellmanFordConfig<E>,
  g: G<N, E, T>,
): Out<Option.Option<Graph.PathResult<E>>> => attempt(() => Graph.bellmanFord(g, config));
/** `Graph.AstarConfig`: a function in a record stays curried, as Flick compiles it. */
export interface FlickAstarConfig<E, N> {
  readonly source: number;
  readonly target: number;
  readonly cost: (e: E) => number;
  readonly heuristic: (source: N) => (target: N) => number;
}

export const flickAstar = <N, E, T extends K>(o: FlickAstarConfig<E, N>, g: G<N, E, T>): Out<Option.Option<Graph.PathResult<E>>> =>
  attempt(() =>
    Graph.astar(g, { source: o.source, target: o.target, cost: o.cost, heuristic: (a: N, b: N) => o.heuristic(a)(b) }),
  );
export const flickFloydWarshall = <N, E, T extends K>(cost: (e: E) => number, g: G<N, E, T>) =>
  attempt(() => {
    const r = Graph.floydWarshall(g, cost);
    return {
      distances: nested(r.distances),
      paths: Array.from(r.paths, ([k, inner]) => [
        k,
        Array.from(inner, ([t, path]) => [t, path === null ? Option.none() : Option.some(path)] as const),
      ] as const),
      edges: nested(r.edges),
      costs: nested(r.costs),
    };
  });

export interface FlickSimplePaths {
  readonly source: number;
  readonly target: number;
  readonly limit: Option.Option<number>;
}

export const flickSimplePaths = <N, E, T extends K>(o: FlickSimplePaths, g: G<N, E, T>): Out<Array<Graph.PathResult<E>>> =>
  attempt(() => Array.from(Graph.simplePaths<N, E, T>(g, { source: o.source, target: o.target, ...opt("limit", o.limit) })));

export interface FlickAllShortestPaths<E> {
  readonly source: number;
  readonly target: number;
  readonly cost: (e: E) => number;
  readonly limit: Option.Option<number>;
}

export const flickAllShortestPaths = <N, E, T extends K>(o: FlickAllShortestPaths<E>, g: G<N, E, T>): Out<Array<Graph.PathResult<E>>> =>
  attempt(() => Array.from(Graph.allShortestPaths(g, { source: o.source, target: o.target, cost: o.cost, ...opt("limit", o.limit) })));

// Walkers

export interface FlickSearch {
  readonly start: Option.Option<Array<number>>;
  readonly direction: Option.Option<Graph.TraversalDirection>;
  readonly radius: Option.Option<number>;
}

const search = (o: FlickSearch): Graph.SearchConfig => ({
  ...opt("start", o.start),
  ...opt("direction", o.direction),
  ...opt("radius", o.radius),
});

/** Walks `walker` now, so a `GraphError` its iteration throws is the `Err`. */
const eager = <N>(walk: () => Graph.NodeWalker<N>): Out<Graph.NodeWalker<N>> =>
  attempt(() => {
    const visits = Array.from(walk());
    return new Graph.Walker(<U>(f: (index: number, data: N) => U) => visits.map(([i, n]) => f(i, n)));
  });

export const flickDfs = <N, E, T extends K>(o: FlickSearch, g: G<N, E, T>): Out<Graph.NodeWalker<N>> =>
  eager(() => Graph.dfs(g, search(o)));
export const flickBfs = <N, E, T extends K>(o: FlickSearch, g: G<N, E, T>): Out<Graph.NodeWalker<N>> =>
  eager(() => Graph.bfs(g, search(o)));
export const flickDfsPostOrder = <N, E, T extends K>(o: FlickSearch, g: G<N, E, T>): Out<Graph.NodeWalker<N>> =>
  eager(() => Graph.dfsPostOrder(g, search(o)));
export const flickTopo = <N, E>(o: { readonly initials: Option.Option<Array<number>> }, g: Directed<N, E>): Out<Graph.NodeWalker<N>> =>
  eager(() => Graph.topo(g, opt("initials", o.initials)));
export const flickExternals = <N, E, T extends K>(o: { readonly direction: Option.Option<Graph.Direction> }, g: G<N, E, T>): Graph.NodeWalker<N> =>
  Graph.externals(g, opt("direction", o.direction));

export const flickIndices = <T, N>(w: Graph.Walker<T, N>): Array<T> => Array.from(Graph.indices(w));
export const flickValues = <T, N>(w: Graph.Walker<T, N>): Array<N> => Array.from(Graph.values(w));
export const flickEntries = <T, N>(w: Graph.Walker<T, N>): Array<[T, N]> => Array.from(Graph.entries(w));

/** `Graph.json kind node edge`: Effect's JSON form of a graph, its snapshot. */
export const flickJson = (kind: K, node: AnySchema, edge: AnySchema): AnySchema =>
  fallible(Schema.toCodecJson(Schema.Graph(kind, node, edge)));
