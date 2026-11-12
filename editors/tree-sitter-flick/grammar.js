// Token-level grammar for highlighting: Flick's layout rules live in the
// compiler, so this tree only nests brackets and keeps every token flat.

const KEYWORDS = [
  'module', 'exposing', 'import', 'as', 'let', 'mut', 'rec', 'assert', 'fun', 'match', 'with',
  'try', 'if', 'then', 'else', 'task', 'await', 'scope', 'provide', 'ignore',
  'todo', 'echo',
  'type', 'opaque', 'refined', 'where', 'kind', 'error', 'service', 'class',
  'instance', 'deriving', 'forall', 'not', 'none',
  'ui', 'in', 'when', 'for', 'yield', 'do', 'break', 'continue',
];

const OPERATORS = [
  '...', '..<', '..', '->', '<-', '=>', '==', '|>', '||', '&&', '!=', '<=', '>=', '++', '**',
  ',', ';', ':', '.', '=', '|', '<', '>', '+', '-', '*', '/', '%', '^', '?',
];

module.exports = grammar({
  name: 'flick',

  externals: $ => [$.block_comment, $.raw_string, $.multiline_string],

  extras: $ => [/[ \t\r]/, $.doc_comment, $.line_comment, $.block_comment],

  word: $ => $._identifier,

  conflicts: $ => [[$._loop, $.identifier], [$._while, $.identifier]],

  rules: {
    source_file: $ => repeat($._token),

    _token: $ => choice(
      $._newline,
      $._loop,
      $._while,
      $.attribute,
      $._foreign,
      $._from,
      $._component,
      $._plugin,
      $._server_fn,
      $._private_module,
      $._type_refinement_and,
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

    _newline: _ => '\n',
    _loop: $ => prec.dynamic(1, seq(alias('loop', $.keyword), $.braces)),
    _while: $ => prec.dynamic(1, seq(alias('while', $.keyword), repeat1($._condition_token), alias('do', $.keyword))),
    _condition_token: $ => choice(
      $.parens, $.brackets, $.braces, $.identifier, $.constructor, $.boolean,
      $.number, $.string, $.raw_string, $.multiline_string, $.member,
      alias(choice(...KEYWORDS.filter(k => !['do', 'let', 'module', 'import', 'type', 'service', 'class', 'instance'].includes(k))), $.keyword),
      alias(choice(...OPERATORS.filter(k => ![',', ';', '='].includes(k))), $.operator),
    ),

    keyword: _ => choice(...KEYWORDS),

    // the mode after `foreign` and `global` after `from` are keywords only there
    _foreign: $ => seq(
      alias('foreign', $.keyword),
      optional(alias(choice('pure', 'sync', 'async', 'effect', 'blocked'), $.keyword)),
    ),
    _from: $ => seq(alias('from', $.keyword), optional(alias('global', $.keyword))),
    // `component name` declares a component; elsewhere the word is a name
    _component: $ => seq(alias('component', $.keyword), $.identifier),
    // `plugin head` in an import's `exposing`, and `server fn`
    _plugin: $ => seq(alias('plugin', $.keyword), $.identifier),
    _server_fn: $ => seq(alias('server', $.keyword), alias('fn', $.keyword)),
    _private_module: $ => seq(alias('private', $.keyword), alias('module', $.keyword)),
    _type_refinement_and: $ => seq(alias('and', $.keyword), alias('type', $.keyword)),
    boolean: _ => choice('true', 'false'),
    // `@lang int`: the attribute's name; its arguments are plain identifiers and strings
    attribute: _ => token(seq('@', /[a-z_][A-Za-z0-9_]*/)),
    operator: _ => choice(...OPERATORS),

    // `.name` is a field or module member even when it spells a keyword
    member: _ => token(seq('.', /[a-z_][A-Za-z0-9_]*/)),

    _identifier: _ => /[a-z_][A-Za-z0-9_]*/,
    identifier: $ => choice($._identifier, 'loop', 'while'),
    constructor: _ => /[A-Z][A-Za-z0-9_]*/,

    number: _ => token(choice(
      /0[xX][0-9a-fA-F_]+n?/,
      /0[oO][0-7_]+n?/,
      /0[bB][01_]+n?/,
      /[0-9][0-9_]*(\.[0-9][0-9_]*)?([eE][+-]?[0-9_]+)?n?/,
    )),

    // a hole `\{…}` holds tokens, so the string is a node, not one token
    string: $ => seq(
      '"',
      repeat(choice($._string_content, $._escape_sequence, $.interpolation)),
      token.immediate('"'),
    ),
    _string_content: _ => token.immediate(prec(1, /[^"\\\n]+/)),
    _escape_sequence: _ => token.immediate(/\\(u\{[0-9a-fA-F]*\}|[^{\n])/),
    interpolation: $ => seq(token.immediate('\\{'), repeat($._token), '}'),

    doc_comment: _ => token(seq('///', optional(seq(/[^/\n]/, /[^\n]*/)))),
    line_comment: _ => token(seq('//', /[^\n]*/)),
  },
});
