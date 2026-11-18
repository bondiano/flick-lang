# Exercises

Each `<topic>/<number>/` contains `check.toml`, `starter.fl`, `solution.fl`,
and `broken.fl`. The starter is the original prompt. The broken answer must compile
and have the requested export, but produce the wrong output.

`check.toml` supplies the exported function name, its TypeScript
`argument_types` and `return_type`, expected trimmed stdout (`output`), and
English/Russian `title`, `task`, and hints (Russian keys end in `_ru`): one
`hint`, or `hints`, a list shown one at a time, empty for an exercise to solve
alone. `question`, `choices` and `answer` (the index of the right choice) add
a question the learner answers before the code is checked.
`starter_error` names the one error code the starter reports (`FLICK-T003`
when absent); CI checks it, while the browser only asks to fix the errors.
`runs = true` asks for an export that runs (`->{}`): its declared result is
an Effect of the `return_type` that needs and fails with nothing.
The current checker supports one or more curried primitive string, number, and boolean arguments,
and those result types plus void. Add support in the shared checker and loader
before using more complex signatures.

The site loads and validates TOML at build time. Its normalized data is embedded
in the lesson page; the browser never parses TOML or reads the filesystem.
`src/playground/exercise.ts` applies the same rules to browser-worker and native
CLI results. `test/lesson.test.ts` discovers all exercises automatically and
checks each starter, solution, and broken answer.
The running `effects/01` exercise is also tested with both emitted runtime
forms and in the browser: errors, wrong output, then a correct answer. The Docs workflow runs these
checks before publishing.

A page of the tour mounts an exercise through `tour/index.json`, with
`{ "id": "<page>", "exercise": "signatures/02" }`; other pages can use
`<Playground locale="en" lesson="signatures/02" />` (and a corresponding
Russian page). State and actions live in Flick. Drafts
and completion use per-exercise local storage keys; existing signatures/01 keys
remain compatible. Showing a solution does not mark it complete.

The goal beside the editor comes from the same `check.toml` fields: the
export with its type as the `.d.ts` declares it (an Effect for `runs`), and
the expected output. It shows nothing the check does not compare. When the
checker reports a `todo`, the goal says what it is: in an expression, a
placeholder of the required type (warning T022) that fails when it runs; in a
signature, error T024, whose fix writes in the inferred type. Neither is an
expression hole that shows its inferred type: the goal does not infer holes.


## Limits of the rules

The evaluator checks the emitted export signature and runs the program's entrypoint,
then compares stdout. It does not invoke an arbitrary exported API with secret
arguments: a compilable wrong-output fixture is a rejection regression, not a
hidden test. Hidden tests, a ban on `todo`, and complex argument/result types
must be added to the shared loader/evaluator with browser and CI regressions
when a course actually needs them. Browser error checking deliberately uses
severity only; `starter_error` is the exact expected diagnostic code in CI.
