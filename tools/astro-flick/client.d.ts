// `import { main } from "./App.fl"` in TypeScript: a compiled Flick module.
declare module "*.fl" {
  const exports: Record<string, unknown>;
  export = exports;
}
