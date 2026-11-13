package flickrt

import (
	"cmp"
	"encoding/binary"
	"maps"
	"slices"
	"strconv"
)

// The `binary` RPC serialization is Effect's SchemaBinary: each value is
// written by a layout compiled from the encoded side of its schema. A
// binNode is that layout for the JSON trees the program's codecs give, as
// Effect 4.1.2 compiles it from the JS runtime's matching schemas, so a Go
// peer writes the bytes a JS peer writes.

type binKind uint8

const (
	binNull binKind = iota
	binUndefined
	binBool
	binNumber
	binString
	binBytes
	binJSON // a value as its JSON text: Schema.Json, Schema.Unknown, a defect
	binNever
	binLiteral
	binStruct
	binArray
	binUnion
	binExit
	binInvalid // a schema Effect has no layout for
)

// binNode is the binary layout of a schema's encoded side.
type binNode struct {
	kind binKind
	leaf binKind // literal: the kind of its values
	lits []Value // literal: its values

	fields    []*binField   // struct: by id, as the wire has them; sentinels are left out
	keys      []string      // struct: every key in declared order, sentinels included
	order     []int         // struct: each key's field index, or -1-i for sentinel i
	sentinels []binSentinel // struct: the literal keys that select it in a union
	extra     *binNode      // struct: the value of every key no field names

	elems []*binNode // array: the leading elements
	rest  *binNode   // array: every element after them; nil for a tuple

	members []*binMember // union: the variants, then the others in probing order

	value, fail, defect *binNode // exit

	err string // invalid: why
}

type binField struct {
	name     string
	id       uint32
	optional bool
	node     *binNode
}

type binSentinel struct {
	key string
	lit Value
}

// binMember is a case of a union: a variant is selected by its sentinels
// and written with their hash, another member by its wire kind.
type binMember struct {
	node    *binNode
	variant bool
	tag     uint32 // a variant's sentinel hash
	kind    byte   // another member's wire kind
	pos     int    // the selector of fingerprint mode
}

// Wire kinds of union members, Effect's `K`.
const (
	binKBool      = 1
	binKNull      = 2
	binKUndefined = 3
	binKNumber    = 4
	binKString    = 5
	binKBytes     = 6
	binKStruct    = 9
	binKVariant   = 10
	binKArray     = 11
	binKJSON      = 17
	binKExit      = 18
)

var binLeaves = func() (out [binNever + 1]*binNode) {
	for k := range out {
		out[k] = &binNode{kind: binKind(k)}
	}
	return out
}()

func binLeaf(k binKind) *binNode { return binLeaves[k] }

func binLits(leaf binKind, values ...Value) *binNode {
	return &binNode{kind: binLiteral, leaf: leaf, lits: values}
}

func binFNV32(b []byte) uint32 {
	h := uint32(0x811C9DC5)
	for _, c := range b {
		h = (h ^ uint32(c)) * 0x01000193
	}
	return h
}

// A field's id is the FNV-1a hash of its name; fields are written by id.
func binFieldOf(name string, optional bool, node *binNode) *binField {
	return &binField{name, binFNV32([]byte(name)), optional, node}
}

func binStructOf(sentinels []binSentinel, fields []*binField) *binNode {
	n := &binNode{kind: binStruct, sentinels: sentinels}
	binSetFields(n, fields)
	return n
}

// binSetFields gives a struct its fields, in declared order.
func binSetFields(n *binNode, fields []*binField) {
	n.keys = nil
	for _, s := range n.sentinels {
		n.keys = append(n.keys, s.key)
	}
	for _, f := range fields {
		n.keys = append(n.keys, f.name)
	}
	n.fields = slices.SortedFunc(slices.Values(fields), func(a, b *binField) int { return cmp.Compare(a.id, b.id) })
	n.order = make([]int, len(n.keys))
	for i, k := range n.keys {
		n.order[i] = slices.IndexFunc(n.fields, func(f *binField) bool { return f.name == k })
		if i < len(n.sentinels) {
			n.order[i] = -1 - i
		}
	}
}

func (n *binNode) wireKind() byte {
	switch n.kind {
	case binLiteral:
		return binLeaf(n.leaf).wireKind()
	case binBool:
		return binKBool
	case binNull:
		return binKNull
	case binUndefined:
		return binKUndefined
	case binNumber:
		return binKNumber
	case binString:
		return binKString
	case binBytes:
		return binKBytes
	case binStruct:
		return binKStruct
	case binArray:
		return binKArray
	case binExit:
		return binKExit
	}
	return binKJSON
}

// binSentinelHash is a variant's tag: the hash of its sentinels sorted by
// key, each as its key and its literal's kind and text.
func binSentinelHash(sentinels []binSentinel) uint32 {
	sorted := slices.SortedFunc(slices.Values(sentinels), func(a, b binSentinel) int { return cmp.Compare(a.key, b.key) })
	var out []byte
	for _, s := range sorted {
		out = append(out, 1) // a string key
		out = binary.LittleEndian.AppendUint32(out, uint32(len(s.key)))
		out = append(out, s.key...)
		text, kind := "", byte(1)
		switch x := s.lit.(type) {
		case string:
			text = x
		case bool:
			text, kind = strconv.FormatBool(x), 3
		}
		out = append(out, kind)
		out = binary.LittleEndian.AppendUint32(out, uint32(len(text)))
		out = append(out, text...)
	}
	return binFNV32(out)
}

// binUnionOf is the layout of a union of `members`, flattened: structs with
// sentinels are variants, literals of a kind merge, a single member is
// itself.
func binUnionOf(members []*binNode) *binNode {
	var variants, lits, rows []*binMember
	seen := map[*binNode]bool{}
	invalid := func(why string) *binNode { return &binNode{kind: binInvalid, err: why} }
	for _, m := range members {
		if seen[m] || m.kind == binNever {
			continue
		}
		seen[m] = true
		if m.kind == binInvalid {
			return m
		}
		if m.kind == binStruct && len(m.sentinels) == 0 {
			var sentinels []binSentinel
			var fields []*binField
			for _, field := range m.fields {
				if !field.optional && field.node.kind == binLiteral && len(field.node.lits) == 1 {
					sentinels = append(sentinels, binSentinel{field.name, field.node.lits[0]})
				} else {
					fields = append(fields, field)
				}
			}
			if len(sentinels) > 0 {
				variant := binStructOf(sentinels, fields)
				variant.extra = m.extra
				m = variant
			}
		}
		switch {
		case m.kind == binStruct && len(m.sentinels) > 0:
			variants = append(variants, &binMember{node: m, variant: true, tag: binSentinelHash(m.sentinels)})
		case m.kind == binLiteral:
			at := slices.IndexFunc(lits, func(l *binMember) bool { return l.node.leaf == m.leaf })
			if at < 0 {
				lits = append(lits, &binMember{node: binLits(m.leaf), kind: m.wireKind()})
				at = len(lits) - 1
			}
			for _, v := range m.lits {
				if !slices.Contains(lits[at].node.lits, v) {
					lits[at].node.lits = append(lits[at].node.lits, v)
				}
			}
		default:
			rows = append(rows, &binMember{node: m, kind: m.wireKind()})
		}
	}
	switch {
	case len(variants) == 0 && len(rows) == 0 && len(lits) == 1:
		return lits[0].node
	case len(variants) == 0 && len(rows) == 0 && len(lits) == 0:
		return binLeaf(binNever)
	}
	others := append(lits, rows...)
	kinds := map[byte]*binNode{}
	others = slices.DeleteFunc(others, func(o *binMember) bool {
		prior := kinds[o.kind]
		kinds[o.kind] = o.node
		// The same plain leaf twice is one member, as Effect's shared leaf schemas are.
		return prior != nil && prior.kind == o.node.kind && o.node.kind < binNever
	})
	if len(kinds) != len(others) {
		return invalid("union members are not uniquely identifiable")
	}
	if len(variants) == 0 && len(others) == 1 && len(lits) == 0 {
		return others[0].node
	}
	byTag := slices.SortedFunc(slices.Values(variants), func(a, b *binMember) int { return cmp.Compare(a.tag, b.tag) })
	for i, v := range byTag {
		if i > 0 && byTag[i-1].tag == v.tag {
			return invalid("sentinel collision " + strconv.Itoa(int(v.tag)))
		}
		v.pos = i
	}
	for i, o := range slices.SortedFunc(slices.Values(others), func(a, b *binMember) int { return cmp.Compare(a.kind, b.kind) }) {
		o.pos = len(variants) + i
	}
	// Probe specific members before a struct, and `json`, which takes anything, last.
	rank := func(o *binMember) int {
		switch o.node.kind {
		case binStruct:
			return 1
		case binJSON:
			return 2
		}
		return 0
	}
	slices.SortStableFunc(others, func(a, b *binMember) int { return cmp.Compare(rank(a), rank(b)) })
	return &binNode{kind: binUnion, members: append(variants, others...)}
}

// binCompiler compiles the layouts of codecs, each schema once, so a
// recursive type refers to its own node.
type binCompiler struct {
	nodes    map[*Schema]*binNode
	variants map[any][]*binNode
}

func newBinCompiler() *binCompiler {
	return &binCompiler{map[*Schema]*binNode{}, map[any][]*binNode{}}
}

func (c *binCompiler) codec(x codec) *binNode {
	switch x := x.(type) {
	case nil:
		return binLeaf(binUndefined)
	case *wire:
		return c.schema(x.s)
	case *listCodec:
		return &binNode{kind: binArray, rest: c.codec(x.item)}
	case *block:
		return c.block(x)
	case *transformed:
		return c.codec(x.from)
	case *fallible:
		return c.codec(x.codec)
	case *stage:
		return c.codec(x.wire)
	case *decodedStage:
		return c.codec(x.wire)
	case *stringAs:
		return binLeaf(binString)
	case *numberAs:
		return binLeaf(binNumber)
	case stringRecord:
		return &binNode{kind: binStruct, extra: binLeaf(binString)}
	case *entriesCodec:
		return &binNode{kind: binStruct, extra: c.codec(x.value)}
	case *checkedEntries:
		return c.codec(x.entriesCodec)
	case *checkedCodec:
		return c.codec(x.from)
	case *fallbackCodec:
		return c.codec(x.from)
	case *wireOnly:
		return c.codec(x.c)
	case *ignoring:
		return c.codec(x.codec)
	case *nonEmptyList:
		item := c.codec(x.item)
		return &binNode{kind: binArray, elems: []*binNode{item}, rest: item}
	case *pair:
		return &binNode{kind: binArray, elems: []*binNode{c.codec(x.first), c.codec(x.second)}}
	case *listEnsure:
		return binUnionOf(append(c.members(x.item), &binNode{kind: binArray, rest: c.codec(x.item)}))
	case *nullOption:
		nodes := append(c.members(x.item), binLeaf(binNull))
		if x.nullish {
			nodes = append(nodes, binLeaf(binUndefined))
		}
		return binUnionOf(nodes)
	case *literalCodec:
		return binLiterals(x.values)
	case *enumCodec:
		if x.adt != nil {
			return c.schema(x.adt)
		}
		values := make([]Value, len(x.values))
		for i, v := range x.values {
			values[i], _ = x.base.encode(nil, v)
		}
		return binLiterals(values)
	case netUnion:
		var nodes []*binNode
		for _, item := range x {
			nodes = append(nodes, c.members(item)...)
		}
		return binUnionOf(nodes)
	case dnsUnion:
		var nodes []*binNode
		for _, tag := range slices.Sorted(maps.Keys(dnsCodecs)) {
			nodes = append(nodes, c.members(dnsCodecs[tag])...)
		}
		return binUnionOf(nodes)
	case maskedRedacted:
		return binLits(binString, maskedText)
	case refused:
		return &binNode{kind: binInvalid, err: x.reason}
	case *stringTree:
		return c.stringTree(x.s)
	case *flipped:
		return c.decoded(x.c)
	case losslessNumber, *taggedOption:
		return binUnionOf(c.members(x))
	case jsonValue:
		return binLeaf(binJSON)
	}
	return &binNode{kind: binInvalid, err: "codec has no binary layout"}
}

func (c *binCompiler) schema(s *Schema) *binNode {
	s = s.resolved()
	if n, ok := c.nodes[s]; ok {
		return n
	}
	// A recursive reference meets the node before it is built; only a
	// record's kind is looked at then, by a union holding it.
	n := &binNode{kind: binStruct}
	c.nodes[s] = n
	*n = *c.build(s)
	return n
}

func (c *binCompiler) build(s *Schema) *binNode {
	switch s.kind {
	case kindNumber, kindInt:
		return binLeaf(binNumber)
	case kindString, kindBigint:
		return binLeaf(binString)
	case kindBool:
		return binLeaf(binBool)
	case kindUnit:
		return binLeaf(binNull)
	case kindNever:
		return binLeaf(binNever)
	case kindList:
		return &binNode{kind: binArray, rest: c.schema(s.item)}
	case kindTuple:
		elems := make([]*binNode, len(s.items))
		for i, item := range s.items {
			elems[i] = c.schema(item)
		}
		return &binNode{kind: binArray, elems: elems}
	case kindOption, kindAdt:
		return binUnionOf(c.members(s.jsonOf()))
	case kindRecord:
		fields := make([]*binField, len(s.labels))
		for i, l := range s.labels {
			fields[i] = binFieldOf(l, false, c.schema(s.items[i]))
		}
		return binStructOf(nil, fields)
	case kindRefined:
		return c.schema(s.item)
	case kindCustom:
		return c.codec(s.jsonOf())
	case kindResult:
		// As Effect, two untagged struct members cannot select a wire case.
		return &binNode{kind: binInvalid, err: "union members are not uniquely identifiable"}
	case kindUnknown:
		return binLeaf(binJSON)
	case kindRedacted:
		return &binNode{kind: binInvalid, err: redactedRefusal}
	}
	return &binNode{kind: binInvalid, err: "schema has no binary layout"}
}

// members is a codec's encoded side as union members: Effect flattens
// nested unions, an option's `null` included.
func (c *binCompiler) members(x codec) []*binNode {
	switch x := x.(type) {
	case *wire:
		s := x.s.resolved()
		switch s.kind {
		case kindAdt:
			return c.adtVariants(s)
		case kindOption:
			return append(slices.Clone(c.members(s.item.jsonOf())), binLeaf(binNull))
		case kindNever:
			return nil
		}
	case *transformed:
		return c.members(x.from)
	case *fallible:
		return c.members(x.codec)
	case *stage:
		return c.members(x.wire)
	case losslessNumber:
		return []*binNode{binLeaf(binNumber), binLits(binString, "NaN", "Infinity", "-Infinity", "-0")}
	case *taggedOption:
		if vs, ok := c.variants[x]; ok {
			return vs
		}
		some := binFieldOf("value", false, c.codec(x.item))
		vs := []*binNode{
			binStructOf([]binSentinel{{"_tag", "None"}}, nil),
			binStructOf([]binSentinel{{"_tag", "Some"}}, []*binField{some}),
		}
		c.variants[x] = vs
		return vs
	}
	node := c.codec(x)
	if node.kind == binUnion {
		nodes := make([]*binNode, len(node.members))
		for i, member := range node.members {
			nodes[i] = member.node
		}
		return nodes
	}
	return []*binNode{node}
}

// adtVariants are the structs of a variant type's constructors, made
// before their fields so a field may hold the type itself.
func (c *binCompiler) adtVariants(s *Schema) []*binNode {
	if vs, ok := c.variants[s]; ok {
		return vs
	}
	vs := make([]*binNode, len(s.variants))
	for i, v := range s.variants {
		vs[i] = &binNode{kind: binStruct, sentinels: []binSentinel{{"_tag", v.Name}}}
	}
	c.variants[s] = vs
	for i := range s.variants {
		var fields []*binField
		for j, f := range s.fields[i][1:] {
			fields = append(fields, binFieldOf(s.wireKeys[i][j+1], false, c.schema(f)))
		}
		binSetFields(vs[i], fields)
	}
	return vs
}

// block is a `codec` block: a struct of its wire names, its optional fields
// optional keys. Ignoring extra fields makes it Effect's
// Record(String, Unknown), every key an extra entry of JSON text.
func (c *binCompiler) block(b *block) *binNode {
	if b.ignoreExtra {
		return &binNode{kind: binStruct, extra: binLeaf(binJSON)}
	}
	fields := make([]*binField, len(b.fields))
	for i, f := range b.fields {
		fields[i] = binFieldOf(f.wire, f.optional, c.codec(f.codec))
	}
	return binStructOf(nil, fields)
}

// binCodecLayout is the layout of a codec's wire side as the JS runtime's
// matching Effect schema has it; a nil codec is Schema.Void.
func binCodecLayout(c codec) *binNode { return newBinCompiler().codec(c) }

func binPayloadLayout(payload codec) *binNode { return binCodecLayout(payload) }

// binChunkLayout is Schema.NonEmptyArray(success): one element, then the rest.
func binChunkLayout(success codec) *binNode {
	item := binCodecLayout(success)
	return &binNode{kind: binArray, elems: []*binNode{item}, rest: item}
}

// binExitLayout is Rpc.exitSchema: a stream's exit succeeds with Void, and
// fails with the union of the error cases.
func binExitLayout(success codec, errors []codec, stream bool) *binNode {
	c := newBinCompiler()
	value := binLeaf(binUndefined)
	if !stream {
		value = c.codec(success)
	}
	var failures []*binNode
	for _, e := range errors {
		failures = append(failures, c.members(e)...)
	}
	return &binNode{kind: binExit, value: value, fail: binUnionOf(failures), defect: binDefectLayout}
}

// binDefectLayout is Schema.Defect(): the JSON text of the defect's JSON form.
var binDefectLayout = binLeaf(binJSON)

// binUnknownExitLayout is Exit(Never, Never, Defect), the exit of a request
// whose tag the server does not know.
var binUnknownExitLayout = &binNode{kind: binExit, value: binLeaf(binNever), fail: binLeaf(binNever), defect: binDefectLayout}

// binEnvelope is RpcMessage.EncodedSchema, every hole as bytes.
var binEnvelope = func() *binNode {
	str, bytes, undefined := binLeaf(binString), binLeaf(binBytes), binLeaf(binUndefined)
	id := binUnionOf([]*binNode{str, binLeaf(binNumber)})
	field := func(name string, n *binNode) *binField { return binFieldOf(name, false, n) }
	// Schema.optional(S) is an optional key of S | undefined.
	optional := func(name string, n *binNode) *binField {
		return binFieldOf(name, true, binUnionOf([]*binNode{n, undefined}))
	}
	message := func(tag string, fields ...*binField) *binNode {
		return binStructOf([]binSentinel{{"_tag", tag}}, fields)
	}
	headers := &binNode{kind: binArray, rest: &binNode{kind: binArray, elems: []*binNode{str, str}}}
	return binUnionOf([]*binNode{
		message("Request", field("id", id), field("tag", str), field("payload", bytes), field("headers", headers),
			optional("isNotification", binLits(binBool, true)), optional("discard", binLits(binBool, true)), optional("traceId", str),
			optional("spanId", str), optional("sampled", binLeaf(binBool))),
		message("Ack", field("requestId", id)),
		message("Interrupt", field("requestId", id)),
		message("Ping"),
		message("Eof"),
		message("Chunk", field("requestId", id), field("values", bytes)),
		message("Exit", field("requestId", id), field("exit", bytes)),
		message("Defect", field("defect", bytes)),
		message("Pong"),
	})
}()

// binEnvelopeFingerprint is Effect 4.1.2's layout fingerprint of the
// envelope; frames whose layout differs are refused, so it is fixed here.
var binEnvelopeFingerprint = []byte{0xa7, 0x1f, 0xbf, 0x48, 0x86, 0x12, 0xff, 0x6b}
