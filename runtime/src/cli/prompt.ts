// `Cli.Prompt`: Effect's interactive prompts, run on the Flick service
// `Cli.Prompt.Tty`. The runner reads one key press each time a prompt waits
// for one, so a key the prompt does not need stays with the service for the
// next prompt; Effect's own runner would read ahead from a queue.



import {
  type Cause,
  type Context,
  DateTime,
  Effect,
  Layer,
  Option,
  Queue,
  Result,
  Terminal,
} from "effect";
import { Prompt } from "effect/cli";

import { nodeModule } from "../host.ts";
import { constructorName, tag } from "../tags.ts";

const QUIT = /*#__PURE__*/ Object.freeze({ _tag: tag("Terminal.QuitError.QuitError") });

/** The operations of `Cli.Prompt.Tty`, as Flick calls them. */
export interface TtyImpl {
  readonly read_key: (unit: undefined) => Effect.Effect<Terminal.UserInput, unknown>;
  readonly display: (text: string) => Effect.Effect<unknown>;
  readonly columns: (unit: undefined) => Effect.Effect<number>;
  readonly rows: (unit: undefined) => Effect.Effect<number>;
}

/** `Cli.Prompt.node`: the terminal of the process; Ctrl+C and Ctrl+D quit. */
export const promptNode = <I>(key: Context.Key<I, TtyImpl>): Layer.Layer<I> =>
  Layer.effect(
    key,
    Effect.gen(function* () {
      const terminal = yield* nodeModule("@effect/platform-node/NodeTerminal").make();
      const input = yield* terminal.readInput;
      return {
        read_key: () => Effect.mapError(Queue.take(input), () => QUIT),
        display: (text: string) => Effect.ignore(terminal.display(text)),
        columns: () => terminal.columns,
        rows: () => terminal.rows,
      };
    }),
  );

interface Loop {
  readonly _tag: "Loop";
  readonly initialState: unknown;
  readonly render: (state: unknown, action: unknown) => Effect.Effect<string, never, unknown>;
  readonly process: (input: unknown, state: unknown) => Effect.Effect<Action, never, unknown>;
  readonly clear: (state: unknown, action: unknown) => Effect.Effect<string, never, unknown>;
  readonly events: Queue.Dequeue<unknown, Cause.Done> | undefined;
}

type Action =
  | { readonly _tag: "Beep" }
  | { readonly _tag: "NextFrame"; readonly state: unknown }
  | { readonly _tag: "Submit"; readonly value: unknown };

type Op =
  | Loop
  | { readonly _tag: "OnSuccess"; readonly prompt: Op; readonly onSuccess: (a: unknown) => Op }
  | { readonly _tag: "Succeed"; readonly value: unknown };

const CURSOR_SHOW = "\x1b[?25h";

/** One loop of a prompt, as Effect's runner draws it, reading keys from the service. */
const runLoop = (loop: Loop, tty: TtyImpl): Effect.Effect<unknown, unknown, unknown> =>
  Effect.gen(function* () {
    let state = Effect.isEffect(loop.initialState) ? yield* loop.initialState : loop.initialState;
    let action: Action = { _tag: "NextFrame", state };
    let clear = "";
    while (true) {
      const msg = yield* loop.render(state, action);
      yield* tty.display(clear + msg);
      clear = "";
      const key = Effect.map(tty.read_key(undefined), (input) => ({ _tag: "Input", input }));
      const input =
        loop.events === undefined
          ? (yield* tty.read_key(undefined))
          : yield* Effect.raceFirst(
            key,
            Effect.map(Queue.take(loop.events), (value) => ({ _tag: "Event", value })),
          );
      action = yield* loop.process(input, state);
      if (action._tag === "Beep") continue;
      clear = yield* loop.clear(state, action);
      if (action._tag === "NextFrame") {
        state = action.state;
        continue;
      }
      const last = yield* loop.render(state, action);
      yield* tty.display(clear + last);
      return action.value;
    }
  }).pipe(Effect.ensuring(Effect.ignore(tty.display(CURSOR_SHOW))));

const runOp = (op: Op, tty: TtyImpl): Effect.Effect<unknown, unknown, unknown> =>
  Effect.suspend(() => {
    if (op._tag === "Loop") return runLoop(op, tty);
    if (op._tag === "OnSuccess") return Effect.flatMap(runOp(op.prompt, tty), (a) => runOp(op.onSuccess(a), tty));
    return Effect.succeed(op.value);
  });

/** Effect's `Terminal` the prompts' handlers ask for its size. */
const terminalOf = (tty: TtyImpl): Terminal.Terminal =>
  Terminal.make({
    columns: Effect.suspend(() => tty.columns(undefined)),
    rows: Effect.suspend(() => tty.rows(undefined)),
    size: Effect.die("Cli.Prompt asks Tty for its columns and rows"),
    readInput: Effect.die("Cli.Prompt reads keys from Tty.read_key"),
    readLine: Effect.die("Cli.Prompt reads keys from Tty.read_key"),
    display: (text) => Effect.asVoid(tty.display(text)),
  });

/** `Cli.Prompt.run`: the File prompt reads the file system of Node.js. */
export const promptRun = <I>(
  key: Context.Key<I, TtyImpl>,
  prompt: Prompt.Prompt<unknown>,
): Effect.Effect<unknown, unknown, unknown> =>
  Effect.flatMap(Effect.service(key), (tty) =>
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- every prompt is Effect's tree of these three nodes
    runOp(prompt as unknown as Op, tty).pipe(
      Effect.provide(
        Layer.mergeAll(
          nodeModule("@effect/platform-node/NodeFileSystem").layer,
          nodeModule("@effect/platform-node/NodePath").layer,
          Layer.succeed(Terminal.Terminal, terminalOf(tty)),
        ),
      ),
    ),
  );

/** `Cli.Prompt.run_themed`. */
export const promptRunThemed = <I>(
  key: Context.Key<I, TtyImpl>,
  theme: Prompt.Theme,
  prompt: Prompt.Prompt<unknown>,
): Effect.Effect<unknown, unknown, unknown> => Effect.provideService(promptRun(key, prompt), Prompt.Theme, theme);


type Opt<A> = Option.Option<A>;

/** `{ key: value }` when the option is `Some`: Effect's options leave a missing one out. */
const some = <K extends string, A>(key: K, option: Opt<A>): { readonly [P in K]?: A } =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- an object of the one computed key
  Option.isSome(option) ? ({ [key]: option.value } as { readonly [P in K]: A }) : {};

/** A Flick check: `Err` is the message the prompt shows. */
type Validate<A> = (value: A) => Result.Result<A, string>;

const effectOf = <A>(r: Result.Result<A, string>): Effect.Effect<A, string> =>
  Result.isSuccess(r) ? Effect.succeed(r.success) : Effect.fail(r.failure);

const check = <A>(validate: Opt<Validate<A>>) =>
  some(
    "validate",
    Option.map(validate, (f) => (value: A) => effectOf(f(value))),
  );

interface FlickText {
  readonly message: string;
  readonly default: Opt<string>;
  readonly validate: Opt<Validate<string>>;
}

const textOptions = (o: FlickText): Prompt.TextOptions => ({
  message: o.message,
  ...some("default", o.default),
  ...check(o.validate),
});

export const promptText = (o: FlickText) => Prompt.String(textOptions(o));
export const promptPassword = (o: FlickText) => Prompt.Password(textOptions(o));
export const promptHidden = (o: FlickText) => Prompt.Hidden(textOptions(o));
export const promptList = (o: FlickText & { readonly delimiter: Opt<string> }) =>
  Prompt.List({ ...textOptions(o), ...some("delimiter", o.delimiter) });

interface FlickInt {
  readonly message: string;
  readonly default: Opt<number>;
  readonly min: Opt<number>;
  readonly max: Opt<number>;
  readonly incrementBy: Opt<number>;
  readonly decrementBy: Opt<number>;
  readonly validate: Opt<Validate<number>>;
}

const intOptions = (o: FlickInt): Prompt.IntOptions => ({
  message: o.message,
  ...some("default", o.default),
  ...some("min", o.min),
  ...some("max", o.max),
  ...some("incrementBy", o.incrementBy),
  ...some("decrementBy", o.decrementBy),
  ...check(o.validate),
});

/** `Cli.Prompt.int`: the bounds default to those of Flick's 32-bit `Int` and hold before `validate` runs. */
export const promptInt = (o: FlickInt) => {
  const min = Option.getOrElse(o.min, () => -0x80000000);
  const max = Option.getOrElse(o.max, () => 0x7fffffff);
  const validate = Option.getOrUndefined(o.validate);
  return Prompt.Int({
    ...intOptions(o),
    min,
    max,
    validate: (n) => {
      if (n < min) return Effect.fail(`${n} must be greater than or equal to ${min}`);
      if (n > max) return Effect.fail(`${n} must be less than or equal to ${max}`);
      return validate === undefined ? Effect.succeed(n) : effectOf(validate(n));
    },
  });
};
export const promptNumber = (o: FlickInt & { readonly precision: Opt<number> }) =>
  Prompt.Number({ ...intOptions(o), ...some("precision", o.precision) });

export const promptToggle = (o: {
  readonly message: string;
  readonly initial: Opt<boolean>;
  readonly active: Opt<string>;
  readonly inactive: Opt<string>;
}) =>
  Prompt.Toggle({
    message: o.message,
    ...some("initial", o.initial),
    ...some("active", o.active),
    ...some("inactive", o.inactive),
  });

export const promptConfirm = (o: {
  readonly message: string;
  readonly initial: Opt<boolean>;
  readonly label: Opt<{ readonly confirm: string; readonly deny: string }>;
  readonly placeholder: Opt<{ readonly defaultConfirm: Opt<string>; readonly defaultDeny: Opt<string> }>;
}) =>
  Prompt.Confirm({
    message: o.message,
    ...some("initial", o.initial),
    ...some("label", o.label),
    ...some(
      "placeholder",
      Option.map(o.placeholder, (p) => ({
        ...some("defaultConfirm", p.defaultConfirm),
        ...some("defaultDeny", p.defaultDeny),
      })),
    ),
  });

type Names = ReadonlyArray<string>;
const at = (names: Names, i: number): string => names[i] ?? "";
type Twelve = [string, string, string, string, string, string, string, string, string, string, string, string];
type Seven = [string, string, string, string, string, string, string];
const twelve = (n: Names): Twelve => [
  at(n, 0),
  at(n, 1),
  at(n, 2),
  at(n, 3),
  at(n, 4),
  at(n, 5),
  at(n, 6),
  at(n, 7),
  at(n, 8),
  at(n, 9),
  at(n, 10),
  at(n, 11),
];
const seven = (n: Names): Seven => [at(n, 0), at(n, 1), at(n, 2), at(n, 3), at(n, 4), at(n, 5), at(n, 6)];

/** `Cli.Prompt.date`: `DateTime` in and out, Effect's prompt works on JS dates. */
export const promptDate = (o: {
  readonly message: string;
  readonly initial: Opt<DateTime.DateTime>;
  readonly dateMask: Opt<string>;
  readonly validate: Opt<Validate<DateTime.DateTime>>;
  readonly locales: Opt<{
    readonly months: Names;
    readonly monthsShort: Names;
    readonly weekdays: Names;
    readonly weekdaysShort: Names;
  }>;
}) =>
  Prompt.map(
    Prompt.Date({
      message: o.message,
      ...some("initial", Option.map(o.initial, DateTime.toDateUtc)),
      ...some("dateMask", o.dateMask),
      ...some(
        "validate",
        Option.map(
          o.validate,
          (f) => (date: Date) => Effect.map(effectOf(f(DateTime.makeUnsafe(date))), DateTime.toDateUtc),
        ),
      ),
      ...some(
        "locales",
        Option.map(o.locales, (l) => ({
          months: twelve(l.months),
          monthsShort: twelve(l.monthsShort),
          weekdays: seven(l.weekdays),
          weekdaysShort: seven(l.weekdaysShort),
        })),
      ),
    }),
    (date) => DateTime.makeUnsafe(date),
  );

type PathType = "file" | "directory" | "either";

const PATH_TYPES: Readonly<Record<string, PathType>> = {
  FilePath: "file",
  DirectoryPath: "directory",
  EitherPath: "either",
};

export const promptFile = (o: {
  readonly type_: Opt<{ readonly _tag: string }>;
  readonly message: Opt<string>;
  readonly startingPath: Opt<string>;
  readonly default: Opt<string>;
  readonly maxPerPage: Opt<number>;
  readonly filter: Opt<(file: string) => boolean>;
}) =>
  Prompt.File({
    ...some(
      "type",
      Option.flatMap(o.type_, (t) => Option.fromUndefinedOr(PATH_TYPES[constructorName(t._tag)])),
    ),
    ...some("message", o.message),
    ...some("startingPath", o.startingPath),
    ...some("default", o.default),
    ...some("maxPerPage", o.maxPerPage),
    ...some("filter", o.filter),
  });

interface FlickChoice<A> {
  readonly title: string;
  readonly value: A;
  readonly description: Opt<string>;
  readonly disabled: boolean;
  readonly selected: boolean;
}

const choice = <A>(c: FlickChoice<A>): Prompt.SelectChoice<A> => ({
  title: c.title,
  value: c.value,
  ...some("description", c.description),
  disabled: c.disabled,
  selected: c.selected,
});

interface FlickSelect<A> {
  readonly message: Opt<string>;
  readonly choices: ReadonlyArray<FlickChoice<A>>;
  readonly maxPerPage: Opt<number>;
}

const selectOptions = <A>(o: FlickSelect<A>): Prompt.SelectOptions<A> => ({
  ...some("message", o.message),
  choices: o.choices.map(choice),
  ...some("maxPerPage", o.maxPerPage),
});

export const promptSelect = (o: FlickSelect<unknown>) => Prompt.Select(selectOptions(o));

export const promptAutoComplete = (o: {
  readonly message: string;
  readonly choices: ReadonlyArray<FlickChoice<unknown>>;
  readonly maxPerPage: Opt<number>;
  readonly filterLabel: Opt<string>;
  readonly filterPlaceholder: Opt<string>;
  readonly emptyMessage: Opt<string>;
}) =>
  Prompt.AutoComplete({
    message: o.message,
    choices: o.choices.map(choice),
    ...some("maxPerPage", o.maxPerPage),
    ...some("filterLabel", o.filterLabel),
    ...some("filterPlaceholder", o.filterPlaceholder),
    ...some("emptyMessage", o.emptyMessage),
  });

export const promptMultiSelect = (
  select: FlickSelect<unknown>,
  multi: {
    readonly selectAll: Opt<string>;
    readonly selectNone: Opt<string>;
    readonly inverseSelection: Opt<string>;
    readonly min: Opt<number>;
    readonly max: Opt<number>;
  },
) =>
  Prompt.MultiSelect({
    ...selectOptions(select),
    ...some("selectAll", multi.selectAll),
    ...some("selectNone", multi.selectNone),
    ...some("inverseSelection", multi.inverseSelection),
    ...some("min", multi.min),
    ...some("max", multi.max),
  });

/** A Flick `Cli.Prompt.Action` as Effect's, and back. */
const BEEP = /*#__PURE__*/ tag("Cli.Prompt.Action.Beep");
const NEXT_FRAME = /*#__PURE__*/ tag("Cli.Prompt.Action.NextFrame");
const SUBMIT = /*#__PURE__*/ tag("Cli.Prompt.Action.Submit");
interface FlickAction {
  readonly _tag: string;
  readonly _0?: unknown;
}
type EffectAction = Prompt.Action<unknown, unknown>;

const toEffectAction = (a: FlickAction): EffectAction =>
  a._tag === NEXT_FRAME
    ? { _tag: "NextFrame", state: a._0 }
    : a._tag === SUBMIT
      ? { _tag: "Submit", value: a._0 }
      : { _tag: "Beep" };

const toFlickAction = (a: EffectAction): FlickAction =>
  a._tag === "NextFrame"
    ? { _tag: NEXT_FRAME, _0: a.state }
    : a._tag === "Submit"
      ? { _tag: SUBMIT, _0: a.value }
      : { _tag: BEEP };

/** `Cli.Prompt.Handlers`: pure functions, curried as Flick holds them in a record. */
interface FlickHandlers<I> {
  readonly render: (state: unknown) => (action: FlickAction) => string;
  readonly process: (input: I) => (state: unknown) => FlickAction;
  readonly clear: (state: unknown) => (action: FlickAction) => string;
}

const handlers = <I, J>(h: FlickHandlers<I>, input: (i: J) => I): Prompt.Handlers<unknown, unknown, J> => ({
  render: (state, action) => Effect.succeed(h.render(state)(toFlickAction(action))),
  process: (i, state) => Effect.succeed(toEffectAction(h.process(input(i))(state))),
  clear: (state, action) => Effect.succeed(h.clear(state)(toFlickAction(action))),
});

export const promptCustom = (state: unknown, h: FlickHandlers<Terminal.UserInput>) =>
  Prompt.Custom(
    state,
    handlers(h, (i: Terminal.UserInput) => i),
  );

const INPUT = /*#__PURE__*/ tag("Cli.Prompt.ProcessInput.Input");
const EVENT = /*#__PURE__*/ tag("Cli.Prompt.ProcessInput.Event");

export const promptCustomEvents = (state: unknown, events: Queue.Dequeue<unknown>, h: FlickHandlers<FlickAction>) =>
  Prompt.Custom(
    state,
    events,
    handlers(
      h,
      (i: Prompt.ProcessInput<unknown>): FlickAction =>
        i._tag === "Input" ? { _tag: INPUT, _0: i.input } : { _tag: EVENT, _0: i.value },
    ),
  );

export const promptAll = (prompts: ReadonlyArray<Prompt.Prompt<unknown>>) => Prompt.all(prompts);
export const promptMap = Prompt.map;
export const promptFlatMap = Prompt.flatMap;
export const promptSucceed = Prompt.succeed;
export const promptDefaultTheme = (): Prompt.Theme => Prompt.makeTheme();
