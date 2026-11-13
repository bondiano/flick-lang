import Formal.LiveRules
import Formal.Live.Invert

/-!
# Checks of the variance of data arguments (regions.md §2.2.1)

Data compares its arguments by the variance of each parameter, and a record field by field.
A signature that gives a parameter a sign must earn it (`Sig.var_ok`): `Ref`, a constructor
that fixes the parameter to an arrow, or a row standing below a flip keeps it invariant.
-/

namespace Flick.LiveChecks

/-- `Unit -> Unit` qualified by `C`. -/
def cb (C : Caps) : Ty := .arrow (.base .unit) .pure (.base .unit) C
def sOnly : Caps := fun a => a = .r 0
def sAndT : Caps := fun a => a = .r 0 ∨ a = .r 1

theorem cb_widens (S : Sig) : LeLive S (cb sOnly) (cb sAndT) :=
  .arrow .refl trivial .refl fun x hx => by
    simp only [Ty.reach, Caps.none, sOnly, sAndT] at hx ⊢
    rcases hx with hx | hx | hx
    · exact .inl hx
    · exact .inr (.inl hx)
    · exact .inr (.inr (.inl hx))

theorem cb_ne : cb Caps.none ≠ cb sOnly := fun h => by
  injection h with _ _ _ hC
  have := congrFun hC (.r 0)
  simp [Caps.none, sOnly] at this

theorem cb_not_equiv : ¬ Ty.Equiv (cb Caps.none) (cb sOnly) := fun h => by
  have := congrFun h.reach (.r 0)
  simp [cb, Ty.reach, Caps.none, sOnly] at this

/-! ### A signature with covariant `List` and `Box`

`type List a = Nil | Cons a (List a)` and `type Box a = Box {f : a}`. -/

def vCtor : Name → Option (Nat × List Ty × Name)
  | "Nil" => some (1, [], "List")
  | "Cons" => some (1, [.tvar 0, .tapp (.adt "List") (.tvar 0)], "List")
  | "Box" => some (1, [.rcons "f" (.tvar 0) .rnil], "Box")
  | _ => none

def vRes (K : Name) : List Ty :=
  match vCtor K with
  | some (k, _, _) => idTs k
  | none => []

def vVar (T : Name) (i : Nat) : Variance :=
  if (T = "List" ∨ T = "Box") ∧ i = 0 then .pos else .inv

theorem vVar_list : vVar "List" 0 = .pos := by simp [vVar]

theorem vVar_ne_pos {T : Name} {i : Nat} (h : vVar T i ≠ .inv) :
    (T = "List" ∨ T = "Box") ∧ i = 0 := by
  unfold vVar at h; split at h
  · assumption
  · exact absurd rfl h

/-- The parameter as itself, at a covariant position. -/
theorem pol_var : Ty.Pol vVar 0 .pos true (.tvar 0) := .var (.inl ⟨rfl, rfl⟩)

/-- `List a` at a covariant position: its parameter is covariant. -/
theorem pol_list : Ty.Pol vVar 0 .pos true (.tapp (.adt "List") (.tvar 0)) := by
  refine .data (T := "List") (us := [.tvar 0]) ?_ ?_ ?_ ?_ ?_ rfl
  all_goals
    intro j u hu hv
    rcases j with _ | j <;> simp at hu
    subst hu
  · exact pol_var
  all_goals simp [vVar] at hv

theorem vVar_ok : ∀ T i, vVar T i ≠ .inv → ∀ K k ts, vCtor K = some (k, ts, T) →
    (∀ j u, (vRes K)[j]? = some u → j ≠ i → u.NoV i) ∧
    ((i < k ∧ (vRes K)[i]? = some (.tvar i) ∧ ∀ f ∈ ts, (vVar T i = .bi → f.NoV i) ∧
        (vVar T i = .pos ∨ vVar T i = .neg → Ty.Pol vVar i (vVar T i) true f)) ∨
      (vVar T i ≠ .bi ∧ (∃ c, (vRes K)[i]? = some c ∧ c.Closed) ∧ ∀ f ∈ ts, f.NoV i)) := by
  intro T i hv K k ts hK
  obtain ⟨hT, rfl⟩ := vVar_ne_pos hv
  have hp : vVar T 0 = .pos := by simp [vVar, hT]
  rw [hp]
  have hk : k = 1 := by unfold vCtor at hK; split at hK <;> cases hK <;> rfl
  subst hk
  have hres : vRes K = [.tvar 0] := by simp [vRes, hK, idTs]
  rw [hres]
  refine ⟨fun j u hu hj => ?_, .inl ⟨by decide, rfl, fun f hf => ⟨fun h => (nomatch h), fun _ => ?_⟩⟩⟩
  · rcases j with _ | j <;> simp at hu hj
  unfold vCtor at hK
  split at hK <;> cases hK
  · simp at hf
  · simp at hf
    rcases hf with rfl | rfl
    · exact pol_var
    · exact pol_list
  · simp at hf; subst hf
    exact .rcons pol_var (.none (by simp [Ty.NoV, Ty.substT]))

def vSig : Sig where
  ctor := vCtor
  impl _ := .errors Row.none
  ctor_closed := by
    intro K k ts T σ hK hσ
    unfold vCtor at hK
    split at hK <;> cases hK <;> simp [Ty.substT, hσ]
  ctor_reach := by
    intro K k ts T hK t ht a ha
    unfold vCtor at hK
    split at hK <;> cases hK <;> simp at ht <;>
      (try rcases ht with rfl | rfl) <;> (try subst ht) <;>
      simp [Ty.reach, Caps.none] at ha <;>
      (try rcases ha with ha | ha) <;> (try subst ha) <;> exact ⟨0, by decide, rfl⟩
  res := vRes
  res_closed := by
    intro K k ts T σ h hσ
    simp only [vRes, h, idTs, List.map_map]
    apply List.map_congr_left
    intro i hi
    simp only [Function.comp, Ty.substT]
    exact hσ i (List.mem_range.mp hi)
  impl_closed _ _ := by simp [Ty.substT, Row.none_substT]
  dict _ := none
  glob _ := none
  dict_closed _ _ _ h := nomatch h
  glob_closed _ _ h := nomatch h
  op _ _ := none
  op_closed _ _ _ _ _ _ h := nomatch h
  op_impl _ _ _ _ _ h := nomatch h
  op_field _ _ _ _ h := nomatch h
  var := vVar
  var_ok := vVar_ok

/-- A covariant argument of data widens with what it holds:
`List ((Unit -> Unit)^{s})` passes where `List ((Unit -> Unit)^{s,t})` is expected. -/
theorem list_widens :
    LeLive vSig (Ty.tcon "List" [cb sOnly]) (Ty.tcon "List" [cb sAndT]) := by
  refine .data (T := "List") (us := [cb sOnly]) (us' := [cb sAndT]) rfl ?_ ?_ ?_ ?_ rfl rfl
  all_goals
    intro j u u' h h'
    rcases j with _ | j <;> simp at h h'
    subst h; subst h'
  · exact (cb_widens vSig).reach
  · intro _ _; exact cb_widens vSig
  all_goals intro _ hv; simp [vSig, vVar_list] at hv

/-- It never narrows: that would lose the region `t` the callback holds. -/
theorem list_no_narrow :
    ¬ LeLive vSig (Ty.tcon "List" [cb sAndT]) (Ty.tcon "List" [cb sOnly]) := fun h => by
  have := h.reach (.r 1) (by simp [Ty.tcon, Ty.reach, cb, sAndT])
  simp [Ty.tcon, Ty.reach, cb, sOnly, Caps.none] at this

/-- A parameter in a record field is covariant (`Ty.Pol.rcons`), as is the record. -/
theorem box_widens :
    LeLive vSig (Ty.tcon "Box" [cb sOnly]) (Ty.tcon "Box" [cb sAndT]) := by
  refine .data (T := "Box") (us := [cb sOnly]) (us' := [cb sAndT]) rfl ?_ ?_ ?_ ?_ rfl rfl
  all_goals
    intro j u u' h h'
    rcases j with _ | j <;> simp at h h'
    subst h; subst h'
  · exact (cb_widens vSig).reach
  · intro _ _; exact cb_widens vSig
  all_goals intro _ hv; simp [vSig, vVar] at hv

/-- A record is immutable: its field widens. -/
theorem record_widens (S : Sig) :
    LeLive S (.rcons "f" (cb sOnly) .rnil) (.rcons "f" (cb sAndT) .rnil) :=
  .rcons' (cb_widens S) .refl

/-! ### What keeps a parameter invariant -/

theorem ref_not_noV (i : Nat) : ¬ (Ty.ref (.tvar i)).NoV i := by
  simp [Ty.NoV, Ty.substT, dropV]

/-- A cell holds its parameter at no polarity. -/
theorem ref_no_sign {var : Name → Nat → Variance} {i : Nat} {s : Variance} {q : Bool} :
    ¬ Ty.Pol var i s q (.ref (.tvar i)) := fun h => by
  cases h with
  | none h => exact ref_not_noV i h
  | data _ _ _ _ _ ht => exact Ty.ne_tcon_of_plain rfl ht

/-- `type Cell a = Cell (Ref a)` is invariant: mutable storage. -/
theorem cell_invariant (S : Sig) (hK : S.ctor "Cell" = some (1, [.ref (.tvar 0)], "Cell")) :
    S.var "Cell" 0 = .inv := Classical.byContradiction fun hv => by
  obtain ⟨-, h⟩ := S.var_ok "Cell" 0 hv "Cell" 1 _ hK
  rcases h with ⟨-, -, hf⟩ | ⟨-, -, hf⟩
  · obtain ⟨hb, hp⟩ := hf _ (.head _)
    cases hs : S.var "Cell" 0 with
    | inv => exact hv hs
    | bi => exact ref_not_noV 0 (hb hs)
    | pos => exact ref_no_sign (hp (.inl hs))
    | neg => exact ref_no_sign (hp (.inr hs))
  · exact ref_not_noV 0 (hf _ (.head _))

/-- `type Tag a = Pure : Tag (Unit -> Unit)` is invariant: matching on `Pure` assumes the
argument is the pure arrow, which subtyping could move. -/
theorem tag_invariant (S : Sig) (hK : S.ctor "Pure" = some (0, [], "Tag"))
    (hr : S.res "Pure" = [cb Caps.none]) : S.var "Tag" 0 = .inv :=
  Classical.byContradiction fun hv => by
    obtain ⟨-, h⟩ := S.var_ok "Tag" 0 hv "Pure" 0 _ hK
    rw [hr] at h
    rcases h with ⟨h, -⟩ | ⟨-, ⟨c, hc, hC⟩, -⟩
    · exact Nat.lt_irrefl 0 h
    · simp at hc; subst hc; exact hC

/-- So `Pure` never reaches a branch at `Tag ((Unit -> Unit)^{s})`. -/
theorem tag_no_widen (S : Sig) (hK : S.ctor "Pure" = some (0, [], "Tag"))
    (hr : S.res "Pure" = [cb Caps.none]) :
    ¬ LeLive S (Ty.tcon "Tag" [cb Caps.none]) (Ty.tcon "Tag" [cb sOnly]) := fun h => by
  generalize hX : Ty.tcon "Tag" [cb Caps.none] = X at h
  generalize hY : Ty.tcon "Tag" [cb sOnly] = Y at h
  cases h with
  | refl => exact cb_ne (by simpa using (Ty.tcon_eq (hX.trans hY.symm)).2)
  | equiv he =>
    rw [← hX, ← hY] at he
    have := congrFun he.reach (.r 0)
    simp [Ty.tcon, cb, Ty.reach, Caps.none, sOnly] at this
  | arrow => exact Ty.arrow_ne_tcon hX.symm
  | eff => exact Ty.eff_ne_tcon hX.symm
  | rcons => exact Ty.rcons_ne_tcon hX.symm
  | hide hi _ _ _ ha _ =>
    obtain ⟨rfl, -⟩ := Ty.tcon_eq (hX.trans ha)
    exact Nat.not_lt_zero _ (S.hidden_ok _ _ hi "Pure" 0 [] hK).1
  | data _ _ _ _ hinv ha hb =>
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hX.trans ha)
    obtain ⟨-, rfl⟩ := Ty.tcon_eq (hY.trans hb)
    refine cb_not_equiv (hinv 0 _ _ rfl rfl (fun hi => ?_) (tag_invariant S hK hr))
    exact Nat.not_lt_zero _ (S.hidden_ok _ _ hi "Pure" 0 [] hK).1

theorem kField_not_noV : ¬ kField.NoV 0 := fun h => by
  simp only [Ty.NoV, kField, Ty.substT, Ty.arrow.injEq] at h
  exact kCallback_not_noV h.1

/-- The row of `K` stands below a flip, so it is not covariant. -/
theorem kField_not_pos {var : Name → Nat → Variance} : ¬ Ty.Pol var 0 .pos true kField :=
  fun h => by
    cases h with
    | none h => exact kField_not_noV h
    | arrow hpa => exact kCallback_not_noV (hpa rfl)
    | data _ _ _ _ _ ht => exact Ty.arrow_ne_tcon ht

/-- `type K (c : CaptureRow) = K ((Unit ->{} Unit)^c ->{} Unit)`: `c` is neither covariant
nor unused. -/
theorem k_not_covariant (S : Sig) (hK : S.ctor "K" = some (1, [kField], "K")) :
    S.var "K" 0 ≠ .pos ∧ S.var "K" 0 ≠ .bi := by
  have key : ∀ v, S.var "K" 0 = v → v ≠ .inv → v ≠ .pos ∧ v ≠ .bi := fun v hs hv => by
    obtain ⟨-, h⟩ := S.var_ok "K" 0 (hs ▸ hv) "K" 1 _ hK
    rcases h with ⟨-, -, hf⟩ | ⟨-, -, hf⟩
    · obtain ⟨hb, hp⟩ := hf _ (.head _)
      refine ⟨fun e => ?_, fun e => kField_not_noV (hb (hs.trans e))⟩
      have := hp (.inl (hs.trans e)); rw [hs, e] at this
      exact kField_not_pos this
    · exact absurd (hf _ (.head _)) kField_not_noV
  cases hs : S.var "K" 0 with
  | inv => exact ⟨nofun, nofun⟩
  | _ => exact key _ hs nofun

/-- So the row of `K` does not move: a callback that needs a region cannot leave its scope
in a `K` at a larger row. -/
theorem k_row_stays (S : Sig) (hK : S.ctor "K" = some (1, [kField], "K")) {C C' : Caps}
    (h : LeLive S (Ty.tcon "K" [.caps C]) (Ty.tcon "K" [.caps C'])) : C = C' := by
  generalize hX : Ty.tcon "K" [.caps C] = X at h
  generalize hY : Ty.tcon "K" [.caps C'] = Y at h
  cases h with
  | refl => simpa using (Ty.tcon_eq (hX.trans hY.symm)).2
  | equiv he =>
    rw [← hX, ← hY] at he
    simpa [Ty.tcon, Ty.reach, Caps.none] using he.reach
  | arrow => exact absurd hX.symm Ty.arrow_ne_tcon
  | eff => exact absurd hX.symm Ty.eff_ne_tcon
  | rcons => exact absurd hX.symm Ty.rcons_ne_tcon
  | @hide T i _ _ _ _ hi _ _ _ ha _ =>
    obtain ⟨rfl, -⟩ := Ty.tcon_eq (hX.trans ha)
    have : i = 0 := by have := (S.hidden_ok _ _ hi "K" 1 _ hK).1; omega
    subst this
    exact absurd hi (contra_row_not_hidden S hK)
  | data _ _ hp hn hinv ha hb =>
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hX.trans ha)
    obtain ⟨-, rfl⟩ := Ty.tcon_eq (hY.trans hb)
    have hh := contra_row_not_hidden S hK
    obtain ⟨np, nb⟩ := k_not_covariant S hK
    cases hs : S.var "K" 0 with
    | pos => exact absurd hs np
    | bi => exact absurd hs nb
    | inv => exact (hinv 0 _ _ rfl rfl hh hs).reach
    | neg => exact (LeLive.eq_plain (hn 0 _ _ rfl rfl hh hs) rfl).reach

end Flick.LiveChecks
