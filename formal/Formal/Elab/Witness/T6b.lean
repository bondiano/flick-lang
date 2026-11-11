import Formal.Elab.Witness.Main
import Formal.Elab.Witness.Consts
import Formal.Elab.Close

/-!
# Witness: T6(b) on a concrete term, without hypotheses of typing

`witness_T6b` takes the typings `RHas` of both fillings as premises. Here they are proved.

`witness_T6b_example`: the block `do { x <- pure (); pure x }` over the lazy `Eff`, elaborated
under the premise `Monad Eff` (closed by the instance `I2`). Its skeleton has three dictionary
holes, `flat_map` and the two `pure`. One filling takes `Applicative Eff` for both `pure` from
the instance `I1`, the other from the slot of the premise `Monad Eff`. Both fillings are typed
by the laws of `runTarget` (`has_do`), so the runs of the core give the same observations.

`witness_T6b_closed`: any two elaborations of one closed source term in the empty context of
premises run alike. Their typings come from T6(a) for `runTarget`.

`witness_T6b_open` (`docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context`): the same under premises `Δ`, closed by any types `θ` and closed
dictionaries `ρ` to ground constraints. The typings of the closed fillings come from
`T6a_close` for `runTarget`; the premises left are syntactic: the skeleton has its holes outside
the abstractions over dictionaries (`Tm.Skel`).
-/

namespace Flick.Elab.Witness

open Flick.Elab

/-- `do { x <- pure (); pure x }` elaborated, with the dictionary of `flat_map` at hole `0` and
those of the two `pure` at holes `1` and `2`. -/
def doSkel : Tm := .app (.app (.meth (.hole 0) 3) (.lam (.app (.meth (.hole 1) 1) (.var 0))))
  (.app (.meth (.hole 2) 1) (.const 8))

def effMonad : Cn := ⟨cMonad, [.con 5]⟩
def effApp : Cn := ⟨cApp, [.con 5]⟩

/-- The row of the block, `(∅ ∪ ∅) ∪ ∅`, and its type `Eff row Unit`. -/
def doRow : Ty := .cun (.cun .cnil .cnil) .cnil
def doTy : TTy := .mono (appQ (.con 5) doRow (.con 1))

/-- The filled block is typed by the laws of any target, from the typings of its dictionaries. -/
theorem has_do (X : Target P) {dM dA dA' : Dict} (hM : DTyp P [] dM (.dict effMonad))
    (hA : DTyp P [] dA (.dict effApp)) (hA' : DTyp P [] dA' (.dict effApp)) :
    X.Has [] [] (doSkel.fill [dM, dA, dA']) doTy .P := by
  have hpure : ∀ Γ d, DTyp P [] d (.dict effApp) →
      X.Has Γ [] (.meth (.dict d) 1) (.mono (arrP (.con 1) (appQ (.con 5) .cnil (.con 1)))) .P :=
    -- the bound `a^c` of `pure` at `a := Unit`, `c := ∅`: `Unit` shows nothing
    fun Γ d h => X.meth (m := 1) (θ := inst [.con 5, .cnil, .con 1]) rfl rfl
      (Bounds.hold_one fun _ h => h) (h.target X Γ .P)
  have hk : X.Has [] [] (.lam (.app (.meth (.dict dA) 1) (.var 0)))
      (.mono (.arr .P (.con 1) (appQ (.con 5) .cnil (.con 1)) .cnil)) .P :=
    X.lam (X.app (hpure _ _ hA) (X.var (s := ⟨0, .mono (.con 1), []⟩) (ts := []) rfl rfl
      Bounds.hold_nil) (.inl rfl))
  have hx : X.Has [] [] (.app (.meth (.dict dA') 1) (.const 8))
      (.mono (appQ (.con 5) .cnil (.con 1))) .P :=
    X.app (hpure _ _ hA') (X.const (s := ⟨0, [], .con 1, .cnil, []⟩) (ts := []) rfl rfl
      Bounds.hold_nil) (.inl rfl)
  exact X.app (X.app (X.meth (m := 3) (θ := inst [.con 5, .cnil, .cnil, .con 1, .con 1, .cnil])
    rfl rfl Bounds.hold_nil (hM.target X [] .P)) hk (.inl rfl)) hx (.inl rfl)

/-- The premise `Monad Eff`, closed by the instance `I2`. -/
def ρEff : Nat → Dict := fun _ => .inst 2

theorem dtyp_inst2 : DTyp P [] (.inst 2) (.dict effMonad) :=
  DTyp.inst (σ := Ty.var) (ins := ⟨cMonad, [.con 5], [], [.inst 1]⟩) rfl
theorem dtyp_inst1 : DTyp P [] (.inst 1) (.dict effApp) :=
  DTyp.inst (σ := Ty.var) (ins := ⟨cApp, [.con 5], [], []⟩) rfl
theorem dtyp_slot : DTyp P [] (.slot (.inst 2) 0) (.dict effApp) := .slot dtyp_inst2 rfl

theorem ent_inst (I : Nat) {Δ Q ins} (h : insts I = some ins) (hc : ins.cls = Q.cls)
    (ha : Q.args = ins.head) (hx : ins.ctx = []) : Ent P Δ [Q] [.inst I] := by
  have := Ent.inst (P := P) (Δ := Δ) (Q := Q) (σ := Ty.var) (ds := []) h hc
    (by rw [ha]; simp [Ty.subst_var])
    (by rw [hx]; exact .nil)
  exact this

/-- **T6(b) on a concrete term**: `pure` from the instance `I1` and from the slot of `Monad Eff`
give the same observations of the run of the core. No premise. -/
theorem witness_T6b_example :
    All2 (Eqv P) ([Dict.var 0, .inst 1, .inst 1].map (·.subst ρEff))
        ([Dict.var 0, .slot (.var 0) 0, .slot (.var 0) 0].map (·.subst ρEff)) ∧
      (runTarget real realRun rootTy).exprs.run (doSkel, doTy, .P)
          ([Dict.var 0, .inst 1, .inst 1].map (·.subst ρEff)) =
        (runTarget real realRun rootTy).exprs.run (doSkel, doTy, .P)
          ([Dict.var 0, .slot (.var 0) 0, .slot (.var 0) 0].map (·.subst ρEff)) :=
  witness_T6b doSkel (Δ := [effMonad]) (θ := Ty.var) (qs := [effMonad, effApp, effApp])
    (fun k Q hk => by
      cases k with
      | zero => cases hk; exact ent_inst 2 rfl rfl rfl rfl
      | succ k => simp at hk)
    (.cons (.given (.given rfl)) (.cons (ent_inst 1 rfl rfl rfl rfl)
      (.cons (ent_inst 1 rfl rfl rfl rfl) .nil)))
    (.cons (.given (.given rfl)) (.cons (.given (.super (.given rfl) rfl))
      (.cons (.given (.super (.given rfl) rfl)) .nil)))
    (has_do (runTarget real realRun rootTy) dtyp_inst2 dtyp_inst1 dtyp_inst1)
    (has_do (runTarget real realRun rootTy) dtyp_inst2 dtyp_slot dtyp_slot)

theorem Dict.subst_var (d : Dict) : d.subst .var = d := by
  induction d <;> simp_all [Dict.subst]

/-- **T6(b) for closed terms of the witness**: two elaborations of one source term in the empty
contexts, whose dictionaries fill one skeleton, run alike. No premise of typing: T6(a) for
`runTarget` gives both. -/
theorem witness_T6b_closed {e : Exp} {τ : Ty} {φ : Stage} (s : Tm) {qs : List Cn}
    {ds₁ ds₂ : List Dict} (h₁ : Ent P [] qs ds₁) (h₂ : Ent P [] qs ds₂)
    (he₁ : Elab P [] [] e τ φ (s.fill ds₁)) (he₂ : Elab P [] [] e τ φ (s.fill ds₂)) :
    All2 (Eqv P) ds₁ ds₂ ∧
      (runTarget real realRun rootTy).exprs.run (s, .mono τ, φ) ds₁ =
        (runTarget real realRun rootTy).exprs.run (s, .mono τ, φ) ds₂ := by
  have hid : ∀ ds : List Dict, ds.map (·.subst .var) = ds := fun ds => by simp [Dict.subst_var]
  have := witness_T6b s (Δ := []) (θ := Ty.var) (ρ := .var) (fun _ _ h => by simp at h) h₁ h₂
    (by rw [hid]; exact T6a (runTarget real realRun rootTy) he₁)
    (by rw [hid]; exact T6a (runTarget real realRun rootTy) he₂)
  rwa [hid, hid] at this

/-- **T6(b) for the witness under premises** (`docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context`): two elaborations of one source term under
`Δ`, whose dictionaries fill one skeleton, run alike once `Δ` is closed by types `θ` and closed
dictionaries `ρ` to ground constraints. No premise of typing: `T6a_close` gives both. -/
theorem witness_T6b_open {e : Exp} {τ : Ty} {φ : Stage} (s : Tm) {Δ : List Cn} {θ : Nat → Ty}
    {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ) (hg : ∀ (k : Nat) (Q : Cn), Δ[k]? = some Q →
      (Q.subst θ).Ground) {qs : List Cn} {ds₁ ds₂ : List Dict} (h₁ : Ent P Δ qs ds₁)
    (h₂ : Ent P Δ qs ds₂) (hs : s.Skel qs.length 0)
    (he₁ : Elab P [] Δ e τ φ (s.fill ds₁)) (he₂ : Elab P [] Δ e τ φ (s.fill ds₂)) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      (runTarget real realRun rootTy).exprs.run (s, .mono (τ.subst θ), φ.subst θ)
        (ds₁.map (·.subst ρ)) =
        (runTarget real realRun rootTy).exprs.run (s, .mono (τ.subst θ), φ.subst θ)
          (ds₂.map (·.subst ρ)) := by
  have o : ∀ {ds : List Dict}, Ent P Δ qs ds → Elab P [] Δ e τ φ (s.fill ds) →
      RHas P Sg N [] [] (s.fill (ds.map (·.subst ρ))) (.mono (τ.subst θ)) (φ.subst θ) := fun hE he => by
    have := T6a_close (runTarget real realRun rootTy) consts_closed (fun _ _ h => by cases h) he
      (Δl := []) rfl hρ hg
    rwa [fill_dsubst (by rw [hE.get.1]; exact hs)] at this
  exact witness_T6b s hρ h₁ h₂ (o h₁ he₁) (o h₂ he₂)

end Flick.Elab.Witness
