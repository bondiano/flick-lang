import Formal.Modules

/-!
# Static higher-order module boundary

This executable fragment isolates grouped static application, lexical module
closures, public shape/nominal matching, and dictionary scope. Operation and
layer bodies are opaque code references: this evaluator has no runtime
operation rule. Source type schemes and dependent type equalities are checked
before this fragment; no theorem here relates it to the Rust compiler.
-/

namespace Flick.Modules.HigherOrder

theorem identity_matches_refl (identity : Identity) :
    Identity.matches identity identity := by
  cases identity <;> simp [Identity.matches]

theorem identity_matches_trans {required middle actual : Identity}
    (first : Identity.matches required middle)
    (second : Identity.matches middle actual) :
    Identity.matches required actual := by
  cases required with
  | abstract => trivial
  | manifest path =>
      have same := manifest_requires_same_path first
      simpa [same] using second

abbrev Nominals := List (String × Identity)

def NominalsLe (actual required : Nominals) : Prop :=
  ∀ entry ∈ required, ∃ identity,
    (entry.1, identity) ∈ actual ∧ Identity.matches entry.2 identity

theorem nominals_refl (identities : Nominals) : NominalsLe identities identities := by
  intro entry member
  exact ⟨entry.2, member, identity_matches_refl entry.2⟩

theorem nominals_trans {a b c : Nominals}
    (first : NominalsLe a b) (second : NominalsLe b c) : NominalsLe a c := by
  intro entry member
  obtain ⟨middle, middleMember, middleMatches⟩ := second entry member
  obtain ⟨actual, actualMember, actualMatches⟩ := first (entry.1, middle) middleMember
  exact ⟨actual, actualMember, identity_matches_trans middleMatches actualMatches⟩

inductive Signature where
  | shape (members : List String) (identities : Nominals)
  | arrow (groupSize : Nat) (parameter result : Signature)

/-- Each arrow retains its atomic-group size (n, n-1, …, 1, as in an iface).
The expected input is a subtype of the provider's input; results covary. -/
inductive Subtype : Signature → Signature → Prop where
  | shape {actual required : List String} {a r : Nominals} :
      (∀ member ∈ required, member ∈ actual) → NominalsLe a r →
      Subtype (.shape actual a) (.shape required r)
  | arrow {group : Nat} {ap ar bp br : Signature} :
      Subtype bp ap → Subtype ar br →
      Subtype (.arrow group ap ar) (.arrow group bp br)

theorem subtype_refl (signature : Signature) : Subtype signature signature := by
  induction signature with
  | shape members identities => exact .shape (fun _ member => member) (nominals_refl identities)
  | arrow group parameter result ihp ihr => exact .arrow ihp ihr

theorem subtype_trans {first middle last : Signature}
    (left : Subtype first middle) (right : Subtype middle last) :
    Subtype first last := by
  induction middle generalizing first last with
  | shape members identities =>
      cases left with
      | shape leftMembers leftIds =>
          cases right with
          | shape rightMembers rightIds =>
              exact .shape (fun name member => leftMembers name (rightMembers name member))
                (nominals_trans leftIds rightIds)
  | arrow group parameter result ihp ihr =>
      cases left with
      | arrow leftParameter leftResult =>
          cases right with
          | arrow rightParameter rightResult =>
              exact .arrow (ihp rightParameter leftParameter) (ihr leftResult rightResult)

theorem matching_arrows_preserve_groups {a b : Nat} {ap ar bp br : Signature}
    (matched : Subtype (.arrow a ap ar) (.arrow b bp br)) : a = b := by
  cases matched
  rfl

/-- Payloads identify deferred runtime code, including operations and layers. -/
abbrev Code := Nat

inductive Expr where
  | structure (exports : List (String × Code))
  | variable (index : Nat)
  | module (arity : Nat) (body : Expr)
  | apply (function : Expr) (arguments : List Expr)

inductive Value where
  | structure (exports : List (String × Code))
  | closure (arity : Nat) (captured : List Value) (body : Expr)

/-- Fuel bounds malformed/untyped static expressions too. Applying a closure
uses its captured environment; arguments are evaluated in the caller's. -/
def evaluate : Nat → List Value → Expr → Option Value
  | 0, _, _ => none
  | fuel + 1, environment, expression =>
      match expression with
      | .structure exports => some (.structure exports)
      | .variable index => environment[index]?
      | .parameterized arity body => some (.closure arity environment body)
      | .apply function arguments => do
          let value ← evaluate fuel environment function
          match value with
          | .structure _ => none
          | .closure arity captured body =>
              if arguments.length = arity then do
                let arguments ← arguments.mapM (evaluate fuel environment)
                evaluate fuel (arguments.reverse ++ captured) body
              else none

theorem parameterized_captures_lexically (fuel arity : Nat) (body : Expr) (environment : List Value) :
    evaluate (fuel + 1) environment (.parameterized arity body) =
      some (.closure arity environment body) := rfl

/-- The caller's equally placed module cannot replace a captured module. -/
theorem application_uses_captured_module (captured argument : Value) (caller : List Value) :
    evaluate 3 (.closure 1 [captured] (.variable 1) :: argument :: caller)
      (.apply (.variable 0) [.variable 1]) = some captured := by
  rfl

theorem returned_parameterized_retains_its_argument (code : List (String × Code)) :
    evaluate 3 [] (.apply (.parameterized 1 (.parameterized 1 (.variable 1))) [.structure code]) =
      some (.closure 1 [.structure code] (.variable 1)) := by
  rfl

theorem atomic_group_rejects_partial_application (body : Expr) (argument : Expr) :
    evaluate 3 [] (.apply (.parameterized 2 body) [argument]) = none := by
  rfl

def project (visible : List String) (exports : List (String × Code)) : List (String × Code) :=
  exports.filter (fun entry => visible.contains entry.1)

theorem projection_hides_unadvertised {visible : List String}
    {exports : List (String × Code)} {name : String} {code : Code}
    (member : (name, code) ∈ project visible exports) : name ∈ visible := by
  have selected := (List.mem_filter.mp member).2
  exact List.contains_iff_mem.mp selected

abbrev Scope := String → Option Code

def localDictionary (scopes : List Scope) (key : String) : Option Code :=
  match scopes with
  | [] => none
  | scope :: rest => match scope key with
    | some dictionary => some dictionary
    | none => localDictionary rest key

def dictionary (given : Scope) (locals : List Scope) (global : Scope) (key : String) : Option Code :=
  match given key with
  | some explicit => some explicit
  | none => match localDictionary locals key with
    | some selected => some selected
    | none => global key

theorem explicit_dictionary_wins (given global : Scope) (locals : List Scope)
    (key : String) (code : Code) (explicit : given key = some code) :
    dictionary given locals global key = some code := by
  simp [dictionary, explicit]

theorem inner_dictionary_wins (given inner global : Scope) (outer : List Scope)
    (key : String) (code : Code) (noGiven : given key = none) (hasLocal : inner key = some code) :
    dictionary given (inner :: outer) global key = some code := by
  simp [dictionary, localDictionary, noGiven, hasLocal]

theorem dictionary_falls_back_to_global (given global : Scope) (locals : List Scope)
    (key : String) (noGiven : given key = none) (noLocal : localDictionary locals key = none) :
    dictionary given locals global key = global key := by
  simp [dictionary, noGiven, noLocal]

end Flick.Modules.HigherOrder
