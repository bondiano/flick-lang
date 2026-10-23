// Codecs a module makes while it loads, for the compiled-decoder tests.
import {
  codecBlock,
  codecCheckWhole,
  codecList,
  codecNumberFinite,
  codecOptionNull,
  codecOptionNullish,
  codecString,
  codecTransformPure,
  dslField,
  dslOnMissing,
  dslOptionalField,
  schemaAdt,
  schemaJsonCodec,
  schemaLazy,
  schemaList,
  schemaNumber,
} from "../../src/index.ts";

export const tree = schemaAdt("Tree", [
  ["T.Leaf", "Leaf", [schemaNumber]],
  ["T.Branch", "Branch", [schemaList(schemaLazy("Tree", () => tree))]],
]);

export const codecs = {
  fields: codecBlock("Row", [
    ["name", dslField("name", codecString)],
    ["nick", dslOptionalField("nick", codecString)],
    ["note", dslField("note", codecOptionNull(codecString))],
    ["alias", dslField("alias", codecOptionNullish(codecString))],
    ["count", dslOnMissing(() => 7, dslField("count", codecNumberFinite))],
  ]),
  list: codecBlock("Doubled", [
    ["xs", dslField("xs", codecList(codecTransformPure((n: number) => n * 2, (n) => n / 2, codecNumberFinite)))],
  ]),
  checked: codecBlock("Short", [["s", dslField("s", codecCheckWhole((s: string) => s.length < 3, "too long", codecString))]]),
  tree: schemaJsonCodec(tree),
};
