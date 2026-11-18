package flickrt

import (
	"os"
	"strings"
	"testing"
)

// liveEnv makes the provider `Config.get` falls back to: the variables the
// live `Host.Environment` lists.
var liveEnv = Fn(func(_ *Fiber, _ Value) Value {
	return Eff(func(*Fiber) Value {
		return ConfigProviderFromEnvRecord(Rec("preserveEmptyStrings", false), PlatformVars())
	})
})

func TestConfigStringReadsEnvironmentAtGet(t *testing.T) {
	const name = "FLICK_TEST_CONFIG_LAZY"
	t.Setenv(name, "constructed")
	config := ConfigString(name)
	t.Setenv(name, "first read")
	if got := ConfigGetWith(rootFiber(), liveEnv, config); got != "first read" {
		t.Fatalf("got %v, want the environment at get", got)
	}
	t.Setenv(name, "second read")
	if got := ConfigGetWith(rootFiber(), liveEnv, config); got != "second read" {
		t.Fatalf("cached a prior read: %v", got)
	}
}

func TestConfigStringMissingEmptyAndDefault(t *testing.T) {
	const name = "FLICK_TEST_CONFIG_MISSING"
	t.Setenv(name, "")
	if err := os.Unsetenv(name); err != nil {
		t.Fatal(err)
	}
	config := ConfigString(name)
	withDefault := ConfigWithDefault("fallback", config)
	_, cause := catch(func() Value { return ConfigGetWith(rootFiber(), liveEnv, config) })
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("missing configuration did not fail: %v", cause)
	}
	error := cause.err.(*Data)
	if error.Tag != Tag("Config.ConfigError.ConfigError") || !strings.Contains(error.Fields()[0].(string), name) {
		t.Fatalf("not the named ConfigError: %v", error)
	}
	if got := ConfigGetWith(rootFiber(), liveEnv, withDefault); got != "fallback" {
		t.Fatalf("missing value got %v", got)
	}
	if got := ConfigGetWith(rootFiber(), liveEnv, ConfigWithDefault("outer", withDefault)); got != "fallback" {
		t.Fatalf("an outer default replaced a successful inner default: %v", got)
	}
	// an empty value is missing
	t.Setenv(name, "")
	if _, cause := catch(func() Value { return ConfigGetWith(rootFiber(), liveEnv, config) }); cause == nil {
		t.Fatalf("an empty environment value was read")
	}
	if got := ConfigGetWith(rootFiber(), liveEnv, withDefault); got != "fallback" {
		t.Fatalf("an empty value did not take the default: %v", got)
	}
	t.Setenv(name, "present")
	if got := ConfigGetWith(rootFiber(), liveEnv, withDefault); got != "present" {
		t.Fatalf("default hid a present environment value: %v", got)
	}
}

func TestConfigGetListsTheEnvironmentIntoArraysAndRecords(t *testing.T) {
	t.Setenv("FLICKTESTVARS_PORTS_0", "80")
	t.Setenv("FLICKTESTVARS_PORTS_1", "443")
	t.Setenv("FLICKTESTVARS_LABELS_TEAM", "core")
	listed := false
	for _, pair := range List(PlatformVars()) {
		kv := List(pair)
		listed = listed || kv[0] == "FLICKTESTVARS_PORTS_1" && kv[1] == "443"
	}
	if !listed {
		t.Fatalf("PlatformVars lacks a variable just set")
	}
	array := ConfigNested("FLICKTESTVARS", ConfigArray(Rec("separator", None), SchemaString, "PORTS"))
	if got := Format(ConfigGetWith(rootFiber(), liveEnv, array)); got != `["80","443"]` {
		t.Fatalf("array from the environment: %v", got)
	}
	labels := ConfigNested("FLICKTESTVARS", ConfigRecord(Rec("separator", None, "keyValueSeparator", None), SchemaString, "LABELS"))
	if got := Format(ConfigGetWith(rootFiber(), liveEnv, labels)); got != `[["TEAM","core"]]` {
		t.Fatalf("record from the environment: %v", got)
	}
}

// hostFiber is a root fiber with the live host services, as `Main` runs one.
func hostFiber() *Fiber {
	fb := rootFiber()
	fb.env = liveHostEnv(fb.env)
	return fb
}

func TestTheDefaultProviderListsTheFibersHostEnvironment(t *testing.T) {
	t.Setenv("FLICKTESTFAKE_PORTS_0", "from the process")
	t.Setenv("FORCE_COLOR", "1")
	fake := Rec(
		"env", Fn(func(_ *Fiber, _ Value) Value { return Eff(func(*Fiber) Value { return None }) }),
		"vars", Fn(func(_ *Fiber, _ Value) Value {
			return Eff(func(*Fiber) Value { return []Value{[]Value{"FLICKTESTFAKE_PORTS_0", "80"}} })
		}),
	)
	fb := rootFiber()
	fb.env = fb.env.withHandler("Host.Environment", record(fake))
	ports := ConfigNested("FLICKTESTFAKE", ConfigArray(Rec("separator", None), SchemaString, "PORTS"))
	if got := Format(ConfigParse(fb, currentProvider(fb), ports)); got != `["80"]` {
		t.Fatalf("the default provider did not list the fake environment: %v", got)
	}
	if got := cliTerminal(fb); got.canColor && !fileIsTerminal(os.Stdout) {
		t.Fatalf("the process forced color, not the fake environment: %+v", got)
	}
}
