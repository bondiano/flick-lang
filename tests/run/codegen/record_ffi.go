package main

import (
	"strings"

	"flick.app/flickrt"
)

// keysOf joins the own keys of a Flick record / JSON object / ReadonlyRecord in stored order.
func keysOf(_ *flickrt.Fiber, value flickrt.Value) flickrt.Value {
	entries := flickrt.List(flickrt.RecordToEntries(value))
	keys := make([]string, len(entries))
	for i, entry := range entries {
		keys[i] = entry.([]flickrt.Value)[0].(string)
	}
	return strings.Join(keys, ",")
}
