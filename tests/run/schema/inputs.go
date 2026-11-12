package main

// Inputs that JSON text cannot spell, and a Standard Schema caller.

import (
	"math"
	"strconv"

	"flick.app/flickrt"
)

func cyclic(_ *flickrt.Fiber) flickrt.Value {
	items := make([]flickrt.Value, 1)
	node := flickrt.Rec("_tag", "Branch", "_0", items)
	items[0] = node
	return node
}

func shared(_ *flickrt.Fiber) flickrt.Value {
	leaf := flickrt.Rec("_tag", "Leaf", "_0", 1.0)
	return flickrt.Rec("_tag", "Branch", "_0", []flickrt.Value{leaf, leaf})
}

func notANumber(_ *flickrt.Fiber) flickrt.Value { return math.NaN() }

// plain is a decoded value as the JS object JSON.stringify sees: a
// constructor's fields under `_0`, `_1`, … after its `_tag`.
func plain(v flickrt.Value) flickrt.Value {
	switch x := v.(type) {
	case flickrt.Constructed:
		fields := []flickrt.Value{"_tag", x.TagName()}
		for i, f := range x.Fields() {
			fields = append(fields, "_"+strconv.Itoa(i), plain(f))
		}
		return flickrt.Rec(fields...)
	case []flickrt.Value:
		out := make([]flickrt.Value, len(x))
		for i, e := range x {
			out[i] = plain(e)
		}
		return out
	}
	return v
}

func validate(_ flickrt.Context, standard, value flickrt.Value) flickrt.Value {
	r := standard.(*flickrt.StandardSchema).Validate(value).(*flickrt.Record)
	if flickrt.RecordKeys(r).([]flickrt.Value)[0] == "issues" {
		issue := r.Get("issues").([]flickrt.Value)[0].(*flickrt.Record)
		return "issues: " + issue.Get("message").(string)
	}
	return "value: " + flickrt.JsonStringify(plain(r.Get("value"))).(string)
}
