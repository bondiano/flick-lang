// Tokens a regular expression cannot express: nested `/* */` comments, raw
// strings closed by as many `#` as they opened with, and `"""` strings.

#include "tree_sitter/parser.h"

enum TokenType { BLOCK_COMMENT, RAW_STRING, MULTILINE_STRING };

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

static bool raw_string(TSLexer *lexer) {
  advance(lexer);
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
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' || lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    lexer->advance(lexer, true);
  }
  switch (lexer->lookahead) {
    case '/': return valid_symbols[BLOCK_COMMENT] && block_comment(lexer);
    case 'r': return valid_symbols[RAW_STRING] && raw_string(lexer);
    case '"': return valid_symbols[MULTILINE_STRING] && multiline_string(lexer);
    default: return false;
  }
}
