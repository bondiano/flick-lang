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
func OptionComposeK(f, g, a Value) Value {
	if b := Apply(g, a); isSome(b) {
		return Apply(f, value(b))
	}
	return None
}

func OptionContainsWith(eq, a, o Value) Value { return isSome(o) && Bool(apply2(eq, value(o), a)) }
func OptionExists(p, o Value) Value           { return isSome(o) && Bool(Apply(p, value(o))) }

func OptionFilter(p, o Value) Value {
	if OptionExists(p, o).(bool) {
		return o
	}
	return None
}

func OptionFilterMap(f, o Value) Value {
	if !isSome(o) {
		return None
	}
	if r := Apply(f, value(o)); isOk(r) {
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
func OptionLift2(f, ob, oa Value) Value { return OptionZipWith(ob, f, oa) }

func OptionLiftPredicate(p, b Value) Value {
	if Bool(Apply(p, b)) {
		return Some(b)
	}
	return None
}

func OptionMatch(options, o Value) Value {
	r := record(options)
	if isSome(o) {
		return Apply(r.Get("onSome"), value(o))
	}
	return Apply(r.Get("onNone"), Unit)
}

func OptionOrElseResult(that, o Value) Value {
	if isSome(o) {
		return Some(Err(value(o)))
	}
	if b := Apply(that, Unit); isSome(b) {
		return Some(Ok(value(b)))
	}
	return None
}

func OptionOrElseSome(onNone, o Value) Value {
	if isSome(o) {
		return o
	}
	return Some(Apply(onNone, Unit))
}

func OptionPartitionMap(f, o Value) Value {
	if !isSome(o) {
		return []Value{None, None}
	}
	if r := Apply(f, value(o)); !isOk(r) {
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

func OptionReduceCompact(b, f, xs Value) Value {
	out := b
	for _, o := range List(xs) {
		if isSome(o) {
			out = apply2(f, out, value(o))
		}
	}
	return out
}

func OptionTap(f, o Value) Value {
	if isSome(o) && isSome(Apply(f, value(o))) {
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

func OptionZipWith(that, f, self Value) Value {
	if isSome(self) && isSome(that) {
		return Some(apply2(f, value(self), value(that)))
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

func ResultFilterOrFail(p, orFail, r Value) Value {
	if !isOk(r) || Bool(Apply(p, value(r))) {
		return r
	}
	return Err(Apply(orFail, value(r)))
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

func ResultLiftPredicate(p, orFail, a Value) Value {
	if Bool(Apply(p, a)) {
		return Ok(a)
	}
	return Err(Apply(orFail, a))
}

func ResultMapBoth(options, r Value) Value {
	o := record(options)
	if isOk(r) {
		return Ok(Apply(o.Get("onSuccess"), value(r)))
	}
	return Err(Apply(o.Get("onFailure"), value(r)))
}

func ResultMatch(options, r Value) Value {
	o := record(options)
	if isOk(r) {
		return Apply(o.Get("onSuccess"), value(r))
	}
	return Apply(o.Get("onFailure"), value(r))
}

func ResultMerge(r Value) Value { return value(r) }

func ResultOrElse(f, r Value) Value {
	if isOk(r) {
		return r
	}
	return Apply(f, value(r))
}

func ResultSucceedSome(a Value) Value { return Ok(Some(a)) }

func ResultTransposeOption(o Value) Value {
	if !isSome(o) {
		return ResultSucceedNone
	}
	return ResultTransposeMapOption(Fn(Identity), o)
}

func ResultTransposeMapOption(f, o Value) Value {
	if !isSome(o) {
		return ResultSucceedNone
	}
	if r := Apply(f, value(o)); isOk(r) {
		return Ok(Some(value(r)))
	} else {
		return r
	}
}
