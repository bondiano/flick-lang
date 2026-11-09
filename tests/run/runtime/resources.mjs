import { Effect, acquireRelease } from "@flick-lang/effect-ts";

const say = (text) => Effect.sync(() => void process.stdout.write(`${text}\n`));

/** A resource of the innermost scope, released when the scope closes. */
export const open = (name) =>
  acquireRelease(Effect.as(say(`open ${name}`), name), () => say(`close ${name}`));

/** A promise that ignores interruption and finishes after `millis`. */
export const slowWrite = (name, millis) =>
  new Promise((resolve) =>
    setTimeout(() => {
      process.stdout.write(`${name} written\n`);
      resolve();
    }, millis),
  );
