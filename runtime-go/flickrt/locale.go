package flickrt

import "strings"

// Locale tags as V8 reads them for Intl and the locale-sensitive String
// functions: checked and canonicalized, with the RangeErrors V8 throws.

// localeID is a canonical tag's language, script and region.
type localeID struct{ language, script, region string }

const incorrectLocale = "Incorrect locale information provided"

func alpha(s string) bool {
	return strings.Trim(s, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ") == ""
}
func allDigits(s string) bool { return strings.Trim(s, "0123456789") == "" }
func alnum(s string) bool {
	return strings.Trim(s, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789") == ""
}

func alnumOf(s string, least, most int) bool { return len(s) >= least && len(s) <= most && alnum(s) }
func languageSubtag(s string) bool {
	return alpha(s) && (len(s) >= 2 && len(s) <= 3 || len(s) >= 5 && len(s) <= 8)
}
func scriptSubtag(s string) bool { return len(s) == 4 && alpha(s) }
func regionSubtag(s string) bool {
	return len(s) == 2 && alpha(s) || len(s) == 3 && allDigits(s)
}
func variantSubtag(s string) bool {
	return alnumOf(s, 5, 8) || len(s) == 4 && s[0] >= '0' && s[0] <= '9' && alnum(s)
}
func singleton(s string) bool { return alnumOf(s, 1, 1) }

// languageTag reads a locale tag as V8 checks and canonicalizes one, or
// gives the message of the RangeError V8 throws for it: V8's own check of
// the language, script, region and variants, then ICU's of the whole tag.
func languageTag(tag string) (localeID, string) {
	// V8 splits as std::getline does, without an empty last subtag
	subtags := strings.Split(strings.TrimSuffix(tag, "-"), "-")
	n, i := len(subtags), 1
	if !languageSubtag(subtags[0]) {
		return localeID{}, incorrectLocale
	}
	id := localeID{language: strings.ToLower(subtags[0])}
	if i < n && scriptSubtag(subtags[i]) {
		id.script = strings.ToUpper(subtags[i][:1]) + strings.ToLower(subtags[i][1:])
		i++
	}
	if i < n && regionSubtag(subtags[i]) {
		id.region = strings.ToUpper(subtags[i])
		i++
	}
	variants := i
	for ; i < n && !singleton(subtags[i]); i++ {
		if !variantSubtag(subtags[i]) {
			return localeID{}, incorrectLocale
		}
	}
	invalid := "Invalid language tag: " + asciiLowerBytes(tag)
	if strings.HasSuffix(tag, "-") {
		return localeID{}, invalid
	}
	seen := map[string]bool{}
	for _, v := range subtags[variants:i] {
		if v = strings.ToLower(v); seen[v] {
			return localeID{}, invalid
		}
		seen[v] = true
	}
	for i < n {
		s := strings.ToLower(subtags[i])
		if !singleton(s) || seen[s] {
			return localeID{}, invalid
		}
		seen[s] = true
		next, ok := extension(s, subtags, i+1)
		if !ok {
			return localeID{}, invalid
		}
		i = next
	}
	return canonicalLocale(id), ""
}

// extension is the end of the extension of `singleton` from subtags[i], by
// the grammar of BCP 47 and Unicode's `u` and `t` extensions.
func extension(singleton string, subtags []string, i int) (int, bool) {
	n, start := len(subtags), i
	values := func(least int) {
		for i < n && alnumOf(subtags[i], least, 8) {
			i++
		}
	}
	switch singleton {
	case "x":
		values(1)
		return i, i > start && i == n
	case "u":
		values(3) // attributes
		for i < n && len(subtags[i]) == 2 && alnum(subtags[i][:1]) && alpha(subtags[i][1:]) {
			i++
			values(3)
		}
	case "t":
		if i < n && languageSubtag(subtags[i]) {
			i++
			if i < n && scriptSubtag(subtags[i]) {
				i++
			}
			if i < n && regionSubtag(subtags[i]) {
				i++
			}
			for i < n && variantSubtag(subtags[i]) {
				i++
			}
		}
		for i < n && len(subtags[i]) == 2 && alpha(subtags[i][:1]) && allDigits(subtags[i][1:]) {
			i++
			field := i
			if values(3); i == field {
				return i, false
			}
		}
	default:
		values(2)
	}
	return i, i > start
}

// asciiLowerBytes is how V8 quotes a tag in its message: lower-cased if it
// is ASCII, else each byte of its UTF-8 a character.
func asciiLowerBytes(s string) string {
	out, ascii := make([]rune, len(s)), true
	for i := 0; i < len(s); i++ {
		out[i], ascii = rune(s[i]), ascii && s[i] < 0x80
	}
	if ascii {
		return strings.ToLower(s)
	}
	return string(out)
}

var localeAliases onFirstUse[map[string]string]

// canonicalLocale replaces deprecated languages and regions as ICU does.
func canonicalLocale(id localeID) localeID {
	aliases := localeAliases.get(func() map[string]string {
		out := map[string]string{}
		for _, entry := range strings.Split(languageAliasData+";"+regionAliasData, ";") {
			from, to, _ := strings.Cut(entry, ">")
			out[from] = to
		}
		return out
	})
	if to, ok := aliases[id.language]; ok {
		parts := strings.Split(to, "-")
		id.language = parts[0]
		for _, p := range parts[1:] {
			if scriptSubtag(p) && id.script == "" {
				id.script = p
			} else if regionSubtag(p) && id.region == "" {
				id.region = p
			}
		}
	}
	if to, ok := aliases[id.region]; ok && id.region != "" {
		id.region = to
	}
	return id
}

// parseLocale is languageTag that throws V8's RangeError.
func parseLocale(tag string) localeID {
	id, problem := languageTag(tag)
	if problem != "" {
		panic(rangeError(problem))
	}
	return id
}
