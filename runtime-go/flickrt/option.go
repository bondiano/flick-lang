package flickrt

// The `Option` and `Result` functions the standard library binds, in the
// order Flick passes the arguments: data-last, `self` at the end.

func opt(v Value) *Data { return v.(*Data) }

func isSome(v Value) bool { return opt(v).Tag == "Some" }
func isOk(v Value) bool   { return opt(v).Tag == "Success" }
func value(v Value) Value { return opt(v).Fields[0] }

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
