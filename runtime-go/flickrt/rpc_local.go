package flickrt

// `Rpc.call_local`: a group called in memory, as Effect's RpcTest joins a
// client and a server without a transport. The codecs do not run; the
// hooks see the headers of `with_headers`; a typed error is the handler's.

// localCall runs a call of the procedure with the handler in the caller's
// fiber, uninterruptibly when the procedure says so: the handler's result,
// and the fiber a stream of it runs in.
func localCall(fb *Fiber, p *rpcProc, handler, payload Value) (Value, *Fiber) {
	headers := headerPairs(headersOf(currentRPCHeaders(fb)))
	var inner *Fiber
	run := func() Value {
		var out Value
		out, inner = rpcInvoke(fb, p, handler, payload, headers)
		return out
	}
	if p.uninterruptible {
		return masked(fb, fb.state.mask+1, run), inner
	}
	return run(), inner
}

// RpcCallLocal is a unary call, in a scope of its own as a server's call.
func RpcCallLocal(fb *Fiber, group, handlers, proc, payload Value) Value {
	p := rpcProcOf(proc)
	return localUnary(fb, p, rpcHandlers(groupOf(group), handlers)[p.name], payload)
}

func localUnary(fb *Fiber, p *rpcProc, handler, payload Value) Value {
	return owned(fb, func(fb *Fiber, _ *Owner) Value {
		out, _ := localCall(fb, p, handler, payload)
		return out
	})
}

// RpcCallStreamLocal is the stream of the handler, the call made when it
// is run; it runs with the service the procedure's check made.
func RpcCallStreamLocal(group, handlers, proc, payload Value) Value {
	p := rpcProcOf(proc)
	return streamOpen(func(fb *Fiber) pull {
		out, inner := localCall(fb, p, rpcHandlers(groupOf(group), handlers)[p.name], payload)
		env := inner.env
		with := func(fb *Fiber) *Fiber {
			f := *fb
			f.env = env
			return &f
		}
		next := streamOf(out).open(with(fb))
		return func(fb *Fiber) ([]Value, bool) { return next(with(fb)) }
	})
}
