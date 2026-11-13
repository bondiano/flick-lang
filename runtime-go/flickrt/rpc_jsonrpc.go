package flickrt

import "strings"

// JSON-RPC 2.0 as Effect's RpcSerialization writes it: a request is a
// `method` call, the protocol's own messages are notifications of
// `@effect/rpc/…` methods, an exit is a result or an error whose data is
// the cause, a chunk a result marked `chunk`. The responses to a batch
// (an array of requests) are held until all are ready and sent together.

type jsonRpcBatch struct {
	size      int
	order     []string
	responses map[string]Value
}

type jsonRpcParser struct {
	framed  *ndjsonParser // nil: one document
	batches map[string]*jsonRpcBatch
}

func (p *jsonRpcParser) decode(data []byte) ([]Value, error) {
	var docs []Value
	if p.framed != nil {
		frames, err := p.framed.decode(data)
		if err != nil {
			return nil, err
		}
		docs = frames
	} else {
		v, err := parseDocument(data)
		if err != nil {
			return nil, err
		}
		docs = []Value{v}
	}
	var out []Value
	for _, d := range docs {
		out = append(out, p.decodeRaw(d)...)
	}
	return out, nil
}

func (p *jsonRpcParser) decodeRaw(doc Value) []Value {
	xs, isBatch := doc.([]Value)
	if !isBatch {
		if r, ok := doc.(*Record); ok {
			return []Value{decodeJsonRpcMessage(r)}
		}
		return nil
	}
	batch := &jsonRpcBatch{responses: map[string]Value{}}
	var out []Value
	for _, x := range xs {
		r, ok := x.(*Record)
		if !ok {
			continue
		}
		m := decodeJsonRpcMessage(r)
		out = append(out, m)
		if msgTag(m) == "Request" && msgField(m, "isNotification") == nil {
			batch.size++
			p.batches[idKey(msgField(m, "id"))] = batch
		}
	}
	return out
}

func isNullish(v Value) bool {
	_, null := v.(*jsonNull)
	return v == nil || null
}

func decodeJsonRpcMessage(r *Record) Value {
	if method := recordField(r, "method"); method != nil {
		name, isString := method.(string)
		if isNullish(recordField(r, "id")) && isString && strings.HasPrefix(name, "@effect/rpc/") {
			out := Rec("_tag", name[len("@effect/rpc/"):]).(*Record)
			if id := msgField(recordField(r, "params"), "requestId"); id != nil {
				out.put("requestId", id)
			}
			return out
		}
		id := recordField(r, "id")
		if isNullish(id) {
			id = ""
		}
		params := recordField(r, "params")
		if isNullish(params) {
			params = JsonNull
		}
		headers := recordField(r, "headers")
		if isNullish(headers) {
			headers = []Value{}
		}
		out := Rec("_tag", "Request", "id", id, "tag", method, "payload", params, "headers", headers).(*Record)
		if recordField(r, "id") == nil {
			out.put("isNotification", true)
		}
		if truthy(recordField(r, "spanId")) {
			out.put("traceId", recordField(r, "traceId"))
			out.put("spanId", recordField(r, "spanId"))
			out.put("sampled", recordField(r, "sampled"))
		}
		return out
	}
	errValue := recordField(r, "error")
	hasError := errValue != nil
	if hasError && msgField(errValue, "_tag") == "Defect" {
		return defectMsg(msgField(errValue, "data"))
	}
	id := recordField(r, "id")
	if isNullish(id) {
		id = ""
	}
	if recordField(r, "chunk") == true {
		return chunkMsg(id, recordField(r, "result"))
	}
	if hasError && !isNullish(errValue) {
		cause := msgField(errValue, "data")
		if msgField(errValue, "_tag") != "Cause" {
			cause = []Value{failReason(errValue)}
		}
		return exitMsg(id, Rec("_tag", "Failure", "cause", cause))
	}
	return exitMsg(id, exitSuccessTree(recordField(r, "result")))
}

// truthy is JS's truthiness of a JSON value.
func truthy(v Value) bool {
	switch x := v.(type) {
	case nil, *jsonNull:
		return false
	case bool:
		return x
	case float64:
		return x == x && x != 0
	case string:
		return x != ""
	}
	return true
}

func (p *jsonRpcParser) encode(msgs []Value, many bool) []byte {
	var encoded Value
	if !many {
		encoded = p.encodeRaw(msgs[0])
	} else {
		var parts []Value
		for _, m := range msgs {
			if e := p.encodeRaw(m); e != nil {
				parts = append(parts, e)
			}
		}
		switch len(parts) {
		case 0:
		case 1:
			encoded = parts[0]
		default:
			var flat []Value
			for _, e := range parts {
				if xs, ok := e.([]Value); ok {
					flat = append(flat, xs...)
				} else {
					flat = append(flat, e)
				}
			}
			encoded = flat
		}
	}
	if encoded == nil {
		return nil
	}
	if p.framed != nil {
		return p.framed.encode([]Value{encoded}, false)
	}
	return []byte(JsonStringify(encoded).(string))
}

// encodeRaw is a message, or the batch it completes; nil while its batch waits.
func (p *jsonRpcParser) encodeRaw(m Value) Value {
	id := msgField(m, "requestId")
	if id == nil {
		return encodeJsonRpcMessage(m)
	}
	key := idKey(id)
	batch := p.batches[key]
	if batch == nil {
		return encodeJsonRpcMessage(m)
	}
	delete(p.batches, key)
	if _, seen := batch.responses[key]; !seen {
		batch.order = append(batch.order, key)
	}
	batch.responses[key] = m
	if batch.size != len(batch.responses) {
		return nil
	}
	out := make([]Value, len(batch.order))
	for i, k := range batch.order {
		out[i] = encodeJsonRpcMessage(batch.responses[k])
	}
	return out
}

func encodeJsonRpcMessage(m Value) Value {
	out := newRecord(6)
	out.put("jsonrpc", "2.0")
	switch tag := msgTag(m); tag {
	case "Request":
		out.put("method", msgField(m, "tag"))
		out.put("params", msgField(m, "payload"))
		if msgField(m, "isNotification") != true {
			out.put("id", msgField(m, "id"))
		}
		if hs, _ := msgField(m, "headers").([]Value); len(hs) > 0 {
			out.put("headers", hs)
		}
		for _, k := range []string{"traceId", "spanId", "sampled"} {
			if v := msgField(m, k); v != nil {
				out.put(k, v)
			}
		}
	case "Ping", "Pong", "Interrupt", "Ack", "Eof":
		out.put("method", "@effect/rpc/"+tag)
		if id := msgField(m, "requestId"); id != nil {
			out.put("params", Rec("requestId", id))
		}
	case "Chunk":
		out.put("chunk", true)
		out.put("id", msgField(m, "requestId"))
		out.put("result", msgField(m, "values"))
	case "Exit":
		out.put("id", msgField(m, "requestId"))
		exit := msgField(m, "exit")
		if msgField(exit, "_tag") == "Success" {
			out.put("result", msgField(exit, "value"))
			break
		}
		cause := msgField(exit, "cause")
		var failed Value
		for _, r := range asList(cause) {
			if msgTag(r) == "Fail" {
				failed = msgField(r, "error")
				break
			}
		}
		code, message := Value(0.0), Value(nil)
		if obj, ok := failed.(*Record); ok && truthy(failed) {
			if c := recordField(obj, "code"); c != nil {
				code = rpcNumberOf(c)
			}
			message = recordField(obj, "message")
		}
		if message == nil {
			message = JsonStringify(cause)
		}
		out.put("error", Rec("_tag", "Cause", "code", code, "message", message, "data", cause))
	case "Defect":
		out.put("id", -32603.0)
		out.put("error", Rec("_tag", "Defect", "code", 1.0, "message", "A defect occurred", "data", msgField(m, "defect")))
	default:
		return Rec()
	}
	return out
}

func asList(v Value) []Value {
	xs, _ := v.([]Value)
	return xs
}
