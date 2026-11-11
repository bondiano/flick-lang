package flickrt

// Sink is, given the pull of chunks of its input and the scope it runs in,
// the value it ends with and its leftovers (nil: none).
type Sink struct {
	transform func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value)
}

func sinkOf(v Value) *Sink { return v.(*Sink) }

func sinkFromTransform(transform func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value)) *Sink {
	return &Sink{transform: transform}
}

// sinkEnd is the done value of a sink's channel.
type sinkEnd struct {
	value    Value
	leftover []Value
}

// sinkFromChannel is the sink that pulls the channel until it ends with
// an end.
func sinkFromChannel(self *Channel) *Sink {
	return sinkFromTransform(func(fb *Fiber, upstream Pull, scope *Owner) (Value, []Value) {
		pull := self.transform(fb, upstream, scope)
		end := catchDone(fb, func(fb *Fiber) Value {
			for {
				Check(fb)
				pull(fb)
			}
		}, func(_ *Fiber, d Value) Value { return d }).(sinkEnd)
		return end.value, end.leftover
	})
}

// toChannel is the channel that runs the sink when pulled and ends with
// its end.
func (s *Sink) toChannel() *Channel {
	return fromTransform(func(_ *Fiber, upstream Pull, scope *Owner) Pull {
		return func(fb *Fiber) Value {
			v, leftover := s.transform(fb, upstream, scope)
			return halt(sinkEnd{v, leftover})
		}
	})
}

// A Flick `End` is the value and its leftovers in an Option.

func toSinkEnd(_ *Fiber, end Value) Value {
	pair := List(end)
	var leftover []Value
	if l, ok := some(pair[1]); ok {
		leftover = List(l)
	}
	return sinkEnd{pair[0], leftover}
}

func fromSinkEnd(_ *Fiber, d Value) Value {
	end := d.(sinkEnd)
	var leftover Value = None
	if end.leftover != nil {
		leftover = Some(end.leftover)
	}
	return []Value{end.value, leftover}
}

func SinkFromChannel(channel Value) Value {
	return sinkFromChannel(channelMapDone(channelOf(channel), toSinkEnd))
}

func SinkToChannel(s Value) Value { return channelMapDone(sinkOf(s).toChannel(), fromSinkEnd) }
