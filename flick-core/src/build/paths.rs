//! Native paths used by generated imports and source mappings.

use std::path::{Component, Path, Prefix};

/// The relative import specifier of `target` from the directory `from`.
pub(crate) fn specifier(from: &Path, target: &Path) -> String {
    let from: Vec<Component> = from.components().collect();
    let to: Vec<Component> = target.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| same_component(a, b))
        .count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    let joined = parts.join("/");
    if joined.starts_with("..") {
        joined
    } else {
        format!("./{joined}")
    }
}

fn same_component(left: &Component<'_>, right: &Component<'_>) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            same_prefix(left.kind(), right.kind())
        }
        _ => left == right,
    }
}

// Canonical Windows paths use verbatim prefixes; the output directory and
// its source files can still name the same disk or UNC share in either form.
// Compare their volume identities without changing normal path components.
fn same_prefix(left: Prefix<'_>, right: Prefix<'_>) -> bool {
    use Prefix::{Disk, UNC, VerbatimDisk, VerbatimUNC};
    match (left, right) {
        (Disk(left) | VerbatimDisk(left), Disk(right) | VerbatimDisk(right)) => left == right,
        (
            UNC(left_server, left_share) | VerbatimUNC(left_server, left_share),
            UNC(right_server, right_share) | VerbatimUNC(right_server, right_share),
        ) => left_server == right_server && left_share == right_share,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::{same_prefix, specifier};
    use std::ffi::OsStr;
    use std::path::{Path, Prefix};

    #[test]
    fn disk_spellings_share_a_source_mapping_root() {
        assert!(same_prefix(Prefix::Disk(b'D'), Prefix::VerbatimDisk(b'D')));
        assert!(same_prefix(Prefix::VerbatimDisk(b'D'), Prefix::Disk(b'D')));
    }

    #[test]
    fn unc_spellings_share_a_source_mapping_root() {
        let server = OsStr::new("server");
        let share = OsStr::new("sources");
        assert!(same_prefix(
            Prefix::UNC(server, share),
            Prefix::VerbatimUNC(server, share)
        ));
        assert!(same_prefix(
            Prefix::VerbatimUNC(server, share),
            Prefix::UNC(server, share)
        ));
    }

    #[test]
    fn distinct_volumes_and_device_namespaces_stay_distinct() {
        let server = OsStr::new("server");
        let share = OsStr::new("sources");
        assert!(!same_prefix(Prefix::Disk(b'C'), Prefix::VerbatimDisk(b'D')));
        assert!(!same_prefix(
            Prefix::UNC(server, share),
            Prefix::VerbatimUNC(server, OsStr::new("other"))
        ));
        assert!(!same_prefix(
            Prefix::UNC(server, share),
            Prefix::VerbatimUNC(OsStr::new("other"), share)
        ));
        assert!(!same_prefix(
            Prefix::DeviceNS(OsStr::new("D:")),
            Prefix::Disk(b'D')
        ));
    }

    #[test]
    fn relative_specifiers_preserve_unicode_and_common_directories() {
        assert_eq!(
            specifier(
                Path::new("/p/проект ⭐/dist/go/main"),
                Path::new("/p/проект ⭐/src/App/Main.fl")
            ),
            "../../../src/App/Main.fl"
        );
        assert_eq!(
            specifier(Path::new("/p/dist/App"), Path::new("/p/dist/App/Other.js")),
            "./Other.js"
        );
        assert_eq!(
            specifier(Path::new("/p/dist/App"), Path::new("/p/dist/Lib/Other.js")),
            "../Lib/Other.js"
        );
    }

    #[cfg(windows)]
    #[test]
    fn source_mappings_accept_mixed_canonical_and_regular_roots() {
        for (from, target) in [
            (
                r"\\?\D:\p\проект ⭐\dist\go\main",
                r"D:\p\проект ⭐\src\App\Main.fl",
            ),
            (
                r"D:\p\проект ⭐\dist\go\main",
                r"\\?\D:\p\проект ⭐\src\App\Main.fl",
            ),
            (
                r"\\?\UNC\server\share\p\dist\go\main",
                r"\\server\share\p\src\App\Main.fl",
            ),
        ] {
            assert_eq!(
                specifier(Path::new(from), Path::new(target)),
                "../../../src/App/Main.fl"
            );
        }
    }
}
