(doc_comment) @comment.doc
(line_comment) @comment
(block_comment) @comment

[(string) (raw_string) (multiline_string)] @string
(interpolation ["\\{" "}"] @punctuation.special)
(number) @number
(boolean) @boolean

(keyword) @keyword
((keyword) @_op
  (#any-of? @_op "not" "orelse")) @keyword.operator
(operator) @operator

(attribute) @attribute
(label) @label
(constructor) @type
(member) @property
(identifier) @variable

; Keywords are plain `keyword` nodes (aliased tokens), so select their text with
; `#eq?`; an anonymous child such as `(keyword "component")` is a query error
; that disables all highlighting. `cli` clause words stay `@variable`: capturing
; them as `@keyword` overlaps the generic identifier rule.
(_ (keyword) @_let . (identifier) @function
  (#eq? @_let "let"))
(_ (identifier) @function . (operator) @_colon
  (#eq? @function "await")
  (#eq? @_colon ":"))
(_ (keyword) @_let . (keyword) @_rec . (identifier) @function
  (#eq? @_let "let")
  (#eq? @_rec "rec"))
(_ (keyword) @_foreign . (keyword) . (identifier) @function
  (#eq? @_foreign "foreign"))
(_ (keyword) @_component . (identifier) @function
  (#eq? @_component "component"))
; captured as `@keyword` again, so the word keeps its colour whichever capture wins
(_ (keyword) @keyword . (identifier) @function
  (#any-of? @keyword "proof" "law"))
; an entry of a contract written on the line of its `where`
(_ (keyword) @_where . (identifier) @keyword
  (#eq? @_where "where")
  (#any-of? @keyword "require" "ensure"))
; the head of a plugin's blocks, as an import exposes it
(_ (keyword) @_plugin . (identifier) @function.macro
  (#eq? @_plugin "plugin"))

; `orelse` is allowed as a record label in `{ orelse = … }` and `{ orelse : … }`
(braces (keyword) @property . (operator) @_assign
  (#eq? @property "orelse")
  (#any-of? @_assign "=" ":"))

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
(operator ["," ";"]) @punctuation.delimiter
