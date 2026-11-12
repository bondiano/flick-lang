// The JSON codecs of `ByteSize`, fallible ones of the strict exporter; the
// rest of the module is written in Flick over the same `bigint`.
import { Schema, SchemaTransformation } from "effect";

import { type AnySchema, jsonNumber } from "../schema/schemas.ts";
import { fallible } from "./Schema.ts";

/** `ByteSize.json`: a size as its text, `"1.5 KiB"`; encodes as `"1536 bytes"`. */
export const flickJson: AnySchema = /*#__PURE__*/ fallible(Schema.ByteSizeFromString);

/** `ByteSize.json_number`: a whole, non-negative JSON number of bytes. */
export const flickJsonNumber: AnySchema = /*#__PURE__*/ fallible(
  jsonNumber.pipe(Schema.decodeTo(Schema.ByteSize, SchemaTransformation.byteSizeFromNumber)),
);

/** `ByteSize.json_bigint`: a number of bytes as a JSON string of digits. */
export const flickJsonBigint: AnySchema = /*#__PURE__*/ fallible(
  Schema.String.check(Schema.isPattern(/^[0-9]+$/)).pipe(
    Schema.decodeTo(Schema.BigInt, SchemaTransformation.bigintFromString),
    Schema.decodeTo(Schema.ByteSize, SchemaTransformation.byteSizeFromBigInt),
  ),
);
