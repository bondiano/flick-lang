# Tour programs and native checks

`index.json` owns chapter and lesson order. A lesson uses its chapter's
`<id>.fl`, a gallery `example`, or an `exercise`. `expect` is `run`, `ui`,
`fail FLICK-…`, or `local`. A runnable lesson has a matching `.stdout`;
both native profiles and the browser compare that output.

The browser runs `main`, not a module's generated `tests`. `local` lessons
show static code and say that the browser did not run it. A runnable lesson
can also have a `local` check, as the proof and app-testing lessons do.
The browser test-runner follow-up is [#904](https://github.com/bondiano/flick-lang/issues/904).

`local` describes the real CLI entry point checked by
`test/helpers/verify-local-lesson.ts` in a private temporary project:

| Field | Check |
| - | - |
| `command` | `test`, `prove`, `check`, or `run`; sources go in `test/`, `proofs/`, or `src/` respectively |
| `exit` | exact CLI exit status, including intentionally failing lessons |
| `contains` | strings that must appear in stdout, such as case counts or a setup failure |
| `flags` | extra CLI arguments |
| `targets` | JS by default; `["js", "go"]` exercises both native targets |
| `adapter` | declare the optional Effect adapter for JS host APIs; use the installed workspace package |
| `source` | override the source root, for a companion beside `src/Main.fl` |
| `files` | additional files, keyed by destination in the temporary project; values are paths relative to `docs/` |
| `listed` | exact lines from `flick test --list` |
| `printed` | exact ordered stdout lines beginning with `hook:` |
| `forbidOnly` | also check that `flick test --forbid-only` rejects one intentional `.only` case |

Native test checks use Flick's runtime for JS. Check-only lessons reject
warnings too; proof checks must discover a proof root. No lesson depends on
an external network service or database. The HTTP API example uses a local
loopback server that its scope owns; SQL prints a bound query without
executing it. Server-function clients are type-checked, not called in CI.
