import { execFileSync } from "node:child_process";
import { test } from "node:test";

test("portable adapters work without process and Node layers fail only when built", () => {
  execFileSync(
    process.execPath,
    [
      "--input-type=module",
      "--eval",
      `
        import assert from "node:assert/strict";
        import { registerHooks } from "node:module";
        registerHooks({
          resolve(specifier, context, next) {
            if (specifier.startsWith("@effect/platform-node/")) throw new Error("loaded Node adapter eagerly");
            return next(specifier, context);
          },
        });
        globalThis.process = undefined;
        const { Context, Effect, Option } = await import("effect");
        const Path = await import("@flick-lang/effect-ts/effect/Path");
        const FileSystem = await import("@flick-lang/effect-ts/effect/FileSystem");
        const Stdio = await import("@flick-lang/effect-ts/effect/Stdio");
        const Terminal = await import("@flick-lang/effect-ts/effect/Terminal");
        const Cli = await import("@flick-lang/effect-ts/cli");
        const pathKey = Context.Service("Test.Browser.Path");
        const fsKey = Context.Service("Test.Browser.FileSystem");
        const stdioKey = Context.Service("Test.Browser.Stdio");
        const terminalKey = Context.Service("Test.Browser.Terminal");
        const ttyKey = Context.Service("Test.Browser.Tty");
        const layers = [
          [pathKey, Path.flickNode(pathKey), "NodePath"],
          [fsKey, FileSystem.flickNode(fsKey), "NodeFileSystem"],
          [stdioKey, Stdio.flickNode(stdioKey), "NodeStdio"],
          [terminalKey, Terminal.flickNode(terminalKey, Option.none()), "NodeTerminal"],
          [ttyKey, Cli.promptNode(ttyKey), "NodeTerminal"],
        ];
        const joined = await Effect.runPromise(
          Effect.flatMap(Effect.service(pathKey), (path) => path.join(["alpha", "..", "beta", "file.fl"]))
            .pipe(Effect.provide(Path.flickLayer(pathKey))),
        );
        assert.equal(joined, "beta/file.fl");
        const prompt = Cli.promptText({ message: "Name", default: Option.none(), validate: Option.none() });
        assert.equal(Cli.multiPrompt(prompt).prompt, prompt);
        const command = Cli.specHandle(() => Effect.void, Cli.specCommand("portable"));
        const run = Cli.cliRunVersion("1.0.0", command, []);
        await assert.rejects(
          Effect.runPromise(run),
          /@effect\\/platform-node\\/NodeServices is available only on Node.js/,
        );
        for (const [key, layer, module] of layers) {
          await assert.rejects(
            Effect.runPromise(Effect.service(key).pipe(Effect.provide(layer))),
            (error) => error.message.includes("@effect/platform-node/" + module + " is available only on Node.js"),
          );
        }
      `,
    ],
    { cwd: new URL("..", import.meta.url) },
  );
});
