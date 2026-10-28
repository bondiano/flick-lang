export declare const setup: () => undefined;
export declare const type: (id: string, value: string) => undefined;
export declare const click: (label: string) => undefined;
export declare const describe: (id: string) => string;
export declare const focused: () => string;
export declare const log: (line: string) => undefined;
export declare const taken: (name: string) => Promise<boolean>;
export declare const settle: () => Promise<undefined>;
