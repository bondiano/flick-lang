package flickrt

// Effect's `Optic`. Every kind is one optic: what it reads, maybe failing,
// and what it writes back into the whole, maybe failing; an iso and a lens
// read without failing, an iso and a prism write without the whole. A
// weaker kind is the same optic, as Effect's are.

type optic struct {
	get     func(fb *Fiber, s Value) (Value, string) // "" when it read
	replace func(fb *Fiber, a, s Value) (Value, string)
	set     func(fb *Fiber, a Value) Value // an iso's and a prism's: the whole from the part
}

func opticOf(v Value) *optic { return v.(*optic) }

// resultString reads a Flick `Result String a`: the value, or the message.
func resultString(r Value) (Value, string) {
	d := r.(*Data)
	if d.Tag == "Success" {
		return d.Fields[0], ""
	}
	return nil, d.Fields[0].(string)
}

func OpticMakeIso(get, set Value) Value {
	o := &optic{
		get: func(fb *Fiber, s Value) (Value, string) { return Apply(fb, get, s), "" },
		set: func(fb *Fiber, a Value) Value { return Apply(fb, set, a) },
	}
	o.replace = func(fb *Fiber, a, _ Value) (Value, string) { return o.set(fb, a), "" }
	return o
}

func OpticMakeLens(get, replace Value) Value {
	return &optic{
		get:     func(fb *Fiber, s Value) (Value, string) { return Apply(fb, get, s), "" },
		replace: func(fb *Fiber, a, s Value) (Value, string) { return Apply(fb, Apply(fb, replace, a), s), "" },
	}
}

func OpticMakePrism(get, set Value) Value {
	o := &optic{
		get: func(fb *Fiber, s Value) (Value, string) { return resultString(Apply(fb, get, s)) },
		set: func(fb *Fiber, a Value) Value { return Apply(fb, set, a) },
	}
	o.replace = func(fb *Fiber, a, _ Value) (Value, string) { return o.set(fb, a), "" }
	return o
}

func OpticMakeOptional(get, replace Value) Value {
	return &optic{
		get: func(fb *Fiber, s Value) (Value, string) { return resultString(Apply(fb, get, s)) },
		replace: func(fb *Fiber, a, s Value) (Value, string) {
			return resultString(Apply(fb, Apply(fb, replace, a), s))
		},
	}
}

// prism is a prism of a check on the whole, which it is unchanged.
func prism(get func(s Value) (Value, string), set func(a Value) Value) Value {
	o := &optic{
		get: func(_ *Fiber, s Value) (Value, string) { return get(s) },
		set: func(_ *Fiber, a Value) Value { return set(a) },
	}
	o.replace = func(fb *Fiber, a, _ Value) (Value, string) { return o.set(fb, a), "" }
	return o
}

func OpticFromCheck(ok, message Value) Value {
	return &optic{
		get: func(fb *Fiber, s Value) (Value, string) {
			if Apply(fb, ok, s).(bool) {
				return s, ""
			}
			return nil, message.(string)
		},
		replace: func(fb *Fiber, a, _ Value) (Value, string) { return a, "" },
		set:     func(_ *Fiber, a Value) Value { return a },
	}
}

func OpticId() Value {
	return OpticMakeIso(Fn(func(_ *Fiber, s Value) Value { return s }), Fn(func(_ *Fiber, a Value) Value { return a }))
}

// tagged is the prism of a constructor with one field; `message` is the
// expectation of Effect's prism.
func tagged(tag string, message string, make func(Value) Value) Value {
	return prism(func(s Value) (Value, string) {
		if d := s.(*Data); d.Tag == tag {
			return d.Fields[0], ""
		}
		return nil, message
	}, make)
}

func OpticSome() Value    { return tagged("Some", "expected a Some value", Some) }
func OpticSuccess() Value { return tagged("Success", "expected a Result.Success value", Ok) }
func OpticFailure() Value { return tagged("Failure", "expected a Result.Failure value", Err) }

func OpticNone() Value {
	return prism(func(s Value) (Value, string) {
		if s.(*Data).Tag == "None" {
			return Unit, ""
		}
		return nil, "expected a None value"
	}, func(Value) Value { return None })
}

func OpticGet(fb *Fiber, o, s Value) Value {
	v, _ := opticOf(o).get(fb, s)
	return v
}

func opticResult(v Value, problem string) Value {
	if problem != "" {
		return Err(schemaErrorOf(fail(problem)))
	}
	return Ok(v)
}

func OpticGetResult(fb *Fiber, o, s Value) Value { return opticResult(opticOf(o).get(fb, s)) }

func OpticSet(fb *Fiber, o, a Value) Value { return opticOf(o).set(fb, a) }

func OpticReplaceResult(fb *Fiber, o, a, s Value) Value {
	return opticResult(opticOf(o).replace(fb, a, s))
}

// OpticReplace writes the part into the whole, or leaves the whole as it
// is when the optic cannot.
func OpticReplace(fb *Fiber, o, a, s Value) Value {
	if out, problem := opticOf(o).replace(fb, a, s); problem == "" {
		return out
	}
	return s
}

func OpticModify(fb *Fiber, o, f, s Value) Value {
	a, problem := opticOf(o).get(fb, s)
	if problem != "" {
		return s
	}
	return OpticReplace(fb, o, Apply(fb, f, a), s)
}

// OpticGetAll is what a traversal focuses, none when it cannot read.
func OpticGetAll(fb *Fiber, o, s Value) Value {
	xs, problem := opticOf(o).get(fb, s)
	if problem != "" {
		return []Value{}
	}
	return xs
}

func OpticModifyAll(fb *Fiber, o, f, s Value) Value {
	return OpticModify(fb, o, Fn(func(fb *Fiber, xs Value) Value { return ListMap(fb, f, xs) }), s)
}

// OpticCompose is the optic of `self`, then `that`.
func OpticCompose(that, self Value) Value {
	outer, inner := opticOf(self), opticOf(that)
	out := &optic{
		get: func(fb *Fiber, s Value) (Value, string) {
			a, problem := outer.get(fb, s)
			if problem != "" {
				return nil, problem
			}
			return inner.get(fb, a)
		},
		replace: func(fb *Fiber, b, s Value) (Value, string) {
			if outer.set != nil && inner.set != nil {
				return outer.set(fb, inner.set(fb, b)), ""
			}
			a, problem := outer.get(fb, s)
			if problem != "" {
				return nil, problem
			}
			a2, problem := inner.replace(fb, b, a)
			if problem != "" {
				return nil, problem
			}
			return outer.replace(fb, a2, s)
		},
	}
	if outer.set != nil && inner.set != nil {
		out.set = func(fb *Fiber, b Value) Value { return outer.set(fb, inner.set(fb, b)) }
	}
	return out
}

// OpticAs is a cast to a weaker kind: the same optic.
func OpticAs(o Value) Value { return o }
