//! Layout of declarations and types.

use super::fmt;

/// Variants share a line when they fit; `deriving` takes its own unless a
/// broken record ends before it.
#[test]
fn lays_out_variants_and_deriving() {
    let src = "type Option a = None | Some a\ntype Shape = Circle Point Float | Rect { origin : Point; size : Point } | Empty | Polygon (List Point) deriving Eq, Show\ntype alias Wrapped (f : Type -> Type) a = {\n  value : f a;\n} deriving Schema\ntype alias Point = { x : Float; y : Float } deriving Eq\nrefined type Port = Float where fun n -> n > 0 && n < 65536\n  deriving Eq, Hash\nkind Nullability = NonNull | Nullable\n";
    insta::assert_snapshot!(fmt(src), @r#"
    type Option a = None | Some a
    type Shape =
      | Circle Point Float
      | Rect { origin : Point; size : Point }
      | Empty
      | Polygon (List Point)
      deriving Eq, Show
    type alias Wrapped (f : Type -> Type) a = {
      value : f a;
    } deriving Schema
    type alias Point = { x : Float; y : Float }
      deriving Eq
    refined type Port = Float where fun n -> n > 0 && n < 65536
      deriving Eq, Hash
    kind Nullability = NonNull | Nullable
    "#);
}

/// The constructors of a GADT take a line each, however short.
#[test]
fn lays_out_a_gadt_one_constructor_per_line() {
    let src = "type Expr a = IntLit Int : Expr Int | Apply (Expr (b -> a)) (Expr b)  :  Expr a | Lit a\ntype S = Show b=>S b:S | ( Eq a,Show a ) => P a : S\n";
    insta::assert_snapshot!(fmt(src), @r#"
    type Expr a =
      | IntLit Int : Expr Int
      | Apply (Expr (b -> a)) (Expr b) : Expr a
      | Lit a
    type S =
      | Show b => S b : S
      | (Eq a, Show a) => P a : S
    "#);
}

/// A signature that does not fit puts its type on the next lines, a parameter per line.
#[test]
fn breaks_signatures_after_their_colon() {
    let src = "load_everything : UserId -> SessionToken -> RequestContext ->{Users, Sessions, Log; NotFound, Expired} Profile\nwith_path : forall l p q h b r e s p2. PureCodec Json p2 -> Endpoint l (Request p q h b ->{r; e} s) -> Endpoint l (Request p2 q h b ->{r; e} s)\nshort : forall a. Eq a => a -> a -> Bool\n";
    insta::assert_snapshot!(fmt(src), @r#"
    load_everything :
      UserId -> SessionToken -> RequestContext ->{Users, Sessions, Log; NotFound, Expired} Profile
    with_path :
      forall l p q h b r e s p2.
      PureCodec Json p2 ->
      Endpoint l (Request p q h b ->{r; e} s) ->
      Endpoint l (Request p2 q h b ->{r; e} s)
    short : forall a. Eq a => a -> a -> Bool
    "#);
}

/// Empty service rows of an arrow are dropped; elsewhere `none` stays.
#[test]
fn drops_empty_arrow_rows() {
    let src = "f : A ->{none; E} B\ng : A ->{none /* why */; E} B\nh : A ->{Log; none} B\ni : Eff none none A\nj : A ->{none; none} B\nk : A ->{; none} B\n";
    insta::assert_snapshot!(fmt(src), @r#"
    f : A ->{; E} B
    g : A ->{/* why */ ; E} B
    h : A ->{Log; none} B
    i : Eff none none A
    j : A ->{} B
    k : A ->{} B
    "#);
}

/// Members, foreign metadata and `rec` groups go one per line.
#[test]
fn puts_members_and_metadata_on_their_own_lines() {
    let src = "module App.Shapes exposing (Shape(..), area, Nullability, aaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbb, cccccccccccc)\nimport App.Domain exposing (User, Role(..))\ninstance Show a => Show (List a) = show : List a -> String\nclass Show a =\n  show : a -> String\n\n  debug : a -> String\nread_text : Path ->{; FsError} String\nforeign async read_text from \"./fs.js\" \"readText\"\n    arity 1\n    catch FsError.from_js\nrec {\n  is_even : Float -> Bool\n  let is_even n = if n == 0 then true else is_odd (n - 1)\n}\nsql table Users \"users\" {\n  id : bigint primary key generated always as UserId via user_id_codec\n}\n";
    insta::assert_snapshot!(fmt(src), @r#"
    module App.Shapes exposing (
      Shape(..),
      area,
      Nullability,
      aaaaaaaaaaaaaaaaaa,
      bbbbbbbbbbbbbbbbbb,
      cccccccccccc,
    )
    import App.Domain exposing (User, Role(..))
    instance Show a => Show (List a) =
      show : List a -> String
    class Show a =
      show : a -> String

      debug : a -> String
    read_text : Path ->{; FsError} String
    foreign async read_text from "./fs.js" "readText"
      arity 1
      catch FsError.from_js
    rec {
      is_even : Float -> Bool
      let is_even n = if n == 0 then true else is_odd (n - 1)
    }
    sql table Users "users" {
      id : bigint primary key generated always as UserId via user_id_codec
    }
    "#);
}

/// An attribute keeps its own line and stays tight after `@`.
#[test]
fn keeps_attributes() {
    let src = "@ lang  int\ntype Int\nclass Monad m =\n  @lang pure\n  pure : a -> m a\ntype Option a = @lang none None | @lang some Some a\n";
    insta::assert_snapshot!(fmt(src), @r"
    @lang int
    type Int
    class Monad m =
      @lang pure
      pure : a -> m a
    type Option a = @lang none None | @lang some Some a
    ");
}

/// The arguments of a derive are its plugin's words, kept as written; the
/// rest of the declaration is laid out.
#[test]
fn keeps_the_arguments_of_a_derive() {
    let src = "untagged type Level = Low = \"low\" | High = \"high\" deriving   Eq,Schema {wire = \"tag\"}\n";
    insta::assert_snapshot!(fmt(src), @r#"
    untagged type Level = Low = "low" | High = "high"
      deriving Eq, Schema {wire = "tag"}
    "#);
}

/// An effect keeps the class of each operation before its name.
#[test]
fn keeps_the_classes_of_an_effects_operations() {
    let src = "effect Emit a =\n  control   emit : a ->{} Unit\n  abort stop :   forall b. Unit ->{} b\n  tail : Unit ->{} a\n";
    insta::assert_snapshot!(fmt(src), @r"
    effect Emit a =
      control emit : a ->{} Unit
      abort stop : forall b. Unit ->{} b
      tail : Unit ->{} a
    ");
}

/// A handler keeps `return` before the pattern of its arm.
#[test]
fn keeps_the_return_arm_of_a_handler() {
    let src = "let collect body =\n  try body () with\n  | Emit.emit x ->   [x] ++ resume ()\n  | return   () -> []\n";
    insta::assert_snapshot!(fmt(src), @r"
    let collect body =
      try body () with
      | Emit.emit x -> [x] ++ resume ()
      | return () -> []
    ");
}
