package main

import (
	"unicode/utf8"

	"flick.app/flickrt"
)

// wellFormed is whether every text of a value is well-formed Unicode: a
// string holds a lone surrogate as invalid UTF-8 (WTF-8).
func wellFormed(v flickrt.Value) bool {
	switch x := v.(type) {
	case string:
		return utf8.ValidString(x)
	case *flickrt.Data:
		return all(x.Fields)
	case *flickrt.Record:
		return all(flickrt.RecordValues(x).([]flickrt.Value))
	case []flickrt.Value:
		return all(x)
	}
	return true
}

func all(vs []flickrt.Value) bool {
	for _, v := range vs {
		if !wellFormed(v) {
			return false
		}
	}
	return true
}

// encodable is whether every text of the page is well-formed Unicode, which a URL can carry.
func encodable(_ *flickrt.Fiber, value flickrt.Value) flickrt.Value { return wellFormed(value) }
