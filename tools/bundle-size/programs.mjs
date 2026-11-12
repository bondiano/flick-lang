// The programs `just bundle-size` tracks, apart from the tool so that
// tools/perf/ratchet.mjs reads the budgets of a base ref without rolldown.

/** Programs and their gzip budgets in KB; raising a budget takes a `reason` beside it. */
export const PROGRAMS = [
  { name: "pure", source: "tools/bundle-size/pure.fl", budget: 13 },
  // Identical source, effects and observable output on the native runtime.
  { name: "pure-rt", source: "tools/bundle-size/pure.fl", rt: true, budget: 5, effectFree: true },
  // Pure arithmetic library, with no host I/O or trusted pure foreigns.
  // UI programs render views on atoms: at most 81 KB and 18 KB for effect/reactivity,
  // and 5 KB for the spans of actions and the debug log of writes;
  // the registry keeps `Stream`, which `effect/reactivity` imports for stream atoms,
  // and since Effect 4.1 `Schema` (13 KB), whose `isSchemaError` hydration calls
  { name: "counter", source: "tests/run/ui/counter.fl", budget: 118, unused: ["effect/dist/Graph.js"] },
  // the same counter on Flick's own runtime: React and the kernel, no Effect at all
  { name: "counter-rt", source: "tests/run/ui/counter.fl", rt: true, budget: 80, effectFree: true },
  {
    name: "playground", source: "docs/src/playground/Playground.fl", budget: 129,
    reason: "The sandboxed preview, saved drafts and shared links, and the first lesson add browser interactions to the playground.",
    unused: ["effect/dist/Graph.js"],
  },
  // the same playground on Flick's own runtime: React and the kernel, no Effect at all
  { name: "playground-rt", source: "docs/src/playground/Playground.fl", rt: true, budget: 92, effectFree: true },
  { name: "atoms", source: "tests/run/ui/atoms.fl", budget: 117, unused: ["effect/dist/Graph.js"] },
  // the same atoms on Flick's own runtime, no Effect at all
  { name: "atoms-rt", source: "tests/run/ui/atoms.fl", rt: true, budget: 80, effectFree: true },
  // a browser client of a procedure keeps no Node server
  {
    name: "rpc",
    source: "tools/bundle-size/rpc.fl",
    budget: 56,
    unused: ["effect/dist/Graph.js", "platform-node/dist/NodeHttpServer.js", "platform-node-shared/dist/NodeSocketServer.js"],
  },
  // a template renders without `react-dom/server`; its view is still a React component,
  // its atoms the registry's, with `Schema` since Effect 4.1
  {
    name: "email",
    source: "tests/run/ui/email.fl",
    budget: 48,
    unused: ["effect/dist/Graph.js", "react-dom/cjs/react-dom-server.browser.production.js", "react-dom/cjs/react-dom-server-legacy.browser.production.js"],
  },
  // the same template on Flick's own runtime: no Effect, and its own atoms
  { name: "email-rt", source: "tests/run/ui/email.fl", rt: true, budget: 12, effectFree: true },
  // the client entry of a project with a server entry keeps no server module, and of a
  // server function only its call: not its body, what the body uses or the Node server
  {
    name: "fullstack",
    source: "tools/bundle-size/fullstack",
    entry: "web",
    // the call of a server function is an RPC client over fetch, as the `rpc` program's
    budget: 56,
    unused: [
      "effect/dist/Graph.js",
      "App/Db.js",
      "App/Api.js",
      "effect/server/FunctionsNode.js",
      "rpc/server.js",
      "platform-node/dist/NodeHttpServer.js",
    ],
  },
];

/** Chunks of the documentation site that load apart from its pages, by the name of their
 * group in docs/astro.config.mjs, and their gzip budgets in KB; docs/scripts/chunks.mjs
 * checks them in docs/dist. */
export const CHUNKS = [
  // the playground's editor: CodeMirror's state, view, commands, lint and autocompletion
  // with @codemirror/language, which commands and autocompletion import; 105.8 KB plus 3%
  { name: "codemirror", budget: 109 },
];
