use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use crate::diagnostics::{Diagnostic, codes};
use crate::project::config::Source;
use crate::project::{Host, Project, discover};
use crate::source::SourceMap;

struct Memory(BTreeMap<PathBuf, String>);

impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

const ROOT: &str = "schema-version = 1
[workspace]
members = [\"apps/web\", \"sql-plugin\"]

[[workspace.plugins]]
package = \"mysql\"
source = { path = \"sql-plugin\" }
grants = { fs-read = [\"schema/\"], env = [\"DB\"] }
";

const PLUGIN: &str = "schema-version = 1
[package]
name = \"mysql\"
version = \"1.0.0\"
plugin = \"mysql.wasm\"
[project]
kind = \"lib\"
";

fn found(files: &[(&str, &str)], at: &str) -> Result<Project, Diagnostic> {
    let host = Memory(
        (files.iter())
            .map(|(p, t)| (Path::new("/w").join(p), (*t).to_owned()))
            .collect(),
    );
    let mut sources = SourceMap::default();
    discover(&host, &mut sources, &Path::new("/w").join(at), None).map(|p| p.expect("a project"))
}

#[test]
fn a_member_depends_on_the_plugins_of_its_workspace_and_grants_them_more() {
    let web = "schema-version = 1
[project]
kind = \"lib\"
[plugins.mysql]
net = [\"localhost:3306\"]
";
    let project = found(
        &[
            ("flick.toml", ROOT),
            ("apps/web/flick.toml", web),
            ("sql-plugin/flick.toml", PLUGIN),
        ],
        "apps/web",
    )
    .expect("valid");
    let mysql = (project.config.dependencies.iter())
        .find(|d| d.name == "mysql")
        .expect("inherited");
    assert_eq!(mysql.source, Source::Path("../../sql-plugin".into()));
    let grants = &project.config.plugins["mysql"];
    assert_eq!(grants.fs_read, ["schema/"]);
    assert_eq!(grants.net, ["localhost:3306"]);
    assert_eq!(grants.env, ["DB"]);
    assert_eq!(project.workspace, Some(PathBuf::from("/w/flick.toml")));
}

#[test]
fn the_package_of_a_plugin_does_not_depend_on_itself() {
    let project = found(
        &[("flick.toml", ROOT), ("sql-plugin/flick.toml", PLUGIN)],
        "sql-plugin",
    )
    .expect("valid");
    assert!(project.config.dependencies.is_empty());
}

#[test]
fn a_member_that_names_the_plugin_elsewhere_is_refused() {
    let web = "schema-version = 1
[project]
kind = \"lib\"
[dependencies]
mysql = { path = \"../../vendor/mysql\" }
";
    let refused = found(
        &[("flick.toml", ROOT), ("apps/web/flick.toml", web)],
        "apps/web",
    )
    .expect_err("two sources");
    assert_eq!(refused.code, codes::P005);
    assert!(
        refused.message.contains("`[[workspace.plugins]]`"),
        "{}",
        refused.message
    );
    assert_eq!(refused.related.len(), 1);
    // the same directory written another way is the same source
    let same = web.replace("../../vendor/mysql", "../../sql-plugin");
    assert!(
        found(
            &[("flick.toml", ROOT), ("apps/web/flick.toml", &same)],
            "apps/web"
        )
        .is_ok()
    );
}

#[test]
fn a_project_the_workspace_does_not_list_gets_nothing() {
    let other = "schema-version = 1\n[project]\nkind = \"lib\"\n";
    let project = found(
        &[("flick.toml", ROOT), ("other/flick.toml", other)],
        "other",
    )
    .expect("valid");
    assert!(project.config.dependencies.is_empty());
    assert_eq!(project.workspace, None);
}
