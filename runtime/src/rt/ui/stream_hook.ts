// How Flick's own runtime meets a `Stream`, which the Effect adapter owns.
// The adapter's bridge sets `text`, which makes a `Stream` of the stream of
// text `Ui.Server.stream` starts with the evidence of the run that pulls it,
// and `each`, the task that runs a `Stream` and hands the last element of
// each chunk to `element`, which `Atom.query_stream` reads. A separate module,
// so the bridge does not load React.
export const streamHooks: {
  text: ((start: () => ReadableStream<string>) => unknown) | undefined;
  each: ((stream: unknown, element: (value: unknown) => void) => unknown) | undefined;
} = { text: undefined, each: undefined };
