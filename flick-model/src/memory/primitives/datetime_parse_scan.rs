//! Lexer and keyword table for V8 `Date.parse` (`datetime_parse.rs`).

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TokTag {
    Invalid,
    Unknown,
    Space,
    Number,
    Symbol,
    End,
    KwInvalid,
    KwMonth,
    KwZone,
    KwTimeSep,
    KwAmPm,
}

#[derive(Clone, Copy)]
pub(super) struct DateToken {
    pub(super) tag: TokTag,
    pub(super) length: usize,
    pub(super) value: i32,
}

impl DateToken {
    pub(super) fn simple(tag: TokTag) -> Self {
        Self {
            tag,
            length: 0,
            value: -1,
        }
    }
    pub(super) fn is_symbol(self, c: char) -> bool {
        self.tag == TokTag::Symbol && self.value == c as i32
    }
    pub(super) fn is_sign(self) -> bool {
        self.is_symbol('+') || self.is_symbol('-')
    }
    pub(super) fn sign(self) -> i32 {
        44 - self.value
    }
    pub(super) fn is_fixed_number(self, n: usize) -> bool {
        self.tag == TokTag::Number && self.length == n
    }
    pub(super) fn is_z(self) -> bool {
        self.tag == TokTag::KwZone && self.length == 1 && self.value == 0
    }
}

const DATE_KEYWORDS: &[(&str, TokTag, i32)] = &[
    ("jan", TokTag::KwMonth, 1),
    ("feb", TokTag::KwMonth, 2),
    ("mar", TokTag::KwMonth, 3),
    ("apr", TokTag::KwMonth, 4),
    ("may", TokTag::KwMonth, 5),
    ("jun", TokTag::KwMonth, 6),
    ("jul", TokTag::KwMonth, 7),
    ("aug", TokTag::KwMonth, 8),
    ("sep", TokTag::KwMonth, 9),
    ("oct", TokTag::KwMonth, 10),
    ("nov", TokTag::KwMonth, 11),
    ("dec", TokTag::KwMonth, 12),
    ("am", TokTag::KwAmPm, 0),
    ("pm", TokTag::KwAmPm, 12),
    ("ut", TokTag::KwZone, 0),
    ("utc", TokTag::KwZone, 0),
    ("z", TokTag::KwZone, 0),
    ("gmt", TokTag::KwZone, 0),
    ("cdt", TokTag::KwZone, -5),
    ("cst", TokTag::KwZone, -6),
    ("edt", TokTag::KwZone, -4),
    ("est", TokTag::KwZone, -5),
    ("mdt", TokTag::KwZone, -6),
    ("mst", TokTag::KwZone, -7),
    ("pdt", TokTag::KwZone, -7),
    ("pst", TokTag::KwZone, -8),
    ("t", TokTag::KwTimeSep, 0),
];

pub(super) struct DateScanner<'a> {
    rest: &'a str,
    ch: char,
    pub(super) peek: DateToken,
}

impl<'a> DateScanner<'a> {
    pub(super) fn new(s: &'a str) -> Self {
        let mut sc = Self {
            rest: s,
            ch: '\0',
            peek: DateToken::simple(TokTag::End),
        };
        sc.advance();
        sc.peek = sc.scan();
        sc
    }

    fn advance(&mut self) {
        let mut iter = self.rest.chars();
        if let Some(c) = iter.next() {
            self.ch = c;
            self.rest = iter.as_str();
        } else {
            self.ch = '\0';
        }
    }

    pub(super) fn next(&mut self) -> DateToken {
        let t = self.peek;
        self.peek = self.scan();
        t
    }

    pub(super) fn skip(&mut self, c: char) -> bool {
        if self.peek.is_symbol(c) {
            self.peek = self.scan();
            true
        } else {
            false
        }
    }

    fn scan(&mut self) -> DateToken {
        let c = self.ch;
        if c == '\0' {
            return DateToken::simple(TokTag::End);
        }
        if c.is_ascii_digit() {
            let mut n = 0i32;
            let mut length = 0usize;
            let mut digits = 0usize;
            while self.ch == '0' {
                length += 1;
                self.advance();
            }
            while self.ch.is_ascii_digit() {
                if digits < 9 {
                    n = n * 10 + (self.ch as i32 - '0' as i32);
                }
                digits += 1;
                length += 1;
                self.advance();
            }
            return DateToken {
                tag: TokTag::Number,
                length,
                value: n,
            };
        }
        if matches!(c, ':' | '-' | '+' | '.' | ')') {
            self.advance();
            return DateToken {
                tag: TokTag::Symbol,
                length: 1,
                value: c as i32,
            };
        }
        if c >= 'A' && !is_js_ws(c) {
            let mut prefix = [0u32; 3];
            let mut length = 0usize;
            while self.ch >= 'A' && !is_js_ws(self.ch) {
                if length < 3 {
                    prefix[length] = (self.ch as u32) | 0x20;
                }
                length += 1;
                self.advance();
            }
            for &(kw, kind, val) in DATE_KEYWORDS {
                let mut want = [0u32; 3];
                for (i, ch) in kw.chars().enumerate() {
                    want[i] = ch as u32;
                }
                if want == prefix && (length <= 3 || kind == TokTag::KwMonth) {
                    return DateToken {
                        tag: kind,
                        length,
                        value: val,
                    };
                }
            }
            return DateToken {
                tag: TokTag::KwInvalid,
                length,
                value: 0,
            };
        }
        if is_js_ws(c) || matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            self.advance();
            return DateToken {
                tag: TokTag::Space,
                length: 1,
                value: -1,
            };
        }
        if c == '(' {
            let mut balance = 0i32;
            loop {
                if self.ch == ')' {
                    balance -= 1;
                } else if self.ch == '(' {
                    balance += 1;
                }
                self.advance();
                if balance <= 0 || self.ch == '\0' {
                    break;
                }
            }
            return DateToken {
                tag: TokTag::Unknown,
                length: 1,
                value: -1,
            };
        }
        self.advance();
        DateToken {
            tag: TokTag::Unknown,
            length: 1,
            value: -1,
        }
    }
}

fn is_js_ws(c: char) -> bool {
    matches!(
        c,
        '\t' | '\x0B' | '\x0C' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}
