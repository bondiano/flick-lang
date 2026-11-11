package flickrt

import (
	"math"
	"sort"
	"strings"
	"sync/atomic"
)

// The bindings of `Metric`: Flick's options, the registry's snapshot and
// dump, histogram boundaries and the metrics of the fiber runtime.

func metricCommon(o *Record) (Value, *Record) {
	return o.Get("description"), attributesOf(o.Get("attributes"))
}

func MetricCounter(name, options Value) Value {
	o := record(options)
	description, attributes := metricCommon(o)
	return newCounter(name.(string), description, attributes, o.Get("incremental").(bool))
}

func MetricGauge(name, options Value) Value {
	description, attributes := metricCommon(record(options))
	return newGauge(name.(string), description, attributes)
}

func MetricFrequency(name, options Value) Value {
	o := record(options)
	description, attributes := metricCommon(o)
	return newFrequency(name.(string), description, attributes, List(o.Get("preregisteredWords")))
}

func MetricHistogram(name, options Value) Value {
	o := record(options)
	description, attributes := metricCommon(o)
	return newHistogram(name.(string), description, attributes, List(o.Get("boundaries")))
}

func MetricSummaryWithTimestamp(name, options Value) Value {
	o := record(options)
	description, attributes := metricCommon(o)
	return newSummary(name.(string), description, attributes, millisOf(dur(o.Get("maxAge"))),
		int(o.Get("maxSize").(int32)), List(o.Get("quantiles")))
}

// MetricSummary is a summary whose inputs are stamped with the clock's time.
func MetricSummary(name, options Value) Value {
	return mapInput(metricOf(MetricSummaryWithTimestamp(name, options)), func(c metricContext, in Value) Value {
		return []Value{in, clockOf(c.fb).millis()}
	})
}

// MetricTimer is a histogram of durations in milliseconds, by default with
// exponential boundaries from half a millisecond.
func MetricTimer(name, options Value) Value {
	o := record(options)
	description, attributes := metricCommon(o)
	boundaries := exponentialBoundaries(0.5, 2, 35)
	if b, ok := some(o.Get("boundaries")); ok {
		boundaries = List(b)
	}
	attributes = mergeAttributes(attributes, attributesOf([]Value{[]Value{"time_unit", "milliseconds"}}))
	return mapInput(newHistogram(name.(string), description, attributes, boundaries), func(_ metricContext, in Value) Value {
		return millisOf(dur(in))
	})
}

func MetricValue(fb *Fiber, m Value) Value { return metricOf(m).value(metricContextOf(fb)) }

func MetricUpdate(fb *Fiber, input, m Value) Value {
	metricOf(m).update(metricContextOf(fb), input)
	return Unit
}

func MetricModify(fb *Fiber, input, m Value) Value {
	metricOf(m).modify(metricContextOf(fb), input)
	return Unit
}

func MetricMapInput(f, m Value) Value {
	return mapInput(metricOf(m), func(c metricContext, in Value) Value { return Apply(c.fb, f, in) })
}

func MetricWithConstantInput(input, m Value) Value {
	return mapInput(metricOf(m), func(metricContext, Value) Value { return input })
}

// MetricWithAttributes is the metric with the attributes added to the
// context's: a series of its own.
func MetricWithAttributes(attributes, m Value) Value {
	self, added := metricOf(m), attributesOf(attributes)
	attributed := func(c metricContext) metricContext { return metricContext{c.fb, mergeAttributes(c.extra, added)} }
	return metricTransform(self,
		func(c metricContext) Value { return self.value(attributed(c)) },
		func(c metricContext, in Value) { self.update(attributed(c), in) },
		func(c metricContext, in Value) { self.modify(attributed(c), in) })
}

func MetricWithCurrentAttributes(fb *Fiber, attributes, task Value) Value {
	return withEnv(fb, metricAttributesKey, attributesOf(attributes), task)
}

var metricStateTags = map[string]string{
	"Counter":   Tag("Metric.State.CounterValue"),
	"Gauge":     Tag("Metric.State.GaugeValue"),
	"Frequency": Tag("Metric.State.FrequencyValue"),
	"Histogram": Tag("Metric.State.HistogramValue"),
	"Summary":   Tag("Metric.State.SummaryValue"),
}

type metricSnapshot struct {
	*metricEntry
	state *Record
}

// snapshotOf is every series of the registry and its state, in the order
// they were made.
func snapshotOf(c metricContext) []metricSnapshot {
	metricRegistry.Lock()
	entries := make([]*metricEntry, len(metricRegistry.keys))
	for i, k := range metricRegistry.keys {
		entries[i] = metricRegistry.entries[k]
	}
	metricRegistry.Unlock()
	out := make([]metricSnapshot, len(entries))
	for i, e := range entries {
		e.hooks.mu.Lock()
		out[i] = metricSnapshot{e, record(e.hooks.get(c))}
		e.hooks.mu.Unlock()
	}
	return out
}

func attributePairs(r *Record) []Value {
	if r == nil {
		return []Value{}
	}
	out := make([]Value, len(r.keys))
	for i, k := range r.keys {
		out[i] = []Value{k, r.vals[i]}
	}
	return out
}

func MetricSnapshot(fb *Fiber) Value {
	all := snapshotOf(metricContextOf(fb))
	out := make([]Value, len(all))
	for i, s := range all {
		out[i] = Rec("id", s.id, "description", s.description, "attributes", attributePairs(s.attributes),
			"state", NewData(metricStateTags[s.kind], s.state))
	}
	return out
}

// attributesText is the text of the attributes: by key.
func attributesText(r *Record) string {
	pairs := attributePairs(r)
	sort.SliceStable(pairs, func(i, j int) bool { return At(pairs[i], 0).(string) < At(pairs[j], 0).(string) })
	parts := make([]string, len(pairs))
	for i, kv := range pairs {
		parts[i] = At(kv, 0).(string) + ": " + At(kv, 1).(string)
	}
	return "attributes=[" + strings.Join(parts, ", ") + "]"
}

func keyValues(pairs []Value) string {
	parts := make([]string, len(pairs))
	for i, kv := range pairs {
		parts[i] = "(" + jsString(At(kv, 0)) + " -> " + jsString(At(kv, 1)) + ")"
	}
	return strings.Join(parts, ", ")
}

func renderState(s metricSnapshot) string {
	st, n := s.state, func(k string) string { return jsString(s.state.Get(k).(float64)) }
	switch s.kind {
	case "Counter":
		return "state=[count: [" + n("count") + "]]"
	case "Frequency":
		return "state=[occurrences: " + keyValues(List(st.Get("occurrences"))) + "]"
	case "Gauge":
		return "state=[value: [" + n("value") + "]]"
	}
	rest := ", count: [" + n("count") + "], min: [" + n("min") + "], max: [" + n("max") + "], sum: [" + n("sum") + "]]"
	if s.kind == "Histogram" {
		return "state=[buckets: [" + keyValues(List(st.Get("buckets"))) + "]" + rest
	}
	quantiles := List(st.Get("quantiles"))
	printable := make([]Value, len(quantiles))
	for i, q := range quantiles {
		v, ok := some(At(q, 1))
		if !ok {
			v = 0.0
		}
		printable[i] = []Value{At(q, 0), v}
	}
	return "state=[quantiles: [" + keyValues(printable) + "]" + rest
}

func padEnd(s string, n int) string { return s + strings.Repeat(" ", max(0, n-utf16Len(s))) }

func utf16Len(s string) int { return int(Length(s).(int32)) }

// MetricDump is `Metric.dump`: a line per series, by name.
func MetricDump(fb *Fiber) Value {
	all := snapshotOf(metricContextOf(fb))
	if len(all) == 0 {
		return ""
	}
	name, description, kind, attributes := 0, 0, 0, 0
	for _, s := range all {
		name = max(name, utf16Len(s.id))
		if d, ok := some(s.description); ok {
			description = max(description, utf16Len(d.(string)))
		}
		kind = max(kind, len(s.kind))
		if s.attributes != nil {
			attributes = max(attributes, utf16Len(attributesText(s.attributes)))
		}
	}
	var ids []string
	groups := map[string][]metricSnapshot{}
	for _, s := range all {
		if _, ok := groups[s.id]; !ok {
			ids = append(ids, s.id)
		}
		groups[s.id] = append(groups[s.id], s)
	}
	sort.Strings(ids)
	lines := make([]string, 0, len(all))
	for _, id := range ids {
		for _, s := range groups[id] {
			d, _ := some(s.description)
			text, _ := d.(string)
			lines = append(lines, "name="+padEnd(s.id, name+2)+"description="+padEnd(text, description+2)+
				"type="+padEnd(s.kind, kind+2)+padEnd(attributesText(s.attributes), attributes+2)+renderState(s))
		}
	}
	return strings.Join(lines, "\n")
}

// MetricBoundariesFromIterable is the positive numbers without duplicates,
// then Infinity.
func MetricBoundariesFromIterable(numbers Value) Value {
	var out []Value
	seen := map[float64]bool{}
	for _, v := range List(numbers) {
		n := v.(float64)
		if n != n || seen[n] {
			continue
		}
		seen[n] = true
		if n > 0 {
			out = append(out, n)
		}
	}
	return append(out, math.Inf(1))
}

// makeBy is `f` of each index below `n`: at least one element.
func makeBy(n float64, f func(i int) float64) []Value {
	out := make([]Value, max(1, int(math.Floor(n))))
	for i := range out {
		out[i] = f(i)
	}
	return out
}

func MetricLinearBoundaries(options Value) Value {
	o := record(options)
	start, width, count := o.Get("start").(float64), o.Get("width").(float64), o.Get("count").(int32)
	return MetricBoundariesFromIterable(makeBy(float64(count)-1, func(i int) float64 { return start + float64(i)*width }))
}

func exponentialBoundaries(start, factor float64, count int32) []Value {
	return List(MetricBoundariesFromIterable(makeBy(float64(count)-1, func(i int) float64 {
		return start * math.Pow(factor, float64(i))
	})))
}

func MetricExponentialBoundaries(options Value) Value {
	o := record(options)
	return exponentialBoundaries(o.Get("start").(float64), o.Get("factor").(float64), o.Get("count").(int32))
}

// The metrics of the fiber runtime: a fiber whose context has them
// enabled counts each child fiber it starts and how it ends.
const runtimeMetricsKey = "effect/Metric/FiberRuntimeMetrics"

var (
	runtimeMetricsUsed atomic.Bool
	fibersActive       = newGauge("child_fibers_active", Some("The current count of active child fibers"), nil)
	fibersStarted      = newCounter("child_fibers_started", Some("The total number of child fibers that have been started"), nil, true)
	fiberSuccesses     = newCounter("child_fiber_successes", Some("The total number of child fibers that have succeeded"), nil, true)
	fiberFailures      = newCounter("child_fiber_failures", Some("The total number of child fibers that have failed"), nil, true)

	MetricEnableRuntimeMetricsLayer  = runtimeMetricsLayer(true)
	MetricDisableRuntimeMetricsLayer = runtimeMetricsLayer(false)
)

func runtimeMetricsLayer(on bool) Value {
	return fromBuild(func(*Fiber, *MemoMap, *Owner) Services {
		runtimeMetricsUsed.Store(true)
		return Services{runtimeMetricsKey: on}
	})
}

func MetricEnableRuntimeMetrics(fb *Fiber, task Value) Value {
	runtimeMetricsUsed.Store(true)
	return withEnv(fb, runtimeMetricsKey, true, task)
}

func MetricDisableRuntimeMetrics(fb *Fiber, task Value) Value {
	return withEnv(fb, runtimeMetricsKey, false, task)
}

// fiberMetrics records the start of the child fiber `fb` when its context
// counts them, and gives what records its end, nil when it does not.
func fiberMetrics(fb *Fiber) func(*Cause) {
	if !runtimeMetricsUsed.Load() || !envOr(fb, runtimeMetricsKey, false).(bool) {
		return nil
	}
	c := metricContextOf(fb)
	fibersStarted.update(c, 1.0)
	fibersActive.modify(c, 1.0)
	return func(cause *Cause) {
		fibersActive.modify(c, -1.0)
		if cause == nil {
			fiberSuccesses.update(c, 1.0)
		} else {
			fiberFailures.update(c, 1.0)
		}
	}
}
