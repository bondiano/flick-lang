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
checks each starter, solution, and broken answer. The Docs workflow runs these
checks before publishing.

A page of the tour mounts an exercise through `tour/index.json`, with
`{ "id": "<page>", "exercise": "signatures/02" }`; other pages can use
`<Playground locale="en" lesson="signatures/02" />` (and a corresponding
Russian page). State and actions live in Flick. Drafts
and completion use per-exercise local storage keys; existing signatures/01 keys
remain compatible. Showing a solution does not mark it complete.
