// Effect's `Optic` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a failure a
// Flick callback gives is a message, and one Effect gives is a `SchemaError`.
import { Optic, Result, Schema, SchemaIssue } from "effect";

import { type SchemaError, toSchemaError } from "../schema/issues.ts";

export * from "effect/Optic";

const toIssue = <A>(r: Result.Result<A, string>): Result.Result<A, SchemaIssue.Issue> =>
  Result.mapError(r, (message) => new SchemaIssue.InvalidValue({ message }));

const toError = <A>(r: Result.Result<A, SchemaIssue.Issue>): Result.Result<A, SchemaError> =>
  Result.mapError(r, (issue) => toSchemaError(false)(new Schema.SchemaError(issue)));

/** `Optic.make_prism get set`: `get` fails with a message. */
export const flickMakePrism = <S, A>(
  getResult: (s: S) => Result.Result<A, string>,
  set: (a: A) => S,
): Optic.Prism<S, A> => Optic.makePrism((s: S) => toIssue(getResult(s)), set);

/** `Optic.make_optional get set`: both fail with a message. */
export const flickMakeOptional = <S, A>(
  getResult: (s: S) => Result.Result<A, string>,
  set: (a: A, s: S) => Result.Result<S, string>,
): Optic.Optional<S, A> =>
  Optic.makeOptional(
    (s: S) => toIssue(getResult(s)),
    (a: A, s: S) => toIssue(set(a, s)),
  );

/** `Optic.from_check ok message`: the values `ok` accepts, otherwise `message`. */
export const flickFromCheck = <A>(ok: (a: A) => boolean, message: string): Optic.Prism<A, A> =>
  Optic.fromChecks(Schema.makeFilter<A>((a) => ok(a) || message));

/** `Optic.get_result optic self`. */
export const flickGetResult = <S, A>(optic: Optic.Optional<S, A>, self: S): Result.Result<A, SchemaError> =>
  toError(Optic.getResult(self, optic));

/** `Optic.replace_result optic value self`. */
export const flickReplaceResult = <S, A>(optic: Optic.Optional<S, A>, value: A, self: S): Result.Result<S, SchemaError> =>
  toError(Optic.replaceResult(self, optic, value));

/** `Optic.compose that self`: the optic of `self`, then `that`. */
export const flickCompose = <S, A, B>(that: Optic.Optional<A, B>, self: Optic.Optional<S, A>): Optic.Optional<S, B> =>
  self.compose(that);

/** The casts of `Optic`: an Effect optic already is every weaker kind. */
export const flickAs = <A>(optic: A): A => optic;
