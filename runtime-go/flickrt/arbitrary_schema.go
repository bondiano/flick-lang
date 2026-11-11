package flickrt

import (
	"math"
	"strconv"
)

// The compiler of a schema's generator:
// a node per AST node and inherited constraint, settled together so a
// recursive type pays for each step into itself and prefers its base
// cases when the budget runs out.

type arbCompiler struct {
	cache   map[*arbAST]map[*arbConstraint]*arbGen
	nodes   []*arbGen
	bodies  map[*arbGen]*arbGen // a suspension's node and its body's
	pending []func()
}

// arbFailure is what a node being compiled cannot be generated for; the
// compiler adds the node's path.
type arbFailure string

func arbFail(what string) { panic(arbFailure(what)) }

// arbError is the error of a schema that cannot be generated, at the path
// of the node in it.
func arbError(what string, path []any) Defect {
	message := "Error: Unable to derive an arbitrary for " + what
	if len(path) > 0 {
		message += "\n  at "
		for _, key := range path {
			switch k := key.(type) {
			case int:
				message += "[" + strconv.Itoa(k) + "]"
			default:
				message += "[" + QuoteJSON(k.(string)) + "]"
			}
		}
	}
	return Defect{message}
}

// at is the path of a node's part at `key`: an index or a property name.
func at(path []any, key any) []any { return append(path[:len(path):len(path)], key) }

func discardAll(*genState) *sample { return nil }

func infiniteCost() float64 { return infinity }

func compiledGen(deps []*arbGen, cost func() float64, generate func(*genState) *sample) *arbGen {
	return &arbGen{minCost: infinity, deps: deps, computeMinCost: cost, generate: generate}
}

// compileSchema is the generator of a schema's values.
func compileSchema(s *Schema) *arbGen {
	asts := &arbASTs{nodes: map[astKey]*arbAST{}}
	return compileAST(asts.of(s, false))
}

func compileAST(ast *arbAST) *arbGen {
	c := &arbCompiler{cache: map[*arbAST]map[*arbConstraint]*arbGen{}, bodies: map[*arbGen]*arbGen{}}
	root := c.recur(ast, nil, nil)
	for i := 0; i < len(c.pending); i++ {
		c.pending[i]()
	}
	dependents := analyzeDependencies(c.nodes, c.bodies)
	var queue []*arbGen
	for _, n := range c.nodes {
		if n.recursive {
			queue = append(queue, n)
		}
	}
	for i := 0; i < len(queue); i++ {
		if n := queue[i]; !n.mayRecurse {
			n.mayRecurse = true
			queue = append(queue, dependents[n]...)
		}
	}
	queue = append([]*arbGen(nil), c.nodes...)
	queued := map[*arbGen]bool{}
	for _, n := range queue {
		queued[n] = true
	}
	for i := 0; i < len(queue); i++ {
		n := queue[i]
		delete(queued, n)
		next := n.computeMinCost()
		if next >= n.minCost {
			continue
		}
		n.minCost = next
		for _, d := range dependents[n] {
			if !queued[d] {
				queued[d] = true
				queue = append(queue, d)
			}
		}
	}
	if root.minCost == infinity {
		panic(arbError("a schema without a finite generation path", nil))
	}
	return root
}

func (c *arbCompiler) recur(ast *arbAST, path []any, inherited *arbConstraint) *arbGen {
	entries := c.cache[ast]
	if cached, ok := entries[inherited]; ok {
		return cached
	}
	if entries == nil {
		entries = map[*arbConstraint]*arbGen{}
		c.cache[ast] = entries
	}
	node := compiledGen(nil, infiniteCost, discardAll)
	entries[inherited] = node
	c.nodes = append(c.nodes, node)
	c.pending = append(c.pending, func() {
		defer func() {
			switch r := recover().(type) {
			case nil:
			case arbFailure:
				panic(arbError(string(r), path))
			default:
				panic(r)
			}
		}()
		constraint := inherited
		for _, ck := range ast.checks {
			if ck.constraint != nil {
				constraint = mergeConstraint(constraint, ck.constraint)
			}
		}
		base := c.compileBase(ast, path, constraint)
		if ast.tag == astSuspend {
			body := base.deps[0]
			node.deps = []*arbGen{body}
			node.computeMinCost = func() float64 {
				if body.minCost == infinity {
					return infinity
				}
				if node.recursive {
					return body.minCost + 1
				}
				return body.minCost
			}
			node.generate = func(st *genState) *sample {
				if node.recursive {
					if *st.budget <= 0 {
						return nil
					}
					*st.budget--
				}
				return body.generate(st)
			}
			c.bodies[node] = body
		} else {
			node.deps, node.computeMinCost, node.generate = base.deps, base.computeMinCost, base.generate
		}
		applyFilters(node, ast.checks)
		if convert := ast.convert; convert != nil {
			generate := node.generate
			node.generate = func(st *genState) *sample {
				if s := generate(st); s != nil {
					return mapSample(s, convert)
				}
				return nil
			}
		}
	})
	return node
}

func applyFilters(node *arbGen, checks []arbFilter) {
	if len(checks) == 0 {
		return
	}
	generate := node.generate
	node.generate = func(st *genState) *sample {
		s := generate(st)
		if s == nil {
			return nil
		}
		passes := func(v Value) bool {
			for _, ck := range checks {
				if !ck.test(st.fb, v) {
					return false
				}
			}
			return true
		}
		if !st.shrinks {
			if passes(s.value) {
				return s
			}
			return nil
		}
		return filterSample(s, passes)
	}
}

func constantNode(v Value) *arbGen {
	s := &sample{v, nil}
	return compiledGen(nil, func() float64 { return 0 }, func(*genState) *sample { return s })
}

func (c *arbCompiler) compileBase(ast *arbAST, path []any, constraint *arbConstraint) *arbGen {
	switch ast.tag {
	case astNever:
		if ast.what != "" {
			arbFail(ast.what)
		}
		return compiledGen(nil, infiniteCost, discardAll)
	case astLiteral, astUndefined:
		return constantNode(ast.literal)
	case astBoolean:
		return compiledGen(nil, func() float64 { return 0 }, func(st *genState) *sample {
			v := st.randomBool()
			if st.shrinks && v {
				return &sample{true, pullEach([]bool{false}, func(b bool) *sample { return &sample{b, nil} })}
			}
			return &sample{v, nil}
		})
	case astString:
		return stringNode(constraint)
	case astNumber:
		return numberNode(constraint)
	case astBigInt:
		return bigintNode(constraint)
	case astDecimal:
		return decimalNode()
	case astArrays:
		return c.compileArrays(ast, path, constraint)
	case astObjects:
		return c.compileObjects(ast, path)
	case astUnion:
		members := make([]*arbGen, len(ast.members))
		for i, m := range ast.members {
			members[i] = c.recur(m, path, constraint)
		}
		if len(members) == 0 {
			arbFail("a union with no members")
		}
		return compiledGen(members, func() float64 {
			out := infinity
			for _, m := range members {
				out = math.Min(out, m.minCost)
			}
			return out
		}, func(st *genState) *sample { return generateUnion(members, st) })
	case astSuspend:
		body := c.recur(ast.thunk(), path, nil)
		return compiledGen([]*arbGen{body}, func() float64 { return body.minCost }, func(st *genState) *sample { return body.generate(st) })
	case astRecord:
		return c.compileRecord(ast, path)
	}
	return c.compileDeclaration(ast, path)
}

// compileDeclaration is an Option, a Result or a Redacted value: its
// parameters', through the union of tagged structs it is linked to; or a
// link's declaration. The parameters are compiled first, so the target's
// structs find them compiled at their paths.
func (c *arbCompiler) compileDeclaration(ast *arbAST, path []any) *arbGen {
	members := ast.members
	if ast.tag == astLink {
		members = members[1:]
	}
	params := make([]*arbGen, len(members))
	for i, m := range members {
		params[i] = c.recur(m, at(path, i), nil)
	}
	var target *arbGen
	wrap := func(v Value) Value { return v }
	switch ast.tag {
	case astLink:
		target, wrap = c.recur(ast.members[0], path, nil), ast.decode
	case astOption:
		target = c.recur(&arbAST{tag: astUnion, members: []*arbAST{
			structNode("Some", ast.members, func(vs []Value) Value { return Some(vs[0]) }),
			structNode("None", nil, func([]Value) Value { return None }),
		}}, path, nil)
	case astResult:
		target = c.recur(&arbAST{tag: astUnion, members: []*arbAST{
			structNode("Success", ast.members[:1], func(vs []Value) Value { return Ok(vs[0]) }),
			structNode("Failure", ast.members[1:], func(vs []Value) Value { return Err(vs[0]) }),
		}}, path, nil)
	default:
		target = c.recur(ast.members[0], path, nil)
		wrap = func(v Value) Value { return &Redacted{v} }
	}
	return compiledGen(append([]*arbGen{target}, params...), func() float64 { return target.minCost }, func(st *genState) *sample {
		if s := target.generate(st); s != nil {
			return mapSample(s, wrap)
		}
		return nil
	})
}

// generateSamples generates children, the ones that may recurse in a
// random order among themselves.
func generateSamples(children []*arbGen, st *genState, reserved float64) []*sample {
	var recursive []int
	for i, ch := range children {
		if reserved != infinity {
			reserved += ch.minCost
		}
		if ch.mayRecurse {
			recursive = append(recursive, i)
		}
	}
	if reserved > *st.budget {
		return nil
	}
	var order []int
	if len(recursive) > 1 {
		order = make([]int, len(children))
		shuffled := shuffle(st, recursive)
		next := 0
		for i, ch := range children {
			order[i] = i
			if ch.mayRecurse {
				order[i] = shuffled[next]
				next++
			}
		}
	}
	return generateProduct(children, st, order, reserved)
}

// compileObjects is a struct of required properties, the only objects a
// schema of the program has.
func (c *arbCompiler) compileObjects(ast *arbAST, path []any) *arbGen {
	props := make([]*arbGen, len(ast.props))
	for i, p := range ast.props {
		props[i] = c.recur(p, at(path, ast.names[i]), nil)
	}
	return compiledGen(props, func() float64 {
		costs := make([]float64, len(props))
		for i, p := range props {
			costs[i] = p.minCost
		}
		return sumCosts(costs...)
	}, func(st *genState) *sample {
		samples := generateSamples(props, st, 0)
		if samples == nil {
			return nil
		}
		if !st.shrinks {
			return &sample{ast.make(values(samples)), nil}
		}
		return objectSample(samples, ast.make)
	})
}

// objectSample shrinks each property in turn: none of the properties can
// be removed.
func objectSample(children []*sample, make func([]Value) Value) *sample {
	var rebuild func(entries []*retained, start int) *sample
	rebuild = func(entries []*retained, start int) *sample {
		var pulls []shrinkPull
		for i := start; i < len(entries); i++ {
			if entries[i].shrinks == nil {
				continue
			}
			pulls = append(pulls, mapRetained(entries[i].shrinks(), func(r *retained) *sample {
				return rebuild(replaceAt(entries, i, r), i)
			}))
		}
		out := &sample{make(retainedValues(entries)), nil}
		if len(pulls) > 0 {
			out.shrinks = concatPulls(pulls)
		}
		return out
	}
	return lazyShrinks(make(values(children)), func() shrinkPull { return rebuild(retainAll(children), 0).shrinks })
}

// lengthBounds are a constraint's bounds of a length, checked.
func lengthBounds(minimum, maximum *float64, label string) (float64, float64) {
	lo, hi := 0.0, infinity
	if minimum != nil {
		lo = *minimum
	}
	if maximum != nil {
		hi = *maximum
	}
	if lo != math.Trunc(lo) || lo < 0 || lo > maxSafe || maximum != nil && (hi != math.Trunc(hi) || hi > maxSafe || hi < lo) {
		arbFail(label + " constraints")
	}
	return lo, hi
}

// compileArrays is a list, `rest` repeated, or a tuple of `elements`.
func (c *arbCompiler) compileArrays(ast *arbAST, path []any, constraint *arbConstraint) *arbGen {
	elements := make([]*arbGen, len(ast.elements))
	for i, e := range ast.elements {
		elements[i] = c.recur(e, at(path, i), nil)
	}
	var head *arbGen
	deps := elements
	if ast.rest != nil {
		head = c.recur(ast.rest, at(path, len(ast.elements)), nil)
		deps = append(append([]*arbGen(nil), elements...), head)
	}
	var minLength, maxLength *float64
	if constraint != nil {
		minLength, maxLength = constraint.minLength, constraint.maxLength
	}
	minimum, maximum := lengthBounds(minLength, maxLength, "array")
	required := float64(len(elements))
	if maximum < required || head == nil && minimum > required {
		arbFail("array constraints")
	}
	// the repeat counts possible within a size: a tuple has only 0
	combinations := func(limit float64) [][2]float64 {
		requiredCost := 0.0
		for _, e := range elements {
			requiredCost = sumCosts(requiredCost, e.minCost)
		}
		upper := math.Min(maximum, math.Max(math.Max(minimum, required), limit))
		minimumRepeat := math.Max(0, minimum-required)
		maximumRepeat := 0.0
		if head != nil {
			maximumRepeat = math.Max(minimumRepeat, upper-required)
		}
		var out [][2]float64
		if required+minimumRepeat > upper || head == nil && minimumRepeat > 0 {
			return out
		}
		for repeat := minimumRepeat; repeat <= maximumRepeat; repeat++ {
			cost := requiredCost
			if repeat > 0 {
				cost += repeat * head.minCost
			}
			out = append(out, [2]float64{repeat, cost})
		}
		return out
	}
	cachedLimit, cachedBudget := math.NaN(), math.NaN()
	var cached, possible [][2]float64
	return compiledGen(deps, func() float64 {
		out := infinity
		for _, c := range combinations(0) {
			out = math.Min(out, c[1])
		}
		return out
	}, func(st *genState) *sample {
		if st.size != cachedLimit {
			cachedLimit, cachedBudget = st.size, math.NaN()
			cached = combinations(st.size)
		}
		if *st.budget != cachedBudget {
			cachedBudget = *st.budget
			possible = nil
			for _, c := range cached {
				if c[1] <= cachedBudget {
					possible = append(possible, c)
				}
			}
		}
		if len(possible) == 0 {
			return nil
		}
		repeat := int(possible[int(st.randomLength(0, float64(len(possible)-1)))][0])
		itemState := st
		if st.size < float64(repeat) {
			itemState = st.withSize(float64(repeat))
		}
		if !st.shrinks && len(elements) == 0 && head != nil {
			return generateRepeated(head, repeat, itemState)
		}
		selected := append([]*arbGen(nil), elements...)
		for range repeat {
			selected = append(selected, head)
		}
		samples := generateSamples(selected, itemState, 0)
		if samples == nil {
			return nil
		}
		return arraySample(samples, arrayShape{len(elements), 0, repeat, 0, int(minimum)}, st.shrinks)
	})
}

// generateRepeated is a list of `count` values of `child`; a recursive
// one fills its places in a random order.
func generateRepeated(child *arbGen, count int, st *genState) *sample {
	out := make([]Value, count)
	var order []int
	if child.mayRecurse {
		order = make([]int, count)
		for i := range order {
			order[i] = i
		}
		order = shuffle(st, order)
	}
	reserved := float64(count) * child.minCost
	for i := range count {
		reserved -= child.minCost
		s := reserving(child, st, reserved)
		if s == nil {
			return nil
		}
		at := i
		if order != nil {
			at = order[i]
		}
		out[at] = s.value
	}
	return &sample{out, nil}
}

// analyzeDependencies finds the strongly connected nodes, Kosaraju's way,
// marks the recursive suspensions and gives each node's dependents.
func analyzeDependencies(nodes []*arbGen, bodies map[*arbGen]*arbGen) map[*arbGen][]*arbGen {
	visited := map[*arbGen]bool{}
	var order, finished []*arbGen
	type frame struct {
		node  *arbGen
		index int
	}
	for _, root := range nodes {
		if visited[root] {
			continue
		}
		visited[root] = true
		order = append(order, root)
		stack := []*frame{{root, 0}}
		for len(stack) > 0 {
			f := stack[len(stack)-1]
			if f.index < len(f.node.deps) {
				d := f.node.deps[f.index]
				f.index++
				if !visited[d] {
					visited[d] = true
					order = append(order, d)
					stack = append(stack, &frame{d, 0})
				}
			} else {
				finished = append(finished, f.node)
				stack = stack[:len(stack)-1]
			}
		}
	}
	reverse := map[*arbGen][]*arbGen{}
	for _, n := range order {
		reverse[n] = []*arbGen{}
	}
	for _, n := range order {
		for _, d := range n.deps {
			reverse[d] = append(reverse[d], n)
		}
	}
	component := map[*arbGen]int{}
	id := 0
	for i := len(finished) - 1; i >= 0; i-- {
		root := finished[i]
		if _, ok := component[root]; ok {
			continue
		}
		component[root] = id
		stack := []*arbGen{root}
		for len(stack) > 0 {
			n := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			for _, d := range reverse[n] {
				if _, ok := component[d]; !ok {
					component[d] = id
					stack = append(stack, d)
				}
			}
		}
		id++
	}
	for s, body := range bodies {
		s.recursive = component[s] == component[body]
	}
	return reverse
}
