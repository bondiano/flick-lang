// How `Ui.Server.stream` hands its markup to the Effect adapter, which owns
// `Stream`: the adapter's bridge sets `text` to make one of a stream of text
// it starts with the evidence of the run that pulls it. A separate module, so
// the bridge does not load React.
export const streamHooks: { text: ((start: () => ReadableStream<string>) => unknown) | undefined } = { text: undefined };
