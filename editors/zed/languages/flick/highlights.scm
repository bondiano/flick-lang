(doc_comment) @comment.doc
(line_comment) @comment
(block_comment) @comment

[(string) (raw_string) (multiline_string)] @string
(number) @number
(boolean) @boolean

(keyword) @keyword
(keyword "not") @keyword.operator
(operator) @operator

(constructor) @type
(member) @property
(identifier) @variable

(_ (keyword "let") . (identifier) @function)
(_ (keyword "let") . (keyword "rec") . (identifier) @function)

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
(operator ["," ";"]) @punctuation.delimiter
