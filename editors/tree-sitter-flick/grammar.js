// Token-level grammar for highlighting: Flick's layout rules live in the
// compiler, so this tree only nests brackets and keeps every token flat.

const KEYWORDS = [
  'module', 'exposing', 'import', 'as', 'let', 'mut', 'rec', 'assert', 'fun', 'match', 'with',
  'try', 'catch', 'if', 'then', 'else', 'task', 'await', 'lazy', 'scope', 'provide', 'ignore',
  'todo', 'echo',
  'type', 'opaque', 'refined', 'where', 'kind', 'error', 'service', 'class',
  'instance', 'deriving', 'forall', 'not', 'orelse', 'none',
  'ui', 'in', 'when', 'for', 'yield', 'do', 'break', 'continue',
];

const OPERATORS = [
  '...', '..<', '..', '->', '<-', '=>', '==', '|>', '<|', '>>', '<<', '||', '&&', '!=', '<=', '>=', '++', '**',
  ',', ';', ':', '.', '=', '|', '<', '>', '+', '-', '*', '/', '%', '^', '?', '~',
];

module.exports = grammar({
  name: 'flick',

  // `proof` or `law` first on its line before a name: the scanner sees the line
  externals: $ => [$.block_comment, $.raw_string, $.multiline_string, $._proof],

  extras: $ => [/[ \t\r]/, $.doc_comment, $.line_comment, $.block_comment],

  word: $ => $._identifier,

  conflicts: $ => [
    [$._loop, $.identifier], [$._defer, $.identifier], [$._result, $.identifier], [$._while, $.identifier], [$._case, $.identifier], [$._hook, $.identifier],
    [$._fixture, $.identifier], [$._case], [$._fixture],
    [$._await_signature, $.keyword], [$._await_definition, $.keyword],
  ],

  rules: {
    source_file: $ => repeat($._token),

    _token: $ => choice(
      $._newline,
      $._loop,
      $._defer,
      $._result,
      $._while,
      $._case,
      $._hook,
      $._fixture,
      $._each,
      $._await_signature,
      $._await_definition,
      alias($._proof, $.keyword),
      $.attribute,
      $.label,
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
    _defer: $ => prec.dynamic(1, seq(alias(choice('defer', 'errdefer'), $.keyword), $.braces)),
    _result: $ => prec.dynamic(1, seq(alias('result', $.keyword), $.braces)),
    _while: $ => prec.dynamic(1, seq(alias('while', $.keyword), repeat1($._condition_token), alias('do', $.keyword))),
    // `suite "name"` and `test "name"`, maybe `.skip` or `.only`, `parallel`
    // before the name and `timeout` after it, start a case of the test DSL, a
    // hook's word before `{` a hook of a suite, and `fixture`, maybe `shared`
    // and `concurrent`, before a name and `=` a fixture; elsewhere the words
    // are names
    _case: $ => prec.dynamic(1, seq(
      alias(choice('suite', 'test'), $.keyword),
      optional(alias(choice('.skip', '.only'), $.keyword)),
      optional(alias('parallel', $.keyword)),
      $.string,
      optional($._timeout),
    )),
    _timeout: $ => prec.dynamic(1, alias('timeout', $.keyword)),
    _hook: $ => prec.dynamic(1, seq(
      alias(choice('before_all', 'before_each', 'after_each', 'after_all'), $.keyword),
      $.braces,
    )),
    // `test.each` starts the cases of a test's rows, whatever its rows are
    _each: $ => prec.dynamic(1, seq(alias('test', $.keyword), alias('.each', $.keyword))),
    _fixture: $ => prec.dynamic(1, seq(
      alias('fixture', $.keyword),
      optional(alias('shared', $.keyword)),
      optional(alias('concurrent', $.keyword)),
      $.identifier,
      alias('=', $.operator),
    )),
    _condition_token: $ => choice(
      $.parens, $.brackets, $.braces, $.label, $.identifier, $.constructor, $.boolean,
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
    label: _ => token(seq("'", /[a-z_][A-Za-z0-9_]*/)),
    operator: _ => choice(...OPERATORS),

    // `.name` is a field or module member even when it spells a keyword
    member: _ => token(seq('.', /[a-z_][A-Za-z0-9_]*/)),

    _identifier: _ => /[a-z_][A-Za-z0-9_]*/,
    identifier: $ => choice(
      $._identifier, 'loop', 'defer', 'errdefer', 'result', 'while', 'suite', 'test', 'timeout', 'before_all', 'before_each', 'after_each', 'after_all',
      'fixture', 'shared', 'concurrent', 'parallel',
    ),
    constructor: _ => /[A-Z][A-Za-z0-9_]*/,

    number: _ => token(choice(
      /0[xX][0-9a-fA-F_]+(n|[iu](8|16|32|64))?/,
      /0[oO][0-7_]+(n|[iu](8|16|32|64))?/,
      /0[bB][01_]+(n|[iu](8|16|32|64))?/,
      /[0-9][0-9_]*(\.[0-9][0-9_]*([eE][+-]?[0-9_]+)?|[eE][+-]?[0-9_]+)(f32)?/,
      /[0-9][0-9_]*(n|[iu](8|16|32|64)|f32)?/,
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

    // The keyword is an identifier in a method header. Dotted member names
    // already accept any word.
    _await_signature: $ => prec.dynamic(1, seq(alias('await', $.identifier), alias(':', $.operator))),
    _await_definition: $ => prec.dynamic(1, seq(alias('let', $.keyword), alias('await', $.identifier))),
  },
});
