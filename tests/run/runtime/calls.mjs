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
    const settle = () => {
      setTimeout(() => {
        process.stdout.write(`${name} settled\n`);
        resolve();
      }, 30);
    };
    // a call cancelled before it started hears no event
    if (signal.aborted) settle();
    else signal.addEventListener("abort", settle);
    latch(name).open();
  });

/** Resolved when the call named `name` is under way. */
export const underWay = (name) => latch(name).opened;

/**
 * A write that ignores interruption: under way at once, it ends only when
 * `releaseWrite` lets it, and says so.
 */
export const heldWrite = (name) => {
  latch(name).open();
  return latch(`${name} released`).opened.then(() => void process.stdout.write(`${name} written\n`));
};

/** Lets the held write named `name` end. */
export const releaseWrite = (name) => latch(`${name} released`).open();
