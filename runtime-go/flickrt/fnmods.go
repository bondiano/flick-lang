package flickrt

// Effect's `Boolean`, `Predicate`, `Filter` and `Function` functions, in
// the order Flick passes the arguments: data-last, so `and that self` is
// `self && that`. A predicate, a filter and a lazy argument are type
// aliases of functions, so a function returning one takes its argument
// last: `Predicate.and that self a`.

func BoolNot(b Value) Value              { return !b.(bool) }
func BoolAnd(that, self Value) Value     { return self.(bool) && that.(bool) }
func BoolNand(that, self Value) Value    { return !(self.(bool) && that.(bool)) }
func BoolOr(that, self Value) Value      { return self.(bool) || that.(bool) }
func BoolNor(that, self Value) Value     { return !(self.(bool) || that.(bool)) }
func BoolXor(that, self Value) Value     { return self.(bool) != that.(bool) }
func BoolEqv(that, self Value) Value     { return self.(bool) == that.(bool) }
func BoolImplies(that, self Value) Value { return !self.(bool) || that.(bool) }

func BoolEvery(xs Value) Value {
	for _, b := range List(xs) {
		if !b.(bool) {
			return false
		}
	}
	return true
}

func BoolSome(xs Value) Value {
	for _, b := range List(xs) {
		if b.(bool) {
			return true
		}
	}
	return false
}

func BoolMatch(fb *Fiber, options, b Value) Value {
	r := record(options)
	if b.(bool) {
		return Apply(fb, r.Get("onTrue"), Unit)
	}
	return Apply(fb, r.Get("onFalse"), Unit)
}

func PredicateMapInput(fb *Fiber, f, p, b Value) Value { return Apply(fb, p, Apply(fb, f, b)) }
func PredicateNot(fb *Fiber, p, a Value) Value         { return !pred(fb, p, a) }

// The joined predicates test `that` only when the operator needs it, as
// `&&` and `||` do.
func PredicateAnd(fb *Fiber, that, self, a Value) Value {
	return pred(fb, self, a) && pred(fb, that, a)
}
func PredicateOr(fb *Fiber, that, self, a Value) Value { return pred(fb, self, a) || pred(fb, that, a) }
func PredicateXor(fb *Fiber, that, self, a Value) Value {
	return pred(fb, self, a) != pred(fb, that, a)
}
func PredicateEqv(fb *Fiber, that, self, a Value) Value {
	return pred(fb, self, a) == pred(fb, that, a)
}
func PredicateNand(fb *Fiber, that, self, a Value) Value {
	return !(pred(fb, self, a) && pred(fb, that, a))
}
func PredicateNor(fb *Fiber, that, self, a Value) Value {
	return !(pred(fb, self, a) || pred(fb, that, a))
}

// PredicateImplies is `antecedent a` implying `consequent a`.
func PredicateImplies(fb *Fiber, consequent, antecedent, a Value) Value {
	return !pred(fb, antecedent, a) || pred(fb, consequent, a)
}

func PredicateEvery(fb *Fiber, ps, a Value) Value {
	for _, p := range List(ps) {
		if !pred(fb, p, a) {
			return false
		}
	}
	return true
}

func PredicateSome(fb *Fiber, ps, a Value) Value {
	for _, p := range List(ps) {
		if pred(fb, p, a) {
			return true
		}
	}
	return false
}

func FilterMake(fb *Fiber, f, i Value) Value { return Apply(fb, f, i) }

// FilterMakeEffect runs the task of an effectful filter.
func FilterMakeEffect(fb *Fiber, f, i Value) Value { return Run(fb, Apply(fb, f, i)) }

func FilterMapFail(fb *Fiber, f, self, i Value) Value {
	if r := Apply(fb, self, i); !isOk(r) {
		return Err(Apply(fb, f, value(r)))
	} else {
		return r
	}
}

func FilterFromPredicate(fb *Fiber, p, i Value) Value {
	if pred(fb, p, i) {
		return Ok(i)
	}
	return Err(i)
}

func FilterFromPredicateOption(fb *Fiber, f, i Value) Value {
	if o := Apply(fb, f, i); isSome(o) {
		return Ok(value(o))
	}
	return Err(i)
}

func FilterToPredicate(fb *Fiber, self, i Value) Value { return isOk(Apply(fb, self, i)) }

func FilterToOption(fb *Fiber, self, i Value) Value {
	if r := Apply(fb, self, i); isOk(r) {
		return Some(value(r))
	}
	return None
}

func FilterToResult(fb *Fiber, self, i Value) Value { return Apply(fb, self, i) }

func FilterEquals(v, u Value) Value {
	if Equals(u, v) {
		return Ok(v)
	}
	return Err(u)
}

func FilterEqualsStrict(v, u Value) Value {
	if Same(u, v) {
		return Ok(v)
	}
	return Err(u)
}

func FilterOr(fb *Fiber, that, self, i Value) Value {
	if r := Apply(fb, self, i); isOk(r) {
		return r
	}
	return Apply(fb, that, i)
}

// FilterZipWith runs `left`, then `right`; the first failure wins.
func FilterZipWith(fb *Fiber, right, f, left, i Value) Value {
	l := Apply(fb, left, i)
	if !isOk(l) {
		return l
	}
	r := Apply(fb, right, i)
	if !isOk(r) {
		return r
	}
	return Ok(apply2(fb, f, value(l), value(r)))
}

func FilterZip(fb *Fiber, right, left, i Value) Value {
	return FilterZipWith(fb, right, Curry2(func(fb *Fiber, a, b Value) Value { return []Value{a, b} }), left, i)
}
func FilterAndLeft(fb *Fiber, right, left, i Value) Value {
	return FilterZipWith(fb, right, Curry2(func(fb *Fiber, a, _ Value) Value { return a }), left, i)
}
func FilterAndRight(fb *Fiber, right, left, i Value) Value {
	return FilterZipWith(fb, right, Curry2(func(fb *Fiber, _, b Value) Value { return b }), left, i)
}

func FilterCompose(fb *Fiber, right, left, i Value) Value {
	if l := Apply(fb, left, i); !isOk(l) {
		return l
	} else {
		return Apply(fb, right, value(l))
	}
}

// FilterComposePassthrough fails with the input when either side fails.
func FilterComposePassthrough(fb *Fiber, right, left, i Value) Value {
	l := Apply(fb, left, i)
	if !isOk(l) {
		return Err(i)
	}
	if r := Apply(fb, right, value(l)); isOk(r) {
		return r
	}
	return Err(i)
}

func FunctionConstant(v, _ Value) Value { return v }
func FunctionConstTrue(Value) Value     { return true }
func FunctionConstFalse(Value) Value    { return false }
func FunctionConstVoid(Value) Value     { return Unit }

func FunctionFlip(fb *Fiber, f, b, a Value) Value      { return apply2(fb, f, a, b) }
func FunctionCompose(fb *Fiber, bc, ab, a Value) Value { return Apply(fb, bc, Apply(fb, ab, a)) }
func FunctionAbsurd(Value) Value                       { panic(Defect{"Error: Cannot call absurd"}) }
func FunctionSK(_, b Value) Value                      { return b }

func FunctionTupled(fb *Fiber, f, pair Value) Value {
	p := List(pair)
	return apply2(fb, f, p[0], p[1])
}

func FunctionUntupled(fb *Fiber, f, a, b Value) Value { return Apply(fb, f, []Value{a, b}) }
