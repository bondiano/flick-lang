//! Final published interfaces, shared by every target and plain-data preparation.

use std::collections::BTreeMap;
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
use super::abi::interface_without_dependencies;
#[cfg(feature = "native-project")]
use crate::iface;
use crate::iface::{Interface, Target};
use crate::project::Compilation;

pub fn published_interfaces<'a>(
    compilation: &'a Compilation,
    package: Option<&str>,
    root: Option<&Path>,
    targets: Option<&[Target]>,
) -> BTreeMap<&'a str, Interface> {
    let modules = &compilation.modules;
    #[cfg(not(target_arch = "wasm32"))]
    let checked_interfaces = |name: &str| {
        modules
            .iter()
            .find(|module| module.name == name)
            .map(|module| module.interface.clone())
    };
    #[cfg(target_arch = "wasm32")]
    let _ = package;
    // `extend`, not `collect`: collecting compiles a sort of these large
    // entries into the wasm compiler
    let mut published_interfaces: BTreeMap<&str, Interface> = BTreeMap::new();
    published_interfaces.extend(modules.iter().map(|module| {
        #[cfg(not(target_arch = "wasm32"))]
        let mut published = interface_without_dependencies(
            &module.ir,
            &module.interface,
            module.package.as_deref().or(package),
            &checked_interfaces,
        );
        #[cfg(target_arch = "wasm32")]
        let mut published = module.interface.clone();
        if let Some(targets) = targets {
            published.targets = targets.to_vec();
        }
        published.source = root
            .and_then(|root| module.path.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"));
        (module.name.as_str(), published)
    }));
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
