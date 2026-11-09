/-!
# Named module results and existential witnesses

A small model of the source module boundary, before elaboration into core
records. This proves the identity rules used by named functor applications and
the elimination shape of a packed abstract type. It does not relate the Rust
checker or the ABI to this model.
-/

namespace Flick.Modules

/-- The declaration site of a named functor application. Reimporting the same
site uses the same path; another name under the same parent is fresh. -/
structure ResultPath where
  parent : String
  name : String
  deriving DecidableEq, Repr

/-- A nominal type declared by a result module. -/
structure TypePath where
  module : ResultPath
  member : String
  deriving DecidableEq, Repr

/-- Two imports of one named result have the same type identity. -/
theorem reimport_preserves_type (site : ResultPath) (member : String) :
    TypePath.mk site member = TypePath.mk site member := rfl

/-- Different application sites cannot give the same result type, even when
both declarations call it `Set`. -/
theorem different_results_have_distinct_types {first second : ResultPath}
    (different : first ≠ second) (member : String) :
    TypePath.mk first member ≠ TypePath.mk second member := by
  intro same
  exact different (congrArg TypePath.module same)

/-- Two named applications under one parent remain distinct even if they use
the same functor and argument; their declaration names are their identities. -/
theorem different_names_have_distinct_types (parent : String)
    {first second : String} (different : first ≠ second) (member : String) :
    TypePath.mk ⟨parent, first⟩ member ≠
      TypePath.mk ⟨parent, second⟩ member := by
  apply different_results_have_distinct_types
  intro same
  exact different (congrArg ResultPath.name same)

/-- A type declared by an inline namespace still belongs to its lexical
child path, even though the backend emits one runtime file for the parent. -/
structure InlineTypePath where
  file : String
  child : Option String
  member : String
  deriving DecidableEq, Repr

/-- Flattening runtime symbols does not equate a child's nominal type with
an equally named type of its parent. -/
theorem inline_child_is_distinct_from_parent (file child member : String) :
    InlineTypePath.mk file (some child) member ≠
      InlineTypePath.mk file none member := by
  intro same
  cases same

/-- Sibling inline namespaces keep separate nominal identities. -/
theorem inline_siblings_are_distinct (file member : String)
    {first second : String} (different : first ≠ second) :
    InlineTypePath.mk file (some first) member ≠
      InlineTypePath.mk file (some second) member := by
  intro same
  exact different (Option.some.inj (congrArg InlineTypePath.child same))

/-- An abstract class or service identity may be supplied by a static functor
argument. A manifest requirement names the declaration itself. -/
inductive Identity where
  | abstract
  | manifest (path : String)
  deriving DecidableEq, Repr

/-- Structural matching of an identity requirement. -/
def Identity.matches : Identity → Identity → Prop
  | .abstract, _ => True
  | .manifest required, .manifest actual => required = actual
  | .manifest _, .abstract => False

/-- Static matching accepts an abstract identity supplied by its argument. -/
theorem abstract_accepts (actual : Identity) :
    Identity.matches .abstract actual := by
  cases actual <;> trivial

/-- A manifest requirement admits exactly its declared identity. An unrelated
class or service with the same operations is not a match. -/
theorem manifest_requires_same_path {required : String} {actual : Identity}
    (hmatch : Identity.matches (.manifest required) actual) :
    actual = .manifest required := by
  cases actual with
  | abstract => cases hmatch
  | manifest path =>
      change required = path at hmatch
      subst path
      rfl

/-- A first-class `Module Sig` can carry only identities fixed in `Sig`. -/
def Packable (requirements : List Identity) : Prop :=
  ∀ identity ∈ requirements, ∃ path, identity = .manifest path

/-- A signature with an abstract class or service cannot be packed. -/
theorem abstract_identity_cannot_be_packed :
    ¬ Packable [.abstract] := by
  intro packable
  obtain ⟨path, impossible⟩ := packable .abstract (by simp)
  cases impossible

/-- Church encoding of `exists K : Sig. K.Key`, with `F α` standing for the
manifest members, dictionary fields and payload at hidden type `α`. A client
must work for every witness type when it opens the package. -/
structure Packed (F : Type → Type) where
  openWitness : {R : Type} → (∀ α : Type, F α → R) → R

/-- Introduce a hidden witness and its value. -/
def pack {F : Type → Type} {α : Type} (value : F α) : Packed F :=
  ⟨fun use => use α value⟩

/-- Open a package within a continuation polymorphic in its witness. -/
def unpack {F : Type → Type} {R : Type} (packed : Packed F)
    (use : ∀ α : Type, F α → R) : R :=
  packed.openWitness use

/-- Introduction followed by elimination passes the original witness and
value to the scoped continuation. -/
theorem unpack_pack {F : Type → Type} {α R : Type}
    (value : F α) (use : ∀ β : Type, F β → R) :
    unpack (pack value) use = use α value := rfl

/-- Two openings give the client separately quantified witnesses; a client
cannot pass an `F α` to a function needing `F β` without additional evidence. -/
def unpackTwo {F : Type → Type} {R : Type} (first second : Packed F)
    (use : ∀ α β : Type, F α → F β → R) : R :=
  unpack first (fun α a => unpack second (fun β b => use α β a b))

theorem unpackTwo_pack {F : Type → Type} {α β R : Type}
    (a : F α) (b : F β) (use : ∀ x y : Type, F x → F y → R) :
    unpackTwo (pack a) (pack b) use = use α β a b := rfl

end Flick.Modules
