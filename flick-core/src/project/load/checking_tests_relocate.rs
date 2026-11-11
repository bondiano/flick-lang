/// An inline module with what its parent's file must hold under a prefix:
/// constructors in expressions and patterns, classes with their instances,
/// one with a context and one under a superclass, derived instances over a
/// type of the module, an effect with a handler, a service and an error;
/// a kind, a type indexed by it and an alias for its declarations.
#[cfg(feature = "ide")]
const RELOCATED: &str = "module App.Parent exposing (answer, label, silence)
module Child exposing (Shape(..), Pair(..), Named(..), Titled(..), Ask, Log, Missing(..), Shade(..), Tinted(..), Shapes, area, probe, twice, unit) =
  type Shape = Circle Float | Square Float
    deriving Eq, Show
  type Pair = Pair Shape Shape
    deriving Eq, Show
  type error Missing = Missing String
  kind Shade = Light | Dark
  type Tinted (s : Shade) = Tinted Shape
  type alias Shapes = List Shape
  class Named a =
    name_of : a -> String
  instance Named Shape =
    let name_of shape =
      match shape with
      | Circle _ -> \"circle\"
      | Square _ -> \"square\"
  instance Named a => Named (List a) =
    let name_of items = String.join \", \" (List.map name_of items)
  class Named a => Titled a =
    title : a -> String
  instance Titled Shape =
    let title shape = name_of shape ++ \"!\"
  effect Ask =
    ask : Unit ->{} Float
  service Log =
    info : String ->{} Unit
  area : Shape -> Float
  let area shape =
    match shape with
    | Circle r -> r * r
    | Square s -> s * s
  twice : Named a => a -> String
  let twice value = name_of value ++ name_of value
  unit : Shape
  let unit = Circle 1
  probe : Unit ->{Ask; Missing} Float
  let probe () =
    let asked = Ask.ask ()
    if asked > 9 then from_result (Err (Missing \"too much\")) else asked + area unit

label : Child.Named a => a -> String
let label value = Child.name_of value ++ Child.twice value

quiet : Child.Log.Impl
let quiet = { info = fun _ -> () }

silence : Layer none none {Child.Log}
let silence = Layer.succeed Child.Log quiet

answer : Unit ->{} String
let answer () =
  let shape = Child.Square 2
  let total =
    try
      provide Child.Log quiet { Child.Log.info \"asked\" }
      Child.probe () + Child.area shape
    with
    | Child.Ask.ask () -> 1
    | Child.Missing _ -> 0
  let known = match shape with | Child.Circle _ -> false | Child.Square _ -> true
  let pair = Child.Pair shape Child.unit
  let Child.Pair first _ = pair
  let second = (fun (Child.Pair _ other) -> other) pair
  label [first, second] ++ Child.title shape ++ show pair ++ show (pair == Child.Pair Child.unit shape) ++ show total ++ show known
";

/// `relocate_ir` holds its walk to the JSON form of the same move, under
/// `cfg(test)`: compiling this module is what compares the two.
#[cfg(feature = "ide")]
#[test]
fn an_inline_module_moves_into_its_parent_as_the_json_form_of_the_ir_moved() {
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Parent.fl"),
        RELOCATED.into(),
    )]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    assert_eq!(analysis.compilation.modules.len(), 1);
}
