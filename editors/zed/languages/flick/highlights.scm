(doc_comment) @comment.doc
(line_comment) @comment
(block_comment) @comment

[(string) (raw_string) (multiline_string)] @string
(interpolation ["\\{" "}"] @punctuation.special)
(number) @number
(boolean) @boolean

(keyword) @keyword
((keyword) @_not
  (#eq? @_not "not")) @keyword.operator
(operator) @operator

(attribute) @attribute
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
; the head of a plugin's blocks, as an import exposes it
(_ (keyword) @_plugin . (identifier) @function.macro
  (#eq? @_plugin "plugin"))

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
(operator ["," ";"]) @punctuation.delimiter
