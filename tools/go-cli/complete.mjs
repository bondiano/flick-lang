// Writes runtime-go/flickrt/testdata/complete_{bash,zsh,powershell}.txt: the
// completion scripts Effect's generators give for the descriptor that
// completeTestCommand (cli_complete_test.go) mirrors. Run from the
// repository root after moving the Effect pin: `node tools/go-cli/complete.mjs`.
import { writeFileSync } from "node:fs";

const internal = new URL("../../packages/effect-ts/node_modules/effect/dist/cli/internal/completions/", import.meta.url);
// the generators are internal, out of reach of the package's exports
const load = (name) => import(new URL(`${name}.js`, internal).href);
const [bash, zsh, powershell] = await Promise.all([load("bash"), load("zsh"), load("powershell")]);

const flag = (name, type, more = {}) => ({ name, aliases: [], description: undefined, type, ...more });
const arg = (name, type, more = {}) => ({ name, description: undefined, required: false, variadic: false, type, ...more });
const deploy = {
  name: "deploy",
  description: "Deploy it",
  flags: [flag("env", { _tag: "Choice", values: ["prod", `it's "qa"`, "a b"] })],
  arguments: [arg("target", { _tag: "Path", pathType: "directory" }, { required: true })],
  subcommands: [],
};
const remote = { name: "remote", description: "Manage remotes: add]", flags: [], arguments: [], subcommands: [deploy] };
const descriptor = {
  name: "my-app",
  description: "Main",
  flags: [
    flag("verbose", { _tag: "Boolean" }, { aliases: ["v"], description: `Be loud: 'very' [x] \\ café 😀` }),
    flag("color", { _tag: "Choice", values: ["auto", "never", "x:y"] }, { aliases: ["c", "colour"] }),
    flag("config", { _tag: "Path", pathType: "file" }, { description: "Config file" }),
    flag("log", { _tag: "String" }, { description: "Log level", optionalValue: true }),
    flag("count", { _tag: "Int" }),
  ],
  arguments: [
    arg("input", { _tag: "Path", pathType: "either" }, { description: "Input path", required: true }),
    arg("rest", { _tag: "Choice", values: ["one", "two"] }, { variadic: true }),
  ],
  subcommands: [remote, { ...remote, name: "rm" }],
};
for (const [name, shell] of [["bash", bash], ["zsh", zsh], ["powershell", powershell]]) {
  writeFileSync(new URL(`../../runtime-go/flickrt/testdata/complete_${name}.txt`, import.meta.url), shell.generate("my-app", descriptor));
}
