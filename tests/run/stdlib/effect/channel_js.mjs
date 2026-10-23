// Web streams and async iterables for the channel tests.
export const readable = (items) => ReadableStream.from(items);

export const asyncIterable = (items, done) => ({
  async *[Symbol.asyncIterator]() {
    for (const item of items) yield item;
    return done;
  },
});

export const upper = () =>
  new TransformStream({
    transform(chunk, controller) {
      controller.enqueue(chunk.toUpperCase());
    },
  });

const written = [];

export const writable = () =>
  new WritableStream({
    write(chunk) {
      written.push(chunk);
    },
  });

export const writtenSoFar = () => written.splice(0);
