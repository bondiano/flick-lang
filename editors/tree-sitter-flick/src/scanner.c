// Tokens a regular expression cannot express: nested `/* */` comments, raw
// strings closed by as many `#` as they opened with, `"""` strings, and the
// `setup` and `render` sections of a component, which start a line.

#include "tree_sitter/parser.h"

enum TokenType { BLOCK_COMMENT, RAW_STRING, MULTILINE_STRING, SECTION };

void *tree_sitter_flick_external_scanner_create(void) { return NULL; }
void tree_sitter_flick_external_scanner_destroy(void *payload) {}
unsigned tree_sitter_flick_external_scanner_serialize(void *payload, char *buffer) { return 0; }
void tree_sitter_flick_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {}

static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }

static bool block_comment(TSLexer *lexer) {
  advance(lexer);
  if (lexer->lookahead != '*') return false;
  advance(lexer);
  unsigned depth = 1;
  while (depth > 0 && !lexer->eof(lexer)) {
    if (lexer->lookahead == '/') {
      advance(lexer);
      if (lexer->lookahead == '*') { advance(lexer); depth++; }
    } else if (lexer->lookahead == '*') {
      advance(lexer);
      if (lexer->lookahead == '/') { advance(lexer); depth--; }
    } else {
      advance(lexer);
    }
  }
  lexer->result_symbol = BLOCK_COMMENT;
  return true;
}

static bool is_word_char(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_';
}

// The rest of `word` (its first letter consumed): a section when the line
// ends after it, or, for `render`, when `ui { … }` follows on its line.
static bool section(TSLexer *lexer, const char *rest, bool render) {
  for (; *rest; rest++) {
    if (lexer->lookahead != *rest) return false;
    advance(lexer);
  }
  if (is_word_char(lexer->lookahead)) return false;
  lexer->mark_end(lexer);
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
  bool ends = lexer->eof(lexer) || lexer->lookahead == '\n' || lexer->lookahead == '\r';
  if (!ends && render && lexer->lookahead == 'u') {
    advance(lexer);
    if (lexer->lookahead != 'i') return false;
    advance(lexer);
    ends = !is_word_char(lexer->lookahead);
  }
  if (ends) lexer->result_symbol = SECTION;
  return ends;
}

// Called after the `r`.
static bool raw_string(TSLexer *lexer) {
  unsigned hashes = 0;
  while (lexer->lookahead == '#') { advance(lexer); hashes++; }
  if (lexer->lookahead != '"') return false;
  advance(lexer);
  while (!lexer->eof(lexer)) {
    if (lexer->lookahead == '"') {
      advance(lexer);
      unsigned closing = 0;
      while (closing < hashes && lexer->lookahead == '#') { advance(lexer); closing++; }
      if (closing == hashes) break;
    } else {
      advance(lexer);
    }
  }
  lexer->result_symbol = RAW_STRING;
  return true;
}

// Called at the first `"`; leaves a plain `"…"` string to the grammar.
static bool multiline_string(TSLexer *lexer) {
  for (int i = 0; i < 3; i++) {
    if (lexer->lookahead != '"') return false;
    advance(lexer);
  }
  unsigned quotes = 0;
  while (quotes < 3 && !lexer->eof(lexer)) {
    if (lexer->lookahead == '"') {
      quotes++;
    } else {
      quotes = 0;
      if (lexer->lookahead == '\\') advance(lexer);
    }
    advance(lexer);
  }
  lexer->result_symbol = MULTILINE_STRING;
  return true;
}

bool tree_sitter_flick_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  bool line_start = false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' || lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    line_start |= lexer->lookahead == '\n';
    lexer->advance(lexer, true);
  }
  bool sections = line_start && valid_symbols[SECTION];
  switch (lexer->lookahead) {
    case '/': return valid_symbols[BLOCK_COMMENT] && block_comment(lexer);
    case 'r':
      advance(lexer);
      if (lexer->lookahead == '#' || lexer->lookahead == '"') return valid_symbols[RAW_STRING] && raw_string(lexer);
      return sections && section(lexer, "ender", true);
    case 's':
      advance(lexer);
      return sections && section(lexer, "etup", false);
    case '"': return valid_symbols[MULTILINE_STRING] && multiline_string(lexer);
    default: return false;
  }
}
