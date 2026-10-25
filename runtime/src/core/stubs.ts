// What compiled code names but a program built with `effect = "none"` never
// runs: the checker rejects every effect, yet a built-in module still
// imports its effectful bindings when it loads. Each throws when called.

// a JS function ignores the arguments it is called with beyond its parameters
const needsEffect =
  (name: string) =>
  (): never => {
    throw new Error(`\`${name}\` needs Effect: the program is built with \`effect = "none"\``);
  };

/** The `Effect` combinators compiled effectful code calls. */
export const Effect = /*#__PURE__*/ Object.freeze({
  gen: needsEffect("Effect.gen"),
  map: needsEffect("Effect.map"),
  succeed: needsEffect("Effect.succeed"),
  suspend: needsEffect("Effect.suspend"),
  sync: needsEffect("Effect.sync"),
});

/** A service tag: a built-in module declares its services when it loads. */
export const service = (key: string): Readonly<{ key: string }> => Object.freeze({ key });

export const bindEff = /*#__PURE__*/ needsEffect("bindEff");
export const codecBlock = /*#__PURE__*/ needsEffect("codecBlock");
export const codecIgnoreExtra = /*#__PURE__*/ needsEffect("codecIgnoreExtra");
export const codecNumberFinite = /*#__PURE__*/ needsEffect("codecNumberFinite");
export const codecNumberLossless = /*#__PURE__*/ needsEffect("codecNumberLossless");
export const codecOptionNull = /*#__PURE__*/ needsEffect("codecOptionNull");
export const codecOptionNullish = /*#__PURE__*/ needsEffect("codecOptionNullish");
export const codecString = /*#__PURE__*/ needsEffect("codecString");
export const dslField = /*#__PURE__*/ needsEffect("dslField");
export const dslFieldEffect = /*#__PURE__*/ needsEffect("dslFieldEffect");
export const dslOnMissing = /*#__PURE__*/ needsEffect("dslOnMissing");
export const dslOnMissingEffect = /*#__PURE__*/ needsEffect("dslOnMissingEffect");
export const dslOptionalField = /*#__PURE__*/ needsEffect("dslOptionalField");
export const flatMapEff = /*#__PURE__*/ needsEffect("flatMapEff");
export const foreignAsync = /*#__PURE__*/ needsEffect("foreignAsync");
export const foreignEffect = /*#__PURE__*/ needsEffect("foreignEffect");
export const foreignSync = /*#__PURE__*/ needsEffect("foreignSync");
export const fromResult = /*#__PURE__*/ needsEffect("fromResult");
export const label = /*#__PURE__*/ needsEffect("label");
export const listEachEffect = /*#__PURE__*/ needsEffect("listEachEffect");
export const listFoldEffect = /*#__PURE__*/ needsEffect("listFoldEffect");
export const listMapEffect = /*#__PURE__*/ needsEffect("listMapEffect");
export const mapEff = /*#__PURE__*/ needsEffect("mapEff");
export const map2Eff = /*#__PURE__*/ needsEffect("map2Eff");
export const operation = /*#__PURE__*/ needsEffect("operation");
export const parallel = /*#__PURE__*/ needsEffect("parallel");
export const parallelAll = /*#__PURE__*/ needsEffect("parallelAll");
export const parallelExits = /*#__PURE__*/ needsEffect("parallelExits");
export const parallelResults = /*#__PURE__*/ needsEffect("parallelResults");
export const provide = /*#__PURE__*/ needsEffect("provide");
export const raceFirst = /*#__PURE__*/ needsEffect("raceFirst");
export const raceSuccess = /*#__PURE__*/ needsEffect("raceSuccess");
export const refinedParse = /*#__PURE__*/ needsEffect("refinedParse");
export const schemaAdt = /*#__PURE__*/ needsEffect("schemaAdt");
export const schemaJsonCodec = /*#__PURE__*/ needsEffect("schemaJsonCodec");
export const schemaList = /*#__PURE__*/ needsEffect("schemaList");
export const schemaOption = /*#__PURE__*/ needsEffect("schemaOption");
export const schemaRedacted = /*#__PURE__*/ needsEffect("schemaRedacted");
export const scope = /*#__PURE__*/ needsEffect("scope");
export const succeed = /*#__PURE__*/ needsEffect("succeed");
export const timeout = /*#__PURE__*/ needsEffect("timeout");
export const timeoutOption = /*#__PURE__*/ needsEffect("timeoutOption");
export const tryWith = /*#__PURE__*/ needsEffect("tryWith");
export const within = /*#__PURE__*/ needsEffect("within");
