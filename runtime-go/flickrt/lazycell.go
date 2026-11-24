package flickrt

import "sync"

// LazyCell is a `Lazy`: a computation that the first fiber to force it runs
// in itself, at most one attempt at a time, whose outcome every later force
// gets. A typed failure or a defect is an outcome; an interruption of the
// fiber running the attempt, or an unwind to one of its handlers, is not:
// the cell is pending again, and a waiter retries it. The mutex guards the
// state, never the run.
type LazyCell struct {
	mu    sync.Mutex
	task  Value         // the initializer, until an attempt settles the cell
	owner *fiberState   // the fiber running the attempt, nil while none runs
	ended chan struct{} // closed when the attempt that runs now ends
	exit  *Exit         // how the attempt that settled the cell ended: final
}

// LazyMake is `lazy task`: a pending cell; nothing runs.
func LazyMake(task Value) Value { return &LazyCell{task: task} }

// LocalLazyCell is an unsynchronized `Lazy` for a cell only its own fiber can
// force (O3 of docs/formal/lazy.md §6): no mutex, no owner tracking, and no
// waiter channel. Pending/Running/Done states are tracked directly; forcing
// while running is still a cached defect (R9(e)), typed failures and defects
// are cached, and interruption or handler abort resets to pending.
type LocalLazyCell struct {
	task    Value // the initializer while pending; cleared on Pending -> Running
	running bool  // true while this fiber is running the initializer
	exit    *Exit // how the attempt that settled the cell ended: final
}

// LazyMakeLocal is an unsynchronized `lazy task` for a fiber-local cell (O3).
func LazyMakeLocal(task Value) Value { return &LocalLazyCell{task: task} }

// LazyDone is a cell already done with `value`: forcing it runs nothing.
func LazyDone(value Value) Value { return &LazyCell{exit: &Exit{value, nil}} }

const lazySelfForce = "lazy forced during its own initialization"

// LazyForce is `await cell`: its outcome, running the initializer in this
// fiber when no attempt has settled it.
func LazyForce(fb *Fiber, l Value) Value {
	if local, ok := l.(*LocalLazyCell); ok {
		return local.force(fb)
	}
	cell := l.(*LazyCell)
	for {
		cell.mu.Lock()
		switch {
		case cell.exit != nil:
			exit := cell.exit
			cell.mu.Unlock()
			if exit.cause != nil {
				panic(&Failure{exit.cause})
			}
			return exit.value
		case cell.owner == nil:
			cell.owner, cell.ended = fb.state, make(chan struct{})
			task := cell.task
			cell.mu.Unlock()
			return cell.attempt(fb, task)
		case cell.owner == fb.state:
			cell.mu.Unlock()
			panic(Defect{lazySelfForce})
		default:
			ended := cell.ended
			cell.mu.Unlock()
			wait(fb, ended)
		}
	}
}

// attempt runs the initializer as the cell's owner and settles the cell with
// its outcome, or makes it pending again.
func (cell *LazyCell) attempt(fb *Fiber, task Value) Value {
	v, c := catch(func() Value { return Run(fb, task) })
	c = cleanCause(c)
	cell.mu.Lock()
	if c == nil || (!c.has(causeInterrupt) && (c.hasRealDie() || !unwinds(c))) {
		cell.exit, cell.task = &Exit{v, c}, nil
	}
	cell.owner = nil
	close(cell.ended)
	cell.mu.Unlock()
	if c != nil {
		panic(&Failure{c})
	}
	return v
}

// force runs the unsynchronized local cell without mutex or waiter tracking.
func (cell *LocalLazyCell) force(fb *Fiber) Value {
	if cell.exit != nil {
		if cell.exit.cause != nil {
			panic(&Failure{cell.exit.cause})
		}
		return cell.exit.value
	}
	if cell.running {
		panic(Defect{lazySelfForce})
	}
	task := cell.task
	cell.task = nil
	cell.running = true
	v, c := catch(func() Value { return Run(fb, task) })
	c = cleanCause(c)
	cell.running = false
	if c == nil || (!c.has(causeInterrupt) && (c.hasRealDie() || !unwinds(c))) {
		cell.exit = &Exit{v, c}
	} else {
		cell.task = task
	}
	if c != nil {
		panic(&Failure{c})
	}
	return v
}
