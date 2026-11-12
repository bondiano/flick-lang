//! A type the code of a block declares names classes as a declaration of
//! the module does: the plugin of each derives its instance, told of the
//! block and of what the type is made of.

use super::{host, library, show};
use crate::ir;
use crate::project::load::{check, compile};
use crate::source::SourceMap;

/// `Pairs` with a class its plugin derives.
const PAIRS: &str = "module Pairs exposing (..)

@derive
class Arbitrary a =
  arbitrary : a

instance Arbitrary Float =
  let arbitrary = 0.0
";

const MAIN: &str = "module App.Main exposing (..)

import Pairs exposing (plugin wrapped)

wrapped Meters = Float deriving Pairs.Arbitrary

length : Meters
let length = Pairs.arbitrary

metres : Float
let metres = Meters.value length
";

fn report(main: &str) -> Vec<String> {
    let host = host(main);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &super::roots());
    found.iter().map(|d| show(&sources, d)).collect()
}

#[test]
fn a_class_of_a_type_a_block_declares_is_derived_by_its_plugin() {
    assert_eq!(report(MAIN), [""; 0]);
}

#[test]
fn the_derive_is_told_what_the_type_wraps() {
    // told so, the derive of `pairs` goes through the type's `make`
    let host = host(MAIN);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (_, compilation) = compile(&host, &mut sources, Some(&project), &super::roots());
    let printed = ir::print(&compilation.modules[0].ir);
    assert!(
        printed.contains("arbitrary = (App.Main.Meters$make <Arbitrary Float>.arbitrary)"),
        "{printed}"
    );
}
