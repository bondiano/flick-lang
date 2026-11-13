package flickrt

import (
	"net/url"
	"os"
	"sync"
)

// The calls of `server fn`, as the JS runtime's: the compiler makes a
// procedure of each, a caller posts it as NDJSON to `Server.path` and
// `Server.handler` serves it. While a handler is mounted in this process,
// a call made here runs in memory instead, with the handler's services,
// the headers of the request being served, and what crosses written and
// read by its codecs.

const serverPath = "/_flick/fn"

// serverFn is `Server.Function r`: an implementation with its procedure.
type serverFn struct {
	proc *rpcProc
	run  Value
}

func ServerProcedure(name, payload, success, errors Value) Value {
	p := &rpcProc{name: name.(string), payload: sch(payload).jsonOf(), success: sch(success).jsonOf()}
	for _, e := range List(errors) {
		p.errors = append(p.errors, sch(e).jsonOf())
	}
	return p
}

func ServerFunction(proc, run Value) Value { return &serverFn{rpcProcOf(proc), run} }

// ServerAuthenticated is each function run with the service `check` makes
// from its call's headers.
func ServerAuthenticated(fb *Fiber, tag, check, functions Value) Value {
	fns := List(functions)
	out := make([]Value, len(fns))
	for i, f := range fns {
		fn := f.(*serverFn)
		guard := Fn(func(fb *Fiber, call Value) Value { return Apply(fb, check, record(call).Get("headers")) })
		out[i] = &serverFn{rpcProcOf(RpcWithAuth(tag, guard, fn.proc)), fn.run}
	}
	return out
}

// inProcess is a mounted handler's procedure, called in memory.
type inProcess struct {
	srv   *rpcServer
	proc  *rpcProc
	owner *Owner
}

// shortcut: one table per process, the last `Server.handler` mounted wins; key it by server once a process serves several
var (
	localMu  sync.Mutex
	localFns = map[string]*inProcess{}
)

func ServerHandler(fb *Fiber, functions Value) Value {
	fns := List(functions)
	g := &rpcGroup{}
	handlers := map[string]Value{}
	for _, f := range fns {
		fn := f.(*serverFn)
		g.procs = append(g.procs, fn.proc)
		handlers[fn.proc.name] = fn.run
	}
	srv := newRPCServer(fb, g, handlers)
	mine := make([]*inProcess, len(g.procs))
	localMu.Lock()
	for i, p := range g.procs {
		mine[i] = &inProcess{srv: srv, proc: p, owner: fb.owner}
		localFns[p.name] = mine[i]
	}
	localMu.Unlock()
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		localMu.Lock()
		for _, l := range mine {
			if localFns[l.proc.name] == l {
				delete(localFns, l.proc.name)
			}
		}
		localMu.Unlock()
		return Unit
	})
	return srv.httpHandler(serializationOf(RpcNdjson))
}

// across is `v` written and read back by its codec, as it crosses the network.
func across(fb *Fiber, c codec, v Value) Value {
	if c == nil {
		return v
	}
	tree, issues := c.encode(fb, v)
	if issues == nil {
		v, issues = c.decode(fb, tree)
	}
	if issues != nil {
		panic(Defect{issuesText(issues)})
	}
	return v
}

// call runs the procedure in memory with the headers of the request the
// fiber serves, as a call over HTTP would see them.
func (l *inProcess) call(fb *Fiber, payload Value) Value {
	headers := headersOf(currentRPCHeaders(fb))
	if q, ok := fb.env.get(serverRequestKey); ok {
		headers = headersOf(HeadersMerge(q.(*ServerRequest).headerRecord(), headers))
	}
	inner := *fb
	inner.env = l.srv.services.with(rpcHeadersKey, headers)
	p := l.proc
	sent := across(fb, p.payload, payload)
	// Admission and mount closure use the owner's same lock. The handler
	// has its own fiber: closing the mount interrupts and joins it before
	// releasing captured services, without interrupting the caller's state.
	child := FiberSpawn(&inner, l.owner, Fn(func(*Fiber, Value) Value { return Succeed(Unit) }), Eff(func(fb *Fiber) Value {
		return localUnary(fb, p, l.srv.handlers[p.name], sent)
	})).(*child)
	// Caller interruption must also cancel and join the server-side work.
	defer stop(fb, child)
	out, c := catch(func() Value { return FiberJoin(fb, child) })
	if c != nil {
		if err, ok := typedFailure(c); ok {
			for _, e := range p.errors {
				if codecHas(fb, e, err) {
					return Fail(across(fb, e, err))
				}
			}
		}
		panic(&Failure{c})
	}
	return across(fb, p.success, out)
}

// ServerCall is a call of the procedure: in memory while a handler of it
// is mounted here, otherwise a POST to FLICK_SERVER_URL.
func ServerCall(fb *Fiber, proc, payload Value) Value {
	p := rpcProcOf(proc)
	localMu.Lock()
	here := localFns[p.name]
	localMu.Unlock()
	if here != nil {
		return here.call(fb, payload)
	}
	base, ok := os.LookupEnv("FLICK_SERVER_URL")
	u, err := url.Parse(base)
	if !ok || err != nil {
		return Fail(rpcClientError("no server for " + p.name + ": set FLICK_SERVER_URL"))
	}
	target := u.ResolveReference(&url.URL{Path: serverPath}).String()
	return RpcCall(fb, callDefaults, target, p, payload)
}

// ServerNoErrors and ServerError construct the typed error witnesses.
var ServerNoErrors Value = []Value{}

func ServerError(witness, errors Value) Value {
	return append([]Value{witness}, List(errors)...)
}
