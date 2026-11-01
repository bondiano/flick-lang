package flickrt

import (
	"math"
	"slices"
	"sort"
	"strings"
	"sync"
)

// Effect's Metric: counters, gauges, frequencies, histograms and summaries
// in the program's registry. A metric finds its series in the registry by
// its kind, name, description and attributes, the fiber's current metric
// attributes among them, and makes it on first use; states are Flick
// records.

const metricAttributesKey = "effect/Metric/CurrentMetricAttributes"

type metric struct {
	id          string
	kind        string  // Counter, Gauge, Frequency, Histogram or Summary
	description Value   // an Option String
	attributes  *Record // nil without
	create      func() *metricHooks
	value       func(c metricContext) Value
	update      func(c metricContext, in Value)
	modify      func(c metricContext, in Value)
}

// metricContext is what Effect's metrics read of a fiber's context: its
// clock and current metric attributes.
type metricContext struct {
	fb    *Fiber
	extra *Record
}

func metricContextOf(fb *Fiber) metricContext {
	return metricContext{fb, envOr(fb, metricAttributesKey, noAttributes).(*Record)}
}

// metricHooks are the state of a series: what reads and updates it, under
// its lock.
type metricHooks struct {
	mu     sync.Mutex
	get    func(c metricContext) Value
	update func(c metricContext, in Value)
	modify func(c metricContext, in Value)
}

type metricEntry struct {
	id          string
	kind        string
	description Value
	attributes  *Record
	hooks       *metricHooks
}

var metricRegistry = struct {
	sync.Mutex
	keys    []string
	entries map[string]*metricEntry
}{entries: map[string]*metricEntry{}}

func metricOf(v Value) *metric { return v.(*metric) }

func newMetric(id, kind string, description Value, attributes *Record, create func() *metricHooks) *metric {
	m := &metric{id: id, kind: kind, description: description, attributes: attributes, create: create}
	m.value = func(c metricContext) Value {
		h := m.hook(c)
		h.mu.Lock()
		defer h.mu.Unlock()
		return h.get(c)
	}
	m.update = func(c metricContext, in Value) {
		h := m.hook(c)
		h.mu.Lock()
		defer h.mu.Unlock()
		h.update(c, in)
	}
	m.modify = func(c metricContext, in Value) {
		h := m.hook(c)
		h.mu.Lock()
		defer h.mu.Unlock()
		h.modify(c, in)
	}
	return m
}

// transformed is Effect's MetricTransform: the metric with other reads and
// updates, and its series.
func metricTransform(m *metric, value func(metricContext) Value, update, modify func(metricContext, Value)) *metric {
	return &metric{id: m.id, kind: m.kind, description: m.description, attributes: m.attributes, create: m.create,
		value: value, update: update, modify: modify}
}

// mergeAttributes is `{ ...self, ...other }`.
func mergeAttributes(self, other *Record) *Record {
	out := newRecord(0)
	for _, r := range []*Record{self, other} {
		if r == nil {
			continue
		}
		for i, k := range r.keys {
			out.put(k, r.vals[i])
		}
	}
	return out
}

// attributesOf is a list of attribute pairs as Effect's record of them.
func attributesOf(pairs Value) *Record {
	out := newRecord(len(List(pairs)))
	for _, kv := range List(pairs) {
		out.put(At(kv, 0).(string), At(kv, 1))
	}
	return out
}

// metricKey is Effect's makeKey: the kind, name, description and the
// attributes by key.
func metricKey(m *metric, attributes *Record) string {
	idx := make([]int, len(attributes.keys))
	for i := range idx {
		idx[i] = i
	}
	sort.SliceStable(idx, func(a, b int) bool { return attributes.keys[idx[a]] < attributes.keys[idx[b]] })
	var b strings.Builder
	b.WriteString(QuoteJSON(m.kind) + "," + QuoteJSON(m.id) + ",")
	if d, ok := some(m.description); ok {
		b.WriteString(QuoteJSON(d.(string)))
	} else {
		b.WriteString("null")
	}
	for _, i := range idx {
		b.WriteString("," + QuoteJSON(attributes.keys[i]) + ":" + QuoteJSON(attributes.vals[i].(string)))
	}
	return b.String()
}

// hook is the series of the metric in the context, made on first use.
func (m *metric) hook(c metricContext) *metricHooks {
	merged := mergeAttributes(m.attributes, c.extra)
	key := metricKey(m, merged)
	metricRegistry.Lock()
	defer metricRegistry.Unlock()
	e, ok := metricRegistry.entries[key]
	if !ok {
		attributes := m.attributes
		if len(c.extra.keys) > 0 {
			attributes = merged
		}
		e = &metricEntry{m.id, m.kind, m.description, attributes, m.create()}
		metricRegistry.entries[key] = e
		metricRegistry.keys = append(metricRegistry.keys, key)
	}
	return e.hooks
}

func hooks(get func(metricContext) Value, update func(metricContext, Value), modify func(metricContext, Value)) *metricHooks {
	if modify == nil {
		modify = update
	}
	return &metricHooks{get: get, update: update, modify: modify}
}

func newCounter(id string, description Value, attributes *Record, incremental bool) *metric {
	return newMetric(id, "Counter", description, attributes, func() *metricHooks {
		count := 0.0
		return hooks(func(metricContext) Value { return Rec("count", count, "incremental", incremental) },
			func(_ metricContext, in Value) {
				if v := in.(float64); !incremental || v >= 0 {
					count += v
				}
			}, nil)
	})
}

func newGauge(id string, description Value, attributes *Record) *metric {
	return newMetric(id, "Gauge", description, attributes, func() *metricHooks {
		value := 0.0
		return hooks(func(metricContext) Value { return Rec("value", value) },
			func(_ metricContext, in Value) { value = in.(float64) },
			func(_ metricContext, in Value) { value += in.(float64) })
	})
}

func newFrequency(id string, description Value, attributes *Record, preregistered []Value) *metric {
	return newMetric(id, "Frequency", description, attributes, func() *metricHooks {
		var words []string
		counts := map[string]float64{}
		add := func(word string) {
			if _, ok := counts[word]; !ok {
				words = append(words, word)
			}
		}
		for _, w := range preregistered {
			add(w.(string))
			counts[w.(string)] = 0
		}
		return hooks(func(metricContext) Value {
			occurrences := make([]Value, len(words))
			for i, w := range words {
				occurrences[i] = []Value{w, counts[w]}
			}
			return Rec("occurrences", occurrences)
		}, func(_ metricContext, in Value) {
			add(in.(string))
			counts[in.(string)]++
		}, nil)
	})
}

func newHistogram(id string, description Value, attributes *Record, bounds []Value) *metric {
	return newMetric(id, "Histogram", description, attributes, func() *metricHooks {
		size := len(bounds)
		values := make([]uint32, size+1)
		boundaries := make([]float64, size)
		for i, b := range bounds {
			boundaries[i] = b.(float64)
		}
		slices.Sort(boundaries)
		count, sum, low, high := 0.0, 0.0, math.MaxFloat64, -math.MaxFloat64
		update := func(_ metricContext, in Value) {
			value := in.(float64)
			from, to := 0, size
			for from != to {
				mid := from + (to-from)/2
				if value <= boundaries[mid] {
					to = mid
				} else {
					from = mid
				}
				if to == from+1 {
					if value <= boundaries[from] {
						to = from
					} else {
						from = to
					}
				}
			}
			values[from]++
			count++
			sum += value
			if value < low {
				low = value
			}
			if value > high {
				high = value
			}
		}
		get := func(metricContext) Value {
			buckets := make([]Value, size)
			cumulated := 0.0
			for i := range size {
				cumulated += float64(values[i])
				buckets[i] = []Value{boundaries[i], cumulated}
			}
			return Rec("buckets", buckets, "count", count, "min", low, "max", high, "sum", sum)
		}
		return hooks(get, update, nil)
	})
}

// newSummary is Effect's SummaryMetric of (value, timestamp) inputs: the
// quantiles of the last `maxSize` values at most `maxAge` old.
func newSummary(id string, description Value, attributes *Record, maxAge float64, maxSize int, quantiles []Value) *metric {
	maxAge = math.Max(maxAge, 0)
	return newMetric(id, "Summary", description, attributes, func() *metricHooks {
		sorted := make([]float64, len(quantiles))
		for i, q := range quantiles {
			sorted[i] = q.(float64)
			if sorted[i] < 0 || sorted[i] > 1 {
				panic(Defect{"Error: Quantile must be between 0 and 1, found: " + ShowNumber(sorted[i])})
			}
		}
		slices.Sort(sorted)
		type observation struct{ timestamp, value float64 }
		observations := make([]*observation, max(maxSize, 0))
		head, count, sum, low, high := 0, 0.0, 0.0, math.MaxFloat64, -math.MaxFloat64
		snapshot := func(now float64) []Value {
			var samples []float64
			for _, o := range observations {
				if o != nil {
					if age := now - o.timestamp; age >= 0 && age <= maxAge {
						samples = append(samples, o.value)
					}
				}
			}
			slices.Sort(samples)
			out := make([]Value, len(sorted))
			for i, q := range sorted {
				switch n := len(samples); {
				case n == 0:
					out[i] = []Value{q, None}
				case q <= 0:
					out[i] = []Value{q, Some(samples[0])}
				case q >= 1:
					out[i] = []Value{q, Some(samples[n-1])}
				default:
					out[i] = []Value{q, Some(samples[int(math.Ceil(q*float64(n)))-1])}
				}
			}
			return out
		}
		get := func(c metricContext) Value {
			return Rec("quantiles", snapshot(clockOf(c.fb).millis()), "count", count, "min", low, "max", high, "sum", sum)
		}
		update := func(_ metricContext, in Value) {
			value, timestamp := At(in, 0).(float64), At(in, 1).(float64)
			if maxSize > 0 {
				observations[head%maxSize] = &observation{timestamp, value}
				head++
			}
			count++
			sum += value
			if value < low {
				low = value
			}
			if value > high {
				high = value
			}
		}
		return hooks(get, update, nil)
	})
}

// mapInput is Effect's `Metric.mapInput`.
func mapInput(m *metric, f func(c metricContext, in Value) Value) *metric {
	return metricTransform(m, m.value,
		func(c metricContext, in Value) { m.update(c, f(c, in)) },
		func(c metricContext, in Value) { m.modify(c, f(c, in)) })
}
