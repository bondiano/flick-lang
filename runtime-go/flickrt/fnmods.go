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

func BoolMatch(options, b Value) Value {
	r := record(options)
	if b.(bool) {
		return Apply(r.Get("onTrue"), Unit)
	}
	return Apply(r.Get("onFalse"), Unit)
}

func PredicateMapInput(f, p, b Value) Value { return Apply(p, Apply(f, b)) }
func PredicateNot(p, a Value) Value         { return !pred(p, a) }

// The joined predicates test `that` only when the operator needs it, as
// `&&` and `||` do.
func PredicateAnd(that, self, a Value) Value  { return pred(self, a) && pred(that, a) }
func PredicateOr(that, self, a Value) Value   { return pred(self, a) || pred(that, a) }
func PredicateXor(that, self, a Value) Value  { return pred(self, a) != pred(that, a) }
func PredicateEqv(that, self, a Value) Value  { return pred(self, a) == pred(that, a) }
func PredicateNand(that, self, a Value) Value { return !(pred(self, a) && pred(that, a)) }
func PredicateNor(that, self, a Value) Value  { return !(pred(self, a) || pred(that, a)) }

// PredicateImplies is `antecedent a` implying `consequent a`.
func PredicateImplies(consequent, antecedent, a Value) Value {
	return !pred(antecedent, a) || pred(consequent, a)
}

func PredicateEvery(ps, a Value) Value {
	for _, p := range List(ps) {
		if !pred(p, a) {
			return false
		}
	}
	return true
}

func PredicateSome(ps, a Value) Value {
	for _, p := range List(ps) {
		if pred(p, a) {
			return true
		}
	}
	return false
}

func FilterMake(f, i Value) Value { return Apply(f, i) }

func FilterMapFail(f, self, i Value) Value {
	if r := Apply(self, i); !isOk(r) {
		return Err(Apply(f, value(r)))
	} else {
		return r
	}
}

func FilterFromPredicate(p, i Value) Value {
	if pred(p, i) {
		return Ok(i)
	}
	return Err(i)
}

func FilterFromPredicateOption(f, i Value) Value {
	if o := Apply(f, i); isSome(o) {
		return Ok(value(o))
	}
	return Err(i)
}

func FilterToPredicate(self, i Value) Value { return isOk(Apply(self, i)) }

func FilterToOption(self, i Value) Value {
	if r := Apply(self, i); isOk(r) {
		return Some(value(r))
	}
	return None
}

func FilterToResult(self, i Value) Value { return Apply(self, i) }

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

func FilterOr(that, self, i Value) Value {
	if r := Apply(self, i); isOk(r) {
		return r
	}
	return Apply(that, i)
}

// FilterZipWith runs `left`, then `right`; the first failure wins.
func FilterZipWith(right, f, left, i Value) Value {
	l := Apply(left, i)
	if !isOk(l) {
		return l
	}
	r := Apply(right, i)
	if !isOk(r) {
		return r
	}
	return Ok(apply2(f, value(l), value(r)))
}

func FilterZip(right, left, i Value) Value {
	return FilterZipWith(right, Curry2(func(a, b Value) Value { return []Value{a, b} }), left, i)
}
func FilterAndLeft(right, left, i Value) Value {
	return FilterZipWith(right, Curry2(func(a, _ Value) Value { return a }), left, i)
}
func FilterAndRight(right, left, i Value) Value {
	return FilterZipWith(right, Curry2(func(_, b Value) Value { return b }), left, i)
}

func FilterCompose(right, left, i Value) Value {
	if l := Apply(left, i); !isOk(l) {
		return l
	} else {
		return Apply(right, value(l))
	}
}

// FilterComposePassthrough fails with the input when either side fails.
func FilterComposePassthrough(right, left, i Value) Value {
	l := Apply(left, i)
	if !isOk(l) {
		return Err(i)
	}
	if r := Apply(right, value(l)); isOk(r) {
		return r
	}
	return Err(i)
}

func FunctionConstant(v, _ Value) Value { return v }
func FunctionConstTrue(Value) Value     { return true }
func FunctionConstFalse(Value) Value    { return false }
func FunctionConstVoid(Value) Value     { return Unit }

func FunctionFlip(f, b, a Value) Value      { return apply2(f, a, b) }
func FunctionCompose(bc, ab, a Value) Value { return Apply(bc, Apply(ab, a)) }
func FunctionAbsurd(Value) Value            { panic(Defect{"Error: Cannot call absurd"}) }
func FunctionSK(_, b Value) Value           { return b }

func FunctionTupled(f, pair Value) Value {
	p := List(pair)
	return apply2(f, p[0], p[1])
}

func FunctionUntupled(f, a, b Value) Value { return Apply(f, []Value{a, b}) }
