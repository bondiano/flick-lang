// Writes runtime-go/flickrt/testdata/cli_parity.txt: what Effect's CLI
// prints and fails with for a corpus of command lines, run through Flick's
// own adapter (runtime/src/cli) on the command that cliParityCommand
// (cli_parity_test.go) mirrors. Run from the repository root after moving
// the Effect pin: `node tools/go-cli/parity.mjs`.
import { writeFileSync } from "node:fs";

process.env.NO_COLOR = "1";
const src = new URL("../../runtime/src/cli/", import.meta.url);
const { cliRunVersion, cliHelpDoc } = await import(new URL("run.ts", src).href);
const { cliDefaultFormatter } = await import(new URL("output.ts", src).href);
const S = await import(new URL("spec.ts", src).href);
const { Console, Effect, Exit, Option, Result } = await import("../../runtime/node_modules/effect/dist/index.js");
const TestConsole = await import("../../runtime/node_modules/effect/dist/testing/TestConsole.js");

const text = { parse: (t) => Effect.succeed(Result.succeed(t)) };
const int = { parse: (t) => Effect.succeed(/^-?\d+$/.test(t) ? Result.succeed(t) : Result.fail(`not an Int: ${t}`)) };

// a field as the Go test shows it: Option, list or text
const show = (v) =>
  Option.isOption(v) ? (Option.isSome(v) ? `Some(${show(v.value)})` : "None") : Array.isArray(v) ? `[${v.map(show).join(",")}]` : String(v);
const handler = (name) => (o) =>
  Console.log(`${name} ${Object.keys(o).sort().map((k) => `${k}=${show(o[k])}`).join(" ")}`);
const action = (name) => (ctx) => Console.log(`${name} at ${ctx.commandPath.join(" ")}`);

let greet = S.specDescription("Prints a greeting", S.specCommand("greet"));
greet = S.specFlag(text, null, null, "loud", ["l"], "Print in capitals", S.multiSwitch, greet);
greet = S.specFlag(int, null, null, "times", ["n"], "How many times", S.multiDefault("1"), greet);
greet = S.specFlag(text, null, null, "level", [], "", S.multiOptionalValue("auto", S.multiOptional), greet);
greet = S.specArg(text, null, null, "name", "Who to greet", S.multiRequired, greet);
greet = S.specExample("tool greet Ada", "Greet Ada", greet);
greet = S.specHandle(handler("greet"), greet);

let sum = S.specShortDescription("Adds", S.specCommand("sum"));
sum = S.specFlag(text, null, null, "tag", [], "A tag", S.multiMany, sum);
sum = S.specArg(int, null, null, "numbers", "", S.multiSome, sum);
sum = S.specHandle(handler("sum"), sum);

let add = S.specCommand("add");
add = S.specArg(text, null, null, "url", "", S.multiRequired, add);
add = S.specHandle(handler("add"), add);
let remote = S.specDescription("Manage remotes", S.specCommand("remote"));
remote = S.specAction("force", "Skip checks", action("force"), remote);
remote = S.specFlag(text, null, null, "remote_name", [], "The remote", S.multiDefault("origin"), remote);
remote = S.specGroup(S.specSubcommand(add, "Add one", remote));

let tool = S.specDescription("Project tools", S.specCommand("tool"));
tool = S.specFlag(text, null, null, "verbose", ["v"], "Verbose output", S.multiSwitch, tool);
tool = S.specFlag(text, null, null, "profile", [], "Profile name", S.multiOptional, tool);
tool = S.specAction("dry", "Show the plan", action("dry"), tool);
tool = S.specGroup(S.specSubcommand(remote, "", S.specSubcommand(sum, "", S.specSubcommand(greet, "", tool))));

const corpus = [
  [],
  ["--help"],
  ["greet", "--help"],
  ["remote", "add", "--help"],
  ["sum", "-h"],
  ["--version"],
  ["greet", "Ada"],
  ["-v", "--profile", "p", "greet", "-l", "--times", "3", "--level", "Ada"],
  ["greet", "--level=x", "Ada"],
  ["greet", "--loud", "--loud", "Ada"],
  ["greet", "-n", "2", "--times", "3", "Ada"],
  ["greet", "--level", "--level=b", "Ada"],
  ["--verbose", "-v", "greet", "Ada"],
  ["--profile", "a", "greet", "--profile", "b", "Ada"],
  ["sum", "--tag", "a", "--tag", "b", "1", "2"],
  ["sum", "1", "x"],
  ["greet", "--times", "x", "Ada"],
  ["greet", "--=x"],
  ["--=x"],
  ["greet", "-ln=3", "Ada"],
  ["greet", "--no-loud", "Ada"],
  ["greet", "--no-loud=true", "Ada"],
  ["greet", "--no-loud", "yes", "Ada"],
  ["greet", "--no-times", "Ada"],
  ["greet", "--lod", "Ada"],
  ["greet", "--times"],
  ["greet"],
  ["greet", "Ada", "Bob"],
  ["gret"],
  ["remote"],
  ["remote", "add", "u"],
  ["remote", "--remote-name", "up", "add", "u"],
  ["remote", "--force", "add", "u"],
  ["greet", "--force", "Ada"],
  ["--dry", "greet", "Ada"],
  ["--dry", "--dry", "greet", "Ada"],
  ["--dry=maybe", "greet", "Ada"],
  ["--log-level", "debug", "greet", "Ada"],
  ["--log-level", "bogus", "greet", "Ada"],
  ["--log-level", "info", "--log-level", "warn", "greet", "Ada"],
  ["--log-level"],
  ["--completions", "nope"],
  ["--trace=nah", "greet", "Ada"],
  ["--trace", "--trace", "greet", "Ada"],
  ["--wizard=maybe", "greet", "Ada"],
];

const run = (effect) =>
  Effect.runPromise(
    Effect.flatMap(TestConsole.make, (console) =>
      effect.pipe(
        Effect.exit,
        Effect.provideService(Console.Console, console),
        Effect.flatMap((exit) =>
          Effect.map(Effect.all([console.logLines, console.errorLines]), ([out, err]) => ({ exit, out, err })),
        ),
      ),
    ),
  );

// a help printed before is named, not repeated: cli_parity_test.go does the same
const blocks = [];
const printed = new Map();
for (const args of corpus) {
  const { exit, out, err } = await run(cliRunVersion("1.0.0", tool, args));
  const title = `$ tool ${args.join(" ")}`.trimEnd();
  const stdout = out.map(String).join("\n");
  const lines = [title];
  if (printed.has(stdout)) lines.push(`(stdout of ${printed.get(stdout)})`);
  else if (out.length > 0) lines.push(stdout), printed.set(stdout, title);
  for (const e of err) lines.push(`stderr: ${String(e)}`);
  if (Exit.isFailure(exit)) {
    const failure = exit.cause.reasons.find((r) => r._tag === "Fail");
    lines.push(failure ? `error: ${failure.error._0}` : `defect: ${String(exit.cause)}`);
  }
  blocks.push(lines.join("\n"));
}
// `Cli.help path command`: the help document as Flick's record, rendered by
// `Cli.default_formatter false`
const plain = cliDefaultFormatter(false);
for (const path of [[], ["greet"], ["remote", "add"]]) {
  const doc = await Effect.runPromise(cliHelpDoc(path, tool));
  blocks.push([`help [${path.join(" ")}]`, plain.formatHelpDoc(doc)].join("\n"));
}
writeFileSync(new URL("../../runtime-go/flickrt/testdata/cli_parity.txt", import.meta.url), blocks.join("\n\n") + "\n");
