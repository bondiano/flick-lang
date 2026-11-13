//! Final published interfaces, shared by every target and plain-data preparation.

use std::collections::BTreeMap;
use std::path::Path;

use super::interface;
#[cfg(feature = "native-project")]
use crate::iface;
use crate::iface::Interface;
use crate::project::Compilation;

pub fn published_interfaces<'a>(
    compilation: &'a Compilation,
    package: Option<&str>,
    root: Option<&Path>,
) -> BTreeMap<&'a str, Interface> {
    let modules = &compilation.modules;
    let checked_interfaces = |name: &str| {
        modules
            .iter()
            .find(|module| module.name == name)
            .map(|module| module.interface.clone())
    };
    let published_interfaces: BTreeMap<&str, Interface> = modules
        .iter()
        .map(|module| {
            let mut published = interface(
                &module.ir,
                &module.interface,
                module.package.as_deref().or(package),
                &[],
                &checked_interfaces,
            );
            published.source = root
                .and_then(|root| module.path.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            (module.name.as_str(), published)
        })
        .collect();
    #[cfg(feature = "native-project")]
    let mut published_interfaces = published_interfaces;
    #[cfg(feature = "native-project")]
    {
        // Hash only finalized ABI, including backend availability, exactly as published.
        let fingerprints: BTreeMap<&str, String> = published_interfaces
            .iter()
            .map(|(name, interface)| (*name, iface::fingerprint(interface)))
            .collect();
        for module in modules {
            let published = published_interfaces
                .get_mut(module.name.as_str())
                .expect("every compiled module has a published interface");
            published.dependencies = module
                .imports
                .iter()
                .filter_map(|name| Some((name.clone(), fingerprints.get(name.as_str())?.clone())))
                .collect();
        }
    }
    published_interfaces
}
