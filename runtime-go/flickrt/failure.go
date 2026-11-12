package flickrt

import (
	"strings"
	"unicode/utf8"
)

// `Failure` and `RawCause` of the core: a `RawCause` is the whole *Cause,
// and a `CauseReport` the line `describeReport` makes of it, built as an
// inert report is on the JS target: no code of a payload runs.

func FailureRawCause(f Value) Value { return causeOfFailure(f) }

func FailureRethrow(_ *Fiber, f Value) Value { panic(&Failure{causeOfFailure(f)}) }

func FailureInspect(_ *Fiber, f Value) Value { return describeCause(causeOfFailure(f)) }

func RawCauseInspect(_ *Fiber, raw Value) Value { return describeCause(raw.(*Cause)) }

// DescribeReport is the report's line, which `inspect` already made.
func DescribeReport(report Value) Value { return report }

// describeCause is the JS runtime's `describeCause`: `fail Boom`,
// `die <text>`, `die error <message>` or `interrupt` per reason.
func describeCause(c *Cause) string {
	parts := make([]string, 0, 1)
	for _, r := range c.reasons() {
		switch r.kind {
		case causeInterrupt:
			parts = append(parts, "interrupt")
		case causeFail:
			shown := "<object>"
			if d, ok := r.err.(Constructed); ok {
				shown = ConstructorName(d.TagName())
			}
			parts = append(parts, "fail "+shown)
		case causeDie:
			if d, ok := r.err.(Constructed); ok {
				parts = append(parts, "die "+ConstructorName(d.TagName()))
				continue
			}
			text := strings.TrimSuffix(r.defect, "\n")
			if message := defectMessage(r.defect); message != text {
				parts = append(parts, "die error "+inert(message))
			} else {
				parts = append(parts, "die "+inert(text))
			}
		}
	}
	return strings.Join(parts, "; ")
}

// inert is a text cut to the 200 characters a report keeps.
func inert(s string) string {
	if utf8.RuneCountInString(s) <= 200 {
		return s
	}
	return string([]rune(s)[:200]) + "…"
}
