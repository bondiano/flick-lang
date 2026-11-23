import Formal.MemoryPass.Tokens

/-!
# Token checking and source liveness for the memory ANF (phase M)

This is the ownership projection of `flick-ir/src/memory.rs`'s `check`,
including its path-specific set of already defined locals. An operation
either consumes its operand list, borrows one receiver, constructs from
ordinary fields and evidence, or reuses a receiver while consuming both
field lists. Operand lists retain multiplicity. A borrow does not consume
the receiver; a fresh result binding introduces exactly one local token.

The finite local universe has arbitrary width. Function-valued masks and
counts abstract the Rust ordered collections. The source validator and
backward liveness below mirror the pass, but insertion is not proved.
`checkTokens` checks both successors of a branch.
`path` selects just one, and neither its trace nor its execution duplicates
the state for runtime ownership.

The soundness theorem and its prefix corollary establish local-token
balance for any accepted annotated ANF and any selected branch path. They
do not establish that Perceus produces accepted output, that an operation
implements its claimed ownership contract, or any heap/value semantics.
A transferred return may still own live heap storage; consumed local
tokens may have become ownership edges inside a constructed result.
Pattern validity, constructor layout, code/global identity and arity checks,
machine-counter bounds, source lowering and physical reclamation are not
part of this token projection.
-/

namespace Flick.Memory.Pass

variable {width : Nat} {α : Type}

/-! ## Source bindings and backward liveness

These definitions project `source`, `before` and `analyze` from
`flick-core/src/opt/perceus.rs`. Source validation permits repeated uses,
but rejects ownership annotations, pre-inserted reuse and path-local
rebinding. The analysis removes a result binding before adding operands;
it unions both successor entries and the branch condition. It does not
count occurrences across mutually exclusive branches.
-/

abbrev Mask (width : Nat) := Local width → Bool

def consumed : Operation width → List (Local width)
  | .consume operands => operands
  | .borrow _ => []
  | .construct fields evidence => fields ++ evidence
  | .record fields => fields
  | .dropReuse object => [object]
  | .reuse receiver fields evidence => receiver :: (fields ++ evidence)
  | .reuseRecord receiver fields => receiver :: fields

def borrowed : Operation width → Option (Local width)
  | .borrow receiver => some receiver
  | _ => none

/-- Source binding checks visit all consuming and borrowed operands. -/
def operands (value : Operation width) : List (Local width) :=
  consumed value ++ (borrowed value).toList

/-- The reusable operation helpers agree with the approved token projection. -/
theorem operationEvents_projection (value : Operation width) :
    operationEvents value =
      (borrowed value).toList.map Event.borrow ++ (consumed value).map Event.take := by
  cases value <;> rfl

def sourceOperation : Operation width → Bool
  | .dropReuse .. | .reuse .. | .reuseRecord .. => false
  | _ => true

/-- Freshness is checked against all names defined on the current path,
not just names still live. Separate successors inherit independent masks. -/
def checkSource : Block width → Mask width → Bool
  | .ret result, defined => defined result
  | .seq (.let_ binding value) next, defined =>
      sourceOperation value && ((operands value).all defined &&
        (!defined binding && checkSource next (update defined binding true)))
  | .seq (.dup _) _, _ | .seq (.drop _) _, _ => false
  | .branch condition yes no, defined =>
      defined condition && (checkSource yes defined && checkSource no defined)

/-- This transfer also matches Rust on annotated instructions: `before`
does not change the mask for Dup/Drop, although `checkSource` rejects them. -/
def before : Instruction width → Mask width → Mask width
  | .let_ binding value, after =>
      fun name => (name != binding && after name) || (operands value).contains name
  | .dup _, after | .drop _, after => after

/-- Nested sequential nodes cache the same per-instruction after mask that
Rust stores in a vector. Both branch caches are retained independently. -/
inductive Analysis (width : Nat) where
  | ret (entry : Mask width)
  | seq (entry after : Mask width) (next : Analysis width)
  | branch (entry : Mask width) (yes no : Analysis width)

def Analysis.entry : Analysis width → Mask width
  | .ret entry | .seq entry .. | .branch entry .. => entry

def analyze : Block width → Analysis width
  | .ret result => .ret (fun name => name == result)
  | .seq instruction next =>
      let tail := analyze next
      .seq (before instruction tail.entry) tail.entry tail
  | .branch condition yes no =>
      let yesLive := analyze yes
      let noLive := analyze no
      .branch (fun name => (yesLive.entry name || noLive.entry name) || name == condition)
        yesLive noLive

def live (block : Block width) : Mask width := (analyze block).entry

/-- A stable executable view of the entry set, in numeric local order. -/
def liveLocals (block : Block width) : List (Local width) :=
  (List.finRange width).filter (live block)

/-- Source acceptance proves availability of the *computed* live-in set.
There is no liveness-correctness premise and no global freshness premise
between disjoint successors. This is not an annotation-balance theorem. -/
theorem checkSource_live_defined (block : Block width) :
    ∀ defined, checkSource block defined = true →
      ∀ name, live block name = true → defined name = true := by
  induction block with
  | ret result =>
      intro defined accepted name needed
      change (name == result) = true at needed
      have same : name = result := by simpa using needed
      subst name
      exact accepted
  | seq instruction next ih =>
      intro defined accepted name needed
      cases instruction with
      | dup unused => simp [checkSource] at accepted
      | drop unused => simp [checkSource] at accepted
      | let_ binding value =>
          have parts : sourceOperation value = true ∧
              (operands value).all defined = true ∧
              (!defined binding) = true ∧
              checkSource next (update defined binding true) = true := by
            simpa only [checkSource, Bool.and_eq_true] using accepted
          change ((name != binding && live next name) ||
            (operands value).contains name) = true at needed
          have cases : (name ≠ binding ∧ live next name = true) ∨
              name ∈ operands value := by
            simpa only [Bool.or_eq_true, Bool.and_eq_true, bne_iff_ne,
              List.contains_iff_mem] using needed
          rcases cases with ⟨different, suffixLive⟩ | operand
          · have known := ih (update defined binding true) parts.2.2.2 name suffixLive
            simpa [update, different] using known
          · exact (List.all_eq_true.mp parts.2.1) name operand
  | branch condition yes no ihYes ihNo =>
      intro defined accepted name needed
      have parts : defined condition = true ∧
          checkSource yes defined = true ∧ checkSource no defined = true := by
        simpa only [checkSource, Bool.and_eq_true] using accepted
      change ((live yes name || live no name) || name == condition) = true at needed
      have cases : (live yes name = true ∨ live no name = true) ∨ name = condition := by
        simpa only [Bool.or_eq_true, beq_iff_eq] using needed
      rcases cases with (yesLive | noLive) | same
      · exact ihYes defined parts.2.1 name yesLive
      · exact ihNo defined parts.2.2 name noLive
      · subst name
        exact parts.1

-- Empty incoming masks and unused result/parameter names are permitted.
example :
    let block : Block 4 := .seq (.let_ 0 (.consume [])) (.ret 0)
    checkSource block (fun _ => false) = true ∧ liveLocals block = [] := by decide
example :
    let block : Block 4 := .seq (.let_ 1 (.consume [])) (.ret 0)
    checkSource block (fun name => [0, 2].contains name) = true ∧
      liveLocals block = [0] := by decide

-- Source uses may repeat; evidence is just as live as ordinary fields.
example :
    let block : Block 4 := .seq (.let_ 1 (.construct [0, 0] [2, 0])) (.ret 1)
    checkSource block (fun name => [0, 2].contains name) = true ∧
      liveLocals block = [0, 2] := by decide

-- A borrowed receiver is live even when its newly bound result is dead.
example :
    let block : Block 4 := .seq (.let_ 1 (.borrow 0)) (.ret 1)
    checkSource block (fun name => name == 0) = true ∧ liveLocals block = [0] := by decide
example :
    let block : Block 4 := .seq (.let_ 1 (.borrow 0))
      (.seq (.let_ 2 (.consume [])) (.ret 2))
    checkSource block (fun name => name == 0) = true ∧ liveLocals block = [0] := by decide

-- A sequential definition is removed from live-in after its uses are found.
example :
    let block : Block 4 := .seq (.let_ 1 (.consume [0]))
      (.seq (.let_ 2 (.construct [1] [])) (.ret 2))
    checkSource block (fun name => name == 0) = true ∧ liveLocals block = [0] := by decide

-- Branch-local freshness is independent, but use from a sibling is invalid.
example :
    let block : Block 4 := .branch 0
      (.seq (.let_ 1 (.consume [0])) (.ret 1))
      (.seq (.let_ 1 (.consume [])) (.ret 1))
    checkSource block (fun name => name == 0) = true ∧ liveLocals block = [0] := by decide
example :
    let block : Block 4 := .branch 0
      (.seq (.let_ 1 (.consume [])) (.ret 1)) (.ret 1)
    checkSource block (fun name => name == 0) = false ∧ liveLocals block = [0, 1] := by decide

-- The condition and both successor entries contribute to the live-in set;
-- repeated uses in different successors do not become an occurrence count.
example :
    let block : Block 4 := .branch 0 (.ret 1) (.ret 2)
    checkSource block (fun name => [0, 1, 2].contains name) = true ∧
      liveLocals block = [0, 1, 2] := by decide
example :
    let block : Block 4 := .branch 0 (.ret 0) (.ret 0)
    checkSource block (fun name => name == 0) = true ∧ liveLocals block = [0] := by decide

-- Existing names cannot be shadowed, even if their old value is unused.
example : checkSource (width := 4)
    (.seq (.let_ 0 (.consume [])) (.ret 0)) (fun name => name == 0) = false := by decide
example : checkSource (width := 4)
    (.seq (.let_ 1 (.consume [])) (.seq (.let_ 1 (.consume [])) (.ret 1)))
    (fun name => name == 0) = false := by decide

-- Every operand, return and branch condition must already be defined.
example : checkSource (width := 4)
    (.seq (.let_ 1 (.consume [2])) (.ret 1)) (fun name => name == 0) = false := by decide
example : checkSource (width := 4)
    (.seq (.let_ 1 (.borrow 2)) (.ret 1)) (fun name => name == 0) = false := by decide
example : checkSource (width := 4)
    (.seq (.let_ 1 (.construct [0] [2])) (.ret 1))
    (fun name => name == 0) = false := by decide
example : checkSource (width := 4) (.ret 1) (fun name => name == 0) = false := by decide
example : checkSource (width := 4) (.branch 1 (.ret 0) (.ret 0))
    (fun name => name == 0) = false := by decide

-- Preannotated ownership operations and reuse are not source ANF.
example : checkSource (width := 4) (.seq (.dup 0) (.ret 0))
    (fun name => name == 0) = false := by decide
example : checkSource (width := 4) (.seq (.drop 0) (.ret 0))
    (fun name => name == 0) = false := by decide
example : checkSource (width := 4) (.seq (.let_ 1 (.reuse 0 [] [])) (.ret 1))
    (fun name => name == 0) = false := by decide

end Flick.Memory.Pass

/-- info: 'Flick.Memory.Pass.checkTokens_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkTokens_sound

/-- info: 'Flick.Memory.Pass.checkTokens_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkTokens_prefix_safe

/-- info: 'Flick.Memory.Pass.checkTokens_event_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkTokens_event_safe

/-- info: 'Flick.Memory.Pass.checkFunction_sound' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkFunction_sound

/-- info: 'Flick.Memory.Pass.checkFunction_prefix_safe' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkFunction_prefix_safe

/-- info: 'Flick.Memory.Pass.checkSource_live_defined' depends on axioms: [propext, Quot.sound] -/
#guard_msgs in
#print axioms Flick.Memory.Pass.checkSource_live_defined
