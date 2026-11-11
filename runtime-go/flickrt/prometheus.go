package flickrt

import (
	"math"
	"regexp"
	"strings"
)

// PrometheusMetrics: the registry in Prometheus's text format.

var (
	metricNameUnsafe = regexp.MustCompile(`[^a-zA-Z0-9_:]`)
	labelNameUnsafe  = regexp.MustCompile(`[^a-zA-Z0-9_]`)
	underscores      = regexp.MustCompile(`_+`)
	prometheusTypes  = map[string]string{
		"Counter": "counter", "Gauge": "gauge", "Histogram": "histogram", "Summary": "summary", "Frequency": "counter",
	}
)

func sanitizeMetricName(name string) string {
	s := metricNameUnsafe.ReplaceAllString(name, "_")
	if s != "" && (s[0] >= '0' && s[0] <= '9' || s[0] == ':') {
		s = "_" + s
	}
	return strings.TrimSuffix(underscores.ReplaceAllString(s, "_"), "_")
}

func sanitizeLabelName(name string) string {
	s := labelNameUnsafe.ReplaceAllString(name, "_")
	if s != "" && s[0] >= '0' && s[0] <= '9' {
		s = "_" + s
	}
	return underscores.ReplaceAllString(s, "_")
}

var (
	labelEscapes = strings.NewReplacer(`\`, `\\`, `"`, `\"`, "\n", `\n`)
	helpEscapes  = strings.NewReplacer(`\`, `\\`, "\n", `\n`)
)

func prometheusValue(n float64) string {
	switch {
	case math.IsInf(n, 1):
		return "+Inf"
	case math.IsInf(n, -1):
		return "-Inf"
	case n != n:
		return "NaN"
	}
	return jsString(n)
}

// prometheusLabels are the attributes but units, then the extra labels.
func prometheusLabels(attributes *Record, extra ...string) string {
	var labels []string
	if attributes != nil {
		for i, k := range attributes.keys {
			if k != "unit" && k != "time_unit" {
				labels = append(labels, sanitizeLabelName(k)+`="`+labelEscapes.Replace(attributes.vals[i].(string))+`"`)
			}
		}
	}
	for i := 0; i < len(extra); i += 2 {
		labels = append(labels, sanitizeLabelName(extra[i])+`="`+labelEscapes.Replace(extra[i+1])+`"`)
	}
	if len(labels) == 0 {
		return ""
	}
	return "{" + strings.Join(labels, ",") + "}"
}

func prometheusData(name string, s metricSnapshot) []string {
	st, n := s.state, func(k string) float64 { return s.state.Get(k).(float64) }
	switch s.kind {
	case "Counter":
		return []string{name + prometheusLabels(s.attributes) + " " + prometheusValue(n("count"))}
	case "Gauge":
		return []string{name + prometheusLabels(s.attributes) + " " + prometheusValue(n("value"))}
	case "Frequency":
		var lines []string
		for _, kv := range List(st.Get("occurrences")) {
			lines = append(lines, name+prometheusLabels(s.attributes, "key", At(kv, 0).(string))+" "+jsString(At(kv, 1).(float64)))
		}
		return lines
	}
	var lines []string
	base := prometheusLabels(s.attributes)
	if s.kind == "Histogram" {
		for _, b := range List(st.Get("buckets")) {
			boundary := At(b, 0).(float64)
			if !math.IsInf(boundary, 1) {
				lines = append(lines, name+"_bucket"+prometheusLabels(s.attributes, "le", jsString(boundary))+" "+jsString(At(b, 1).(float64)))
			}
		}
		lines = append(lines, name+"_bucket"+prometheusLabels(s.attributes, "le", "+Inf")+" "+jsString(n("count")))
	} else {
		for _, q := range List(st.Get("quantiles")) {
			if v, ok := some(At(q, 1)); ok {
				lines = append(lines, name+prometheusLabels(s.attributes, "quantile", jsString(At(q, 0).(float64)))+" "+prometheusValue(v.(float64)))
			}
		}
	}
	return append(lines, name+"_sum"+base+" "+prometheusValue(n("sum")), name+"_count"+base+" "+jsString(n("count")))
}

// prometheusFormat is the text of the registry: a family per name, its
// HELP and TYPE from its first series.
func prometheusFormat(fb *Fiber, prefix string, rename func(string) string) string {
	if prefix != "" {
		prefix = sanitizeMetricName(prefix) + "_"
	}
	var names []string
	families := map[string][]metricSnapshot{}
	for _, s := range snapshotOf(metricContextOf(fb)) {
		name := prefix + sanitizeMetricName(rename(s.id))
		if _, ok := families[name]; !ok {
			names = append(names, name)
		}
		families[name] = append(families[name], s)
	}
	var lines []string
	for _, name := range names {
		metrics := families[name]
		if d, ok := some(metrics[0].description); ok && d.(string) != "" {
			lines = append(lines, "# HELP "+name+" "+helpEscapes.Replace(d.(string)))
		}
		lines = append(lines, "# TYPE "+name+" "+prometheusTypes[metrics[0].kind])
		for _, s := range metrics {
			lines = append(lines, prometheusData(name, s)...)
		}
	}
	if len(lines) == 0 {
		return ""
	}
	return strings.Join(lines, "\n") + "\n"
}

func PrometheusFormat(fb *Fiber) Value {
	return prometheusFormat(fb, "", func(name string) string { return name })
}

func PrometheusFormatWith(fb *Fiber, options, _ Value) Value {
	o := record(options)
	prefix, _ := some(o.Get("prefix"))
	p, _ := prefix.(string)
	rename := func(name string) string { return name }
	if f, ok := some(o.Get("rename")); ok {
		rename = func(name string) string { return Apply(fb, f, name).(string) }
	}
	return prometheusFormat(fb, p, rename)
}
