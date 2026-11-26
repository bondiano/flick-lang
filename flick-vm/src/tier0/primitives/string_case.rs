//! Case conversion and locale helpers for `String` intrinsics.

use super::Error;

pub(super) fn snake_to(s: &str, capitalize_first: bool) -> String {
    if s.is_empty() || (s.len().is_multiple_of(2) && s.bytes().all(|b| b == b'_')) {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    if capitalize_first {
        out.extend(chars[0].to_uppercase());
    } else {
        out.push(chars[0]);
    }
    let mut i = 1;
    while i < chars.len() {
        if chars[i] == '_' {
            i += 1;
            if let Some(&next) = chars.get(i) {
                out.extend(next.to_uppercase());
            }
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

pub(super) fn camel_to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

pub(super) fn pascal_to_snake(s: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(s.len() + 4);
    out.extend(first.to_lowercase());
    for c in chars {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn split_words(input: &str, split_digits: bool) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut separated = String::with_capacity(input.len() + 8);
    for i in 0..chars.len() {
        let cur = chars[i];
        if cur.is_ascii_alphanumeric() {
            separated.push(cur);
        } else {
            separated.push('\0');
            continue;
        }
        let Some(&next) = chars.get(i + 1) else {
            continue;
        };
        let lower_or_digit_to_upper =
            (cur.is_ascii_lowercase() || cur.is_ascii_digit()) && next.is_ascii_uppercase();
        let acronym_to_word = cur.is_ascii_uppercase()
            && next.is_ascii_uppercase()
            && chars.get(i + 2).is_some_and(char::is_ascii_lowercase);
        let alpha_digit_boundary = split_digits
            && ((cur.is_ascii_alphabetic() && next.is_ascii_digit())
                || (cur.is_ascii_digit() && next.is_ascii_alphabetic()));
        if lower_or_digit_boundary(
            lower_or_digit_to_upper,
            acronym_to_word,
            alpha_digit_boundary,
        ) {
            separated.push('\0');
        }
    }
    separated
        .split('\0')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn lower_or_digit_boundary(a: bool, b: bool, c: bool) -> bool {
    a || b || c
}

fn pascal_word(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::with_capacity(word.len());
    out.extend(first.to_uppercase());
    for c in chars {
        out.extend(c.to_lowercase());
    }
    out
}

pub(super) fn no_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn pascal_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .map(|w| pascal_word(&w))
        .collect::<String>()
}

pub(super) fn camel_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                w.to_lowercase()
            } else {
                pascal_word(&w)
            }
        })
        .collect::<String>()
}

pub(super) fn constant_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

pub(super) fn config_case(input: &str) -> String {
    split_words(input, false)
        .into_iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

pub(super) fn kebab_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

pub(super) fn snake_case(input: &str) -> String {
    split_words(input, true)
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Validates a BCP 47 locale tag and extracts its canonical lowercase primary language subtag.
pub(super) fn parse_locale_language(tag: &str) -> Result<String, Error> {
    if tag.is_empty() {
        return Err(Error::RangeError(format!("Invalid language tag: {tag}")));
    }
    let parts: Vec<&str> = tag.split('-').collect();
    let lang = parts[0];
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(Error::RangeError(format!("Invalid language tag: {tag}")));
    }
    for sub in &parts[1..] {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(Error::RangeError(format!("Invalid language tag: {tag}")));
        }
    }
    let lower = lang.to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "tur" => "tr",
        "aze" => "az",
        "lit" => "lt",
        "ell" | "gre" => "el",
        other => other,
    };
    Ok(canonical.to_owned())
}

pub(super) fn to_locale_upper(locale: &str, text: &str) -> Result<String, Error> {
    let lang = parse_locale_language(locale)?;
    match lang.as_str() {
        "el" => Ok(greek_upper(text)),
        "tr" | "az" => {
            let mut out = String::with_capacity(text.len());
            for c in text.chars() {
                if c == 'i' {
                    out.push('İ');
                } else {
                    out.extend(c.to_uppercase());
                }
            }
            Ok(out)
        }
        "lt" => {
            let chars: Vec<char> = text.chars().collect();
            let mut out = String::with_capacity(text.len());
            for (i, &c) in chars.iter().enumerate() {
                if c == '\u{0307}' && i > 0 && matches!(chars[i - 1], 'i' | 'j' | 'į') {
                    continue;
                }
                out.extend(c.to_uppercase());
            }
            Ok(out)
        }
        _ => Ok(text.to_uppercase()),
    }
}

pub(super) fn to_locale_lower(locale: &str, text: &str) -> Result<String, Error> {
    let lang = parse_locale_language(locale)?;
    match lang.as_str() {
        "tr" | "az" => {
            let chars: Vec<char> = text.chars().collect();
            let mut out = String::with_capacity(text.len());
            for (i, &c) in chars.iter().enumerate() {
                if c == 'İ' {
                    out.push('i');
                } else if c == '\u{0307}' && i > 0 && chars[i - 1] == 'I' {
                    // Combining dot above after 'I' consumed in tr/az lowercasing.
                } else if c == 'I' && chars.get(i + 1) != Some(&'\u{0307}') {
                    out.push('ı');
                } else {
                    out.extend(c.to_lowercase());
                }
            }
            Ok(out)
        }
        "lt" => {
            let chars: Vec<char> = text.chars().collect();
            let mut out = String::with_capacity(text.len());
            for (i, &c) in chars.iter().enumerate() {
                match c {
                    'Ì' => out.push_str("i\u{0307}\u{0300}"),
                    'Í' => out.push_str("i\u{0307}\u{0301}"),
                    'Ĩ' => out.push_str("i\u{0307}\u{0303}"),
                    'I' | 'J' | 'Į'
                        if chars
                            .get(i + 1)
                            .is_some_and(|n| matches!(n, '\u{0300}'..='\u{0314}')) =>
                    {
                        out.extend(c.to_lowercase());
                        out.push('\u{0307}');
                    }
                    _ => out.extend(c.to_lowercase()),
                }
            }
            Ok(out)
        }
        _ => Ok(text.to_lowercase()),
    }
}

fn greek_info(c: char) -> Option<(char, bool, bool, bool, bool)> {
    // Returns (capital, is_vowel, has_accent, has_dialytika, has_ypogegrammeni).
    Some(match c {
        'α' | 'Α' => ('Α', true, false, false, false),
        'ά' | 'Ά' | 'ὰ' | 'ᾶ' => ('Α', true, true, false, false),
        'ᾳ' | 'ᾼ' => ('Α', true, false, false, true),
        'ᾴ' | 'ᾲ' | 'ᾷ' => ('Α', true, true, false, true),
        'ε' | 'Ε' => ('Ε', true, false, false, false),
        'έ' | 'Έ' | 'ὲ' => ('Ε', true, true, false, false),
        'η' | 'Η' => ('Η', true, false, false, false),
        'ή' | 'Ή' | 'ὴ' | 'ῆ' => ('Η', true, true, false, false),
        'ῃ' | 'ῌ' => ('Η', true, false, false, true),
        'ῄ' | 'ῂ' | 'ῇ' => ('Η', true, true, false, true),
        'ι' | 'Ι' => ('Ι', true, false, false, false),
        'ί' | 'Ί' | 'ὶ' | 'ῖ' => ('Ι', true, true, false, false),
        'ϊ' | 'Ϊ' => ('Ι', true, false, true, false),
        'ΐ' | 'ῒ' | 'ῗ' => ('Ι', true, true, true, false),
        'ο' | 'Ο' => ('Ο', true, false, false, false),
        'ό' | 'Ό' | 'ὸ' => ('Ο', true, true, false, false),
        'υ' | 'Υ' => ('Υ', true, false, false, false),
        'ύ' | 'Ύ' | 'ὺ' | 'ῦ' => ('Υ', true, true, false, false),
        'ϋ' | 'Ϋ' => ('Υ', true, false, true, false),
        'ΰ' | 'ῢ' | 'ῧ' => ('Υ', true, true, true, false),
        'ω' | 'Ω' => ('Ω', true, false, false, false),
        'ώ' | 'Ώ' | 'ὼ' | 'ῶ' => ('Ω', true, true, false, false),
        'ῳ' | 'ῼ' => ('Ω', true, false, false, true),
        'ῴ' | 'ῲ' | 'ῷ' => ('Ω', true, true, false, true),
        _ => return None,
    })
}

fn greek_upper(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut after_cased = false;
    let mut after_accent = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_cased = c.is_alphabetic();
        let Some((mut cap, is_vowel, mut has_acc, mut has_dia, has_ypo)) = greek_info(c) else {
            out.extend(c.to_uppercase());
            after_cased = is_cased;
            after_accent = false;
            i += 1;
            continue;
        };
        let mut next = i + 1;
        let mut iotas = usize::from(has_ypo);
        while next < chars.len() {
            match chars[next] {
                '\u{0300}' | '\u{0301}' | '\u{0342}' | '\u{0302}' | '\u{0303}' | '\u{0311}' => {
                    has_acc = true;
                }
                '\u{0308}' => has_dia = true,
                '\u{0344}' => {
                    has_dia = true;
                    has_acc = true;
                }
                '\u{0345}' => iotas += 1,
                '\u{0304}' | '\u{0306}' | '\u{0313}' | '\u{0314}' | '\u{0343}' => {}
                _ => break,
            }
            next += 1;
        }
        if is_vowel && matches!(cap, 'Ι' | 'Υ') && after_accent {
            has_dia = true;
        }
        let next_cased = chars.get(next).is_some_and(|n| n.is_alphabetic());
        if cap == 'Η' && has_acc && iotas == 0 && !after_cased && !next_cased {
            cap = 'Ή';
        } else if has_dia && cap == 'Ι' {
            cap = 'Ϊ';
        } else if has_dia && cap == 'Υ' {
            cap = 'Ϋ';
        }
        out.push(cap);
        for _ in 0..iotas {
            out.push('Ι');
        }
        after_accent = is_vowel && has_acc && !has_dia;
        after_cased = is_cased;
        i = next;
    }
    out
}
