export declare const readable: (items: ReadonlyArray<string>) => ReadableStream<string>;
export declare const asyncIterable: (items: ReadonlyArray<number>, done: string) => AsyncIterable<number, string>;
export declare const upper: () => TransformStream<string, string>;
export declare const writable: () => WritableStream<string>;
export declare const writtenSoFar: () => Array<string>;
