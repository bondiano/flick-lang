(doc_comment) @comment.doc
(line_comment) @comment
(block_comment) @comment

[(string) (raw_string) (multiline_string)] @string
(number) @number
(boolean) @boolean

(constructor) @type
(member) @property
(identifier) @variable

; Record field labels (`{ host = "localhost"; orelse = 1 }`)
(braces (identifier) @property . (operator) @_eq
  (#eq? @_eq "="))
(braces (keyword) @property . (operator) @_eq
  (#eq? @_eq "="))
(braces (constructor) @property . (operator) @_eq
  (#eq? @_eq "="))

(operator) @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
(operator ["," ";"]) @punctuation.delimiter
