package flickrt

// binLiterals keeps each JSON primitive's wire kind, as Schema.Literals.
func binLiterals(values []Value) *binNode {
	var nodes []*binNode
	for _, value := range values {
		var kind binKind
		switch value.(type) {
		case string:
			kind = binString
		case bool:
			kind = binBool
		case float64, int32:
			kind = binNumber
		case *jsonNull:
			nodes = append(nodes, binLeaf(binNull))
			continue
		default:
			return &binNode{kind: binInvalid, err: "literal has no binary layout"}
		}
		nodes = append(nodes, binLits(kind, value))
	}
	return binUnionOf(nodes)
}

func (c *binCompiler) stringTree(s *Schema) *binNode {
	return c.tree(s, map[*Schema]*binNode{})
}

func (c *binCompiler) tree(s *Schema, seen map[*Schema]*binNode) *binNode {
	s = s.resolved()
	if prior := seen[s]; prior != nil {
		return prior
	}
	node := &binNode{}
	seen[s] = node
	switch s.kind {
	case kindString, kindBigint:
		node.kind = binString
	case kindBool:
		*node = *binLits(binString, "true", "false")
	case kindNumber, kindInt:
		// The adapter's Number and custom Int32 filter keep competing string members:
		// the numeric text and the Infinity/-Infinity/NaN literals.
		node.kind, node.err = binInvalid, "union members are not uniquely identifiable"
	case kindRefined:
		*node = *c.tree(s.item, seen)
	case kindList:
		node.kind, node.rest = binArray, c.tree(s.item, seen)
	case kindTuple:
		node.kind = binArray
		for _, item := range s.items {
			node.elems = append(node.elems, c.tree(item, seen))
		}
	case kindRecord:
		fields := make([]*binField, len(s.labels))
		for i, label := range s.labels {
			fields[i] = binFieldOf(label, false, c.tree(s.items[i], seen))
		}
		*node = *binStructOf(nil, fields)
	default:
		node.kind, node.err = binInvalid, "schema has no string-tree binary layout"
	}
	return node
}

func (c *binCompiler) decoded(x codec) *binNode {
	switch x := x.(type) {
	case *flipped:
		return c.codec(x.c)
	case *decodedStage:
		return c.decoded(x.decoded)
	case *wire:
		return c.schema(x.s)
	case *transformed:
		if x.typed != nil {
			return c.schema(x.typed)
		}
	case *checkedCodec:
		return c.decoded(x.from)
	case *fallible:
		return c.decoded(x.codec)
	case *wireOnly:
		return c.codec(x.c)
	case *fallbackCodec:
		return c.decoded(x.from)
	}
	// Rpc accepts only PureCodec Json p: a flipped codec's decoded side
	// must therefore be Json. Its schema's JSON tree uses the JSON layout;
	// flipping a Float/String/record codec cannot satisfy that wire type.
	return binLeaf(binJSON)
}
