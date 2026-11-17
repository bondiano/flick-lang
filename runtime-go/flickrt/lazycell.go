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

const lazySelfForce = "lazy forced during its own initialization"

// LazyForce is `await cell`: its outcome, running the initializer in this
// fiber when no attempt has settled it.
func LazyForce(fb *Fiber, l Value) Value {
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
	cell.mu.Lock()
	if c == nil || !(c.has(causeInterrupt) || unwinds(c)) {
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
