# Exercises

Each `<topic>/<number>/` contains `check.toml`, `starter.fl`, `solution.fl`,
and `broken.fl`. The starter is the original prompt. The broken answer must compile
and have the requested export, but produce the wrong output.

`check.toml` supplies the exported function name, its TypeScript
`argument_types` and `return_type`, expected trimmed stdout (`output`), and
English/Russian `title`, `task`, and `hint` strings (Russian keys end in `_ru`).
The current checker supports one or more curried primitive string, number, and boolean arguments,
and those result types plus void. Add support in the shared checker and loader
before using more complex signatures.

The site loads and validates TOML at build time. Its normalized data is embedded
in the lesson page; the browser never parses TOML or reads the filesystem.
`src/playground/exercise.ts` applies the same rules to browser-worker and native
CLI results. `test/lesson.test.ts` discovers all exercises automatically and
checks each starter, solution, and broken answer. The Docs workflow runs these
checks before publishing.

Mount an exercise with `<Playground locale="en" lesson="signatures/02" />`
(and a corresponding Russian page). State and actions live in Flick. Drafts
and completion use per-exercise local storage keys; existing signatures/01 keys
remain compatible. Showing a solution does not mark it complete.
