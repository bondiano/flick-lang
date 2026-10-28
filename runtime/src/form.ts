// Forms of a record type that derives `Form`: a text atom and an error atom
// per field. Validation reads the texts as the field types, checks the
// record with its schema reporting every issue, and puts each issue on the
// field its path starts with.
import { Effect, Option, Result } from "effect";
import { atomState, label as labelled } from "./atoms.ts";
import { WriteService, nativeRegistry } from "./effect/reactivity/Atom.ts";
import { coerce } from "./route.ts";
import { type AnySchema, schemaValidateAll } from "./schema/index.ts";

/** A state of the registry, as `Atom.make` gives. */
type Writable<A> = ReturnType<typeof atomState<A>>;

/** A field: its label, the witness of its value, and whether it is an `Option`. */
type FieldSpec = readonly [label: string, schema: AnySchema, optional: boolean];

/** `Form a`, what `deriving Form` gives: the record's schema, its name and its fields. */
export interface FormSpec {
  readonly schema: AnySchema;
  readonly name: string;
  readonly fields: ReadonlyArray<FieldSpec>;
}

/** `T$form` of a type that derives `Form`. */
export const formSpec = (schema: AnySchema, name: string, fields: ReadonlyArray<FieldSpec>): FormSpec =>
  Object.freeze({ schema, name, fields });

/** `Form.Draft a`: the text and the error of each field. */
export interface Draft {
  readonly spec: FormSpec;
  readonly texts: ReadonlyMap<string, Writable<string>>;
  readonly errors: ReadonlyMap<string, Writable<Option.Option<string>>>;
}

/**
 * `Form.draft spec`: empty texts and no errors. The texts are labelled
 * `Type.field`, so a module replaced in development keeps what was typed.
 */
export const formDraft = (spec: FormSpec): Draft =>
  Object.freeze({
    spec,
    texts: new Map(spec.fields.map(([field]) => [field, labelled(`${spec.name}.${field}`, atomState(""))])),
    errors: new Map(spec.fields.map(([field]) => [field, atomState(Option.none<string>())])),
  });

const atomOf = <A>(atoms: ReadonlyMap<string, A>, field: string): A => {
  const atom = atoms.get(field);
  if (atom === undefined) throw new TypeError(`no field \`${field}\` in this form`);
  return atom;
};

export const formText = (draft: Draft, field: string): Writable<string> => atomOf(draft.texts, field);

export const formError = (draft: Draft, field: string): Writable<Option.Option<string>> =>
  atomOf(draft.errors, field);

/** `Form.id draft field`: the id of the field's input; its error's is this with `-error`. */
export const formId = (draft: Draft, field: string): string => `form-${draft.spec.name}-${field}`;

/** Focuses the input of `fields` that comes first in the page. */
const focusFirst = (draft: Draft, fields: ReadonlyArray<string>): void => {
  const inputs = fields.flatMap((field) => document.getElementById(formId(draft, field)) ?? []);
  const first = inputs.reduce<HTMLElement | undefined>(
    (a, b) => (a === undefined || a.compareDocumentPosition(b) & a.DOCUMENT_POSITION_PRECEDING ? b : a),
    undefined,
  );
  first?.focus();
};

/** The field an issue path starts with: `age` of `age` or `age.value`. */
const fieldOf = (path: string): string => path.split(/[.[]/)[0] ?? path;

/**
 * `Form.validate draft`: the record of the texts, or `None` with each
 * field's first issue in its error atom and the first invalid input focused.
 */
export const formValidate = (draft: Draft): Effect.Effect<Option.Option<unknown>, never, unknown> =>
  Effect.map(Effect.service(WriteService), (write) => {
    const registry = nativeRegistry(write);
    if (registry === undefined) return Option.none();
    const record: Record<string, unknown> = {};
    for (const [label, schema, optional] of draft.spec.fields) {
      const text = registry.get(atomOf(draft.texts, label));
      const value = text === "" && optional ? Option.none() : Option.getOrElse(coerce(schema, text), () => text);
      record[label] = optional && text !== "" ? Option.some(value) : value;
    }
    const result = schemaValidateAll(draft.spec.schema, record);
    const issues = Result.isFailure(result) ? result.failure._0 : [];
    const first = new Map<string, string>();
    for (const issue of issues) {
      const field = fieldOf(issue.path);
      if (!first.has(field)) first.set(field, issue.message);
    }
    for (const [label] of draft.spec.fields) {
      registry.set(atomOf(draft.errors, label), Option.fromUndefinedOr(first.get(label)));
    }
    if (typeof document !== "undefined") focusFirst(draft, [...first.keys()]);
    return Result.isSuccess(result) ? Option.some(result.success) : Option.none();
  });
