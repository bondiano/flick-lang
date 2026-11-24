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
