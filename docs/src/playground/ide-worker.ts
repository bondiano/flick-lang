// The shapes the editor worker answers with, which its client and the
// CodeMirror extension both read.

/** A name the checker resolved and what it is. */
export interface Token {
  /** UTF-16 offset of the name's start in the module. */
  readonly from: number;
  /** UTF-16 offset of its end. */
  readonly to: number;
  /** Its kind, one of `Tokens.types`. */
  readonly type: string;
}

/** The names the checker resolved, by their index in `types`. */
export interface Tokens {
  readonly types: ReadonlyArray<string>;
  readonly tokens: ReadonlyArray<Token>;
}

/** A type the code leaves out, after the UTF-16 offset `at`. */
export interface Hint {
  readonly at: number;
  readonly label: string;
}

/** The signature of the call at the cursor; `params` index into `label`. */
export interface Signature {
  readonly label: string;
  readonly params: ReadonlyArray<{ readonly from: number; readonly to: number }>;
  readonly active: number;
}

/** A rewrite, its edits in UTF-16 offsets of the module it was asked about. */
export interface Action {
  readonly title: string;
  readonly kind: string;
  readonly edits: ReadonlyArray<{ readonly from: number; readonly to: number; readonly text: string }>;
}
