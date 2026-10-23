export declare const post: (url: string, body: string) => Promise<string>;
export declare const get: (url: string) => Promise<string>;
export declare const text: (url: string) => Promise<string>;
export declare const start: (url: string) => Promise<string>;
export declare const wait: (pending: Promise<string>) => Promise<string>;
export declare const head: (url: string) => Promise<string>;
