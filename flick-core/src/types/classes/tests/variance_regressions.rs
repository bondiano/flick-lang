use crate::types::ForeignBackendSpec;
use std::slice;

use super::{Dict, check_all};
use crate::diagnostics::codes;
use crate::iface::Decl;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;
use crate::types::env::Builtins;
use crate::types::lang::Lang;
use crate::types::regions;
use crate::types::ty::{Effects, Row, Type, Written};
use crate::types::{
    Elem, Limits, Options, StaticContext, Thresholds, Variance, Watched, builtins,
    check_with_evidence, checker_in,
};

#[test]
fn bivariant_phantom_data_params_stay_exact_for_dispatch_and_overlap() {
    // Disjoint phantom-tag instances must NOT overlap under K1', and each call
    // must resolve to its own dictionary.
    let ok_src = "\
module App.PhantomOk exposing (..)

type Proxy a = Proxy

class Describe a =
  describe : a -> String

instance Describe (Proxy Int) =
  let describe _ = \"int\"

instance Describe (Proxy String) =
  let describe _ = \"string\"

use_int : String
let use_int = describe (Proxy : Proxy Int)

use_str : String
let use_str = describe (Proxy : Proxy String)
";
    let (diags, dicts) = check_all(&[("App.PhantomOk", ok_src)]);
    assert!(
        diags.is_empty(),
        "disjoint phantom-tag instances must not report overlap: {diags:?}"
    );
    let globals: Vec<&str> = dicts
        .iter()
        .filter_map(|d| match d {
            Dict::Instance { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        globals.len() >= 2 && globals[0] != globals[1],
        "distinct phantom tags must resolve to distinct dictionaries: {globals:?}"
    );

    // Conversely, `Default (Proxy Int)` must NOT serve `Proxy String`.
    let bad_src = "\
module App.PhantomBad exposing (..)

type Proxy a = Proxy

class Default a =
  default : a

instance Default (Proxy Int) =
  let default = Proxy

bad : Proxy String
let bad = default
";
    let (diags, _) = check_all(&[("App.PhantomBad", bad_src)]);
    assert!(
        diags.iter().any(|d| d.code == codes::C001),
        "Default (Proxy Int) must be rejected for Proxy String with C001: {diags:?}"
    );
}

#[test]
fn hidden_capture_holds_grow_before_class_solving_in_instances_and_defaults() {
    // Valid method passing a hidden-capture value to polymorphic `Wrap a` must
    // succeed inside BOTH an `instance` method body AND a `class` default body
    // (no spurious FLICK-G002 from premature capture-row meta closing).
    let ok_src = "\
module App.CaptureInMethodsOk exposing (..)

type File (s : Region)

service Files =
  read_f : forall s. File s ->{} Int

type Hold a = Hold (Unit ->{Files} a)

class Wrap a =
  wrap : a -> Int

instance Wrap a =
  let wrap _ = 1

class Runner t =
  do_it : forall s. t -> File s ->{Files} Int
  let do_it _ f = wrap (Hold (fun () -> Files.read_f f))

instance Runner Unit =
  let do_it () f = wrap (Hold (fun () -> Files.read_f f))

instance Runner Int
";
    let (diags, _) = check_all(&[("App.CaptureInMethodsOk", ok_src)]);
    assert!(
        diags.is_empty(),
        "hidden-capture Hold in instance method and class default must pass: {diags:?}"
    );

    // Conversely, passing a hidden-capture `Hold` to a capture-free `-` instance
    // `Leak (Hold Int)` must be rejected with `FLICK-C001` inside BOTH an
    // `instance` method body AND a `class` default body (matching top-level).
    let bad_inst_src = "\
module App.CaptureInInstanceFail exposing (..)

type File (s : Region)

service Files =
  read_f : forall s. File s ->{} Int

type Hold a = Hold (Unit ->{Files} a)

class Leak a =
  leak : a -> Int

instance Leak (Hold Int) =
  let leak _ = 0

class BadInst t =
  run_inst : forall s. t -> File s ->{Files} Int

instance BadInst Unit =
  let run_inst () f = leak (Hold (fun () -> Files.read_f f))
";
    let (diags, _) = check_all(&[("App.CaptureInInstanceFail", bad_inst_src)]);
    assert!(
        diags.iter().any(|d| d.code == codes::C001),
        "capture-free `-` Leak (Hold Int) must fail with C001 inside instance method: {diags:?}"
    );

    let bad_default_src = "\
module App.CaptureInDefaultFail exposing (..)

type File (s : Region)

service Files =
  read_f : forall s. File s ->{} Int

type Hold a = Hold (Unit ->{Files} a)

class Leak a =
  leak : a -> Int

instance Leak (Hold Int) =
  let leak _ = 0

class BadDefault t =
  run_def : forall s. t -> File s ->{Files} Int
  let run_def _ f = leak (Hold (fun () -> Files.read_f f))
";
    let (diags, _) = check_all(&[("App.CaptureInDefaultFail", bad_default_src)]);
    assert!(
        diags.iter().any(|d| d.code == codes::C001),
        "capture-free `-` Leak (Hold Int) must fail with C001 inside class default: {diags:?}"
    );
}

#[test]
fn k1_prime_detects_overlap_across_contravariant_sibling_reach() {
    // Both declaration orders for `(Unit -> Unit) -> (Unit ->{Scope s} Unit)` vs
    // `(Unit ->{Scope s} Unit) -> (Unit -> Unit)` under a `-` parameter must
    // report FLICK-C003.
    let order_a = "\
module App.SiblingReachA exposing (..)

type File (s : Region)

class Consume2 (s : Region) a =
  consume2 : File s -> a -> Int

instance Consume2 s ((Unit -> Unit) -> (Unit ->{Scope s} Unit)) =
  let consume2 _ _ = 1

instance Consume2 s ((Unit ->{Scope s} Unit) -> (Unit -> Unit)) =
  let consume2 _ _ = 2
";
    let order_b = "\
module App.SiblingReachB exposing (..)

type File (s : Region)

class Consume2 (s : Region) a =
  consume2 : File s -> a -> Int

instance Consume2 s ((Unit ->{Scope s} Unit) -> (Unit -> Unit)) =
  let consume2 _ _ = 2

instance Consume2 s ((Unit -> Unit) -> (Unit ->{Scope s} Unit)) =
  let consume2 _ _ = 1
";
    for (name, src) in [
        ("App.SiblingReachA", order_a),
        ("App.SiblingReachB", order_b),
    ] {
        let (diags, _) = check_all(&[(name, src)]);
        assert!(
            diags.iter().any(|d| d.code == codes::C003),
            "{name} must detect K1' overlap with FLICK-C003: {diags:?}"
        );
    }
}

#[test]
fn k1_prime_detects_rigid_region_sibling_reach_overlap() {
    // Direct unit check on `heads_overlap_k1_prime` with a rigid (non-head-var)
    // region `Type::Skolem(0)` to exercise the `!has_type_var_in` reach check.
    let src = "\
module App.DirectOverlap exposing (..)

class ShowFn a =
  show_fn : a -> String
";
    let mut sources = SourceMap::default();
    let file = sources.add("App.DirectOverlap.fl", src);
    let parsed = syntax::parse(src, file);
    let mod_syntax = resolve::module_syntax(&parsed.tree, src, file);
    let module = Module {
        name: "App.DirectOverlap",
        file,
        text: src,
        tree: &parsed.tree,
        syntax: &mod_syntax,
        generated: resolve::NO_GENERATED,
    };
    let imported = |_: &str| None;
    let known = Builtins::new(builtins());
    let local = |_: &str| true;
    let options = Options {
        local: &local,
        limits: Limits::default(),
        no_let_assert: false,
        thresholds: Thresholds::default(),
        watched: Watched::none(),
        stages_by_class: false,
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
        plugin_attributes: &[],
    };
    let mut checker = checker_in(
        &module,
        &imported,
        &known,
        options,
        StaticContext::default(),
    );
    checker.module(&parsed.tree);
    let unit = Type::lang(Lang::Unit);
    let pure_fn = Type::pure_fun(unit.clone(), unit.clone());
    let skolem_s = Type::Skolem(0);
    let scope_ty = Type::app(Type::lang(Lang::Scope), skolem_s);
    let scope_row = Type::Row(Row {
        entries: vec![(Elem::from(regions::scope()), scope_ty)],
        tail: None,
        written: Written::default(),
    });
    let scope_fn = Type::Fun {
        param: Box::new(unit.clone()),
        effects: Some(Box::new(Effects {
            services: scope_row,
            errors: Type::empty_row(),
            direct: false,
            sync: false,
            provides: None,
            written: Written::default(),
        })),
        result: Box::new(unit),
    };
    let f1 = Type::pure_fun(pure_fn.clone(), scope_fn.clone());
    let f2 = Type::pure_fun(scope_fn, pure_fn);
    assert!(
        checker
            .heads_overlap_k1_prime(
                ("App.DirectOverlap", "ShowFn"),
                &[],
                slice::from_ref(&f1),
                &[],
                slice::from_ref(&f2),
            )
            .is_some(),
        "rigid-region sibling reach overlap (f1, f2) must be detected"
    );
    assert!(
        checker
            .heads_overlap_k1_prime(
                ("App.DirectOverlap", "ShowFn"),
                &[],
                slice::from_ref(&f2),
                &[],
                slice::from_ref(&f1),
            )
            .is_some(),
        "rigid-region sibling reach overlap (f2, f1) must be detected"
    );
}

#[test]
fn local_class_variance_through_transparent_alias_is_order_independent() {
    let base_before_sub = "\
module App.AliasOrderA exposing (..)

class Base a =
  base : a -> String

type alias BaseAlias a = Base a

class BaseAlias a => Sub a =
  sub : a -> String

instance Base (Unit ->{Async} Unit) =
  let base _ = \"base\"

instance Sub (Unit ->{Async} Unit) =
  let sub _ = \"sub\"

first : String
let first = base (fun () -> ())

ok : String
let ok = sub (fun () -> ())
";
    let sub_before_base = "\
module App.AliasOrderB exposing (..)

class Base a =
  base : a -> String

type alias BaseAlias a = Base a

class BaseAlias a => Sub a =
  sub : a -> String

instance Base (Unit ->{Async} Unit) =
  let base _ = \"base\"

instance Sub (Unit ->{Async} Unit) =
  let sub _ = \"sub\"

ok : String
let ok = sub (fun () -> ())

first : String
let first = base (fun () -> ())
";
    for (name, src) in [
        ("App.AliasOrderA", base_before_sub),
        ("App.AliasOrderB", sub_before_base),
    ] {
        let mut sources = SourceMap::default();
        let file = sources.add(format!("{name}.fl"), src);
        let parsed = syntax::parse(src, file);
        let mod_syntax = resolve::module_syntax(&parsed.tree, src, file);
        let module = Module {
            name,
            file,
            text: src,
            tree: &parsed.tree,
            syntax: &mod_syntax,
            generated: resolve::NO_GENERATED,
        };
        let imported = |_: &str| None;
        let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
        diagnostics.extend(parsed.diagnostics);
        let local = |_: &str| true;
        let options = Options {
            local: &local,
            limits: Limits::default(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
            foreign_backend: ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
            plugin_attributes: &[],
        };
        let (more, _) =
            check_with_evidence(&module, &imported, builtins(), options, &mut interface);
        diagnostics.extend(more);
        diagnostics.retain(|d| !d.unnecessary());
        assert!(
            diagnostics.is_empty(),
            "{name} must infer Sub as `-` regardless of definition order: {diagnostics:?}"
        );
        let sub_var = interface
            .exports
            .iter()
            .find(|e| e.entity.name == "Sub" && e.decl == Decl::Class)
            .and_then(|e| e.definition.as_ref())
            .map(|d| d.variance.as_slice());
        assert_eq!(
            sub_var,
            Some([Variance::Contravariant].as_slice()),
            "{name} exported variance for Sub must be Contravariant"
        );
    }
}
