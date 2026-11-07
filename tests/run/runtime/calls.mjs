const calls = new Map();

/** The latch of the call named `name`, opened when the call is under way. */
const latch = (name) => {
  if (!calls.has(name)) {
    let open;
    const opened = new Promise((resolve) => {
      open = resolve;
    });
    calls.set(name, { open, opened });
  }
  return calls.get(name);
};

/** A promise that settles some time after its call is cancelled, and says so. */
export const settleLater = (name, signal) =>
  new Promise((resolve) => {
    signal.addEventListener("abort", () => {
      setTimeout(() => {
        process.stdout.write(`${name} settled\n`);
        resolve();
      }, 30);
    });
    latch(name).open();
  });

/** Resolved when the call named `name` is under way. */
export const underWay = (name) => latch(name).opened;
