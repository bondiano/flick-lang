---
version: 1
slug: "docs-src-content-docs-index-mdx"
primary_target: "docs/src/pages/index.astro"
related_targets: ["docs/src/pages/ru/index.astro","docs/src/content/docs","docs/src/components","docs/src/playground","docs/src/styles","docs/src/widgets"]
---

# Flick documentation

Scope: docs/src. Homepage / and /ru/ are Persuade; Learn, Guides, API, Ecosystem and existing reference pages are Read; Play and Transpile are Operate.

Audience: TypeScript/Effect developers evaluating or learning Flick. Confirmed by owner. Primary action: Start learning; secondary: Open playground. Proof: working Flick code, real compiler output, balanced comparison on the shared Effect foundation. Every custom interactive widget is authored in Flick, with TypeScript restricted to FFI/mounting.

Direction: owner selected Signal — first mockup, .impeccable/mocks/signal.png. This explicit competitor-grounded brief overrides seed b4336790's assignment. References: Effector wayfinding, Effect code-first clarity, Reatom gentle background motion. A quiet cool-white split homepage; precise geometric F with spark; large two-line black/violet headline; live code at right; dense, quiet three-column docs inside. Do not reproduce generated code spelling mistakes or invented runtime claims from the mockup.

Approved comp: .impeccable/mocks/signal.png; approved sidecar .impeccable/mocks/signal.json. Structured question answer: Signal — первый макет. Physical scene: a developer reading and experimenting in a daytime editor/browser; cool light surfaces preserve code contrast, with an accessible optional dark appearance.

Sampled comp fields: ground and code #fafafc (700,700 and 1200,450), headline ink #101016 (520,240), violet headline #584dd0 (400,336). Thin cool-gray borders; 12px code corners; restrained no-shadow editor; display roughly 76px/1.05 at 1536 desktop; body 18px/1.6; mono code 15–17px. Desktop split around 47/49%; stacked mobile without compressing code. Active tabs are ink/violet with a thin underline; controls are compact slightly rounded rectangles; only callouts use a soft field.

| Visible role | Implementation medium | Commitment |
| --- | --- | --- |
| Logo + spark | authored SVG | crisp geometric black F, violet spark; no raster wordmark |
| Header | semantic links + Flick controls | server-rendered controls with stable hydration; full navigation, search, locale, Play and mobile menu; no motion toggle |
| Headline + actions | HTML/CSS | two-line statement left, violet second line, adjacent learning/play actions |
| Primary action | semantic link / CSS | solid violet, white text, quiet 6px corners, no gradient |
| Live editor | Flick ui + generated highlighted code | substantial right panel, Flick/Effect tabs, filename, output/run status |
| Dot wave | generated SVG geometry / CSS | 6,820 pale points over the right half of the hero, flowing interference field; continuous 24s group drift on desktop/mobile, disabled by system reduced motion |
| Feature row | semantic HTML | divider, heading, three unboxed useful feature columns |
| Comparison | real equivalent source + prose | no numeric marketing; clearly credit Effect strengths and separate-language cost |
| Docs shell | Starlight / CSS + Flick chrome | quiet sidebar, 68–74ch readable article, right contents; preserve locale and reference URLs |
| Play / Transpile | shared Flick widget | real initial view in server HTML, editable during compiler loading; safe source/JS token spans, compiler worker, isolated program runner, diagnostics, both locales |

Unresolved product decisions: none blocking implementation. No photography or other raster-native artwork in approved composition; generated mockups are review evidence only, never shipped as UI.
