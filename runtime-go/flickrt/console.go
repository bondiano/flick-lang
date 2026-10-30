package flickrt

// Console as Effect's default console: a line to stdout.
//
// shortcut: only `log` of the default console; bind the others, and `with_console`, as programs use them on Go
func ConsoleLog(_ *Fiber, message Value) Value { return StdoutLine(message) }
