//! The host profiles an environment `lua/<platform>` selects, as the files
//! of `profiles/` record them with their evidence; a test keeps this table
//! equal to those files.

pub(crate) struct HostProfile {
    /// The platform of the environment `lua/<platform>`.
    pub platform: &'static str,
    /// The client or release the profile pins, for messages.
    pub title: &'static str,
    /// The profile file, `profiles/<id>.toml`.
    pub id: &'static str,
    /// The dialects the host runs.
    pub dialects: &'static [&'static str],
    /// Whether the host runs `LuaJIT`, with its `jit` library and FFI.
    pub luajit: bool,
    /// Whether a chunk can `require` a module.
    pub require: bool,
    /// Whether a host callback can leave work suspended to resume later.
    pub suspension: bool,
    /// Global roots the host has only with a `LuaJIT` dialect or
    /// capability: the root and the tag it needs.
    pub needs: &'static [(&'static str, &'static str)],
    /// Every global path the host defines, one per line.
    pub api: &'static str,
}

pub(crate) const PROFILES: &[HostProfile] = &[
    HostProfile {
        platform: "love",
        title: "LÖVE 11.5",
        id: "love-11.5",
        dialects: &["lua51", "luajit"],
        luajit: true,
        require: true,
        suspension: true,
        needs: &[("bit", "luajit"), ("jit", "jit")],
        api: include_str!("../profiles/api/love-11.5.txt"),
    },
    HostProfile {
        platform: "wow-forever",
        title: "World of Warcraft: Forever 1.60.1.71037",
        id: "wow-forever",
        dialects: &["lua51"],
        luajit: false,
        require: false,
        suspension: false,
        needs: &[],
        api: include_str!("../profiles/api/wow-forever.txt"),
    },
    HostProfile {
        platform: "wow-retail",
        title: "World of Warcraft Retail 12.1.5.70961",
        id: "wow-retail",
        dialects: &["lua51"],
        luajit: false,
        require: false,
        suspension: false,
        needs: &[],
        api: include_str!("../profiles/api/wow-retail.txt"),
    },
];

pub(crate) fn find(platform: &str) -> Option<&'static HostProfile> {
    PROFILES.iter().find(|p| p.platform == platform)
}

impl HostProfile {
    /// Whether the API snapshot defines the global path `name`.
    pub fn defines(&self, name: &str) -> bool {
        self.api.lines().any(|line| line == name)
    }
}

/// A dialect a `dialect` tag selects, with the compile-time limits of its
/// VM as `profiles/<id>.toml` records them; a test keeps the two equal.
pub(crate) struct Dialect {
    /// The tag of the `dialect` axis.
    pub tag: &'static str,
    /// The profile file, `profiles/<id>.toml`.
    pub id: &'static str,
    /// The VM, for messages.
    pub title: &'static str,
    /// The most locals a function has in scope at once (`max-locals`).
    pub max_locals: usize,
    /// The most upvalues one function captures (`max-upvalues`).
    pub max_upvalues: usize,
    /// The most constants of one function (`max-constants`): of every kind
    /// together, or with `constants_by_kind` of each kind apart.
    pub max_constants: usize,
    /// Whether numbers and the other constants are counted apart, as
    /// `LuaJIT` does (`limits-observed`).
    pub constants_by_kind: bool,
    /// A function's frame holds fewer registers than this (`max-registers`).
    pub max_registers: usize,
    /// The registers a call takes besides its arguments: the callee, and
    /// on `LuaJIT` the frame link (`limits-observed`: a call after one local
    /// fails at `max_registers - 1 - call_slots` arguments).
    pub call_slots: usize,
    /// Whether a `-0.0` literal keeps its sign (`negative-zero-literal`).
    pub negative_zero: bool,
}

pub(crate) const DIALECTS: &[Dialect] = &[
    Dialect {
        tag: "lua51",
        id: "lua-5.1",
        title: "Lua 5.1.5",
        max_locals: 200,
        max_upvalues: 60,
        max_constants: 262_143,
        constants_by_kind: false,
        max_registers: 250,
        call_slots: 1,
        negative_zero: false,
    },
    Dialect {
        tag: "luajit",
        id: "luajit-2.1",
        title: "LuaJIT 2.1",
        max_locals: 200,
        max_upvalues: 60,
        max_constants: 65_536,
        constants_by_kind: true,
        max_registers: 250,
        call_slots: 2,
        negative_zero: true,
    },
];

/// The dialect of the tag `luajit`, or of `lua51` without it.
pub(crate) fn dialect(tags: &[String]) -> &'static Dialect {
    let tag = if tags.iter().any(|t| t == "luajit") {
        "luajit"
    } else {
        "lua51"
    };
    DIALECTS
        .iter()
        .find(|d| d.tag == tag)
        .unwrap_or(&DIALECTS[0])
}
