export declare function lookup(id: string): Promise<string | undefined>;
export declare function nextId(): number;
export declare function toDate(value: string | number): Date;
export declare function each<T>(items: T[], visit: (item: T) => void): void;
