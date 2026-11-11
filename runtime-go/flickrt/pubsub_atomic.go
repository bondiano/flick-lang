package flickrt

import "strconv"

// The backing hubs of PubSub: bounded ones on a ring (one slot apart), an
// unbounded one on a linked list, and the replay buffer a new subscriber
// reads first. The hub's lock guards them all.

// hubAtomic is the backing hub of a PubSub.
type hubAtomic interface {
	capacity() int
	isEmpty() bool
	isFull() bool
	size() int
	publish(v Value) bool
	publishAll(xs []Value) []Value
	slide()
	subscribe() hubBacking
	replay() *replayBuffer
}

// hubBacking is the backing subscription of a hub.
type hubBacking interface {
	isEmpty() bool
	size() int
	poll() (Value, bool)
	pollUpTo(n int) []Value
	unsubscribe()
}

// hubAbsent marks an emptied slot.
var hubAbsent Value = &struct{ absent bool }{true}

// unboundedHubCapacity is JS's `Number.MAX_SAFE_INTEGER`.
const unboundedHubCapacity = 1<<53 - 1

// makeAtomicBounded is a bounded hub: capacity 1 has a hub of its own, the
// others a ring, whatever their capacity.
func makeAtomicBounded(capacity int, replay int) hubAtomic {
	if capacity <= 0 {
		panic(Defect{"Error: Cannot construct PubSub with capacity of " + strconv.Itoa(capacity)})
	}
	buffer := newReplayBuffer(replay)
	if capacity == 1 {
		return &hubSingle{buffer: buffer}
	}
	return &hubRing{
		array:       make([]Value, capacity),
		replayIndex: make([]int, capacity),
		subscribers: make([]int, capacity),
		buffer:      buffer,
	}
}

func newReplayBuffer(replay int) *replayBuffer {
	if replay <= 0 {
		return nil
	}
	head := &replayNode{}
	return &replayBuffer{capacity: replay, head: head, tail: head, slides: make([]replayEntry, replay)}
}

// hubRing is a bounded hub on a ring.
type hubRing struct {
	array            []Value
	replayIndex      []int
	publisherIndex   int
	subscribers      []int
	subscriberCount  int
	subscribersIndex int
	buffer           *replayBuffer
}

func (h *hubRing) capacity() int         { return len(h.array) }
func (h *hubRing) replay() *replayBuffer { return h.buffer }
func (h *hubRing) isEmpty() bool         { return h.publisherIndex == h.subscribersIndex }
func (h *hubRing) isFull() bool          { return h.publisherIndex == h.subscribersIndex+len(h.array) }
func (h *hubRing) size() int             { return h.publisherIndex - h.subscribersIndex }

func (h *hubRing) put(v Value) {
	index := h.publisherIndex % len(h.array)
	h.array[index] = v
	if h.buffer != nil {
		h.replayIndex[index] = h.buffer.offer(v)
	}
	h.subscribers[index] = h.subscriberCount
	h.publisherIndex++
}

func (h *hubRing) publish(v Value) bool {
	if h.isFull() {
		return false
	}
	if h.subscriberCount != 0 {
		h.put(v)
	} else if h.buffer != nil {
		h.buffer.offer(v)
	}
	return true
}

func (h *hubRing) publishAll(xs []Value) []Value {
	if h.subscriberCount == 0 {
		if h.buffer != nil {
			h.buffer.offerAll(xs)
		}
		return nil
	}
	n := min(len(xs), len(h.array)-h.size())
	for _, x := range xs[:n] {
		h.put(x)
	}
	return xs[n:]
}

func (h *hubRing) slide() {
	if h.subscribersIndex != h.publisherIndex {
		index := h.subscribersIndex % len(h.array)
		v := h.array[index]
		h.array[index] = hubAbsent
		h.subscribers[index] = 0
		h.subscribersIndex++
		if h.buffer != nil {
			h.buffer.slide(v, h.replayIndex[index])
		}
	}
}

func (h *hubRing) subscribe() hubBacking {
	h.subscriberCount++
	return &hubRingSub{hub: h, index: h.publisherIndex}
}

type hubRingSub struct {
	hub          *hubRing
	index        int
	unsubscribed bool
}

func (s *hubRingSub) isEmpty() bool {
	return s.unsubscribed || s.hub.publisherIndex == s.index || s.hub.publisherIndex == s.hub.subscribersIndex
}

func (s *hubRingSub) size() int {
	if s.unsubscribed {
		return 0
	}
	return s.hub.publisherIndex - max(s.index, s.hub.subscribersIndex)
}

// release is one subscriber done with the slot `index`.
func (h *hubRing) release(index int) {
	h.subscribers[index]--
	if h.subscribers[index] == 0 {
		h.array[index] = hubAbsent
		h.subscribersIndex++
	}
}

func (s *hubRingSub) poll() (Value, bool) {
	if s.unsubscribed {
		return nil, false
	}
	h := s.hub
	s.index = max(s.index, h.subscribersIndex)
	if s.index == h.publisherIndex {
		return nil, false
	}
	index := s.index % len(h.array)
	v := h.array[index]
	h.release(index)
	s.index++
	return v, true
}

func (s *hubRingSub) pollUpTo(n int) []Value {
	n = max(n, 0)
	if s.unsubscribed {
		return []Value{}
	}
	h := s.hub
	s.index = max(s.index, h.subscribersIndex)
	toPoll := min(n, h.publisherIndex-s.index)
	out := make([]Value, 0, max(toPoll, 0))
	for ; toPoll > 0; toPoll-- {
		index := s.index % len(h.array)
		out = append(out, h.array[index])
		h.release(index)
		s.index++
	}
	return out
}

func (s *hubRingSub) unsubscribe() {
	if s.unsubscribed {
		return
	}
	h := s.hub
	s.unsubscribed = true
	h.subscriberCount--
	for s.index = max(s.index, h.subscribersIndex); s.index != h.publisherIndex; s.index++ {
		h.release(s.index % len(h.array))
	}
}

// hubSingle is a bounded hub of capacity 1.
type hubSingle struct {
	publisherIndex  int
	subscriberCount int
	subscribers     int
	value           Value
	replayIndex     int
	buffer          *replayBuffer
}

func (h *hubSingle) capacity() int         { return 1 }
func (h *hubSingle) replay() *replayBuffer { return h.buffer }
func (h *hubSingle) isEmpty() bool         { return h.subscribers == 0 }
func (h *hubSingle) isFull() bool          { return !h.isEmpty() }

func (h *hubSingle) size() int {
	if h.isEmpty() {
		return 0
	}
	return 1
}

func (h *hubSingle) publish(v Value) bool {
	if h.isFull() {
		return false
	}
	replayIndex, replayed := 0, h.buffer != nil
	if replayed {
		replayIndex = h.buffer.offer(v)
	}
	if h.subscriberCount != 0 {
		h.value = v
		if replayed {
			h.replayIndex = replayIndex
		}
		h.subscribers = h.subscriberCount
		h.publisherIndex++
	}
	return true
}

func (h *hubSingle) publishAll(xs []Value) []Value {
	if h.subscriberCount == 0 {
		if h.buffer != nil {
			h.buffer.offerAll(xs)
		}
		return nil
	}
	if len(xs) == 0 || !h.publish(xs[0]) {
		return xs
	}
	return xs[1:]
}

func (h *hubSingle) slide() {
	if h.isFull() {
		v := h.value
		h.subscribers = 0
		h.value = hubAbsent
		if h.buffer != nil {
			h.buffer.slide(v, h.replayIndex)
		}
	}
}

func (h *hubSingle) subscribe() hubBacking {
	h.subscriberCount++
	return &hubSingleSub{hub: h, index: h.publisherIndex}
}

type hubSingleSub struct {
	hub          *hubSingle
	index        int
	unsubscribed bool
}

func (s *hubSingleSub) isEmpty() bool {
	return s.unsubscribed || s.hub.subscribers == 0 || s.index == s.hub.publisherIndex
}

func (s *hubSingleSub) size() int {
	if s.isEmpty() {
		return 0
	}
	return 1
}

func (s *hubSingleSub) poll() (Value, bool) {
	if s.isEmpty() {
		return nil, false
	}
	h := s.hub
	v := h.value
	h.subscribers--
	if h.subscribers == 0 {
		h.value = hubAbsent
	}
	s.index = h.publisherIndex
	return v, true
}

func (s *hubSingleSub) pollUpTo(n int) []Value {
	if n < 1 || s.isEmpty() {
		return []Value{}
	}
	v, _ := s.poll()
	return []Value{v}
}

func (s *hubSingleSub) unsubscribe() {
	if s.unsubscribed {
		return
	}
	h := s.hub
	s.unsubscribed = true
	h.subscriberCount--
	if h.subscribers != 0 && s.index != h.publisherIndex {
		h.subscribers--
		if h.subscribers == 0 {
			h.value = hubAbsent
		}
	}
}

// hubNode is a message of the unbounded hub; `absent` once every
// subscriber took it.
type hubNode struct {
	value       Value
	absent      bool
	replayIndex int
	subscribers int
	next        *hubNode
}

// hubList is an unbounded hub on a linked list.
type hubList struct {
	head, tail       *hubNode
	publisherIndex   int
	subscribersIndex int
	buffer           *replayBuffer
}

func makeAtomicUnbounded(replay int) hubAtomic {
	head := &hubNode{absent: true}
	return &hubList{head: head, tail: head, buffer: newReplayBuffer(replay)}
}

func (h *hubList) capacity() int         { return unboundedHubCapacity }
func (h *hubList) replay() *replayBuffer { return h.buffer }
func (h *hubList) isEmpty() bool         { return h.head == h.tail }
func (h *hubList) isFull() bool          { return false }
func (h *hubList) size() int             { return h.publisherIndex - h.subscribersIndex }

func (h *hubList) publish(v Value) bool {
	replayIndex := 0
	if h.buffer != nil {
		replayIndex = h.buffer.offer(v)
	}
	if n := h.tail.subscribers; n != 0 {
		h.tail.next = &hubNode{value: v, replayIndex: replayIndex, subscribers: n}
		h.tail = h.tail.next
		h.publisherIndex++
	}
	return true
}

func (h *hubList) publishAll(xs []Value) []Value {
	if h.tail.subscribers != 0 {
		for _, x := range xs {
			h.publish(x)
		}
	} else if h.buffer != nil {
		h.buffer.offerAll(xs)
	}
	return nil
}

func (h *hubList) slide() {
	if h.head != h.tail {
		node := h.head.next
		h.head = node
		v := node.value
		node.value, node.absent = nil, true
		h.subscribersIndex++
		if h.buffer != nil {
			h.buffer.slide(v, node.replayIndex)
		}
	}
}

func (h *hubList) subscribe() hubBacking {
	h.tail.subscribers++
	return &hubListSub{hub: h, head: h.tail, index: h.publisherIndex}
}

type hubListSub struct {
	hub          *hubList
	head         *hubNode
	index        int
	unsubscribed bool
}

func (s *hubListSub) isEmpty() bool {
	if s.unsubscribed {
		return true
	}
	for s.head != s.hub.tail {
		if !s.head.next.absent {
			return false
		}
		s.head = s.head.next
		s.index++
	}
	return true
}

func (s *hubListSub) size() int {
	if s.unsubscribed {
		return 0
	}
	return s.hub.publisherIndex - max(s.index, s.hub.subscribersIndex)
}

// taken is the subscriber done with the node after its head.
func (h *hubList) taken(at *hubNode) {
	at.subscribers--
	if at.subscribers == 0 {
		h.head = h.head.next
		h.head.value, h.head.absent = nil, true
		h.subscribersIndex++
	}
}

func (s *hubListSub) poll() (Value, bool) {
	if s.unsubscribed {
		return nil, false
	}
	for s.head != s.hub.tail {
		next := s.head.next
		if !next.absent {
			v := next.value
			s.hub.taken(s.head)
			s.head = next
			s.index++
			return v, true
		}
		s.head = next
		s.index++
	}
	return nil, false
}

func (s *hubListSub) pollUpTo(n int) []Value {
	out := []Value{}
	for i := 0; i < max(n, 0); i++ {
		v, ok := s.poll()
		if !ok {
			break
		}
		out = append(out, v)
	}
	return out
}

func (s *hubListSub) unsubscribe() {
	if s.unsubscribed {
		return
	}
	s.unsubscribed = true
	s.hub.tail.subscribers--
	for s.head != s.hub.tail {
		if !s.head.next.absent {
			s.hub.taken(s.head)
		}
		s.head = s.head.next
	}
}

// replayBuffer is the last `capacity` messages, and those slid out of a
// full hub, which a window catches up with.
type replayBuffer struct {
	capacity       int
	head, tail     *replayNode
	slides         []replayEntry
	size           int
	index          int
	publisherIndex int
}

type replayNode struct {
	value Value
	index int
	next  *replayNode
}

type replayEntry struct {
	value Value
	index int
}

func (b *replayBuffer) slide(v Value, publisherIndex int) {
	b.slides[b.index%b.capacity] = replayEntry{v, publisherIndex}
	b.index++
}

func (b *replayBuffer) offer(v Value) int {
	index := b.publisherIndex
	b.publisherIndex++
	b.tail.value, b.tail.index = v, index
	b.tail.next = &replayNode{}
	b.tail = b.tail.next
	if b.size == b.capacity {
		b.head = b.head.next
	} else {
		b.size++
	}
	return index
}

func (b *replayBuffer) offerAll(xs []Value) {
	for _, x := range xs {
		b.offer(x)
	}
}

// replayWindow is a subscriber's view of the buffer when it subscribed;
// nil is the empty window.
type replayWindow struct {
	buffer      *replayBuffer
	values      []Value
	index       int
	remaining   int
	slideIndex  int
	newestIndex int
}

func newReplayWindow(b *replayBuffer) *replayWindow {
	if b == nil {
		return &replayWindow{}
	}
	w := &replayWindow{buffer: b, remaining: b.size, slideIndex: b.index, newestIndex: -1, values: make([]Value, b.size)}
	node := b.head
	for i := range w.values {
		w.values[i] = node.value
		w.newestIndex = node.index
		node = node.next
	}
	return w
}

func (w *replayWindow) close() {
	w.values = nil
	w.remaining = 0
}

func (w *replayWindow) sync() {
	slides := w.buffer.index - w.slideIndex
	if slides == 0 || w.remaining == 0 {
		return
	}
	count := min(slides, w.buffer.capacity)
	start := w.buffer.index - count
	for i := range count {
		entry := w.buffer.slides[(start+i)%w.buffer.capacity]
		if entry.index > w.newestIndex {
			w.index = (w.index + 1) % len(w.values)
			w.values[(w.index+w.remaining-1)%len(w.values)] = entry.value
			w.newestIndex = entry.index
		}
	}
	w.slideIndex = w.buffer.index
}

func (w *replayWindow) take() Value {
	if w.remaining == 0 {
		return nil
	}
	w.sync()
	v := w.values[w.index]
	w.values[w.index] = hubAbsent
	w.index = (w.index + 1) % len(w.values)
	w.remaining--
	if w.remaining == 0 {
		w.close()
	}
	return v
}

func (w *replayWindow) takeN(n int) []Value {
	out := make([]Value, min(max(n, 0), w.remaining))
	for i := range out {
		out[i] = w.take()
	}
	return out
}

func (w *replayWindow) takeAll() []Value { return w.takeN(w.remaining) }

// hubReplay is the replay of a hub's options: a count, 0 for `None`.
func hubReplay(replay Value) int {
	if r, ok := some(replay); ok {
		return int(r.(int32))
	}
	return 0
}
