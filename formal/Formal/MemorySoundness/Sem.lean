import Formal.MemoryAggregate.RepCheck
import Formal.MemoryScopes

/-!
# Operational Memory IR and Source Semantics (#463)

Defines:
- Pure semantic values `SemVal` erasing heap identities, `ValueRep` (`cell` vs
  `aggregate`), and `Data::BoxedAggregate` fallback-cell wrappers;
- Acyclic denotation judgment `Denotes h v sv` and its determinism and
  heap-transition preservation theorem `Denotes.preserve`;
- Operational memory IR (`MemOp`, `MemInst`, `MemBlock`) projecting cleanly to
  `RepBlock` (`toRepBlock`), together with erased source evaluation `evalSource`
  (treating `dup`/`drop` as no-ops, `boxAggregate`/`unboxBorrow`/`unboxConsume`
  as semantic identity, and `reuse` identically to `construct`).
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Pass
open Flick.Memory.Aggregate

/-! ## Pure semantic values and denotation -/

inductive SemVal where
  | imm
  | node (tag : Nat) (fields : List SemVal)
  deriving Repr

mutual
  /-- Inductive denotation of a counted heap/aggregate value into a pure `SemVal`.
  Erases `ObjId` indirection, `ValueRep::Cell` vs `ValueRep::Aggregate`, and
  `AggSlot.boxedAgg` fallback-cell wrappers. -/
  inductive Denotes (h : AggHeap) : Aggregate.Value → SemVal → Prop where
    | imm : Denotes h .imm .imm
    | agg (tag : Nat) (fields : List Aggregate.Value) (sfields : List SemVal) :
        DenotesList h fields sfields → Denotes h (.agg tag fields) (.node tag sfields)
    | tagged (id : ObjId) (rc tag : Nat) (layout : CellLayout)
        (fields : List Aggregate.Value) (sfields : List SemVal) :
        h id = some ⟨rc, .tagged tag layout fields⟩ → 0 < rc →
        DenotesList h fields sfields → Denotes h (.boxed id) (.node tag sfields)
    | boxedAgg (id : ObjId) (rc : Nat) (inner : Aggregate.Value) (layout : CellLayout)
        (sv : SemVal) :
        h id = some ⟨rc, .boxedAgg inner layout⟩ → 0 < rc →
        Denotes h inner sv → Denotes h (.boxed id) sv
    | opaque_ (id : ObjId) (rc : Nat) :
        h id = some ⟨rc, .opaque⟩ → 0 < rc → Denotes h (.boxed id) .imm
    | reuseToken (id : ObjId) (rc : Nat) (layout : CellLayout) :
        h id = some ⟨rc, .reuseToken layout⟩ → 0 < rc → Denotes h (.boxed id) .imm

  inductive DenotesList (h : AggHeap) : List Aggregate.Value → List SemVal → Prop where
    | nil : DenotesList h [] []
    | cons (v : Aggregate.Value) (sv : SemVal)
        (vs : List Aggregate.Value) (svs : List SemVal) :
        Denotes h v sv → DenotesList h vs svs → DenotesList h (v :: vs) (sv :: svs)
end

theorem DenotesList.getElem? {h : AggHeap} :
    ∀ {vs : List Aggregate.Value} {svs : List SemVal},
      DenotesList h vs svs →
      ∀ (i : Nat) (v : Aggregate.Value), vs[i]? = some v →
        ∃ sv, svs[i]? = some sv ∧ Denotes h v sv
  | _, _, .nil, _, _, hv => by simp at hv
  | _, _, .cons _ sv0 _ _ hv0 hvs, 0, _, hv => by
      simp only [List.getElem?_cons_zero, Option.some.injEq] at hv
      subst hv
      exact ⟨sv0, rfl, hv0⟩
  | _, _, .cons _ _ _ _ _ hvs, j + 1, v, hv => by
      simp only [List.getElem?_cons_succ] at hv ⊢
      exact DenotesList.getElem? hvs j v hv

theorem DenotesList.getElem?_rev {h : AggHeap} :
    ∀ {vs : List Aggregate.Value} {svs : List SemVal},
      DenotesList h vs svs →
      ∀ (i : Nat) (sv : SemVal), svs[i]? = some sv →
        ∃ v, vs[i]? = some v ∧ Denotes h v sv
  | _, _, .nil, _, _, hsv => by simp at hsv
  | _, _, .cons v0 _ _ _ hv0 _, 0, _, hsv => by
      simp only [List.getElem?_cons_zero, Option.some.injEq] at hsv
      subst hsv
      exact ⟨v0, rfl, hv0⟩
  | _, _, .cons _ _ _ _ _ hvs, j + 1, sv, hsv => by
      simp only [List.getElem?_cons_succ] at hsv ⊢
      exact DenotesList.getElem?_rev hvs j sv hsv

/-- Pure lookup along a nested path of field indices in a `SemVal`. -/
def lookupSemPath : SemVal → List Nat → Option SemVal
  | sv, [] => some sv
  | .node _ fields, i :: rest =>
      match fields[i]? with
      | none => none
      | some f => lookupSemPath f rest
  | .imm, _ :: _ => none

theorem Denotes.lookupPath_sound {h : AggHeap} {v : Aggregate.Value} {sv : SemVal}
    (path : List Nat) (field : Aggregate.Value)
    (hd : Denotes h v sv) (hl : Aggregate.lookupPath v path = some field) :
    ∃ sfield, lookupSemPath sv path = some sfield ∧ Denotes h field sfield := by
  induction path generalizing v sv with
  | nil =>
      simp only [Aggregate.lookupPath, Option.some.injEq] at hl
      subst hl
      exact ⟨sv, rfl, hd⟩
  | cons i rest ih =>
      cases v with
      | imm | boxed _ => cases hl
      | agg tag fields =>
          cases hd with
          | agg _ _ sfields hds =>
              unfold Aggregate.lookupPath at hl
              cases hf : fields[i]? with
              | none => rw [hf] at hl; cases hl
              | some f =>
                  rw [hf] at hl
                  obtain ⟨sf, hsf, hdf⟩ := hds.getElem? i f hf
                  obtain ⟨sres, hres, hdres⟩ := ih hdf hl
                  exact ⟨sres, by simp [lookupSemPath, hsf, hres], hdres⟩

mutual
  theorem Denotes.det {h : AggHeap} :
      ∀ {v : Aggregate.Value} {sv1 sv2 : SemVal},
        Denotes h v sv1 → Denotes h v sv2 → sv1 = sv2
    | _, _, _, .imm, .imm => rfl
    | _, _, _, .agg tag _ _ hds1, .agg _ _ _ hds2 =>
        congrArg (SemVal.node tag) (DenotesList.det hds1 hds2)
    | _, _, _, .tagged _ _ tag _ _ _ h1 _ hds1, .tagged _ _ _ _ _ _ h2 _ hds2 => by
        rw [h1] at h2; cases h2
        exact congrArg (SemVal.node tag) (DenotesList.det hds1 hds2)
    | _, _, _, .tagged _ _ _ _ _ _ h1 _ _, .boxedAgg _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .tagged _ _ _ _ _ _ h1 _ _, .opaque_ _ _ h2 _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .tagged _ _ _ _ _ _ h1 _ _, .reuseToken _ _ _ h2 _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .boxedAgg _ _ _ _ _ h1 _ hd1, .boxedAgg _ _ _ _ _ h2 _ hd2 => by
        rw [h1] at h2; cases h2
        exact Denotes.det hd1 hd2
    | _, _, _, .boxedAgg _ _ _ _ _ h1 _ _, .tagged _ _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .boxedAgg _ _ _ _ _ h1 _ _, .opaque_ _ _ h2 _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .boxedAgg _ _ _ _ _ h1 _ _, .reuseToken _ _ _ h2 _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .opaque_ _ _ _ _, .opaque_ _ _ _ _ => rfl
    | _, _, _, .opaque_ _ _ h1 _, .tagged _ _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .opaque_ _ _ h1 _, .boxedAgg _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .opaque_ _ _ _ _, .reuseToken _ _ _ _ _ => rfl
    | _, _, _, .reuseToken _ _ _ _ _, .reuseToken _ _ _ _ _ => rfl
    | _, _, _, .reuseToken _ _ _ h1 _, .tagged _ _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .reuseToken _ _ _ h1 _, .boxedAgg _ _ _ _ _ h2 _ _ => by
        rw [h1] at h2; cases h2
    | _, _, _, .reuseToken _ _ _ _ _, .opaque_ _ _ _ _ => rfl

  theorem DenotesList.det {h : AggHeap} :
      ∀ {vs : List Aggregate.Value} {svs1 svs2 : List SemVal},
        DenotesList h vs svs1 → DenotesList h vs svs2 → svs1 = svs2
    | _, _, _, .nil, .nil => rfl
    | _, _, _, .cons _ _ _ _ hv1 hvs1, .cons _ _ _ _ hv2 hvs2 => by
        rw [Denotes.det hv1 hv2, DenotesList.det hvs1 hvs2]
end

mutual
  /-- Heap-transition preservation of `Denotes`: if `P` holds for every boxed leaf
  of `v`, is closed under `slotBoxedLeaves`, and every `P`-object stays live with
  the same `slot` in `h2`, then `Denotes h1 v sv` transfers verbatim to `h2`. -/
  theorem Denotes.preserve (h1 h2 : AggHeap) (P : ObjId → Prop)
      (hP : ∀ id obj, P id → h1 id = some obj → 0 < obj.rc →
        ∃ o2, h2 id = some o2 ∧ 0 < o2.rc ∧ o2.slot = obj.slot)
      (hclosed : ∀ id obj, P id → h1 id = some obj → 0 < obj.rc →
        ∀ c ∈ slotBoxedLeaves obj.slot, P c) :
      ∀ {v : Aggregate.Value} {sv : SemVal},
        Denotes h1 v sv → (∀ id ∈ boxedLeaves v, P id) → Denotes h2 v sv
    | _, _, .imm, _ => .imm
    | _, _, .agg tag fields sfields hds, hmem =>
        .agg tag fields sfields (DenotesList.preserve h1 h2 P hP hclosed hds hmem)
    | _, _, .tagged id rc tag layout fields sfields hobj hpos hds, hmem => by
        have hPid : P id := hmem id (by simp [boxedLeaves])
        obtain ⟨⟨rc2, slot2⟩, hobj2, hpos2, hslot⟩ :=
          hP id ⟨rc, .tagged tag layout fields⟩ hPid hobj hpos
        simp only at hslot
        subst hslot
        have hch : ∀ c ∈ boxedLeavesList fields, P c :=
          hclosed id ⟨rc, .tagged tag layout fields⟩ hPid hobj hpos
        exact .tagged id rc2 tag layout fields sfields hobj2 hpos2
          (DenotesList.preserve h1 h2 P hP hclosed hds hch)
    | _, _, .boxedAgg id rc inner layout sv hobj hpos hdi, hmem => by
        have hPid : P id := hmem id (by simp [boxedLeaves])
        obtain ⟨⟨rc2, slot2⟩, hobj2, hpos2, hslot⟩ :=
          hP id ⟨rc, .boxedAgg inner layout⟩ hPid hobj hpos
        simp only at hslot
        subst hslot
        have hch : ∀ c ∈ boxedLeaves inner, P c :=
          hclosed id ⟨rc, .boxedAgg inner layout⟩ hPid hobj hpos
        exact .boxedAgg id rc2 inner layout sv hobj2 hpos2
          (Denotes.preserve h1 h2 P hP hclosed hdi hch)
    | _, _, .opaque_ id rc hobj hpos, hmem => by
        have hPid : P id := hmem id (by simp [boxedLeaves])
        obtain ⟨⟨rc2, slot2⟩, hobj2, hpos2, hslot⟩ :=
          hP id ⟨rc, .opaque⟩ hPid hobj hpos
        simp only at hslot
        subst hslot
        exact .opaque_ id rc2 hobj2 hpos2
    | _, _, .reuseToken id rc layout hobj hpos, hmem => by
        have hPid : P id := hmem id (by simp [boxedLeaves])
        obtain ⟨⟨rc2, slot2⟩, hobj2, hpos2, hslot⟩ :=
          hP id ⟨rc, .reuseToken layout⟩ hPid hobj hpos
        simp only at hslot
        subst hslot
        exact .reuseToken id rc2 layout hobj2 hpos2

  theorem DenotesList.preserve (h1 h2 : AggHeap) (P : ObjId → Prop)
      (hP : ∀ id obj, P id → h1 id = some obj → 0 < obj.rc →
        ∃ o2, h2 id = some o2 ∧ 0 < o2.rc ∧ o2.slot = obj.slot)
      (hclosed : ∀ id obj, P id → h1 id = some obj → 0 < obj.rc →
        ∀ c ∈ slotBoxedLeaves obj.slot, P c) :
      ∀ {vs : List Aggregate.Value} {svs : List SemVal},
        DenotesList h1 vs svs → (∀ id ∈ boxedLeavesList vs, P id) → DenotesList h2 vs svs
    | _, _, .nil, _ => .nil
    | _, _, .cons v sv vs svs hv hvs, hmem =>
        .cons v sv vs svs
          (Denotes.preserve h1 h2 P hP hclosed hv
            (fun id hid => hmem id (by simp [boxedLeavesList, hid])))
          (DenotesList.preserve h1 h2 P hP hclosed hvs
            (fun id hid => hmem id (by simp [boxedLeavesList, hid])))
end

/-! ## Operational Memory IR and Erased Source Evaluation -/

variable {width : Nat}

/-- Operational memory-IR operations refining `RepOp` with concrete tags,
field indices, inline projection paths, and `CellLayout` descriptors. -/
inductive MemOp (width : Nat) where
  | lit
  | project (receiver : Local width) (rep : ValueRep) (field : Nat)
  | projectPath (receiver : Local width) (path : List Nat)
  | construct (rep : ValueRep) (tag : Nat) (layout : CellLayout)
      (fields evidence : List (Local width))
  | boxAggregate (value : Local width) (layout : CellLayout)
  | unboxBorrow (object : Local width)
  | unboxConsume (object : Local width)
  | dropReuse (object : Local width)
  | reuse (receiver : Local width) (rep : ValueRep) (tag : Nat) (layout : CellLayout)
      (fields evidence : List (Local width))
  deriving DecidableEq, Repr

inductive MemInst (width : Nat) where
  | let_ (binding : Local width) (op : MemOp width)
  | dup (name : Local width)
  | drop (name : Local width)
  deriving DecidableEq, Repr

inductive MemBlock (width : Nat) where
  | ret (result : Local width)
  | seq (inst : MemInst width) (next : MemBlock width)
  | branch (condition : Local width) (yes no : MemBlock width)
  deriving DecidableEq, Repr

/-- Erase operational metadata (tags, field indices, layouts) to `RepOp`. -/
def toRepOp : MemOp width → RepOp width
  | .lit => .consume []
  | .project recv rep _ => .borrow recv rep
  | .projectPath recv _ => .borrow recv .aggregate
  | .construct rep _ _ fields evidence => .construct rep fields evidence
  | .boxAggregate val _ => .boxAggregate val
  | .unboxBorrow obj => .unboxBorrow obj
  | .unboxConsume obj => .unboxConsume obj
  | .dropReuse obj => .dropReuse obj
  | .reuse recv rep _ _ fields evidence => .reuse recv rep fields evidence

def toRepInst : MemInst width → RepInst width
  | .let_ b op => .let_ b (toRepOp op)
  | .dup n => .dup n
  | .drop n => .drop n

def toRepBlock : MemBlock width → RepBlock width
  | .ret r => .ret r
  | .seq i next => .seq (toRepInst i) (toRepBlock next)
  | .branch c yes no => .branch c (toRepBlock yes) (toRepBlock no)

/-- Maximum number of sequential/branch steps in a `MemBlock`. -/
def blockSteps : MemBlock width → Nat
  | .ret _ => 1
  | .seq _ next => 1 + blockSteps next
  | .branch _ yes no => 1 + max (blockSteps yes) (blockSteps no)

/-- Pure source evaluation of a `MemOp`, erasing `ValueRep`, `CellLayout`,
`BoxAggregate`/`Unbox*`, `DropReuse`, and `Reuse`. -/
def evalSourceOp (op : MemOp width) (senv : Local width → Option SemVal) : Option SemVal :=
  match op with
  | .lit => some .imm
  | .project recv _ idx =>
      match senv recv with
      | some (.node _ fields) => fields[idx]?
      | _ => none
  | .projectPath recv path =>
      match senv recv with
      | some sv => lookupSemPath sv path
      | none => none
  | .construct _ tag _ fields evidence =>
      ((fields ++ evidence).mapM senv).map (SemVal.node tag)
  | .boxAggregate val _ => senv val
  | .unboxBorrow obj => senv obj
  | .unboxConsume obj => senv obj
  | .dropReuse _ => some .imm
  | .reuse _ _ tag _ fields evidence =>
      ((fields ++ evidence).mapM senv).map (SemVal.node tag)

/-- Pure source evaluation of a `MemBlock` along branch choices `choices`,
erasing `dup` and `drop`. -/
def evalSource : MemBlock width → List Bool → (Local width → Option SemVal) → Option SemVal
  | .ret r, _, senv => senv r
  | .seq (.dup _) next, choices, senv => evalSource next choices senv
  | .seq (.drop _) next, choices, senv => evalSource next choices senv
  | .seq (.let_ b op) next, choices, senv =>
      match evalSourceOp op senv with
      | none => none
      | some sv => evalSource next choices (update senv b (some sv))
  | .branch c yes no, choices, senv =>
      match senv c with
      | none => none
      | some _ =>
          match choices with
          | true :: rest => evalSource yes rest senv
          | false :: rest => evalSource no rest senv
          | [] => evalSource no [] senv

end Flick.Memory.Soundness
