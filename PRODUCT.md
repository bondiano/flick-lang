# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

People who build applications in TypeScript, Go or an ML-family language and want signatures they can trust. They arrive to evaluate Flick, learn it from a tour, a book or a course, and try real programs without installing anything. TypeScript and Effect developers are one way in, not the only audience. English and Russian are both supported, with strict parity on hand-written pages.

## Product Purpose

Say in one line why Flick exists, teach it through a book, a tour and a course, and let visitors edit, compile and run Flick in the browser.

## Positioning

- **Headline:** Functions that do what they say.
- **One sentence:** Flick is a language for whole apps, where every function does what it says.
- **Support:** A language for whole apps: browser, server and CLI. The signature says what a function needs and how it can fail, the compiler holds the body to it, and the code still reads like plain code.
- **Further sections, in this order:** one app and one language with checked seams; code you did not write; what is proved in Lean; the host's libraries.

Every claim has a row in `docs/research/claims.md` with its evidence and its limit. Today's main limit: host calls such as `Console.log`, the clock and randomness have an empty row, so signatures do not name them. Say so beside the claim until that changes.

Comparisons show equivalent tasks and recognize what the other tool does well. On JavaScript, effects run on Effect for now; say that as a status, not as the reason to use Flick.

## Voice

Direct, in short sentences, speaking to "you", with a little dry humour and no hype. Compiler errors are part of the story. Name a limit next to the promise it limits.

## Operating Context

The repository contains an Astro/Starlight documentation site, a Rust compiler exposed through WebAssembly, a React-backed Flick UI DSL, and an Astro/Vite integration for importing `.fl` modules. The local roadmap for this site is `docs/research/ROADMAP-docs.md`.

## Capabilities and Constraints

- Top-level structure: Tour, Book, Course, Docs, Playground.
- One Playground replaces Play and Transpile, using the real compiler.
- Interactive widgets are implemented in Flick itself. JavaScript/TypeScript is limited to browser/compiler interoperability and framework mounting, not widget state or behavior.
- Use actual compiler/runtime capabilities and checked source material; do not invent adoption, performance benchmarks, roadmap completion, or package availability.
- Preserve error reporting and worker isolation when running visitor-authored programs. A shared link never runs code by itself.

## Brand Commitments

The product name is Flick. The identity calls for a distinctive logo and beautiful, more interesting documentation. Structural references are the book and playground of Silk, the tour of Gleam, the course of Lean Academy, and the Start and History sections of Reatom. Take structure from them, never their look or text.

The identity favors distinctive visual wordplay like the logos at [Tonsky](https://tonsky.me/design/) and [Evil Martians](https://evilmartians.com/opensource), not a childish cartoon mascot. The identity is the brand «Развилка» (Fork), version 1.0, which replaced the woodpecker on 2026-11-07: a rounded square cut by two transparent channels, one running straight and one leaving at 45°, with a violet wedge between them, and a lowercase `flick` in outlined Overpass 800. Violet is the brand's accent there and never means an error. The package with the master files and the rules is local, in `docs/research/brand/flick-brand-v1/`; `DESIGN.md` records how the site applies it.

Two things would make the site wrong however polished it looks: reading as academic and cold, and being hard to read at length. The book and the reference come first for comfort; the landing page carries the spectacle.

## Evidence on Hand

- `docs/research/claims.md`: every public claim, its evidence and its limit.
- `docs/src/playground/`: working Flick UI, WebAssembly compiler bridge, and isolated program runner.
- `tools/astro-flick/`: Flick-to-Astro/Vite integration.
- `runtime/`, `flick-core/`, `flick-wasm/`, `editors/`: actual implementation and tooling.
- `formal/`, `docs/formal/`: Lean proofs of the core and their prose.
- No verified adoption figures, customer testimonials, or comparative performance measurements have been provided.

## Product Principles

- Demonstrate working language behavior instead of making unsupported claims.
- Every promise has evidence and a stated limit.
- Teach from a program and its compiler errors, not from a list of features.
- Keep reading and wayfinding quiet; make experimentation immediate.
- Dogfood Flick for interactive documentation widgets.

## Zen

1. The signature is the contract. (Limit: host calls are not named yet.)
2. Effects read like code.
3. A small core; the rest is packages. (A direction, not yet a fact.)
4. Use the host, don't rewrite it.
5. Prove it, or say it isn't proved.
