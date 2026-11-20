use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::Ide;
use super::tests::fixture_of;
use crate::diagnostics::Diagnostic;
use crate::project::{Host, ModuleCache, analyze_with};
use crate::source::{FileId, SourceMap};
use crate::syntax::parse;

const HOVER_PROVIDER: &str = "\
module App.Lib exposing (Box, AWrapSink, Sink, Cell, Phantom, WrapPhantom, DoubleSink, Thunk, BoxErr(..), SinkErr(..), CellErr(..), Alias, Abstract)

/// A covariant opaque box.
opaque type Box a = Box { value : a }
opaque type AWrapSink a = AWrapSink (Sink a)
opaque type Sink a = Sink (a -> Unit)
opaque type Cell a = Cell (Ref a)
opaque type Phantom a = Phantom
opaque type WrapPhantom a = WrapPhantom (Phantom a)
opaque type DoubleSink a = DoubleSink (Sink (Sink a))
opaque type Thunk (r : ServiceRow) a = Thunk (Unit ->{r} a)
type error BoxErr a = BoxErr a
type error SinkErr a = SinkErr (a -> Unit)
type error CellErr a = CellErr (a -> a)
opaque type alias Alias a = List a
type Abstract a
";

const HOVER_MAIN: &str = "\
module App.Main exposing (..)

import App.Lib exposing (Box, AWrapSink, Sink, Cell, Phantom, WrapPhantom, DoubleSink, Thunk, BoxErr(..), SinkErr(..), CellErr(..), Alias, Abstract)

type LocalPair a b = LocalPair (a -> b)
type error LocalErr a = LocalErr (a -> Unit)
type WrapSink a = WrapSink (Sink a)

use_wrap_sink : WrapSink Int -> WrapSink Int
let use_wrap_sink x = x

use_a_wrap_sink : AWrapSink Int -> AWrapSink Int
let use_a_wrap_sink x = x

use_wrap_phantom : WrapPhantom Int -> WrapPhantom Int
let use_wrap_phantom x = x

use_double_sink : DoubleSink Int -> DoubleSink Int
let use_double_sink x = x

use_box : Box Int -> Box Int
let use_box x = x

use_sink : Sink Int -> Sink Int
let use_sink x = x

use_cell : Cell Int -> Cell Int
let use_cell x = x

use_phantom : Phantom Int -> Phantom Int
let use_phantom x = x

use_thunk : Thunk none Int -> Thunk none Int
let use_thunk x = x

use_box_err : BoxErr Int -> BoxErr Int
let use_box_err x = x

use_sink_err : SinkErr Int -> SinkErr Int
let use_sink_err x = x

use_cell_err : CellErr Int -> CellErr Int
let use_cell_err x = x

use_alias : Alias Int -> Alias Int
let use_alias x = x

use_abstract : Abstract Int -> Abstract Int
let use_abstract x = x

use_list : List Int -> Option Int
let use_list _ = None
";

#[test]
fn data_variance_appears_in_hover_for_local_opaque_and_builtin_types() {
    let f = fixture_of(&[
        ("src/App/Lib.fl", HOVER_PROVIDER),
        ("src/App/Main.fl", HOVER_MAIN),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );

    let main_file = f.file("App.Main");
    let type_at = |needle: &str| {
        let delta = u32::try_from(needle.rfind(' ').expect("space") - 1).expect("offset");
        f.at("App.Main", needle, 0) + delta
    };
    for (needle, expected) in [
        (": WrapSink Int", "Variance: `a` (`-`, contravariant)"),
        (": AWrapSink Int", "Variance: `a` (`-`, contravariant)"),
        (": WrapPhantom Int", "Variance: `a` (`+`, covariant)"),
        (": DoubleSink Int", "Variance: `a` (`inv`, invariant)"),
        (": Box Int", "Variance: `a` (`+`, covariant)"),
        (": Sink Int", "Variance: `a` (`-`, contravariant)"),
        (": Cell Int", "Variance: `a` (`inv`, invariant)"),
        (": Phantom Int", "Variance: `a` (`bi`, bivariant)"),
        (
            ": Thunk none",
            "Variance: `r` (`inv`, invariant), `a` (`+`, covariant)",
        ),
        (": BoxErr Int", "Variance: `a` (`+`, covariant)"),
        (": SinkErr Int", "Variance: `a` (`-`, contravariant)"),
        (": CellErr Int", "Variance: `a` (`inv`, invariant)"),
        (
            "LocalPair a",
            "Variance: `a` (`-`, contravariant), `b` (`+`, covariant)",
        ),
        ("LocalErr a", "Variance: `a` (`-`, contravariant)"),
        (": List Int", "Variance: `a` (`+`, covariant)"),
        ("-> Option Int", "Variance: `a` (`+`, covariant)"),
    ] {
        let (markdown, _) = ide.hover(main_file, type_at(needle)).expect(needle);
        assert!(markdown.contains(expected), "{needle}: {markdown}");
    }

    let pair_a = f.at("App.Main", "a b =", 0);
    let (hover_a, _) = ide.hover(main_file, pair_a).expect("LocalPair param a");
    assert!(
        hover_a.contains("Variance: `-` (contravariant)"),
        "{hover_a}"
    );
    let pair_b = f.at("App.Main", "b =", 0);
    let (hover_b, _) = ide.hover(main_file, pair_b).expect("LocalPair param b");
    assert!(hover_b.contains("Variance: `+` (covariant)"), "{hover_b}");
    let err_a = f.at("App.Main", "a = LocalErr", 0);
    let (hover_err_a, _) = ide.hover(main_file, err_a).expect("LocalErr param a");
    assert!(
        hover_err_a.contains("Variance: `-` (contravariant)"),
        "{hover_err_a}"
    );

    for needle in [": Alias Int", ": Abstract Int"] {
        let (markdown, _) = ide.hover(main_file, type_at(needle)).expect(needle);
        assert!(!markdown.contains("Variance:"), "{needle}: {markdown}");
    }
}

#[test]
fn data_variance_hover_updates_on_edit_and_falls_back_without_source() {
    struct Mem(BTreeMap<PathBuf, String>);
    impl Host for Mem {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .map(|t| t.clone().into_bytes())
                .ok_or_else(|| ErrorKind::NotFound.into())
        }
        fn display(&self, path: &Path) -> String {
            path.display().to_string()
        }
    }

    let provider = "module App.Lib exposing (Box)\nopaque type Box a = Box { value : a }\n";
    let main = "module App.Main exposing (use_box)\nimport App.Lib exposing (Box)\nuse_box : Box Int -> Box Int\nlet use_box x = x\n";
    let mut files = BTreeMap::from([
        (PathBuf::from("/p/src/App/Lib.fl"), provider.to_owned()),
        (PathBuf::from("/p/src/App/Main.fl"), main.to_owned()),
    ]);
    let mut cache = ModuleCache::default();
    let roots = [
        PathBuf::from("/p/src/App/Lib.fl"),
        PathBuf::from("/p/src/App/Main.fl"),
    ];
    let mut sources = SourceMap::default();
    let mut first = analyze_with(&Mem(files.clone()), &mut sources, None, &roots, &mut cache);
    let first_main = first.module("App.Main").expect("main").file;
    let at = u32::try_from(main.find("Box Int").expect("Box")).expect("small");
    let first_ide = Ide {
        analysis: &first,
        sources: &sources,
    };
    let (before, _) = first_ide.hover(first_main, at + 1).expect("before edit");
    assert!(
        before.contains("Variance: `a` (`+`, covariant)"),
        "{before}"
    );

    first
        .modules
        .iter_mut()
        .find(|m| m.name == "App.Lib")
        .expect("App.Lib")
        .tree = parse("", FileId::default()).tree;
    let ide_no_src = Ide {
        analysis: &first,
        sources: &sources,
    };
    let (positional, _) = ide_no_src
        .hover(first_main, at + 1)
        .expect("positional fallback hover");
    assert!(
        positional.contains("Variance: `+` (covariant)"),
        "interface-only fallback: {positional}"
    );

    files.insert(
        PathBuf::from("/p/src/App/Lib.fl"),
        provider.replace(
            "opaque type Box a = Box { value : a }",
            "opaque type Box a = Box (Ref a)",
        ),
    );
    let second = analyze_with(&Mem(files), &mut sources, None, &roots, &mut cache);
    let second_ide = Ide {
        analysis: &second,
        sources: &sources,
    };
    let second_main = second.module("App.Main").expect("main").file;
    let (after, _) = second_ide.hover(second_main, at + 1).expect("after edit");
    assert!(
        after.contains("Variance: `a` (`inv`, invariant)"),
        "{after}"
    );
}
