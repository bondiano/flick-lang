package main

import (
	"flick.app/flickrt"
	"fmt"
	"math/big"
)

type policyState struct {
	steps    int64
	provider flickrt.Value
}

var policyStates = map[string]*policyState{}

func policyMake(label string, owner flickrt.Value) flickrt.Value {
	if _, exists := policyStates[label]; exists {
		panic(label + ": factory called twice")
	}
	state := &policyState{}
	policyStates[label] = state
	var work flickrt.Eff
	work = func(fb *flickrt.Fiber) flickrt.Value {
		provider := label == "row" && state.steps == 0
		expected := owner
		if provider {
			expected = state.provider
		}
		if flickrt.ScopeCurrent(fb, flickrt.Unit) != expected || (provider && expected == owner) {
			panic(fmt.Sprintf("%s: wrong raw owner at %d", label, state.steps))
		}
		state.steps++
		if label == "idle" {
			return flickrt.Unit
		}
		return work
	}
	return work
}

func inspect(_ *flickrt.Fiber, labelValue, expectedValue flickrt.Value) flickrt.Value {
	label := labelValue.(string)
	state, ok := policyStates[label]
	if !ok || big.NewInt(state.steps).Cmp(expectedValue.(*big.Int)) != 0 {
		panic(label + ": wrong step count")
	}
	return fmt.Sprintf("%s:%d", label, state.steps)
}

func makeNonregular(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return policyMake("nonregular", owner)
}
func makeRow(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value { return policyMake("row", owner) }
func makeProjected(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return policyMake("projected", owner)
}
func makeLongId(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return policyMake("longid", owner)
}
func makeChain(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return policyMake("chain", owner)
}
func makeHigher(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value {
	return policyMake("higher", owner)
}
func makeIdle(_ *flickrt.Fiber, owner flickrt.Value) flickrt.Value { return policyMake("idle", owner) }

func markProvider(_ *flickrt.Fiber, label flickrt.Value) flickrt.Value {
	return flickrt.Eff(func(fb *flickrt.Fiber) flickrt.Value {
		policyStates[label.(string)].provider = flickrt.ScopeCurrent(fb, flickrt.Unit)
		return flickrt.Unit
	})
}
