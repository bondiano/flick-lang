package flickrt

import "testing"

// What Node's Intl.getCanonicalLocales gives: the language, script and
// region of the canonical tag, or the message of the RangeError it throws.
func TestLocaleTagsAreV8s(t *testing.T) {
	for tag, want := range map[string]string{
		"en": "en", "tr-TR": "tr--TR", "TUR": "tr", "sh-RS": "sr-Latn-RS", "iw": "he", "en-UK": "en--GB",
		"de-1901": "de", "en-US-u-ca": "en--US", "en-u-xx": "en", "en-t-de": "en", "en-t-k0-abc": "en", "en-q-ab": "en",
		"!!": incorrectLocale, "tr_TR": incorrectLocale, "": incorrectLocale, "en--US": incorrectLocale,
		"-en": incorrectLocale, "a": incorrectLocale, "abcdefghi": incorrectLocale, "root": incorrectLocale,
		"en-Latn-Latn": incorrectLocale, "en-12": incorrectLocale, "i-default": incorrectLocale, "ın": incorrectLocale,
		"x-a":                          incorrectLocale,
		"en-":                          "Invalid language tag: en-",
		"en-US-":                       "Invalid language tag: en-us-",
		"en-a":                         "Invalid language tag: en-a",
		"en-u":                         "Invalid language tag: en-u",
		"en-x-":                        "Invalid language tag: en-x-",
		"en-t":                         "Invalid language tag: en-t",
		"en-t-k0":                      "Invalid language tag: en-t-k0",
		"en-US-u-ca-gregory-u-nu-latn": "Invalid language tag: en-us-u-ca-gregory-u-nu-latn",
		"de-1901-1901":                 "Invalid language tag: de-1901-1901",
		"en-u-cä":                      "Invalid language tag: en-u-cÃ¤",
		"en-u-attr-ca-x1":              "Invalid language tag: en-u-attr-ca-x1",
		"en-u-a0":                      "Invalid language tag: en-u-a0",
		"en-x-abcdefghi":               "Invalid language tag: en-x-abcdefghi",
	} {
		id, problem := languageTag(tag)
		got := problem
		if problem == "" {
			got = id.language
			if id.script != "" || id.region != "" {
				got += "-" + id.script
			}
			if id.region != "" {
				got += "-" + id.region
			}
		}
		if got != want {
			t.Errorf("%q: got %q, want %q", tag, got, want)
		}
	}
}
