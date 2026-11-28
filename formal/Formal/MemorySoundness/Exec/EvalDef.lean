import Formal.MemorySoundness.Exec.Frame

/-!
# Counted operational execution definitions and helpers (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate
open Flick.Memory.Reuse

variable {width : Nat}

local instance : ReflBEq (Local width) where
  rfl := beq_iff_eq.mpr rfl

/-! ## Counted operational execution (`evalMemPrefix`) -/

inductive Outcome (width : Nat) where
  | returned (nextId : Nat) (h : AggHeap) (res : Aggregate.Value)
  | aborted (exit : MemoryScopes.ExitKind) (nextId : Nat) (h : AggHeap)
      (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)

/-- Consume a list of operand locals from `tok`, returning their runtime values
and the decremented token map. -/
def takeOperands (env : Local width → Option Aggregate.Value) :
    List (Local width) → (Local width → Nat) → Option (List Aggregate.Value × (Local width → Nat))
  | [], tok => some ([], tok)
  | x :: rest, tok =>
      if tok x == 0 then none
      else
        match env x with
        | none => none
        | some v =>
            match takeOperands env rest (update tok x (tok x - 1)) with
            | none => none
            | some (vs, tok') => some (v :: vs, tok')

/-- Counted operational semantics of a single `MemOp`. -/
def evalMemOp (dropFuel nextId : Nat) (h : AggHeap)
    (env : Local width → Option Aggregate.Value) (tok : Local width → Nat)
    (op : MemOp width) : Option (Nat × AggHeap × Aggregate.Value × (Local width → Nat)) :=
  match op with
  | .lit => some (nextId, h, .imm, tok)
  | .project recv rep idx =>
      if tok recv == 0 then none
      else
        match env recv with
        | none => none
        | some v =>
            match rep, v with
            | .aggregate, .agg _ fields =>
                match fields[idx]? with
                | none => none
                | some field => some (nextId, dupValue h field, field, tok)
            | .cell, .boxed id =>
                match h id with
                | some ⟨rc, .tagged _ _ fields⟩ =>
                    if 0 < rc then
                      match fields[idx]? with
                      | none => none
                      | some field => some (nextId, dupValue h field, field, tok)
                    else none
                | _ => none
            | _, _ => none
  | .projectPath recv path =>
      if tok recv == 0 then none
      else
        match env recv with
        | none => none
        | some v =>
            match projectPath h v path with
            | none => none
            | some (h', field) => some (nextId, h', field, tok)
  | .construct rep tag layout fields evidence =>
      match takeOperands env (fields ++ evidence) tok with
      | none => none
      | some (vals, tok') =>
          match rep with
          | .aggregate => some (nextId, h, .agg tag vals, tok')
          | .cell =>
              some (nextId + 1, setAggObj h nextId (some ⟨1, .tagged tag layout vals⟩),
                .boxed nextId, tok')
  | .boxAggregate val layout =>
      match takeOperands env [val] tok with
      | some ([v], tok') =>
          let (h', id) := boxAggregateHeap h nextId layout v
          some (nextId + 1, h', .boxed id, tok')
      | _ => none
  | .unboxBorrow obj =>
      if tok obj == 0 then none
      else
        match env obj with
        | some (.boxed id) =>
            match unboxBorrowHeap h id with
            | none => none
            | some (h', inner) => some (nextId, h', inner, tok)
        | _ => none
  | .unboxConsume obj =>
      match takeOperands env [obj] tok with
      | some ([.boxed id], tok') =>
          match unboxConsumeHeap h id with
          | none => none
          | some (h', inner) => some (nextId, h', inner, tok')
      | _ => none
  | .dropReuse obj =>
      match takeOperands env [obj] tok with
      | some ([v], tok') =>
          match dropReuseRec dropFuel h v with
          | none => none
          | some (h', res) => some (nextId, h', res, tok')
      | _ => none
  | .reuse recv rep tag layout fields evidence =>
      match rep with
      | .aggregate => none
      | .cell =>
          match takeOperands env (recv :: (fields ++ evidence)) tok with
          | some (cand :: vals, tok') =>
              match reuseTypedCellRec dropFuel nextId h cand tag layout vals with
              | none => none
              | some (nextId', h', res, _) => some (nextId', h', res, tok')
          | _ => none

/-- Counted operational execution of a `MemBlock` up to `steps` steps:
when `steps = 0`, aborts cleanly with `exit` and the exact live frame `(env, tok)`
(modeling failure, cancellation, or fuel cut at any execution prefix). -/
def evalMemPrefix (dropFuel : Nat) (exit : MemoryScopes.ExitKind) :
    Nat → MemBlock width → List Bool → Nat → AggHeap →
    (Local width → Option Aggregate.Value) → (Local width → Nat) → Option (Outcome width)
  | 0, _, _, nextId, h, env, tok =>
      some (.aborted exit nextId h env tok)
  | _ + 1, .ret r, _, nextId, h, env, tok =>
      if tok r == 0 then none
      else
        match env r with
        | none => none
        | some v => some (.returned nextId h v)
  | steps + 1, .seq (.dup x) next, choices, nextId, h, env, tok =>
      if tok x == 0 then none
      else
        match env x with
        | none => none
        | some v =>
            evalMemPrefix dropFuel exit steps next choices nextId (dupValue h v) env
              (update tok x (tok x + 1))
  | steps + 1, .seq (.drop x) next, choices, nextId, h, env, tok =>
      if tok x == 0 then none
      else
        match env x with
        | none => none
        | some v =>
            evalMemPrefix dropFuel exit steps next choices nextId (dropValFuel dropFuel h v) env
              (update tok x (tok x - 1))
  | steps + 1, .seq (.let_ b op) next, choices, nextId, h, env, tok =>
      match evalMemOp dropFuel nextId h env tok op with
      | none => none
      | some (nextId', h', v, tok') =>
          evalMemPrefix dropFuel exit steps next choices nextId' h'
            (update env b (some v)) (update tok' b 1)
  | steps + 1, .branch c yes no, choices, nextId, h, env, tok =>
      if tok c == 0 then none
      else
        match env c with
        | none => none
        | some v =>
            let h' := dropValFuel dropFuel h v
            let tok' := update tok c (tok c - 1)
            match choices with
            | true :: rest => evalMemPrefix dropFuel exit steps yes rest nextId h' env tok'
            | false :: rest => evalMemPrefix dropFuel exit steps no rest nextId h' env tok'
            | [] => evalMemPrefix dropFuel exit steps no [] nextId h' env tok'

/-! ## Helper lemmas for operand consumption and denotation preservation -/

theorem takeOperands_spec (xs : List (Local width))
    (env : Local width → Option Aggregate.Value) (h : AggHeap)
    (senv : Local width → Option SemVal) :
    ∀ (st st' : Pass.State width) (vals : List Aggregate.Value) (tok' : Local width → Nat),
      runEvents (xs.map Event.take) st = some st' →
      takeOperands env xs st.tokens = some (vals, tok') →
      EnvDenotes h env st.tokens senv →
      tok' = st'.tokens ∧
      (∀ id, frameRefs env st.tokens id = frameRefs env st'.tokens id + valsTokens vals id) ∧
      ∃ svals, xs.mapM senv = some svals ∧ DenotesList h vals svals := by
  induction xs with
  | nil =>
      intro st st' vals tok' hrun htake _
      simp only [List.map_nil, runEvents, Option.some.injEq] at hrun
      simp only [takeOperands, Option.some.injEq, Prod.mk.injEq] at htake
      subst st'; rcases htake with ⟨rfl, rfl⟩
      exact ⟨rfl, fun id => by simp, ⟨[], rfl, .nil⟩⟩
  | cons x rest ih =>
      intro st st' vals tok' hrun htake hden
      simp only [List.map_cons, runEvents, Pass.step] at hrun
      by_cases hz : (st.tokens x == 0) = true
      · simp [hz] at hrun
      · have hzf : (st.tokens x == 0) = false := Bool.eq_false_iff.mpr hz
        have hxPos : 0 < st.tokens x := by
          have : st.tokens x ≠ 0 := by intro h0; simp [h0] at hzf
          omega
        simp only [hzf, Bool.false_eq_true, ite_false, Option.bind_some] at hrun
        simp only [takeOperands, hzf, Bool.false_eq_true, ite_false] at htake
        obtain ⟨vx, svx, hvx, hsvx, hdx⟩ := hden x hxPos
        simp only [hvx] at htake
        cases htr : takeOperands env rest (update st.tokens x (st.tokens x - 1)) with
        | none => simp [htr] at htake
        | some p =>
            rcases p with ⟨restVals, tokRest⟩
            simp only [htr, Option.some.injEq, Prod.mk.injEq] at htake
            rcases htake with ⟨rfl, rfl⟩
            let stMid : Pass.State width := { st with tokens := update st.tokens x (st.tokens x - 1) }
            have hdenMid : EnvDenotes h env stMid.tokens senv := by
              intro y hy
              have hyOrig : 0 < st.tokens y := by
                by_cases hyx : y = x
                · subst y; exact hxPos
                · have hbeq : (y == x) = false := beq_false_of_ne hyx
                  simpa [stMid, update, hbeq] using hy
              exact hden y hyOrig
            obtain ⟨htokEq, hframeRest, srest, hsrest, hdrest⟩ :=
              ih stMid st' restVals tokRest hrun htr hdenMid
            refine ⟨htokEq, ?_, svx :: srest, ?_, .cons vx svx restVals srest hdx hdrest⟩
            · intro id
              have hupd := frameRefs_update_tok env st.tokens x (st.tokens x - 1) id
              have hrest := hframeRest id
              simp only [stMid] at hrest
              simp only [hvx, optValTokens, valsTokens_cons] at hupd ⊢
              have hmul : (st.tokens x - 1) * boxedTokens vx id + boxedTokens vx id =
                  st.tokens x * boxedTokens vx id := by
                have h1 : st.tokens x - 1 + 1 = st.tokens x := by omega
                calc (st.tokens x - 1) * boxedTokens vx id + boxedTokens vx id
                    = (st.tokens x - 1) * boxedTokens vx id + 1 * boxedTokens vx id := by simp
                  _ = (st.tokens x - 1 + 1) * boxedTokens vx id := (Nat.add_mul _ _ _).symm
                  _ = st.tokens x * boxedTokens vx id := by rw [h1]
              omega
            · simp [hsvx, hsrest]

theorem denotesList_preserve_ext {nextId1 nextId2 bound1 bound2 : Nat}
    {h1 h2 : AggHeap} {rank1 rank2 : ObjId → Nat} {roots1 roots2 : ObjId → Nat}
    (hb1 : HeapBalanced nextId1 h1 rank1 bound1 roots1)
    (hb2 : HeapBalanced nextId2 h2 rank2 bound2 roots2)
    (excl : Option ObjId)
    (hexcl : ∀ c, excl = some c → heapInternalRefs nextId1 h1 c = 0)
    (hsame : ∀ id o1 o2, some id ≠ excl → h1 id = some o1 → h2 id = some o2 → o1.slot = o2.slot)
    {vs : List Aggregate.Value} {svs : List SemVal}
    (hds : DenotesList h1 vs svs)
    (hsurv : ∀ id ∈ boxedLeavesList vs, some id ≠ excl ∧ 0 < rcOf h2 id) :
    DenotesList h2 vs svs := by
  cases hds with
  | nil => exact .nil
  | cons v sv rest srest hv hrest =>
      refine .cons v sv rest srest ?_ ?_
      · exact denotes_preserve_of_balanced hb1 hb2 excl hexcl hsame hv
          (fun id hid => hsurv id (by simp [boxedLeavesList, hid]))
      · exact denotesList_preserve_ext hb1 hb2 excl hexcl hsame hrest
          (fun id hid => hsurv id (by simp [boxedLeavesList, hid]))

end Flick.Memory.Soundness
