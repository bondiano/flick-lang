// Token-level grammar for highlighting: Flick's layout rules live in the
// compiler, so this tree only nests brackets and keeps every token flat.

const KEYWORDS = [
  'module', 'exposing', 'import', 'as', 'let', 'rec', 'fun', 'match', 'with',
  'try', 'if', 'then', 'else', 'task', 'await', 'scope', 'provide', 'ignore',
  'type', 'opaque', 'refined', 'where', 'kind', 'error', 'service', 'class',
  'instance', 'deriving', 'foreign', 'from', 'forall', 'not', 'none',
  'schema', 'codec', 'sql', 'ui', 'cli', 'in', 'when', 'do',
];

const OPERATORS = [
  '..', '->', '=>', '==', '|>', '||', '&&', '!=', '<=', '>=', '++', '**',
  ',', ';', ':', '.', '=', '|', '<', '>', '+', '-', '*', '/', '%', '^',
];

module.exports = grammar({
  name: 'flick',

  externals: $ => [$.block_comment, $.raw_string, $.multiline_string],

  extras: $ => [/\s/, $.doc_comment, $.line_comment, $.block_comment],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._token),

    _token: $ => choice(
      $.parens,
      $.brackets,
      $.braces,
      $.keyword,
      $.boolean,
      $.member,
      $.identifier,
      $.constructor,
      $.number,
      $.string,
      $.raw_string,
      $.multiline_string,
      $.operator,
    ),

    parens: $ => seq('(', repeat($._token), ')'),
    brackets: $ => seq('[', repeat($._token), ']'),
    braces: $ => seq('{', repeat($._token), '}'),

    keyword: _ => choice(...KEYWORDS),
    boolean: _ => choice('true', 'false'),
    operator: _ => choice(...OPERATORS),

    // `.name` is a field or module member even when it spells a keyword
    member: _ => token(seq('.', /[a-z_][A-Za-z0-9_]*/)),

    identifier: _ => /[a-z_][A-Za-z0-9_]*/,
    constructor: _ => /[A-Z][A-Za-z0-9_]*/,

    number: _ => token(choice(
      /0[xX][0-9a-fA-F_]+n?/,
      /0[oO][0-7_]+n?/,
      /0[bB][01_]+n?/,
      /[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9_]+)?n?/,
    )),

    string: _ => token(seq('"', repeat(choice(/[^"\\\n]/, /\\./)), '"')),

    doc_comment: _ => token(seq('///', optional(seq(/[^/\n]/, /[^\n]*/)))),
    line_comment: _ => token(seq('//', /[^\n]*/)),
  },
});
