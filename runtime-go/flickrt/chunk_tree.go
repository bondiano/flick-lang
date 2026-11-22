package flickrt

import "math"

// Chunk is an immutable sequence. A leaf holds a slice of elements. Branches
// share their children and cache their length and height. Their children's
// heights differ by at most one.
type Chunk struct {
	leaf        []Value
	left, right *Chunk
	size        int
	height      int
}

const chunkLeafLimit = 32

var ChunkEmpty Value = &Chunk{}

func ChunkFromList(xs Value) Value { return chunkOf(xs) }
func ChunkToList(c Value) Value    { return items(c) }
func ChunkSize(c Value) Value      { return int32(c.(*Chunk).length()) }

func chunkOf(xs Value) *Chunk { return &Chunk{leaf: List(xs)} }
func items(c Value) []Value   { return c.(*Chunk).flat() }

func (c *Chunk) length() int {
	if c.left == nil {
		return len(c.leaf)
	}
	return c.size
}

func chunkBranch(left, right *Chunk) *Chunk {
	leftSize, rightSize := left.length(), right.length()
	if rightSize > math.MaxInt-leftSize {
		panic("Chunk length overflow")
	}
	return &Chunk{
		left:   left,
		right:  right,
		size:   leftSize + rightSize,
		height: 1 + max(left.height, right.height),
	}
}

func joinChunks(left, right *Chunk) *Chunk {
	switch {
	case left.length() == 0:
		return right
	case right.length() == 0:
		return left
	case left.height > right.height+1:
		return balanceChunks(left.left, joinChunks(left.right, right))
	case right.height > left.height+1:
		return balanceChunks(joinChunks(left, right.left), right.right)
	case left.height == 0 && right.height == 0 && len(left.leaf)+len(right.leaf) <= chunkLeafLimit:
		xs := make([]Value, len(left.leaf)+len(right.leaf))
		copy(xs, left.leaf)
		copy(xs[len(left.leaf):], right.leaf)
		return &Chunk{leaf: xs}
	default:
		return chunkBranch(left, right)
	}
}

// After a join down the taller side, a single or double rotation restores
// balance. Each rotation only constructs branches and keeps leaf order.
func balanceChunks(left, right *Chunk) *Chunk {
	if left.height > right.height+1 {
		if left.left.height < left.right.height {
			middle := left.right
			return chunkBranch(chunkBranch(left.left, middle.left), chunkBranch(middle.right, right))
		}
		return chunkBranch(left.left, chunkBranch(left.right, right))
	}
	if right.height > left.height+1 {
		if right.right.height < right.left.height {
			middle := right.left
			return chunkBranch(chunkBranch(left, middle.left), chunkBranch(middle.right, right.right))
		}
		return chunkBranch(chunkBranch(left, right.left), right.right)
	}
	return chunkBranch(left, right)
}

func (c *Chunk) at(i int) Value {
	for c.left != nil {
		leftSize := c.left.length()
		if i < leftSize {
			c = c.left
		} else {
			i -= leftSize
			c = c.right
		}
	}
	return c.leaf[i]
}

// slice shares whole subtrees and views the boundary leaves.
func (c *Chunk) slice(start, end int) *Chunk {
	if start == end {
		return ChunkEmpty.(*Chunk)
	}
	if start == 0 && end == c.length() {
		return c
	}
	if c.left == nil {
		return &Chunk{leaf: c.leaf[start:end:end]}
	}
	leftSize := c.left.length()
	if end <= leftSize {
		return c.left.slice(start, end)
	}
	if start >= leftSize {
		return c.right.slice(start-leftSize, end-leftSize)
	}
	return joinChunks(c.left.slice(start, leftSize), c.right.slice(0, end-leftSize))
}

// walk visits leaves in sequence order and stops as soon as visit returns
// false. Each full traversal visits each branch and leaf once.
func (c *Chunk) walk(reverse bool, visit func(Value) bool) bool {
	if c.left != nil {
		if reverse {
			return c.right.walk(true, visit) && c.left.walk(true, visit)
		}
		return c.left.walk(false, visit) && c.right.walk(false, visit)
	}
	if reverse {
		for i := len(c.leaf) - 1; i >= 0; i-- {
			if !visit(c.leaf[i]) {
				return false
			}
		}
	} else {
		for _, x := range c.leaf {
			if !visit(x) {
				return false
			}
		}
	}
	return true
}

func (c *Chunk) flat() []Value {
	if c.left == nil {
		return c.leaf
	}
	xs := make([]Value, c.size)
	c.copyTo(xs)
	return xs
}

func (c *Chunk) copyTo(xs []Value) {
	if c.left == nil {
		copy(xs, c.leaf)
		return
	}
	leftSize := c.left.length()
	c.left.copyTo(xs[:leftSize])
	c.right.copyTo(xs[leftSize:])
}

func (c *Chunk) fold(fb *Fiber, acc, f Value, reverse bool) Value {
	if c.left != nil {
		if reverse {
			return c.left.fold(fb, c.right.fold(fb, acc, f, true), f, true)
		}
		return c.right.fold(fb, c.left.fold(fb, acc, f, false), f, false)
	}
	if reverse {
		for i := len(c.leaf) - 1; i >= 0; i-- {
			acc = Apply2(fb, f, acc, c.leaf[i])
		}
	} else {
		for _, x := range c.leaf {
			acc = Apply2(fb, f, acc, x)
		}
	}
	return acc
}

// chunkLeaves walks a chunk incrementally, keeping only unvisited right
// subtrees. The total work to visit all leaves is linear.
type chunkLeaves struct {
	current *Chunk
	pending []*Chunk
}

func (it *chunkLeaves) next() []Value {
	c := it.current
	if c == nil {
		last := len(it.pending) - 1
		if last < 0 {
			return nil
		}
		c, it.pending = it.pending[last], it.pending[:last]
	}
	for c.left != nil {
		it.pending = append(it.pending, c.right)
		c = c.left
	}
	it.current = nil
	return c.leaf
}

func (c *Chunk) equals(other *Chunk) bool {
	return c.equalsWith(other, false)
}

func (c *Chunk) equalsWith(other *Chunk, strictZero bool) bool {
	if c.length() != other.length() {
		return false
	}
	if c.left == nil && other.left == nil {
		return equalValues(c.leaf, other.leaf, strictZero)
	}
	left, right := chunkLeaves{current: c}, chunkLeaves{current: other}
	xs, ys := left.next(), right.next()
	for len(xs) != 0 {
		n := min(len(xs), len(ys))
		for i := range n {
			if !equalValues(xs[i], ys[i], strictZero) {
				return false
			}
		}
		xs, ys = xs[n:], ys[n:]
		if len(xs) == 0 {
			xs = left.next()
		}
		if len(ys) == 0 {
			ys = right.next()
		}
	}
	return true
}
