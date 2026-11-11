package flickrt

// `References`: the settings every fiber carries, read by the current task
// or set around one.

func ReferencesCurrentLogLevel(fb *Fiber) Value { return currentLogLevel(fb) }

func ReferencesWithCurrentLogLevel(fb *Fiber, level, task Value) Value {
	return withEnv(fb, currentLogLevelKey, level, task)
}

// unhandledLogLevel is the level a failure no one handled is logged at:
// `Error` unless the fiber sets another or none.
func unhandledLogLevel(fb *Fiber) (string, bool) {
	if l, ok := some(envOr(fb, unhandledLogLevelKey, Some("Error"))); ok {
		return l.(string), true
	}
	return "", false
}

// logUnhandled logs `message` with a failure no one handles, if any, at
// the unhandled log level.
func logUnhandled(fb *Fiber, message string, cause *Cause) {
	if level, ok := unhandledLogLevel(fb); ok && cause != nil {
		logWith(fb, level, []Value{message}, cause)
	}
}

func ReferencesWithUnhandledLogLevel(fb *Fiber, level, task Value) Value {
	return withEnv(fb, unhandledLogLevelKey, level, task)
}

// referenceText is a value of the References adapters as text: JSON of
// one that is not a string.
func referenceText(v Value) string {
	if s, ok := v.(string); ok {
		return s
	}
	return Format(v)
}

func textPairs(r *Record) Value {
	out := make([]Value, len(r.keys))
	for i, k := range r.keys {
		out[i] = []Value{k, referenceText(r.vals[i])}
	}
	return out
}

func ReferencesCurrentLogAnnotations(fb *Fiber) Value { return textPairs(logAnnotationsOf(fb)) }

// ReferencesCurrentLogSpans is the labels of the log spans, innermost first.
func ReferencesCurrentLogSpans(fb *Fiber) Value {
	spans := logSpansOf(fb)
	out := make([]Value, len(spans))
	for i, s := range spans {
		out[i] = s.label
	}
	return out
}

func ReferencesCurrentStackFrame(fb *Fiber) Value { return envOptional(fb, stackFrameKey) }

func ReferencesWithDisablePropagation(fb *Fiber, disabled, task Value) Value {
	return withEnv(fb, disablePropagationKey, disabled, task)
}

func ReferencesWithTracerEnabled(fb *Fiber, enabled, task Value) Value {
	return withEnv(fb, tracerEnabledKey, enabled, task)
}

func ReferencesWithTracerTimingEnabled(fb *Fiber, enabled, task Value) Value {
	return withEnv(fb, tracerTimingKey, enabled, task)
}

func ReferencesTracerSpanAnnotations(fb *Fiber) Value {
	return textPairs(envOr(fb, spanAnnotationsKey, noAttributes).(*Record))
}
