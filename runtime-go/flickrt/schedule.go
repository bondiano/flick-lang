package flickrt

import "time"

// Schedule as Effect's, as far as retrying goes: each run of a schedule
// decides, for each input, whether to go on and after which delay.
//
// shortcut: only `recurs` is a schedule on the go target, with no delay or output; port Effect's Schedule when a program needs another
type Schedule struct {
	start func() func(input Value) (time.Duration, bool)
}

func scheduleOf(v Value) *Schedule { return v.(*Schedule) }

// ScheduleRecurs goes on `n` times, at once.
func ScheduleRecurs(n Value) Value {
	limit := int(n.(int32))
	return &Schedule{start: func() func(Value) (time.Duration, bool) {
		count := 0
		return func(Value) (time.Duration, bool) {
			count++
			return 0, count <= limit
		}
	}}
}

// while is the schedule going on only for the inputs `f` accepts.
func (s *Schedule) while(f func(Value) bool) *Schedule {
	return &Schedule{start: func() func(Value) (time.Duration, bool) {
		next := s.start()
		return func(input Value) (time.Duration, bool) {
			if !f(input) {
				return 0, false
			}
			return next(input)
		}
	}}
}
