# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Developers evaluating the Flick language (repository visitors, potential
users, conference/tutorial audiences) browsing the RealWorld Conduit
example. Secondary: Flick's maintainers, who run it as a living regression
rig.

## Product Purpose

The Conduit client is the flagship showcase of Flick: a complete RealWorld
Medium clone whose whole frontend is Flick source. Its job is to make a
developer reading the repository believe "real apps get written in this
language" — the design must look like a shipped product, not a test fixture.
Success: a visitor lands on the home page and the page itself reads as
evidence of the language's maturity.

## Positioning

The only Medium-style demo app whose pages, router, data layer and forms
are all one typed language over one typed contract — no hand-written glue
JS in between.

## Operating Context

Run via `just conduit-run` or served by the Go server (`STATIC_DIR`), built
with Vite (`pnpm --dir client build`). Developed against the API on
`API_PORT`. Verified by the vendored RealWorld Playwright suite, which
finds elements by CSS class, `name` attributes, placeholders and visible
text — those selectors, the routes, and the copy they match are frozen.

## Capabilities and Constraints

- Frozen by the vendored e2e spec: CSS classes in markup, form `name`
  attributes, placeholders, button/link text, routes, `jwtToken` storage
  key, `window.__conduit_debug__`.
- `vendor/` stays unchanged; the client owns its own stylesheet instead of
  importing the vendored theme.
- No foreign JS in the client beyond the entry script; visual work lives in
  CSS and `index.html`.
- Article bodies are Markdown rendered through `Markdown.to_html` (escaped).

## Brand Commitments

- Chosen visual world (2026-11-03, owner pick): two-ink risograph zine —
  warm paper, one fluoro-orange ink, near-black indigo ink, poster
  grotesque display; loud but print-native, never glassmorphic.

## Product Principles

1. The page is evidence: production-grade finish, because the demo argues
   for the language.
2. The spec is law: never trade an e2e selector or spec text for looks.
3. Loud with two inks, not ten: one accent does the shouting; ink does the
   structure.

## Evidence on Hand

- Seeded demo content via the API (users alice/bob/clara/dave, ten
  articles with Markdown bodies) — synthetic, authored for the showcase.
- Before/after captures under `/tmp/before` this session; not committed.
