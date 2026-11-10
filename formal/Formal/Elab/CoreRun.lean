import Formal.Elab.CoreLoad
import Formal.Elab.Close

/-!
# λFlick⁺ as a `Target` with the run of the core

`runTarget` is the core with its run: states are configurations, `load e` puts the translation
`trE` of `e` (the function `Tr` is) in the main thread, a step is a `CStep`, the result is the
literal of the main thread, and `Rel = CfgR (DP Sg root)`. Its `Has` is the typing of the core,
`Has`, with the typing `DTyp` of a dictionary by the laws (`RHas`): `load_rel` needs the
dictionaries in the holes of a skeleton typed in the empty context, and the core's typing of the
whole term does not show them; the rest of two fillings is the same (`fill_cg`). The slots of
the instances name only their premises (`slots_below`): their bodies are closed (`RootTy`).

`T6b_core` is T6(b) for the core with its run, without the hypothesis `Sim`.
-/

namespace Flick.Elab

variable {P : Prog} {Sg : Sig} {N : Names} {root : Roots}

variable (N) in
/-- The translation of an elaborated term, a function of the term (`Tr.eq`). -/
def trE : (Nat → Nat) → (Nat → Nat) → Tm → Flick.Tm
  | γ, _, .var x => .var (γ x)
  | _, _, .hole _ => .todo
  | _, δ, .dict d => trDt N δ d
  | γ, δ, .meth e m => .proj (trE γ δ e) (N.meth m)
  | _, _, .const c => .glob (N.const c)
  | γ, δ, .app f a => .app (trE γ δ f) (trE γ δ a)
  | γ, δ, .lam b => .lam (trE (ext γ) (fun k => δ k + 1) b)
  | γ, δ, .let_ e b => .let_ (trE γ δ e) (trE (ext γ) (fun k => δ k + 1) b)
  | γ, δ, .lamD l b => lamsC l (trE (fun x => γ x + l) (dext l δ) b)
  | γ, δ, .ite c a b =>
    .match_ (trE γ δ c) (.acons (.lit (.bool true)) (trE γ δ a) (.acons .wild (trE γ δ b) .anil))
  | γ, δ, .caseOpt e n s =>
    .match_ (trE γ δ e) (.acons (.app (.ctor N.some) .var) (trE (ext γ) (fun k => δ k + 1) s)
      (.acons .wild (trE γ δ n) .anil))
  | _, _, .con K => .ctor (N.ctor K)
  | γ, δ, .caseK e K n m b r =>
    .match_ (trE γ δ e) (.acons (Pat.spine (N.ctor K) (List.replicate (m + n) .var))
      (trE (aγ m n γ) (aδ m n δ) b) (.acons .wild (trE γ δ r) .anil))

theorem Tr.eq {γ δ : Nat → Nat} {e : Tm} {e' : Flick.Tm} (h : Tr N γ δ e e') : e' = trE N γ δ e := by
  induction h with
  | dict hd => exact hd.eq
  | _ => simp_all [trE]

theorem lamsC_cg {B : Flick.Tm → Flick.Tm → Prop} {b b' : Flick.Tm} (h : Cg B false b b') :
    ∀ l, Cg B false (lamsC l b) (lamsC l b')
  | 0 => h
  | l + 1 => .lam (lamsC_cg h l)

/-- **Two fillings of a skeleton by `≈` typed dictionaries translate to related terms.** -/
theorem fill_cg (hR : Real P Sg N) (hRun : RealRun P N root) (hS : SlotsBelow P) {ds ds' : List Dict}
    (h : All2 (Eqv P) ds ds') (ht : ∀ (i : Nat) (d : Dict), ds[i]? = some d → ∃ Δ T, DTyp P Δ d T) :
    ∀ (s : Tm) γ δ, Cg (DP Sg root) false (trE N γ δ (s.fill ds)) (trE N γ δ (s.fill ds')) := by
  intro s
  induction s with
  | hole i =>
    intro γ δ
    have hd : ∃ Δ T, DTyp P Δ (args ds i) T := by
      cases hi : ds[i]? with
      | none =>
        simp only [args, List.getD_eq_getElem?_getD, hi, Option.getD_none]
        exact ⟨[⟨0, []⟩], _, .var rfl⟩
      | some d =>
        simp only [args, List.getD_eq_getElem?_getD, hi, Option.getD_some]
        exact ht i d hi
    obtain ⟨Δ, T, hd⟩ := hd
    exact dsim_cg hR hRun (eqv_dsim hS (args_eqv h i)) hd δ
  | lamD _ _ ih => intro γ δ; exact lamsC_cg (ih _ _) _
  | ite _ _ _ ihc iha ihb =>
    intro γ δ; exact .match_ (ihc γ δ) (.acons (iha γ δ) (.acons (ihb γ δ) .anil))
  | caseOpt _ _ _ ihe ihn ihs =>
    intro γ δ; exact .match_ (ihe γ δ) (.acons (ihs _ _) (.acons (ihn γ δ) .anil))
  | caseK _ _ _ _ _ _ ihe ihb ihr =>
    intro γ δ; exact .match_ (ihe γ δ) (.acons (ihb _ _) (.acons (ihr γ δ) .anil))
  | meth _ _ ih => intro γ δ; exact .proj (ih γ δ)
  | app _ _ ih₁ ih₂ => intro γ δ; exact .app (ih₁ γ δ) (ih₂ γ δ)
  | lam _ ih => intro γ δ; exact .lam (ih _ _)
  | let_ _ _ ih₁ ih₂ => intro γ δ; exact .let_ (ih₁ γ δ) (ih₂ _ _)
  | var | dict | const | con => intro γ δ; exact Cg.refl _ _

theorem trDt_free {δ : Nat → Nat} {s : Dict} {k : Nat} (h : s.Occ k) : (trDt N δ s).Free (δ k) := by
  induction s with
  | var j => cases h; rfl
  | slot d i ih => exact ih h
  | ap d e ih₁ ih₂ => exact h.elim (fun h => .inl (ih₁ h)) (fun h => .inr (ih₂ h))
  | _ => exact h.elim

theorem field_free {r w : Flick.Tm} {l : Label} (h : r.field l = some w) {j : Nat} (hj : w.Free j) :
    r.Free j := by
  induction r with
  | rcons l' e rest _ ih =>
    simp only [Flick.Tm.field] at h
    split at h
    · cases h; exact .inl hj
    · exact .inr (ih h)
  | drec c r ih => exact ih h
  | _ => simp [Flick.Tm.field] at h

/-- **The slots of the instances name only their premises**: the bodies are closed. -/
theorem slots_below (hRun : RealRun P N root) (hroot : RootTyM Sg .live root) : SlotsBelow P := by
  intro I ins i s k hI hs hk
  obtain ⟨r, hg, hf⟩ := hRun.inst hI
  obtain ⟨-, sc, -, ht⟩ := hroot.glob _ _ hg
  refine Classical.byContradiction fun hl => ?_
  have hfree : (lamsC ins.ctx.length (.drec (N.cls ins.cls ins.head.length) r)).Free
      (k - ins.ctx.length) := by
    rw [lamsC_free, Nat.sub_add_cancel (by omega)]
    have := trDt_free (N := N) (δ := dext ins.ctx.length id) hk
    simp only [dext, if_neg hl, id, Nat.sub_add_cancel (Nat.le_of_not_lt hl)] at this
    exact field_free (hf i s hs) this
  have := ht.frees _ hfree
  simp at this

variable (P Sg N) in
/-- The typing of the core with the typing of the dictionaries of the term. -/
def RHas (Γ : List TSch) (Δ : List Cn) (e : Tm) (t : TTy) (φ : Stage) : Prop :=
  Has Sg N Γ Δ e t φ ∧ ∀ d, e = .dict d → DTyp P Δ d t

/-- **λFlick⁺ as a `Target` with the run of the core.** -/
def runTarget (hR : Real P Sg N) (hRun : RealRun P N root) (hroot : RootTyM Sg .live root) : Target P where
  Has := RHas P Sg N
  var hx hk := ⟨(typingTarget hR).var hx hk, fun _ h => by cases h⟩
  allLam h := ⟨(typingTarget hR).allLam h.1, fun _ h => by cases h⟩
  allVar hx hk := ⟨(typingTarget hR).allVar hx hk, fun _ h => by cases h⟩
  varAll hx hk hA hus := ⟨(typingTarget hR).varAll hx hk hA hus, fun _ h => by cases h⟩
  const hc hk := ⟨(typingTarget hR).const hc hk, fun _ h => by cases h⟩
  app h₁ h₂ hψ := ⟨(typingTarget hR).app h₁.1 h₂.1 hψ, fun _ h => by cases h⟩
  ite h₁ h₂ h₃ := ⟨(typingTarget hR).ite h₁.1 h₂.1 h₃.1, fun _ h => by cases h⟩
  caseOpt h₁ h₂ h₃ := ⟨(typingTarget hR).caseOpt h₁.1 h₂.1 h₃.1, fun _ h => by cases h⟩
  con hK hk := ⟨(typingTarget hR).con hK hk, fun _ h => by cases h⟩
  caseK he hK hT hlen hn hU hb hρ hg hcl hr :=
    ⟨(typingTarget hR).caseK he.1 hK hT hlen hn hU hb.1 hρ hg hcl hr.1, fun _ h => by cases h⟩
  caseKDead he hK hT hlen hC hr :=
    ⟨(typingTarget hR).caseKDead he.1 hK hT hlen hC hr.1, fun _ h => by cases h⟩
  lam h := ⟨(typingTarget hR).lam h.1, fun _ h => by cases h⟩
  let_ h₁ hk hφ h₂ := ⟨(typingTarget hR).let_ h₁.1 hk hφ h₂.1, fun _ h => by cases h⟩
  lamD h hφ := ⟨(typingTarget hR).lamD h.1 hφ, fun _ h => by cases h⟩
  dapp h₁ h₂ := ⟨(typingTarget hR).dapp h₁.1 h₂.1, fun _ h => by cases h⟩
  meth hm hc h := ⟨(typingTarget hR).meth hm hc h.1, fun _ h => by cases h⟩
  dvar hk := ⟨(typingTarget hR).dvar hk, fun _ h => by cases h; exact .var hk⟩
  slot h hs := ⟨(typingTarget hR).slot h.1 hs, fun _ e => by cases e; exact .slot (h.2 _ rfl) hs⟩
  dap h₁ h₂ := ⟨(typingTarget hR).dap h₁.1 h₂.1,
    fun _ e => by cases e; exact .ap (h₁.2 _ rfl) (h₂.2 _ rfl)⟩
  inst hI := ⟨(typingTarget hR).inst hI, fun _ e => by cases e; exact .inst hI⟩
  strct h₁ h₂ := ⟨(typingTarget hR).strct h₁ h₂, fun _ e => by cases e; exact .strct h₁ h₂⟩
  prim h₁ h₂ := ⟨(typingTarget hR).prim h₁ h₂, fun _ e => by cases e; exact .prim h₁ h₂⟩
  St := Cfg
  load e := ⟨[], [], trE N id id e⟩
  Step := CS root
  Obs := Lit
  obs := obsC
  Rel := CfgR (DP Sg root)
  load_rel h ht _ _ :=
    ⟨fill_cg hR hRun (slots_below hRun hroot) h
      (fun i d hd => let ⟨q, hq⟩ := ht i d hd; ⟨[], _, hq.2 _ rfl⟩) _ id id, .nil, .nil⟩
  step_rel hr hs := let ⟨_, hs⟩ := hs; cstep_sim hroot hr hs
  final_rel hr ho := ⟨_, .refl, final_sim hr ho⟩

/-- **T6(b) for the core with its run**: `Sim` is `Target.sim` of `runTarget`. -/
theorem T6b_core (hR : Real P Sg N) (hRun : RealRun P N root) (hroot : RootTyM Sg .live root)
    {lt : Cn → Cn → Prop} (h : Coherent P lt) (s : Tm) {t : TTy} {φ : Stage}
    {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ) {qs : List Cn}
    {ds₁ ds₂ : List Dict} (h₁ : Ent P Δ qs ds₁) (h₂ : Ent P Δ qs ds₂)
    (o₁ : RHas P Sg N [] [] (s.fill (ds₁.map (·.subst ρ))) t φ)
    (o₂ : RHas P Sg N [] [] (s.fill (ds₂.map (·.subst ρ))) t φ) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      (runTarget hR hRun hroot).exprs.run (s, t, φ) (ds₁.map (·.subst ρ)) =
        (runTarget hR hRun hroot).exprs.run (s, t, φ) (ds₂.map (·.subst ρ)) :=
  T6b_target (runTarget hR hRun hroot) h s hρ h₁ h₂ o₁ o₂

end Flick.Elab
