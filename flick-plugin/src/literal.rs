//! The value of a string token of Flick, as a block's text holds it: what a
//! plugin reads raw text from, such as `r#"create table …"#`, and what the
//! compiler's own literals mean.

/// The value of a `String` token: `"…"`, `r#"…"#` or `"""…"""`.
/// Line breaks inside the literal are always `\n`, also for `\r\n` sources.
pub use fln_literals::decode_string as string_value;

/// The value of the literal of an enum's constructor, as text: a string's
/// value, an integer with its sign, `-1` of `- 1`.
pub fn literal_text(text: &str) -> String {
    if text.ends_with('"') {
        string_value(text)
    } else {
        text.split_whitespace().collect()
    }
}
