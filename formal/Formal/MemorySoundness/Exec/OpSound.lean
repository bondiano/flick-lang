import Formal.MemorySoundness.Exec.OpSoundProj
import Formal.MemorySoundness.Exec.OpSoundBox
import Formal.MemorySoundness.Exec.OpSoundUnbox
import Formal.MemorySoundness.Exec.OpSoundReuse

/-!
# Single-operation operational soundness dispatcher (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

theorem evalMemOp_sound {dropFuel nextId bound : Nat}
    {h : AggHeap} {rank otherRoots : ObjId → Nat}
    {env : Local width → Option Aggregate.Value}
    {state afterPass : Pass.State width}
    {senv : Local width → Option SemVal}
    {ext : List Aggregate.Value} {sext : List SemVal}
    (b : Local width) (op : MemOp width)
    (nextId' : Nat) (h' : AggHeap) (res : Aggregate.Value) (tok' : Local width → Nat)
    (hrunOp : runEvents (operationEvents (eraseOp (toRepOp op))) state = some afterPass)
    (hfreshB : afterPass.defined b = false)
    (heval : evalMemOp dropFuel nextId h env state.tokens op = some (nextId', h', res, tok'))
    (hb : HeapBalanced nextId h rank bound (fun id => frameRefs env state.tokens id + otherRoots id))
    (henv : EnvTokensOk env state)
    (hden : EnvDenotes h env state.tokens senv)
    (hext : DenotesList h ext sext)
    (hextRoots : ∀ id, valsTokens ext id ≤ otherRoots id)
    (hfuel : bound ≤ dropFuel) :
    tok' = afterPass.tokens ∧
    ∃ rank' sv,
      evalSourceOp op senv = some sv ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun id => frameRefs (update env b (some res)) (update afterPass.tokens b 1) id + otherRoots id) ∧
      EnvTokensOk (update env b (some res))
        ⟨update afterPass.tokens b 1, update afterPass.defined b true⟩ ∧
      EnvDenotes h' (update env b (some res)) (update afterPass.tokens b 1) (update senv b (some sv)) ∧
      DenotesList h' ext sext := by
  cases op with
  | lit =>
      exact evalMemOp_sound_lit b nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | project recv rep idx =>
      exact evalMemOp_sound_project b recv rep idx nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | projectPath recv path =>
      exact evalMemOp_sound_projectPath b recv path nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | construct rep tag layout fields evidence =>
      exact evalMemOp_sound_construct b rep tag layout fields evidence nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | boxAggregate val layout =>
      exact evalMemOp_sound_boxAggregate b val layout nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | unboxBorrow obj =>
      exact evalMemOp_sound_unboxBorrow b obj nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | unboxConsume obj =>
      exact evalMemOp_sound_unboxConsume b obj nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots
  | dropReuse obj =>
      exact evalMemOp_sound_dropReuse b obj nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots hfuel
  | reuse recv rep tag layout fields evidence =>
      exact evalMemOp_sound_reuse b recv rep tag layout fields evidence nextId' h' res tok' hrunOp hfreshB heval hb henv hden hext hextRoots hfuel

end Flick.Memory.Soundness
