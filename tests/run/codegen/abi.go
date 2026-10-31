package main

import (
	"math/big"

	"flick.app/flickrt"
)

// raw is the value as it is.
func raw(_ *flickrt.Fiber, value flickrt.Value) flickrt.Value { return value }

// kind is JS `typeof` of the value.
func kind(_ *flickrt.Fiber, value flickrt.Value) flickrt.Value {
	switch value.(type) {
	case int32, float64:
		return "number"
	case string:
		return "string"
	case bool:
		return "boolean"
	case *big.Int:
		return "bigint"
	}
	return "object"
}
