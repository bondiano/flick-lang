/** A promise resolved after `millis`. */
export const sleep = (millis) => new Promise((resolve) => setTimeout(resolve, millis));

/** A promise that ends only when its call is cancelled. */
export const hang = (_unit, signal) =>
  new Promise((resolve) =>
    signal.addEventListener("abort", () => {
      process.stdout.write("hang cancelled\n");
      resolve();
    }),
  );
