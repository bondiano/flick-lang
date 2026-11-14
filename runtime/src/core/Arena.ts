// A scoped storage owner and nonowning indexed views. Host mirrors clear
// their strong storage edges deterministically; counted reclamation is
// checked independently by flick-model, not by host garbage collection.
export class Arena {
  #slots: Array<unknown> | undefined = [];

  /** Runtime diagnostic, not a Flick binding or a dereference API. */
  get liveSlots(): number {
    return this.#slots?.length ?? 0;
  }

  allocate(value: unknown): ArenaValue {
    const slots = this.#slots;
    if (slots === undefined) throw new Error("arena is closed");
    const index = slots.length;
    slots.push(value);
    return new ArenaValue(this, index);
  }

  read(index: number): unknown {
    const slots = this.#slots;
    if (slots === undefined) throw new Error("arena is closed");
    if (!Number.isSafeInteger(index) || index < 0 || index >= slots.length) throw new Error("invalid arena value");
    return slots[index];
  }

  close(): void {
    this.#slots = undefined;
  }
}

/** Holds metadata, never a direct strong edge to the stored value. */
export class ArenaValue {
  readonly #arena: Arena;
  readonly #index: number;

  constructor(arena: Arena, index: number) {
    this.#arena = arena;
    this.#index = index;
  }

  read(): unknown {
    return this.#arena.read(this.#index);
  }
}

export const arenaAlloc = (arena: Arena, value: unknown): ArenaValue => arena.allocate(value);
export const arenaGet = (value: ArenaValue): unknown => value.read();
