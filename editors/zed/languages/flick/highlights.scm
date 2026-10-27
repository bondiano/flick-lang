(doc_comment) @comment.doc
(line_comment) @comment
(block_comment) @comment

[(string) (raw_string) (multiline_string)] @string
(number) @number
(boolean) @boolean

(keyword) @keyword
((keyword) @_not
  (#eq? @_not "not")) @keyword.operator
(operator) @operator

(constructor) @type
(member) @property
(identifier) @variable

; Keywords are plain `keyword` nodes (aliased tokens), so select their text with
; `#eq?`; an anonymous child such as `(keyword "component")` is a query error
; that disables all highlighting. `cli` clause words stay `@variable`: capturing
; them as `@keyword` overlaps the generic identifier rule.
(_ (keyword) @_let . (identifier) @function
  (#eq? @_let "let"))
(_ (keyword) @_let . (keyword) @_rec . (identifier) @function
  (#eq? @_let "let")
  (#eq? @_rec "rec"))
(_ (keyword) @_foreign . (keyword) . (identifier) @function
  (#eq? @_foreign "foreign"))
(_ (keyword) @_component . (identifier) @function
  (#eq? @_component "component"))

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
(operator ["," ";"]) @punctuation.delimiter
