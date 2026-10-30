package flickrt

import (
	"bytes"
	"cmp"
	"encoding/binary"
	"errors"
	"math"
	"slices"
	"unicode/utf8"
)

// Reading a JSON tree by its layout, as SchemaBinary's decoder does, from
// bytes a peer sent: every length is checked against the region it is in.

type binReader struct {
	b          []byte
	pos, end   int // the region being read
	positional bool
	intern     *[]string // the table a row run hands to the value it reads
	depth      int
}

var errBinShort = errors.New("binary: incomplete value")

// binMaxDepth bounds the nesting of recursive layouts, as Effect does.
const binMaxDepth = 512

func binInvalidData(what string) error { return errors.New("binary: invalid " + what) }

func (r *binReader) byte() (byte, error) {
	if r.pos >= r.end {
		return 0, errBinShort
	}
	r.pos++
	return r.b[r.pos-1], nil
}

func (r *binReader) uvarint() (uint64, error) {
	v, n := binary.Uvarint(r.b[r.pos:r.end])
	if n <= 0 {
		return 0, errBinShort
	}
	r.pos += n
	return v, nil
}

func (r *binReader) take(n uint64) ([]byte, error) {
	if n > uint64(r.end-r.pos) {
		return nil, errBinShort
	}
	r.pos += int(n)
	return r.b[r.pos-int(n) : r.pos], nil
}

// enter narrows the region to the next `n` bytes; leave moves past them.
func (r *binReader) enter(n uint64) (int, error) {
	if n > uint64(r.end-r.pos) {
		return 0, errBinShort
	}
	saved := r.end
	r.end = r.pos + int(n)
	return saved, nil
}

func (r *binReader) leave(saved int) { r.pos, r.end = r.end, saved }

// region reads a value in the next `n` bytes, all of them.
func (r *binReader) region(n *binNode, size uint64) (Value, error) {
	saved, err := r.enter(size)
	if err != nil {
		return nil, err
	}
	v, err := r.checked(n)
	r.leave(saved)
	return v, err
}

func (r *binReader) sized(n *binNode) (Value, error) {
	size, err := r.uvarint()
	if err != nil {
		return nil, err
	}
	return r.region(n, size)
}

// slot reads a field or an element: inline slots have a known size.
func (r *binReader) slot(n *binNode) (Value, error) {
	if binInline(n) {
		size := uint64(0)
		if n.kind == binBool || n.kind == binLiteral {
			size = 1
		}
		return r.region(n, size)
	}
	return r.sized(n)
}

func (r *binReader) checked(n *binNode) (Value, error) {
	v, err := r.value(n)
	if err == nil && r.pos != r.end {
		err = binInvalidData("leftover bytes")
	}
	return v, err
}

func (r *binReader) f64() (float64, error) {
	b, err := r.take(8)
	if err != nil {
		return 0, err
	}
	return math.Float64frombits(binary.LittleEndian.Uint64(b)), nil
}

func (r *binReader) signMag() (float64, error) {
	code, err := r.uvarint()
	m := float64(code >> 1)
	if code&1 == 1 {
		m = -m
	}
	return m, err
}

func (r *binReader) value(n *binNode) (Value, error) {
	if r.depth >= binMaxDepth {
		return nil, binInvalidData("nesting deeper than 512")
	}
	r.depth++
	defer func() { r.depth-- }()
	rest := r.end - r.pos
	switch n.kind {
	case binLiteral:
		v, err := r.value(binLeaf(n.leaf))
		if err == nil && !binHasLit(n, v) {
			err = binInvalidData("literal")
		}
		return v, err
	case binBool:
		b, err := r.byte()
		if err != nil || rest != 1 || b > 1 {
			return nil, binInvalidData("bool")
		}
		return b == 1, nil
	case binNull, binUndefined:
		if rest != 0 {
			return nil, binInvalidData("empty value")
		}
		return JsonNull, nil
	case binNumber:
		return r.number()
	case binString:
		b, _ := r.take(uint64(rest))
		if !utf8.Valid(b) {
			return nil, binInvalidData("utf-8")
		}
		return string(b), nil
	case binBytes:
		b, _ := r.take(uint64(rest))
		return bytes.Clone(b), nil
	case binJSON:
		b, _ := r.take(uint64(rest))
		v, reason := parseJSON(string(b))
		if reason != "" {
			return nil, binInvalidData("json: " + reason)
		}
		return v, nil
	case binStruct:
		if r.positional {
			return r.structPositional(n)
		}
		return r.structFields(n)
	case binArray:
		return r.array(n)
	case binUnion:
		return r.union(n)
	case binExit:
		return r.exit(n)
	case binInvalid:
		return nil, errors.New("binary: " + n.err)
	}
	return nil, binExpected("nothing (Never)")
}

// number is a varint, a varint mantissa and a scale byte, or an f64, told
// apart by the region's length.
func (r *binReader) number() (Value, error) {
	switch rest := r.end - r.pos; {
	case rest == 8:
		return r.f64()
	case rest == 0 || rest > 7:
		return nil, binInvalidData("number")
	}
	x, err := r.signMag()
	if err != nil || r.pos == r.end {
		return x, err
	}
	s, _ := r.byte()
	if r.pos != r.end || s == 0 || int(s) >= len(binPow10) {
		return nil, binInvalidData("decimal")
	}
	return x / binPow10[s], nil
}

// scalar is a field payload carried in its wire kind.
func (r *binReader) scalar(n *binNode, kind uint64) (Value, error) {
	var v Value
	var err error
	switch kind {
	case 3, 4:
		v = kind == 4
	case 1:
		v, err = r.signMag()
	case 2:
		v, err = r.f64()
	case 5:
		var m float64
		var s uint64
		if m, err = r.signMag(); err == nil {
			s, err = r.uvarint()
		}
		if err == nil && (s == 0 || s >= uint64(len(binPow10))) {
			err = binInvalidData("decimal scale")
		}
		if err == nil {
			v = m / binPow10[s]
		}
	}
	if err == nil && n.kind == binLiteral && !binHasLit(n, v) {
		err = binInvalidData("literal")
	}
	return v, err
}

// binWireMask is the wire kinds a layout reads; another is skipped.
func binWireMask(n *binNode) int {
	switch n.kind {
	case binLiteral:
		return binWireMask(binLeaf(n.leaf))
	case binBool:
		return 1<<3 | 1<<4
	case binNumber:
		return 1<<1 | 1<<2 | 1<<5
	}
	return 1
}

func (r *binReader) skip(kind uint64) error {
	var err error
	switch kind {
	case 0:
		var n uint64
		if n, err = r.uvarint(); err == nil {
			_, err = r.take(n)
		}
	case 1:
		_, err = r.uvarint()
	case 2:
		_, err = r.take(8)
	case 5:
		if _, err = r.uvarint(); err == nil {
			_, err = r.uvarint()
		}
	case 6:
		_, err = r.take(4)
	}
	return err
}

// payload reads a field's value in wire kind `kind`; ok is false when the
// layout does not read that kind and it was skipped.
func (r *binReader) payload(n *binNode, kind uint64) (Value, bool, error) {
	if binWireMask(n)&(1<<kind) == 0 {
		return nil, false, r.skip(kind)
	}
	if kind != 0 {
		v, err := r.scalar(n, kind)
		return v, true, err
	}
	v, err := r.sized(n)
	return v, true, err
}

func (r *binReader) structFields(n *binNode) (Value, error) {
	var table *[]string
	if n.extra != nil {
		table, r.intern = r.intern, nil
	}
	vals := make([]Value, len(n.fields))
	var extras *Record
	for r.pos < r.end {
		tag, err := r.uvarint()
		if err != nil {
			return nil, err
		}
		id, kind := tag/8, tag%8
		if id == 0 {
			if extras != nil || kind != 0 {
				return nil, binInvalidData("extra field block")
			}
			size, err := r.uvarint()
			if err != nil {
				return nil, err
			}
			if extras, err = r.extras(n, table, size); err != nil {
				return nil, err
			}
			continue
		}
		at, found := slices.BinarySearchFunc(n.fields, id, func(f *binField, id uint64) int { return cmp.Compare(uint64(f.id), id) })
		if !found {
			if err := r.skip(kind); err != nil {
				return nil, err
			}
			continue
		}
		if vals[at] != nil {
			return nil, binInvalidData("repeated field id")
		}
		v, ok, err := r.payload(n.fields[at].node, kind)
		if err != nil {
			return nil, err
		}
		if ok {
			vals[at] = v
		}
	}
	return n.record(vals, extras)
}

// extras reads the extra keys and their values in the next `size` bytes;
// with a table, a key may be a back-reference to one read before.
func (r *binReader) extras(n *binNode, table *[]string, size uint64) (*Record, error) {
	saved, err := r.enter(size)
	if err != nil {
		return nil, err
	}
	out := newRecord(0)
	for r.pos < r.end {
		code, err := r.uvarint()
		if err != nil {
			return nil, err
		}
		kind, length, ref := code%8, code/8, false
		if table != nil {
			kind, length, ref = code%16/2, code/16, code&1 == 1
		}
		var key string
		if ref {
			if length >= uint64(len(*table)) {
				return nil, binInvalidData("back-reference")
			}
			key = (*table)[length]
		} else {
			b, err := r.take(length)
			if err != nil {
				return nil, err
			}
			key = string(b)
			if table != nil {
				*table = append(*table, key)
			}
		}
		if slices.Contains(n.keys, key) || out.index(key) >= 0 {
			return nil, binInvalidData("extra key " + key)
		}
		v, ok, err := r.payload(n.extra, kind)
		if err != nil {
			return nil, err
		}
		if ok {
			out.put(key, v)
		}
	}
	r.leave(saved)
	return out, nil
}

// record is a struct's JSON tree: its keys in declared order, a variant's
// sentinels as their literals, then the extra keys.
func (n *binNode) record(vals []Value, extras *Record) (Value, error) {
	out := newRecord(len(n.keys))
	for i, k := range n.keys {
		switch o := n.order[i]; {
		case o < 0:
			out.put(k, n.sentinels[-1-o].lit)
		case vals[o] != nil:
			out.put(k, vals[o])
		case !n.fields[o].optional:
			return nil, binMissing(k)
		}
	}
	if extras != nil {
		for i, k := range extras.keys {
			out.put(k, extras.vals[i])
		}
	}
	return out, nil
}

func (r *binReader) structPositional(n *binNode) (Value, error) {
	optional := 0
	for _, f := range n.fields {
		if f.optional {
			optional++
		}
	}
	bitmap, err := r.take(uint64(optional+7) / 8)
	if err != nil {
		return nil, err
	}
	vals := make([]Value, len(n.fields))
	i := 0
	for at, f := range n.fields {
		if f.optional {
			present := bitmap[i/8]&(1<<(i%8)) != 0
			i++
			if !present {
				continue
			}
		}
		if vals[at], err = r.slot(f.node); err != nil {
			return nil, err
		}
	}
	return n.record(vals, nil)
}

func (r *binReader) array(n *binNode) (Value, error) {
	count := uint64(len(n.elems))
	if n.rest != nil {
		var err error
		if count, err = r.uvarint(); err != nil {
			return nil, err
		}
		// A count is backed by the frame's bytes, apart from a bounded allowance.
		limit := uint64(r.end-r.pos) + 1<<20
		if n.rest.kind == binNull || n.rest.kind == binUndefined {
			limit = 1 << 22
		}
		if count > limit {
			return nil, binInvalidData("array count")
		}
	}
	if count < uint64(len(n.elems)) {
		return nil, binMissing("of a tuple")
	}
	if run := binRunOf(n); run != nil && count > 0 {
		return r.run(run, int(count))
	}
	out := make([]Value, count)
	if u := n.rest; len(n.elems) == 0 && u != nil {
		if u.kind == binNumber {
			return r.numbers(out)
		}
		table := r.intern
		r.intern = nil
		for i := range out {
			var err error
			if table != nil {
				out[i], err = r.interned(table, u)
			} else {
				out[i], err = r.slot(u)
			}
			if err != nil {
				return nil, err
			}
		}
		return out, nil
	}
	for i := range out {
		slot := n.rest
		if i < len(n.elems) {
			slot = n.elems[i]
		}
		if n.rest == nil && r.pos == r.end && slot.kind != binNull && slot.kind != binUndefined {
			return nil, binMissing("of a tuple")
		}
		var err error
		if out[i], err = r.slot(slot); err != nil {
			return nil, err
		}
	}
	if n.rest == nil && r.pos < r.end {
		return nil, binInvalidData("tuple length")
	}
	return out, nil
}

func (r *binReader) numbers(out []Value) (Value, error) {
	mode, err := r.byte()
	if err != nil {
		return nil, err
	}
	if mode == 0 && r.end-r.pos != 8*len(out) || mode > 2 {
		return nil, binInvalidData("number run")
	}
	for i := range out {
		switch mode {
		case 0:
			out[i], err = r.f64()
		case 1:
			out[i], err = r.signMag()
		case 2:
			var code uint64
			if code, err = r.uvarint(); err != nil {
				return nil, err
			}
			scale, c := code%16, code/16
			if scale >= uint64(len(binPow10)) {
				return nil, binInvalidData("decimal")
			}
			x := float64(c >> 1)
			if c&1 == 1 {
				x = -x
			}
			out[i] = x / binPow10[scale]
		}
		if err != nil {
			return nil, err
		}
	}
	return out, nil
}

func (r *binReader) interned(table *[]string, n *binNode) (Value, error) {
	code, err := r.uvarint()
	if err != nil {
		return nil, err
	}
	if code&1 == 1 {
		if code>>1 >= uint64(len(*table)) {
			return nil, binInvalidData("back-reference")
		}
		return (*table)[code>>1], nil
	}
	v, err := r.region(n, code/2)
	s, _ := v.(string)
	*table = append(*table, s)
	return v, err
}

// Slots of a row besides a field's index: the extra block, and a field
// this layout does not have.
const (
	binExtraSlot   = -1
	binUnknownSlot = -2
)

func (r *binReader) run(st *binNode, count int) (Value, error) {
	out := make([]Value, count)
	var shapes [][]int
	tables := make([][]string, len(st.fields))
	for i := range out {
		size, err := r.uvarint()
		if err != nil {
			return nil, err
		}
		saved, err := r.enter(size)
		if err != nil {
			return nil, err
		}
		if out[i], err = r.row(st, &shapes, tables); err != nil {
			return nil, err
		}
		r.leave(saved)
	}
	return out, nil
}

func (r *binReader) row(st *binNode, shapes *[][]int, tables [][]string) (Value, error) {
	vals := make([]Value, len(st.fields))
	var extras *Record
	code, err := r.uvarint()
	if err != nil {
		return nil, err
	}
	if code > uint64(len(*shapes)) {
		return nil, binInvalidData("row shape")
	}
	if code > 0 {
		for _, slot := range (*shapes)[code-1] {
			if err := r.runSlot(st, slot, vals, &extras, tables); err != nil {
				return nil, err
			}
		}
		if r.pos != r.end {
			return nil, binInvalidData("leftover bytes")
		}
		return st.record(vals, extras)
	}
	// shortcut: a new shape is declared by field ids as without a fingerprint; the envelope, the one fingerprinted layout, has no row run
	var declared []int
	for r.pos < r.end {
		id, err := r.uvarint()
		if err != nil {
			return nil, err
		}
		slot := binExtraSlot
		if id != 0 {
			at, found := slices.BinarySearchFunc(st.fields, id, func(f *binField, id uint64) int { return cmp.Compare(uint64(f.id), id) })
			slot = at
			if !found {
				slot = binUnknownSlot
			}
		}
		declared = append(declared, slot)
		if err := r.runSlot(st, slot, vals, &extras, tables); err != nil {
			return nil, err
		}
	}
	if len(st.fields) <= 30 {
		*shapes = append(*shapes, declared)
	}
	return st.record(vals, extras)
}

// runSlot reads a slot of a row: a back-reference to a value interned
// before, or a region of doubled length.
func (r *binReader) runSlot(st *binNode, slot int, vals []Value, extras **Record, tables [][]string) error {
	code, err := r.uvarint()
	if err != nil {
		return err
	}
	if slot >= 0 && vals[slot] != nil {
		return binInvalidData("repeated field id")
	}
	if code&1 == 1 {
		if slot < 0 {
			return nil
		}
		if code>>1 >= uint64(len(tables[slot])) {
			return binInvalidData("back-reference")
		}
		vals[slot] = tables[slot][code>>1]
		return nil
	}
	switch slot {
	case binExtraSlot:
		*extras, err = r.extras(st, nil, code/2)
		return err
	case binUnknownSlot:
		_, err = r.take(code / 2)
		return err
	}
	node := st.fields[slot].node
	kind := binInternKind(node)
	if kind == binInternElements || kind == binInternKeys {
		r.intern = &tables[slot]
	}
	vals[slot], err = r.region(node, code/2)
	r.intern = nil
	if s, ok := vals[slot].(string); ok && kind == binInternSelf {
		tables[slot] = append(tables[slot], s)
	}
	return err
}

func (r *binReader) union(n *binNode) (Value, error) {
	var found *binMember
	if r.positional {
		pos, err := r.uvarint()
		if err != nil {
			return nil, err
		}
		for _, m := range n.members {
			if uint64(m.pos) == pos {
				found = m
			}
		}
	} else {
		kind, err := r.byte()
		if err != nil {
			return nil, err
		}
		var tag uint32
		if kind == binKVariant {
			b, err := r.take(4)
			if err != nil {
				return nil, err
			}
			tag = binary.LittleEndian.Uint32(b)
		}
		for _, m := range n.members {
			if m.variant && kind == binKVariant && m.tag == tag || !m.variant && m.kind == kind {
				found = m
			}
		}
	}
	if found == nil {
		// shortcut: Effect reads an unknown member as absent, which drops an optional field holding it; here it fails the value, until peers evolve schemas apart
		return nil, binInvalidData("union member")
	}
	return r.checked(found.node)
}

func (r *binReader) exit(n *binNode) (Value, error) {
	tag, err := r.byte()
	if err != nil {
		return nil, err
	}
	switch tag {
	case 0:
		v, err := r.checked(n.value)
		return Rec("_tag", "Success", "value", v), err
	case 1:
		count, err := r.uvarint()
		cause := []Value{}
		for i := uint64(0); err == nil && i < count; i++ {
			var size uint64
			var saved int
			if size, err = r.uvarint(); err == nil {
				saved, err = r.enter(size)
			}
			if err != nil {
				break
			}
			var reason Value
			if reason, err = r.reason(n); reason != nil {
				cause = append(cause, reason)
			}
			r.leave(saved)
		}
		return Rec("_tag", "Failure", "cause", cause), err
	}
	return nil, binInvalidData("exit")
}

// reason reads a cause reason; one of an unknown kind is skipped, as
// Effect does.
func (r *binReader) reason(n *binNode) (Value, error) {
	tag, err := r.byte()
	if err != nil {
		return nil, err
	}
	switch rest := r.end - r.pos; tag {
	case 0:
		v, err := r.checked(n.fail)
		return Rec("_tag", "Fail", "error", v), err
	case 1:
		v, err := r.checked(n.defect)
		return Rec("_tag", "Die", "defect", v), err
	case 2:
		if rest != 0 {
			return nil, binInvalidData("interrupt")
		}
		return Rec("_tag", "Interrupt", "fiberId", JsonNull), nil
	case 3:
		if rest != 8 {
			return nil, binInvalidData("interrupt")
		}
		id, err := r.f64()
		return Rec("_tag", "Interrupt", "fiberId", id), err
	}
	r.pos = r.end
	return nil, nil
}
