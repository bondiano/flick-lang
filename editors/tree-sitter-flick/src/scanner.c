// Tokens a regular expression cannot express: nested `/* */` comments, raw
// strings closed by as many `#` as they opened with, `"""` strings, and
// `proof`, a keyword only first on its line and before a name.

#include "tree_sitter/parser.h"

enum TokenType { BLOCK_COMMENT, RAW_STRING, MULTILINE_STRING, PROOF };

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

static bool is_lower(int32_t c) { return (c >= 'a' && c <= 'z') || c == '_'; }

// `proof name` or `law name`, the token ending after `word`.
static bool proof(TSLexer *lexer, const char *word) {
  for (const char *c = word; *c; c++) {
    if (lexer->lookahead != *c) return false;
    advance(lexer);
  }
  lexer->mark_end(lexer);
  if (lexer->lookahead != ' ' && lexer->lookahead != '\t') return false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
  if (!is_lower(lexer->lookahead)) return false;
  lexer->result_symbol = PROOF;
  return true;
}

bool tree_sitter_flick_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
  // only blanks since the line began: either a line break was skipped, or as
  // many blanks were skipped as the column of what follows them
  bool newline = false;
  unsigned blanks = 0;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' || lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    if (lexer->lookahead == '\n') {
      newline = true;
    } else {
      blanks++;
    }
    lexer->advance(lexer, true);
  }
  switch (lexer->lookahead) {
    case 'p':
      return valid_symbols[PROOF] && (newline || lexer->get_column(lexer) == blanks) && proof(lexer, "proof");
    case 'l':
      return valid_symbols[PROOF] && (newline || lexer->get_column(lexer) == blanks) && proof(lexer, "law");
    case '/': return valid_symbols[BLOCK_COMMENT] && block_comment(lexer);
    case 'r': return valid_symbols[RAW_STRING] && raw_string(lexer);
    case '"': return valid_symbols[MULTILINE_STRING] && multiline_string(lexer);
    default: return false;
  }
}
