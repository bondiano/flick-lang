package flickrt

// `LogLevel`: a level is its name, ordered by its ordinal.

import "math"

const minimumLogLevelKey = "effect/References/MinimumLogLevel"

var LogLevelValues Value = []Value{"All", "Fatal", "Error", "Warn", "Info", "Debug", "Trace", "None"}

func logLevelOrdinal(level string) float64 {
	switch level {
	case "All":
		return -(1<<53 - 1)
	case "Fatal":
		return 50000
	case "Error":
		return 40000
	case "Warn":
		return 30000
	case "Info":
		return 20000
	case "Debug":
		return 10000
	case "Trace":
		return 0
	}
	return math.Pow(2, 53) - 1
}

func LogLevelOrdinal(level Value) Value { return logLevelOrdinal(level.(string)) }

func logLevelCompare(that, self Value) float64 {
	return logLevelOrdinal(self.(string)) - logLevelOrdinal(that.(string))
}

// LogLevelGreaterThan and the rest are data-last comparisons: `self`
// against `that`.
func LogLevelGreaterThan(that, self Value) Value        { return logLevelCompare(that, self) > 0 }
func LogLevelGreaterThanOrEqual(that, self Value) Value { return logLevelCompare(that, self) >= 0 }
func LogLevelLessThan(that, self Value) Value           { return logLevelCompare(that, self) < 0 }
func LogLevelLessThanOrEqual(that, self Value) Value    { return logLevelCompare(that, self) <= 0 }

// minimumLogLevel is `References.MinimumLogLevel` of the fiber.
func minimumLogLevel(fb *Fiber) string {
	if level, ok := fb.env.get(minimumLogLevelKey); ok {
		return level.(string)
	}
	return "Info"
}

// LogLevelIsEnabled is `LogLevel.is_enabled`: whether the fiber logs at the level.
func LogLevelIsEnabled(fb *Fiber, level Value) Value {
	return logLevelOrdinal(minimumLogLevel(fb)) <= logLevelOrdinal(level.(string))
}

// ReferencesWithMinimumLogLevel is `References.with_minimum_log_level`.
func ReferencesWithMinimumLogLevel(fb *Fiber, level, task Value) Value {
	return withEnv(fb, minimumLogLevelKey, level, task)
}
