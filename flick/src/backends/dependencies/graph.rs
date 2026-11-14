//! A pin covers a package's own bytes and its actual Node dependency graph.
//! Canonical paths identify shared instances internally; they never enter hashes.

use super::{content, names, resolve};
use flick_backend::{RuntimeDependency, sha256};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct Node {
    name: String,
    version: String,
    content: String,
    dependencies: Vec<(String, usize)>,
}

#[derive(Clone)]
struct Package {
    manifest: Value,
    content: String,
}

#[derive(Default)]
pub(super) struct Graph {
    packages: BTreeMap<PathBuf, Package>,
}

impl Graph {
    pub(super) fn pin(&mut self, name: &str, manifest: &Path) -> Result<RuntimeDependency, String> {
        let mut nodes = Vec::new();
        let mut seen = BTreeMap::new();
        self.visit(manifest, &mut nodes, &mut seen)?;
        let mut bytes = Vec::new();
        ciborium::into_writer(&nodes, &mut bytes).map_err(|e| e.to_string())?;
        let version = nodes[0].as_ref().expect("root package").version.clone();
        Ok(RuntimeDependency {
            package: name.into(),
            version,
            sha256: sha256(&bytes),
        })
    }

    fn visit(
        &mut self,
        manifest: &Path,
        nodes: &mut Vec<Option<Node>>,
        seen: &mut BTreeMap<PathBuf, usize>,
    ) -> Result<usize, String> {
        let manifest = fs::canonicalize(manifest).map_err(|e| e.to_string())?;
        if let Some(&index) = seen.get(&manifest) {
            return Ok(index);
        }
        if nodes.len() >= 10_000 {
            return Err("runtime dependency graph is too large".into());
        }
        let index = nodes.len();
        seen.insert(manifest.clone(), index);
        nodes.push(None);
        let root = manifest
            .parent()
            .ok_or("package manifest has no directory")?;
        let package = if let Some(package) = self.packages.get(&manifest) {
            package.clone()
        } else {
            let data: Value =
                serde_json::from_slice(&fs::read(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let package = Package {
                manifest: data,
                content: content(root)?,
            };
            self.packages.insert(manifest.clone(), package.clone());
            package
        };
        let mut dependencies = Vec::new();
        for dependency in names(&package.manifest, true) {
            if let Some(path) = resolve(root, &dependency)? {
                let child = self.visit(&path, nodes, seen)?;
                dependencies.push((dependency, child));
            }
        }
        nodes[index] = Some(Node {
            name: package.manifest["name"].as_str().unwrap_or_default().into(),
            version: package.manifest["version"]
                .as_str()
                .unwrap_or_default()
                .into(),
            content: package.content,
            dependencies,
        });
        Ok(index)
    }
}
