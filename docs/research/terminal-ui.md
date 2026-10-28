# Rendering atoms to a terminal

Researched 2026-10-28 at `af16aa3`. Question: should a CLI written in Flick be able to render a live view over atoms (a progress board, a list you can select from) in the terminal? Should it use Ink through `Ui.mount_with`, or a renderer of our own?

**Citation keys**
- `fc:` = `flick-core/src/`, `rt:` = `runtime/src/`
- `M:` = a measurement made for this note: `ink@8.0.0` (published 2026-10-03) and `react@19.3.1`, installed outside the repository, bundled with the repository's rolldown for `platform: "node"`, minified, `NODE_ENV=production`. The scripts are in the session scratchpad and are not committed.

## 1. What already exists

- `Ui.mount_with : Renderer -> (Failure none ->{r; none} Unit)^c -> Ui^c ->{Atom.Registry, Atom.Write | r; none} Never` (`fc:types/builtins/Ui.fl:236-243`). It shares the registry, events, supervision and cleanup with `Ui.Dom.mount`.
- `Renderer = () => RenderRoot`, where `RenderRoot` is `{ render(node); unmount() }` (`rt:ui/react/index.ts:33-39`).
- Limit: the DOM helpers of `Ui` (`text`, `div`, `button`, …) build DOM elements. A non-DOM host needs its own components, brought in through `Component p` and `foreign` (doc comment on `mount_with`).
- Prompts without a live view are already covered by `Cli.Prompt`.

## 2. Ink through `mount_with`

The whole adapter (M: tested; a demo counter rerenders three times and unmounts):

```js
import { render } from "ink";
export const ink = () => {
  let app;
  return {
    render: (node) => (app ? app.rerender(node) : (app = render(node))),
    unmount: () => app?.unmount(),
  };
};
```

On the Flick side, `foreign ink : Ui.Renderer` (**[unverified]**: the JavaScript adapter was run, but a Flick program binding it was not compiled for this note), plus `foreign` bindings for Ink's `Text` and `Box` as `Component p`. `Ui.text` cannot be used: Ink rejects a bare string outside `<Text>`.

## 3. Weight (M)

| Bundle | Minified | gzip |
|---|---|---|
| The adapter with Ink and everything it imports | 395 KB | 135 KB |
| of which `yoga-layout` (layout engine, wasm inlined as base64) | 127 KB | 52 KB |
| of which `react-reconciler` | 141 KB | 43 KB |
| `react` alone | 1.8 KB | 0.9 KB |

- Installed: Ink's dependency tree, React included, is 8.3 MB on disk (41 packages).
- Loading: `import("ink")` takes 136 ms on a cold start.

A CLI runs in Node and is not bundled for a browser, so gzip size matters less than install size and start time. Still, for comparison: the largest tracked browser program, the playground, has a budget of 112 KB gzip for everything.

## 4. Our own renderer

A renderer of our own still needs a React reconciler (43 KB gzip) to keep `Ui`'s React semantics: components, keys, hooks behind `Ui.reactive`. Without one, it means a second implementation of `Ui` with no React. It would save yoga's 52 KB at most, at the cost of writing layout, line diffing and input handling. Nothing measured here makes that worth it.

## 5. Decision

- `@flick-lang/runtime` does not depend on Ink and gets no terminal renderer. Adding Ink would add 8.3 MB and 136 ms to every program that uses the runtime, while most CLIs need only `Cli.Prompt`.
- A terminal view is a user or third-party package: the 9-line adapter above, `foreign` components for `Text`/`Box`, and `Ui.mount_with`. That works today without compiler changes.
- Revisit when `Ui` has host-neutral primitives instead of DOM helpers. Then one view could render to both DOM and terminal, and an adapter package in this repository would pay off.
