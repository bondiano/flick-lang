package flickrt

import (
	"encoding/binary"
	"errors"
	"math"
	"slices"
	"sort"
)

// Writing a JSON tree by its layout, as SchemaBinary's encoder does. A
// value is written into a region whose length the reader knows: a string
// or a number takes what is left of it, a field or an element that is not
// fixed-size gets a uvarint length first.

type binWriter struct {
	buf []byte
	// positional is fingerprint mode, the envelope's: unions by position,
	// structs as a presence bitmap and their fields in order.
	positional bool
	// intern is the string table a row run hands to the struct or string
	// array it is writing.
	intern *binTable
}

// binTable interns the strings of one field of a row run: a repeat is a
// back-reference to its first appearance.
type binTable struct {
	refs     map[string]int
	hits     int
	disabled bool
}

func (t *binTable) ref(s string) (int, bool) {
	i, ok := t.refs[s]
	if ok {
		t.hits++
	}
	return i, ok
}

func (t *binTable) add(s string) {
	// Effect stops interning a table of 64 strings that none repeated.
	if t.hits == 0 && len(t.refs) >= 64 {
		t.disabled, t.refs = true, nil
		return
	}
	if t.refs == nil {
		t.refs = map[string]int{}
	}
	t.refs[s] = len(t.refs)
}

func binTableAt(tables []*binTable, i int) *binTable {
	if tables[i] == nil {
		tables[i] = &binTable{}
	}
	return tables[i]
}

// What a row run interns for a field: the string itself, the strings of an
// array, or the extra keys of a struct.
const (
	binInternNone = iota
	binInternSelf
	binInternElements
	binInternKeys
)

func binInternKind(n *binNode) int {
	switch {
	case n.kind == binString:
		return binInternSelf
	case n.kind == binArray && len(n.elems) == 0 && n.rest != nil && n.rest.kind == binString:
		return binInternElements
	case n.kind == binStruct && n.extra != nil:
		return binInternKeys
	}
	return binInternNone
}

// binInline is a slot written without a length: a bool's byte, or nothing.
func binInline(n *binNode) bool {
	switch n.kind {
	case binLiteral:
		return n.leaf == binBool
	case binBool, binNull, binUndefined:
		return true
	}
	return false
}

func binHasLit(n *binNode, v Value) bool { return slices.Contains(n.lits, v) }

func binExpected(what string) error { return errors.New("binary: expected " + what) }

func binMissing(key string) error { return errors.New("binary: missing key " + key) }

const (
	binVarintMax   = 1<<48 - 1 // numbers in up to seven varint bytes
	binMantissaMax = 1<<41 - 1 // a short decimal's mantissa
)

var binPow10 = [...]float64{1, 10, 100, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8}

func binIsVarint(x float64) bool { return x == math.Trunc(x) && math.Abs(x) <= binVarintMax }

// binRound is JS Math.round: halves toward +Infinity.
func binRound(x float64) float64 {
	r := math.Floor(x)
	if x-r >= 0.5 {
		r++
	}
	return r
}

// binDecimalScale is the scale that makes `x` an exact short-decimal
// mantissa, or 0.
func binDecimalScale(x float64) int {
	for s := 1; s < len(binPow10); s++ {
		m := binRound(x * binPow10[s])
		if !(m >= -binMantissaMax && m <= binMantissaMax) {
			return 0
		}
		if m/binPow10[s] == x {
			return s
		}
	}
	return 0
}

func (w *binWriter) uvarint(n uint64) { w.buf = binary.AppendUvarint(w.buf, n) }

func (w *binWriter) f64(x float64) {
	w.buf = binary.LittleEndian.AppendUint64(w.buf, math.Float64bits(x))
}

// signMag writes an integer as its magnitude doubled plus its sign, which
// keeps -0.
func (w *binWriter) signMag(x float64) {
	code := uint64(math.Abs(x)) * 2
	if math.Signbit(x) {
		code++
	}
	w.uvarint(code)
}

func (w *binWriter) number(x float64) {
	if binIsVarint(x) {
		w.signMag(x)
		return
	}
	if s := binDecimalScale(x); s > 0 {
		w.signMag(binRound(x * binPow10[s]))
		w.buf = append(w.buf, byte(s))
		return
	}
	w.f64(x)
}

// begin reserves a length byte; end writes there the length of what
// follows it, times `scale`: a row run doubles it to leave odd codes for
// back-references.
func (w *binWriter) begin() int {
	w.buf = append(w.buf, 0)
	return len(w.buf) - 1
}

func (w *binWriter) end(mark int, scale uint64) {
	code := uint64(len(w.buf)-mark-1) * scale
	if code < 0x80 {
		w.buf[mark] = byte(code)
		return
	}
	w.buf = slices.Replace(w.buf, mark, mark+1, binary.AppendUvarint(nil, code)...)
}

func (w *binWriter) sized(n *binNode, v Value, scale uint64) error {
	mark := w.begin()
	if err := w.value(n, v); err != nil {
		return err
	}
	w.end(mark, scale)
	return nil
}

func (w *binWriter) value(n *binNode, v Value) error {
	switch n.kind {
	case binLiteral:
		if !binHasLit(n, v) {
			return binExpected("one of the literals")
		}
		return w.value(binLeaf(n.leaf), v)
	case binBool:
		b, ok := v.(bool)
		if !ok {
			return binExpected("a boolean")
		}
		w.buf = append(w.buf, 0)
		if b {
			w.buf[len(w.buf)-1] = 1
		}
	case binNull:
		if v != JsonNull {
			return binExpected("null")
		}
	case binUndefined:
		// Void is null in the JSON trees.
		if v != nil && v != JsonNull {
			return binExpected("undefined")
		}
	case binNumber:
		x, ok := number(v)
		if !ok {
			return binExpected("a number")
		}
		w.number(x)
	case binString:
		s, ok := v.(string)
		if !ok {
			return binExpected("a string")
		}
		w.buf = append(w.buf, s...)
	case binBytes:
		b, ok := v.([]byte)
		if !ok {
			return binExpected("bytes")
		}
		w.buf = append(w.buf, b...)
	case binJSON:
		if !isJSONTree(v) {
			return binExpected("a JSON value")
		}
		w.buf = append(w.buf, JsonStringify(v).(string)...)
	case binStruct:
		r, ok := v.(*Record)
		if !ok {
			return binExpected("an object")
		}
		if w.positional {
			return w.structPositional(n, r)
		}
		return w.structFields(n, r)
	case binArray:
		xs, ok := v.([]Value)
		if !ok {
			return binExpected("an array")
		}
		return w.array(n, xs)
	case binUnion:
		return w.union(n, v)
	case binExit:
		return w.exit(n, v)
	case binNever:
		return binExpected("nothing (Never)")
	case binInvalid:
		return errors.New("binary: " + n.err)
	}
	return nil
}

// binWire is how a field or an extra entry carries `v`: its wire kind in
// the low three bits, and a decimal's scale above them. Booleans live in
// the kind, numbers are varints, f64 or decimals, the rest is sized.
func binWire(n *binNode, v Value) (uint64, error) {
	switch n.kind {
	case binLiteral:
		if !binHasLit(n, v) {
			return 0, binExpected("one of the literals")
		}
		return binWire(binLeaf(n.leaf), v)
	case binBool:
		b, ok := v.(bool)
		if !ok {
			return 0, binExpected("a boolean")
		}
		if b {
			return 4, nil
		}
		return 3, nil
	case binNumber:
		x, ok := number(v)
		if !ok {
			return 0, binExpected("a number")
		}
		if binIsVarint(x) {
			return 1, nil
		}
		if s := binDecimalScale(x); s > 0 {
			return uint64(s)*8 + 5, nil
		}
		return 2, nil
	}
	return 0, nil
}

func (w *binWriter) payload(n *binNode, v Value, wire uint64) error {
	x, _ := number(v)
	switch wire % 8 {
	case 3, 4:
	case 1:
		w.signMag(x)
	case 2:
		w.f64(x)
	case 5:
		w.signMag(binRound(x * binPow10[wire/8]))
		w.uvarint(wire / 8)
	default:
		return w.sized(n, v, 1)
	}
	return nil
}

// structFields writes each present field as its id and wire kind, then its
// payload; extra keys come first, as a block under the id 0.
func (w *binWriter) structFields(n *binNode, r *Record) error {
	if n.extra != nil {
		table := w.intern
		w.intern = nil
		if keys := binExtraKeys(n, r); len(keys) > 0 {
			w.uvarint(0)
			mark := w.begin()
			if err := w.extras(n, keys, r, table); err != nil {
				return err
			}
			w.end(mark, 1)
		}
	}
	for _, f := range n.fields {
		at := r.index(f.name)
		if at < 0 {
			if f.optional {
				continue
			}
			return binMissing(f.name)
		}
		wire, err := binWire(f.node, r.vals[at])
		if err != nil {
			return err
		}
		w.uvarint(uint64(f.id)*8 + wire%8)
		if err := w.payload(f.node, r.vals[at], wire); err != nil {
			return err
		}
	}
	return nil
}

// binExtraKeys are the keys no field names, by their UTF-8 bytes.
func binExtraKeys(n *binNode, r *Record) []string {
	var out []string
	for _, k := range r.keys {
		if !slices.Contains(n.keys, k) {
			out = append(out, k)
		}
	}
	sort.Strings(out)
	return out
}

// extras writes each extra key and its value; with a table, a key seen
// before is a back-reference.
func (w *binWriter) extras(n *binNode, keys []string, r *Record, table *binTable) error {
	for _, k := range keys {
		v := r.vals[r.index(k)]
		wire, err := binWire(n.extra, v)
		if err != nil {
			return err
		}
		kind := wire % 8
		switch {
		case table == nil:
			w.uvarint(uint64(len(k))*8 + kind)
			w.buf = append(w.buf, k...)
		default:
			if !table.disabled {
				if ref, ok := table.ref(k); ok {
					w.uvarint(uint64(ref)*16 + kind*2 + 1)
					break
				}
				table.add(k)
			}
			w.uvarint(uint64(len(k))*16 + kind*2)
			w.buf = append(w.buf, k...)
		}
		if err := w.payload(n.extra, v, wire); err != nil {
			return err
		}
	}
	return nil
}

// structPositional writes a bitmap of the optional fields present, then
// each present field in order.
func (w *binWriter) structPositional(n *binNode, r *Record) error {
	optional := 0
	for _, f := range n.fields {
		if f.optional {
			optional++
		}
	}
	bitmap := len(w.buf)
	w.buf = append(w.buf, make([]byte, (optional+7)/8)...)
	i := 0
	for _, f := range n.fields {
		at := r.index(f.name)
		if f.optional {
			if at >= 0 {
				w.buf[bitmap+i/8] |= 1 << (i % 8)
			}
			i++
		}
		if at < 0 {
			if f.optional {
				continue
			}
			return binMissing(f.name)
		}
		var err error
		if binInline(f.node) {
			err = w.value(f.node, r.vals[at])
		} else {
			err = w.sized(f.node, r.vals[at], 1)
		}
		if err != nil {
			return err
		}
	}
	// shortcut: no extra keys in fingerprint mode, which only the envelope uses and which has none; port them if a fingerprinted layout gains a record
	return nil
}

// binRunOf is the struct every element of an array has, which makes the
// array a row run: Schema.Array(S) and Schema.NonEmptyArray(S).
func binRunOf(n *binNode) *binNode {
	u := n.rest
	if u == nil || u.kind != binStruct {
		return nil
	}
	for _, e := range n.elems {
		if e != u {
			return nil
		}
	}
	return u
}

func (w *binWriter) array(n *binNode, xs []Value) error {
	if n.rest == nil && len(xs) > len(n.elems) {
		return errors.New("binary: unexpected element")
	}
	if len(xs) < len(n.elems) {
		return binMissing("of a tuple")
	}
	if n.rest != nil {
		w.uvarint(uint64(len(xs)))
	}
	if run := binRunOf(n); run != nil && len(xs) > 0 {
		return w.run(run, xs)
	}
	if u := n.rest; len(n.elems) == 0 && u != nil {
		if u.kind == binNumber {
			return w.numbers(xs)
		}
		table := w.intern
		w.intern = nil
		for _, x := range xs {
			var err error
			switch {
			case table != nil:
				err = w.interned(table, u, x)
			case binInline(u):
				err = w.value(u, x)
			default:
				err = w.sized(u, x, 1)
			}
			if err != nil {
				return err
			}
		}
		return nil
	}
	for i, x := range xs {
		slot := n.rest
		if i < len(n.elems) {
			slot = n.elems[i]
		}
		var err error
		if binInline(slot) {
			err = w.value(slot, x)
		} else {
			err = w.sized(slot, x, 1)
		}
		if err != nil {
			return err
		}
	}
	return nil
}

// numbers writes an array of numbers under one mode: all varints, all
// short decimals, or all f64.
func (w *binWriter) numbers(xs []Value) error {
	varint, decimal := true, true
	scales := make([]int, len(xs))
	for i, v := range xs {
		x, ok := number(v)
		if !ok {
			return binExpected("a number")
		}
		if binIsVarint(x) {
			decimal = decimal && math.Abs(x) <= binMantissaMax
		} else {
			varint = false
			scales[i] = binDecimalScale(x)
			decimal = decimal && scales[i] > 0
		}
		if !varint && !decimal {
			break
		}
	}
	switch {
	case varint:
		w.buf = append(w.buf, 1)
		for _, v := range xs {
			x, _ := number(v)
			w.signMag(x)
		}
	case decimal:
		w.buf = append(w.buf, 2)
		for i, v := range xs {
			x, _ := number(v)
			m := x
			if scales[i] > 0 {
				m = binRound(x * binPow10[scales[i]])
			}
			code := uint64(math.Abs(m)) * 2
			if math.Signbit(m) {
				code++
			}
			w.uvarint(code*16 + uint64(scales[i]))
		}
	default:
		w.buf = append(w.buf, 0)
		for _, v := range xs {
			x, _ := number(v)
			w.f64(x)
		}
	}
	return nil
}

// interned writes a string of a table: a back-reference, or the string in
// a region of doubled length.
func (w *binWriter) interned(t *binTable, n *binNode, v Value) error {
	if !t.disabled {
		s, _ := v.(string)
		if ref, ok := t.ref(s); ok {
			w.uvarint(uint64(ref)*2 + 1)
			return nil
		}
		t.add(s)
	}
	return w.sized(n, v, 2)
}

// run writes the rows of an array of structs: each row names its shape,
// the set of fields present, by its index among the shapes seen, or
// declares a new one by its field ids; string fields are interned.
func (w *binWriter) run(st *binNode, xs []Value) error {
	wide := len(st.fields) > 30 // a shape is a 32-bit mask
	var shapes []uint32
	tables := make([]*binTable, len(st.fields))
	for _, x := range xs {
		r, ok := x.(*Record)
		if !ok {
			return binExpected("an object")
		}
		mark := w.begin()
		if err := w.row(st, r, wide, &shapes, tables); err != nil {
			return err
		}
		w.end(mark, 1)
	}
	return nil
}

func (w *binWriter) row(st *binNode, r *Record, wide bool, shapes *[]uint32, tables []*binTable) error {
	var extras []string
	var mask uint32
	if st.extra != nil {
		if extras = binExtraKeys(st, r); len(extras) > 0 {
			mask = 1 << 30
		}
	}
	for i, f := range st.fields {
		if r.index(f.name) >= 0 {
			if !wide {
				mask |= 1 << i
			}
		} else if !f.optional {
			return binMissing(f.name)
		}
	}
	shape := slices.Index(*shapes, mask)
	if shape >= 0 {
		w.uvarint(uint64(shape) + 1)
	} else {
		w.uvarint(0)
		if !wide {
			*shapes = append(*shapes, mask)
		}
	}
	declare := shape < 0
	if len(extras) > 0 {
		if declare {
			w.uvarint(0)
		}
		mark := w.begin()
		if err := w.extras(st, extras, r, nil); err != nil {
			return err
		}
		w.end(mark, 2)
	}
	for i, f := range st.fields {
		at := r.index(f.name)
		if at < 0 {
			continue
		}
		if declare {
			w.uvarint(uint64(f.id))
		}
		var err error
		switch kind := binInternKind(f.node); kind {
		case binInternSelf:
			err = w.interned(binTableAt(tables, i), f.node, r.vals[at])
		default:
			if kind != binInternNone {
				w.intern = binTableAt(tables, i)
			}
			err = w.sized(f.node, r.vals[at], 2)
			w.intern = nil
		}
		if err != nil {
			return err
		}
	}
	return nil
}

func (w *binWriter) union(n *binNode, v Value) error {
	for _, m := range n.members {
		if !m.matches(v) {
			continue
		}
		switch {
		case w.positional:
			w.uvarint(uint64(m.pos))
		case m.variant:
			w.buf = binary.LittleEndian.AppendUint32(append(w.buf, binKVariant), m.tag)
		default:
			w.buf = append(w.buf, m.kind)
		}
		return w.value(m.node, v)
	}
	return binExpected("a member of the union")
}

func (m *binMember) matches(v Value) bool {
	if m.variant {
		r, ok := v.(*Record)
		if !ok {
			return false
		}
		for _, s := range m.node.sentinels {
			if at := r.index(s.key); at < 0 || r.vals[at] != s.lit {
				return false
			}
		}
		return true
	}
	switch n := m.node; n.kind {
	case binLiteral:
		return binHasLit(n, v)
	case binBool:
		_, ok := v.(bool)
		return ok
	case binNull:
		return v == JsonNull
	case binUndefined:
		return v == nil
	case binNumber:
		_, ok := number(v)
		return ok
	case binString:
		_, ok := v.(string)
		return ok
	case binBytes:
		_, ok := v.([]byte)
		return ok
	case binArray:
		_, ok := v.([]Value)
		return ok
	case binStruct, binExit:
		_, ok := v.(*Record)
		return ok
	case binJSON:
		return true
	}
	return false
}

// exit writes an Exit's JSON tree: 0 and the value, or 1 and the cause's
// reasons, each sized: a Fail's error, a Die's defect, an Interrupt with
// or without its fiber id.
func (w *binWriter) exit(n *binNode, v Value) error {
	r, ok := v.(*Record)
	if !ok {
		return binExpected("an Exit")
	}
	switch recordField(r, "_tag") {
	case "Success":
		w.buf = append(w.buf, 0)
		return w.value(n.value, recordField(r, "value"))
	case "Failure":
		cause, ok := recordField(r, "cause").([]Value)
		if !ok {
			return binExpected("a cause")
		}
		w.buf = append(w.buf, 1)
		w.uvarint(uint64(len(cause)))
		for _, c := range cause {
			mark := w.begin()
			if err := w.reason(n, c); err != nil {
				return err
			}
			w.end(mark, 1)
		}
		return nil
	}
	return binExpected("an Exit")
}

func (w *binWriter) reason(n *binNode, v Value) error {
	r, ok := v.(*Record)
	if !ok {
		return binExpected("a cause reason")
	}
	switch recordField(r, "_tag") {
	case "Fail":
		w.buf = append(w.buf, 0)
		return w.value(n.fail, recordField(r, "error"))
	case "Die":
		w.buf = append(w.buf, 1)
		return w.value(n.defect, recordField(r, "defect"))
	case "Interrupt":
		if id, ok := number(recordField(r, "fiberId")); ok {
			w.buf = append(w.buf, 3)
			w.f64(id)
			return nil
		}
		w.buf = append(w.buf, 2)
		return nil
	}
	return binExpected("a cause reason")
}
