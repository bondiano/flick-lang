# Vendored RealWorld spec

From [realworld-apps/realworld](https://github.com/realworld-apps/realworld) at commit
`25588846c314b15a5e427ffe816e6b4af7be68c7` (2026-11-02), MIT (see `LICENSE`).

- `specs/api`: `openapi.yml`, the Hurl suite (`hurl/*.hurl`) and its runner
  `run-api-tests-hurl.sh` (`HOST` defaults to `http://localhost:8000`).
- `specs/e2e`: the Playwright suite for frontends (`SELECTORS.md` lists the selectors it needs).
- `assets/theme/styles.css`: the shared Conduit theme.

The generated Bruno collection is left out. Files are copied unchanged; update by copying a
newer commit and changing the hash above.
