package flickrt

import "sync"

// The `Rpc` module as the JS runtime's: procedures and groups as frozen
// data, served by the dispatcher of rpc_server.go and called by the client
// of rpc_client.go over the serializations of rpc_wire.go.

var (
	rpcServeErrorTag  = Tag("Rpc.ServeError.ServeError")
	rpcClientErrorTag = Tag("Rpc.ClientError.ClientError")
)

func rpcServeError(message string) Value  { return NewData(rpcServeErrorTag, message) }
func rpcClientError(message string) Value { return NewData(rpcClientErrorTag, message) }

// rpcProc is `Rpc.Rpc l f`: a procedure as data.
type rpcProc struct {
	name            string
	payload         codec // nil: Effect's Schema.Void
	success         codec // nil: Schema.Void; a stream's element otherwise
	errors          []codec
	stream          bool
	middleware      []Value // hooks of `Call p`, run in order before the handler
	uninterruptible bool
	authTag         string // "": no `with_auth`
	authCheck       Value
	binOnce         sync.Once
	bin             *rpcBinLayouts
}

// rpcBinLayouts are the binary layouts of a procedure's holes, built once.
type rpcBinLayouts struct{ payload, chunk, exit *binNode }

func (p *rpcProc) layouts() *rpcBinLayouts {
	p.binOnce.Do(func() {
		p.bin = &rpcBinLayouts{
			payload: binPayloadLayout(p.payload),
			chunk:   binChunkLayout(p.success),
			exit:    binExitLayout(p.success, p.errors, p.stream),
		}
	})
	return p.bin
}

func rpcProcOf(v Value) *rpcProc { return v.(*rpcProc) }

// with is a copy of the procedure, changed by `change`.
func (p *rpcProc) with(change func(*rpcProc)) Value {
	out := &rpcProc{
		name: p.name, payload: p.payload, success: p.success, errors: p.errors, stream: p.stream,
		middleware: p.middleware, uninterruptible: p.uninterruptible, authTag: p.authTag, authCheck: p.authCheck,
	}
	change(out)
	return out
}

func RpcMake(name Value) Value { return &rpcProc{name: name.(string)} }

func RpcWithPayload(c, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) { o.payload = codecOf(c) })
}

func RpcWithSuccess(c, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) { o.success, o.stream = codecOf(c), false })
}

func RpcWithStream(c, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) { o.success, o.stream = codecOf(c), true })
}

// rpcCases is `Rpc.ErrorCases e`: a codec per case, in order.
type rpcCases []codec

var RpcErrors Value = rpcCases(nil)

func RpcErrorCase(c, cases Value) Value {
	cs := cases.(rpcCases)
	return append(cs[:len(cs):len(cs)], codecOf(c))
}

func RpcWithErrors(cases, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) {
		o.errors = append(o.errors[:len(o.errors):len(o.errors)], cases.(rpcCases)...)
	})
}

func RpcWithMiddleware(hook, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) {
		o.middleware = append(o.middleware[:len(o.middleware):len(o.middleware)], hook)
	})
}

func RpcWithAuth(tag, check, p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) { o.authTag, o.authCheck = tag.(string), check })
}

func RpcUninterruptible(p Value) Value {
	return rpcProcOf(p).with(func(o *rpcProc) { o.uninterruptible = true })
}

// rpcGroup is `Rpc.Group r hs`.
type rpcGroup struct{ procs []*rpcProc }

func groupOf(v Value) *rpcGroup { return v.(*rpcGroup) }

var RpcGroup Value = &rpcGroup{}

// RpcAdd is `Rpc.add`, after the dictionaries of its record constraints.
func RpcAdd(_, _, p, g Value) Value {
	procs := groupOf(g).procs
	return &rpcGroup{append(procs[:len(procs):len(procs)], rpcProcOf(p))}
}

// RpcAuthenticated is the group, each of whose procedures must make the
// service `tag` with `with_auth`.
func RpcAuthenticated(tag, g Value) Value {
	for _, p := range groupOf(g).procs {
		if p.authTag != tag.(string) {
			panic(Defect{"TypeError: the procedure " + p.name + " has no `with_auth` making " + tag.(string)})
		}
	}
	return g
}

// rpcTransport is `Rpc.Transport`: how a client reaches its server.
type rpcTransport string

var (
	RpcHttp      Value = rpcTransport("http")
	RpcWebsocket Value = rpcTransport("websocket")
	RpcTcp       Value = rpcTransport("tcp")
)

// rpcHandlers is the handler of each procedure of a group, by name, as
// the record of handlers the program gives.
func rpcHandlers(g *rpcGroup, handlers Value) map[string]Value {
	all := record(handlers)
	out := make(map[string]Value, len(g.procs))
	for _, p := range g.procs {
		h := recordField(all, p.name)
		if h == nil {
			panic(Defect{"TypeError: no handler for " + p.name})
		}
		out[p.name] = h
	}
	return out
}
