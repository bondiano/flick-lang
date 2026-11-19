//! Diagnostic code identity, formatting and plugin interning.
use super::lookup;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::{fmt, str};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Area {
    Syntax,
    Resolve,
    Types,
    Effects,
    Classes,
    Regions,
    Ffi,
    Schema,
    Sql,
    Ui,
    Cli,
    Project,
    /// The codes of plugins, outside the registry: the number of one is its
    /// place among the codes plugins gave in this process.
    Plugin,
}

impl Area {
    pub const ALL: [Area; 12] = [
        Area::Syntax,
        Area::Resolve,
        Area::Types,
        Area::Effects,
        Area::Classes,
        Area::Regions,
        Area::Ffi,
        Area::Schema,
        Area::Sql,
        Area::Ui,
        Area::Cli,
        Area::Project,
    ];

    pub const fn letter(self) -> char {
        match self {
            Area::Syntax => 'S',
            Area::Resolve => 'R',
            Area::Types => 'T',
            Area::Effects => 'E',
            Area::Classes => 'C',
            Area::Regions => 'G',
            Area::Ffi => 'F',
            Area::Schema => 'D',
            Area::Sql => 'Q',
            Area::Ui => 'U',
            Area::Cli => 'L',
            Area::Project => 'P',
            // never shown: a plugin's code is spelled whole
            Area::Plugin => '?',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code {
    pub area: Area,
    pub number: u16,
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.area {
            Area::Plugin => f.write_str(plugin_codes()[usize::from(self.number)]),
            area => write!(f, "FLICK-{}{:03}", area.letter(), self.number),
        }
    }
}

// shortcut: a plugin's codes are interned for the process, a few strings per
// plugin; make `Code` carry its text if that ever grows without bound.
static PLUGIN_CODES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn plugin_codes() -> MutexGuard<'static, Vec<&'static str>> {
    PLUGIN_CODES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The code `code` a plugin of `package` gives, shown as `<package>/<code>`;
/// `None` when `code` is not capital letters and three digits, such as
/// `L001`, or plugins gave too many codes.
pub fn plugin(package: &str, code: &str) -> Option<Code> {
    if !code.is_ascii() {
        return None;
    }
    let (letters, digits) = code.split_at(code.len().checked_sub(3)?);
    let well_formed = (1..=4).contains(&letters.len())
        && letters.bytes().all(|b| b.is_ascii_uppercase())
        && digits.bytes().all(|b| b.is_ascii_digit());
    if !well_formed {
        return None;
    }
    let whole = format!("{package}/{code}");
    let mut codes = plugin_codes();
    let number = if let Some(i) = codes.iter().position(|c| *c == whole) {
        i
    } else {
        codes.push(whole.leak());
        codes.len() - 1
    };
    let number = u16::try_from(number).ok()?;
    Some(Code {
        area: Area::Plugin,
        number,
    })
}

impl str::FromStr for Code {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        lookup(s).map(|info| info.code).ok_or(())
    }
}
