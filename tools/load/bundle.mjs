// Bundles the client entry of tools/load/ssr for the browser, as
// `just bundle-size` does: React in production mode, one ES module.
//
//   node tools/load/bundle.mjs tools/load/ssr/dist/flick-client.js tools/load/ssr/dist/client.js
import { rolldown } from "rolldown";

const [input, file] = process.argv.slice(2);
if (input === undefined || file === undefined) {
  console.error("usage: bundle.mjs <client entry> <bundle>");
  process.exit(2);
}
const bundle = await rolldown({
  input,
  platform: "browser",
  transform: { define: { "process.env.NODE_ENV": '"production"' } },
  logLevel: "silent",
});
await bundle.write({ file, format: "esm", minify: true, codeSplitting: false });
await bundle.close();
