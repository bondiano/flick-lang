package flickrt

// Effect's `Option` and `Result` functions the standard library binds, in
// the order Flick passes the arguments: data-last, `self` at the end.

func opt(v Value) *Data { return v.(*Data) }

func isSome(v Value) bool { return opt(v).Tag == "Some" }
func isOk(v Value) bool   { return opt(v).Tag == "Success" }
func value(v Value) Value { return opt(v).Fields[0] }

func OptionAll(xs Value) Value {
	out := []Value{}
	for _, o := range List(xs) {
		if !isSome(o) {
			return None
		}
		out = append(out, value(o))
	}
	return Some(out)
}

func OptionAs(b, o Value) Value {
	if isSome(o) {
		return Some(b)
	}
	return None
}

func OptionAsVoid(o Value) Value { return OptionAs(Unit, o) }

var OptionVoid Value = Some(Unit)

// OptionComposeK is `a` through `g`, then `f`.
func OptionComposeK(fb *Fiber, f, g, a Value) Value {
	if b := Apply(fb, g, a); isSome(b) {
		return Apply(fb, f, value(b))
	}
	return None
}

func OptionContainsWith(fb *Fiber, eq, a, o Value) Value {
	return isSome(o) && Bool(apply2(fb, eq, value(o), a))
}
func OptionExists(fb *Fiber, p, o Value) Value { return isSome(o) && Bool(Apply(fb, p, value(o))) }

func OptionFilter(fb *Fiber, p, o Value) Value {
	if OptionExists(fb, p, o).(bool) {
		return o
	}
	return None
}

func OptionFilterMap(fb *Fiber, f, o Value) Value {
	if !isSome(o) {
		return None
	}
	if r := Apply(fb, f, value(o)); isOk(r) {
		return Some(value(r))
	}
	return None
}

func OptionFirstSomeOf(xs Value) Value {
	for _, o := range List(xs) {
		if isSome(o) {
			return o
		}
	}
	return None
}

func OptionFlatten(o Value) Value {
	if isSome(o) {
		return value(o)
	}
	return None
}

// OptionLift2 is `f` of the values of `oa`, then `ob`.
func OptionLift2(fb *Fiber, f, ob, oa Value) Value { return OptionZipWith(fb, ob, f, oa) }

func OptionLiftPredicate(fb *Fiber, p, b Value) Value {
	if Bool(Apply(fb, p, b)) {
		return Some(b)
	}
	return None
}

func OptionMatch(fb *Fiber, options, o Value) Value {
	r := record(options)
	if isSome(o) {
		return Apply(fb, r.Get("onSome"), value(o))
	}
	return Apply(fb, r.Get("onNone"), Unit)
}

func OptionOrElseResult(fb *Fiber, that, o Value) Value {
	if isSome(o) {
		return Some(Err(value(o)))
	}
	if b := Apply(fb, that, Unit); isSome(b) {
		return Some(Ok(value(b)))
	}
	return None
}

func OptionOrElseSome(fb *Fiber, onNone, o Value) Value {
	if isSome(o) {
		return o
	}
	return Some(Apply(fb, onNone, Unit))
}

func OptionPartitionMap(fb *Fiber, f, o Value) Value {
	if !isSome(o) {
		return []Value{None, None}
	}
	if r := Apply(fb, f, value(o)); !isOk(r) {
		return []Value{None, Some(value(r))}
	} else {
		return []Value{Some(value(r)), None}
	}
}

func OptionProduct(that, self Value) Value {
	if isSome(self) && isSome(that) {
		return Some([]Value{value(self), value(that)})
	}
	return None
}

func OptionProductMany(xs, self Value) Value {
	if !isSome(self) {
		return None
	}
	out := []Value{value(self)}
	for _, o := range List(xs) {
		if !isSome(o) {
			return None
		}
		out = append(out, value(o))
	}
	return Some(out)
}

func OptionReduceCompact(fb *Fiber, b, f, xs Value) Value {
	out := b
	for _, o := range List(xs) {
		if isSome(o) {
			out = apply2(fb, f, out, value(o))
		}
	}
	return out
}

func OptionTap(fb *Fiber, f, o Value) Value {
	if isSome(o) && isSome(Apply(fb, f, value(o))) {
		return o
	}
	return None
}

func OptionZipLeft(that, self Value) Value {
	if isSome(self) && isSome(that) {
		return self
	}
	return None
}

func OptionZipRight(that, self Value) Value {
	if isSome(self) {
		return that
	}
	return None
}

func OptionZipWith(fb *Fiber, that, f, self Value) Value {
	if isSome(self) && isSome(that) {
		return Some(apply2(fb, f, value(self), value(that)))
	}
	return None
}

func ResultAll(xs Value) Value {
	out := []Value{}
	for _, r := range List(xs) {
		if !isOk(r) {
			return r
		}
		out = append(out, value(r))
	}
	return Ok(out)
}

var (
	ResultFailVoid    Value = Err(Unit)
	ResultVoid        Value = Ok(Unit)
	ResultSucceedNone Value = Ok(None)
)

func ResultFilterOrFail(fb *Fiber, p, orFail, r Value) Value {
	if !isOk(r) || Bool(Apply(fb, p, value(r))) {
		return r
	}
	return Err(Apply(fb, orFail, value(r)))
}

func ResultFlip(r Value) Value {
	if isOk(r) {
		return Err(value(r))
	}
	return Ok(value(r))
}

func ResultGetFailure(r Value) Value {
	if isOk(r) {
		return None
	}
	return Some(value(r))
}

func ResultLiftPredicate(fb *Fiber, p, orFail, a Value) Value {
	if Bool(Apply(fb, p, a)) {
		return Ok(a)
	}
	return Err(Apply(fb, orFail, a))
}

func ResultMapBoth(fb *Fiber, options, r Value) Value {
	o := record(options)
	if isOk(r) {
		return Ok(Apply(fb, o.Get("onSuccess"), value(r)))
	}
	return Err(Apply(fb, o.Get("onFailure"), value(r)))
}

func ResultMatch(fb *Fiber, options, r Value) Value {
	o := record(options)
	if isOk(r) {
		return Apply(fb, o.Get("onSuccess"), value(r))
	}
	return Apply(fb, o.Get("onFailure"), value(r))
}

func ResultMerge(r Value) Value { return value(r) }

func ResultOrElse(fb *Fiber, f, r Value) Value {
	if isOk(r) {
		return r
	}
	return Apply(fb, f, value(r))
}

func ResultSucceedSome(a Value) Value { return Ok(Some(a)) }

func ResultTransposeOption(fb *Fiber, o Value) Value {
	if !isSome(o) {
		return ResultSucceedNone
	}
	return ResultTransposeMapOption(fb, Fn(func(_ *Fiber, v Value) Value { return Identity(v) }), o)
}

func ResultTransposeMapOption(fb *Fiber, f, o Value) Value {
	if !isSome(o) {
		return ResultSucceedNone
	}
	if r := Apply(fb, f, value(o)); isOk(r) {
		return Ok(Some(value(r)))
	} else {
		return r
	}
}
