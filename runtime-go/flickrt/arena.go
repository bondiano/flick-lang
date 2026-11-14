package flickrt

import "sync"

// arena owns its slots until its lexical owner has finished all finalizers.
// A view keeps only identity/index metadata; close removes every strong
// storage edge even when views remain. Host GC is not a counted-heap proof.
type arena struct {
	mu     sync.Mutex
	closed bool
	slots  []Value
}

type arenaValue struct {
	arena *arena
	index int
}

func (a *arena) liveSlots() int {
	a.mu.Lock()
	defer a.mu.Unlock()
	return len(a.slots)
}

func (a *arena) close() {
	a.mu.Lock()
	a.closed = true
	a.slots = nil
	a.mu.Unlock()
}

func (o *Owner) closeArenas() {
	o.mu.Lock()
	o.arenasClosed = true
	arenas := o.arenas
	o.arenas = nil
	o.mu.Unlock()
	for _, a := range arenas {
		a.close()
	}
}

// ArenaMake is Arena.make (): the compiler has selected fb.owner by region.
func ArenaMake(fb *Fiber, _ Value) Value {
	if fb.owner == nil {
		panic(Defect{"arena requires a scope owner"})
	}
	o := fb.owner
	o.mu.Lock()
	if o.arenasClosed {
		o.mu.Unlock()
		panic(Defect{"arena owner is closed"})
	}
	a := &arena{}
	o.arenas = append(o.arenas, a)
	o.mu.Unlock()
	return a
}

// ArenaAlloc transfers the value into a strong slot, returning a nonowning view.
func ArenaAlloc(owner, value Value) Value {
	a := owner.(*arena)
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		panic(Defect{"arena is closed"})
	}
	index := len(a.slots)
	a.slots = append(a.slots, value)
	a.mu.Unlock()
	return &arenaValue{arena: a, index: index}
}

// ArenaGet reads an ordinary immutable value; the view does not extend storage life.
func ArenaGet(value Value) Value {
	v := value.(*arenaValue)
	a := v.arena
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		panic(Defect{"arena is closed"})
	}
	if v.index < 0 || v.index >= len(a.slots) {
		a.mu.Unlock()
		panic(Defect{"invalid arena value"})
	}
	out := a.slots[v.index]
	a.mu.Unlock()
	return out
}
