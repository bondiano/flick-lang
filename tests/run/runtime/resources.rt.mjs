import { acquireRelease } from "@flick-lang/runtime/rt";

const say = (text) => void process.stdout.write(`${text}\n`);

/** A resource of the innermost scope, released when the scope closes. */
export const open = (name) =>
  acquireRelease(
    () => {
      say(`open ${name}`);
      return name;
    },
    () => say(`close ${name}`),
  );

/** A promise that ignores interruption and finishes after `millis`. */
export const slowWrite = (name, millis) =>
  new Promise((resolve) =>
    setTimeout(() => {
      process.stdout.write(`${name} written\n`);
      resolve();
    }, millis),
  );
