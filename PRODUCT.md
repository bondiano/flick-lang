# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The documentation primarily serves TypeScript and Effect developers evaluating Flick, learning its syntax, and trying real programs. The owner confirmed this audience. English and Russian documentation are existing supported locales and remain supported.

## Product Purpose

Explain Flick accurately, teach it through usable documentation, and let visitors edit, compile, and run Flick in the browser without installation.

## Positioning

Flick is an ML-family language with higher-kinded types, type classes, typed effects and errors, and regions. It compiles to JavaScript/TypeScript on the Effect runtime. Concise language syntax is a reason to consider Flick; Effect is the runtime foundation, not an opponent to dismiss. Comparisons must show equivalent tasks and explicitly recognize Effect's TypeScript integration and ecosystem.

## Operating Context

The repository contains an Astro/Starlight documentation site, a Rust compiler exposed through WebAssembly, a React-backed Flick UI DSL, and an Astro/Vite integration for importing `.fl` modules. Readers move between learning material, task guides, API references, ecosystem tooling, runnable examples, and generated JavaScript.

## Capabilities and Constraints

- Preserve the existing English and Russian documentation and working reference URLs.
- Organize documentation into Learn, Guides, API, and Ecosystem.
- Provide working Play and Transpile surfaces using the real compiler.
- The owner requires interactive widgets to be implemented in Flick itself. JavaScript/TypeScript is limited to browser/compiler interoperability and framework mounting, not widget state or behavior.
- Use actual compiler/runtime capabilities and checked source material; do not invent adoption, performance benchmarks, roadmap completion, or package availability.
- Preserve error reporting and worker isolation when running visitor-authored programs.

## Brand Commitments

The product name is Flick. The owner requested a distinctive logo, beautiful documentation, and a gentle animated background. Their explicit visual references are Effector documentation, effect.website, and v1001.reatom.dev. Effect's Play and Flow's Try are interaction references for Play and Transpile. The owner chose visual mockups before implementation.

## Evidence on Hand

- `docs/src/content/docs/`: existing English and Russian tutorials and language/runtime references.
- `docs/src/playground/`: working Flick UI, WebAssembly compiler bridge, and isolated program runner.
- `tools/astro-flick/`: Flick-to-Astro/Vite integration.
- `runtime/`, `flick-core/`, `flick-wasm/`, `editors/`: actual implementation and tooling.
- No verified adoption figures, customer testimonials, or comparative performance measurements have been provided.

## Product Principles

- Demonstrate working language behavior instead of making unsupported claims.
- Teach from TypeScript/Effect familiarity while making the separate-language tradeoff explicit.
- Keep reading and wayfinding quiet; make experimentation immediate.
- Dogfood Flick for interactive documentation widgets.
