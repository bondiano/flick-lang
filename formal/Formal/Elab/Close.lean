import Formal.Elab.CloseTy
import Formal.Elab.Run
import Formal.Elab.SugarBase
import Formal.Elab.Keeps

/-!
# Elaboration: closing the premises by types and dictionaries (`docs/formal/elaboration.md#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context`)

`Closes P Δ θ ρ` closes the premises `Δ` of an elaboration by types `θ` and closed dictionaries
`ρ` (Lemma 6.1). `T6a_close`: an elaboration under `Δl ++ Δ`, with `Δ` closed, has in any
target the type `τθ` under `Δl θ`, with the premises of `Δ` replaced by their dictionaries
(`Tm.dsubst`). A `letSig` adds its own premises in front, so the closed ones are those from
`L` on, `L` the number of premises bound inside (`dsub`). The closed premises are ground, so
the shift of a `letSig` keeps them closed; structural and primitive dictionaries are typed at
the instance `θ` of their type (`docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type`). For a skeleton whose holes lie outside every
abstraction over dictionaries (`Tm.Skel`), closing the filling is filling with the closed
dictionaries (`fill_dsubst`).
-/

namespace Flick.Elab

variable {P : Prog}

/-- The premises from `L` on, closed by `ρ`; those below `L` are bound inside the term. -/

theorem Ent.weaken_nil {Δ qs : List Cn} {ds : List Dict} (h : Ent P [] qs ds) : Ent P Δ qs ds := by
  induction h with
  | nil => exact .nil
  | cons _ _ ih₁ ih₂ => exact .cons ih₁ ih₂
  | given hc => exact (Clo.not_nil hc).elim
  | prim h₁ h₂ => exact .prim h₁ h₂
  | strct h₁ h₂ _ ih => exact .strct h₁ h₂ ih
  | inst h₁ h₂ h₃ _ ih => exact .inst h₁ h₂ h₃ ih

variable (X : Target P)

theorem clo_close {Γ : List TSch} {Δl Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} {φ : Stage}
    {d : Dict} {Q : Cn} (hρ : Closes P Δ θ ρ) (h : Clo P (Δl ++ Δ) d Q) :
    X.Has Γ (Δl.map (·.subst θ)) (.dict (d.subst (dsub Δl.length ρ))) (.dict (Q.subst θ)) φ := by
  induction h with
  | @given k Q hk =>
    by_cases hl : k < Δl.length
    · rw [List.getElem?_append_left hl] at hk
      simp only [Dict.subst, dsub, hl, if_true]
      exact X.dvar (by rw [List.getElem?_map, hk]; rfl)
    · rw [List.getElem?_append_right (by omega)] at hk
      simp only [Dict.subst, dsub, hl, if_false]
      cases ent_typed X (Γ := Γ) (φ := φ) (Ent.weaken_nil (Δ := Δl.map (·.subst θ)) (hρ _ _ hk)) with
      | cons r _ => exact r
  | super _ hs ih => exact X.slot ih (Prog.super_subst θ hs)

/-- **Closed dictionaries are typed**: `Δl, Δ ⊩ Q̄ ⇝ d̄` and `Δ` closed by `θ`, `ρ` give
`⟦Δl θ⟧ ⊢⁺ d̄[ρ] : D_{Q̄θ}`. -/
theorem ent_close {Γ : List TSch} {Δl Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} {φ : Stage}
    {qs : List Cn} {ds : List Dict} (hρ : Closes P Δ θ ρ) (h : Ent P (Δl ++ Δ) qs ds) :
    DTy X Γ (Δl.map (·.subst θ)) φ (ds.map (·.subst (dsub Δl.length ρ))) (qs.map (·.subst θ)) := by
  induction h with
  | nil => exact .nil
  | cons _ _ ih₁ ih₂ => cases ih₁ with | cons r _ => exact .cons r ih₂
  | given hc => exact .cons (clo_close X hρ hc) .nil
  | prim h₁ h₂ => exact .cons (X.prim h₁ h₂) .nil
  | strct h₁ h₂ _ ih =>
    simp only [List.map_map, Function.comp_def, Cn.subst, List.map_cons, List.map_nil] at ih
    simp only [List.map_cons, List.map_nil, apps_subst, Cn.subst]
    exact .cons (dict_apps X (X.strct h₁ h₂) ih) .nil
  | @inst Q I ins σ ds h₁ h₂ h₃ _ ih =>
    have hq : Q = ⟨ins.cls, ins.head.map (·.subst σ)⟩ := by cases Q; simp_all
    subst hq
    simp only [List.map_map, Function.comp_def, Cn.subst_comp] at ih
    simp only [List.map_cons, List.map_nil, apps_subst, Cn.subst, List.map_map,
      Function.comp_def, Ty.subst_comp]
    exact .cons (dict_apps X (X.inst (σ := fun n => (σ n).subst θ) h₁) ih) .nil

theorem Stage.subst_or {ψ φ : Stage} (θ : Nat → Ty) (h : ψ = .P ∨ ψ = φ) :
    ψ.subst θ = .P ∨ ψ.subst θ = φ.subst θ :=
  h.imp (fun h => by rw [h]; rfl) (congrArg _)

/-- **T6(a) with the premises closed.** `Γ; Δl, Δ ⊢ e : τ ! φ ⇝ e'` and `Δ` closed by `θ`, `ρ`
to ground constraints give `⟦Γθ⟧, ⟦Δl θ⟧ ⊢⁺ e'[ρ] : τθ ! φθ` in any target. -/
theorem T6a_close (hP : P.ConstsClosed) (hC : P.CtorsClosed) {Γ D e τ φ e'}
    (h : Elab P Γ D e τ φ e') :
    ∀ {Δl Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}, D = Δl ++ Δ → Closes P Δ θ ρ →
      (∀ (k : Nat) (Q : Cn), Δ[k]? = some Q → (Q.subst θ).Ground) →
      X.Has ((Γ.map (·.subst θ)).map Scheme.tr) (Δl.map (·.subst θ)) (e'.dsubst ρ Δl.length)
        (.mono (τ.subst θ)) (φ.subst θ) := by
  induction h with
  | @var Γ D x σ ts ds φ hx hk hb he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    have hv := X.var (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (Δ := Δl.map (·.subst θ)) (φ := φ.subst θ)
      (s := (σ.subst θ).tr) (ts := ts.map (·.subst θ))
      (by rw [List.getElem?_map, List.getElem?_map, hx]; rfl) (by simpa [Scheme.subst] using hk)
      (by
        have := hb.subst θ
        simp only [Scheme.tr, Scheme.subst, List.map_map, Function.comp_def] at this ⊢
        simpa [Bound.subst, Ty.lift_inst hk] using this)
    rw [show (σ.subst θ).tr.ty.subst (inst (ts.map (·.subst θ))) =
        dicts ((σ.q.subst (inst ts)).subst θ) ((σ.ctx.map (·.subst (inst ts))).map (·.subst θ))
          (.mono ((σ.ty.subst (inst ts)).subst θ)) by
      simp only [Scheme.tr, Scheme.subst, TTy.subst_dicts, TTy.subst, List.map_map,
        Function.comp_def, Cn.lift_inst hk, Ty.lift_inst hk]] at hv
    rw [Tm.dsubst_dapps]
    exact tm_dapps X hv (ent_close X hρ he)
  | @meth Γ D m C σ c₀ rest ts d₀ ds φ hm hc _ _ hb h₀ he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    rw [Tm.dsubst_dapps]
    cases ent_close X (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (φ := φ.subst θ) hρ h₀ with
    | cons r _ =>
      simp only [Cn.subst_comp] at r
      have hv := X.meth hm hc (by rw [← Bound.subst_comp]; exact hb.subst θ) r
      rw [Ty.subst_comp]
      refine tm_dapps X hv ?_
      simpa [List.map_map, Function.comp_def, Cn.subst_comp] using ent_close X hρ he
  | @const Γ D c σ ts ds φ hc hk hb he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    have hv := X.const (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (Δ := Δl.map (·.subst θ))
      (φ := φ.subst θ) (ts := ts.map (·.subst θ)) hc (by simpa using hk)
      (by rw [const_bd_inst hP hc hk θ]; exact hb.subst θ)
    rw [const_inst hP hc hk θ] at hv
    rw [Tm.dsubst_dapps]
    exact tm_dapps X hv (ent_close X hρ he)
  | @con Γ D K c ts ds φ hK hk he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    have hv := X.con (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (Δ := Δl.map (·.subst θ))
      (φ := φ.subst θ) (ts := ts.map (·.subst θ)) hK (by simpa using hk)
    rw [ctor_inst hC hK hk θ] at hv
    rw [Tm.dsubst_dapps]
    exact tm_dapps X hv (ent_close X hρ he)
  | @caseK Γ D e K n b r T us t φ c θa e' b' r' _ hK hT hlen hn hU hb _ ihe _ ihr =>
    intro Δl Δ θ ρ hD hρ hg; subst hD
    have hΓ : (Γ.map (·.subst θ)).map Scheme.tr = (Γ.map Scheme.tr).map (·.subst θ) := by
      simp [Scheme.tr_subst]
    have hb' := T6a X hb
    rw [armS_tr] at hb'
    -- the dictionaries of premises that `Δ` does not have are never read: close them anyway
    let ρ' : Nat → Dict := fun k => if k < Δ.length then ρ k else .inst 0
    have hρ' : Closes P Δ θ ρ' := fun k Q hk => by
      simp only [ρ', if_pos (List.getElem?_eq_some_iff.mp hk).1]; exact hρ k Q hk
    have hcl : ∀ k, (ρ' k).Below 0 := fun k => by
      simp only [ρ']
      split
      · rename_i hk
        obtain ⟨Q, hQ⟩ : ∃ Q, Δ[k]? = some Q := ⟨_, List.getElem?_eq_getElem hk⟩
        simpa using Ent.dbelow (hρ k Q hQ) _ (.head _)
      · trivial
    have h := X.caseK (Γ := Γ.map Scheme.tr) (Δl := Δl) (Δ := Δ) (σ := θ) (ρ := ρ')
      (by rw [← hΓ]; exact ihe rfl hρ hg) hK hT hlen hn hU hb' hρ' hg hcl
      (by rw [← hΓ]; exact ihr rfl hρ hg)
    have e₁ : b'.dsubst ρ (c.ctx.length + Δl.length) = b'.dsubst ρ' (c.ctx.length + Δl.length) :=
      Tm.dsubst_congr (M := Δ.length) (fun k hk => by simp [ρ', hk])
        (by have := hb.dbelow; simpa [armΔ, Nat.add_assoc] using this)
    rw [← hΓ, ← e₁] at h
    exact h
  | caseKDead _ hK hT hlen _ hC' _ ihe ihr =>
    intro _ _ θ _ hD hρ hg
    exact X.caseKDead (σ := θ) (ihe hD hρ hg) hK hT hlen hC' (ihr hD hρ hg)
  | app _ _ hψ ih₁ ih₂ =>
    intro _ _ θ _ hD hρ hg
    exact X.app (ih₁ hD hρ hg) (ih₂ hD hρ hg) (Stage.subst_keeps_or θ hψ)
  | @allVar Γ D x σ ψ hx hc hb hk =>
    intro _ _ θ _ _ _ _
    simp only [Ty.subst, liftTy_eq]
    refine X.allVar (k := σ.k) (t := σ.ty.subst (lift σ.k θ)) ?_ hk
    rw [List.getElem?_map, List.getElem?_map, hx]; simp [Scheme.tr, Scheme.subst, hc, hb, dicts]
  | @varAll Γ D x σ ts k t us φ hx hc hb hts hA hus =>
    intro _ _ θ _ _ _ _
    have hA' := congrArg (Ty.subst θ) hA
    simp only [Ty.subst, liftTy_eq] at hA'
    rw [← Ty.lift_inst hts] at hA'
    rw [← Ty.lift_inst hus]
    refine X.varAll (k₀ := σ.k) (t₀ := σ.ty.subst (lift σ.k θ)) (ts := ts.map (·.subst θ))
      ?_ (by simpa using hts) hA' (by simpa using hus)
    rw [List.getElem?_map, List.getElem?_map, hx]; simp [Scheme.tr, Scheme.subst, hc, hb, dicts]
  | @allLam Γ D k b t ψ b' _ ih =>
    intro Δl Δ θ ρ hD hρ hg; subst hD
    have hΔ : ∀ (j : Nat) (Q : Cn), (Δ.map (·.subst (up k)))[j]? = some Q →
        ∃ Q₀ : Cn, Δ[j]? = some Q₀ ∧ Q.subst (lift k θ) = Q₀.subst θ := fun j Q hj => by
      rw [List.getElem?_map] at hj
      cases hQ : Δ[j]? with
      | none => rw [hQ] at hj; cases hj
      | some Q₀ =>
        rw [hQ] at hj; cases hj
        exact ⟨Q₀, rfl, by simp only [Cn.up_lift, Cn.subst_ground (hg j Q₀ hQ)]⟩
    have h₁ := ih (Δl := Δl.map (·.subst (up k))) (Δ := Δ.map (·.subst (up k))) (θ := lift k θ)
      (ρ := ρ) (by simp) (fun j Q hj => let ⟨Q₀, h₀, e⟩ := hΔ j Q hj; e ▸ hρ j Q₀ h₀)
      (fun j Q hj => let ⟨Q₀, h₀, e⟩ := hΔ j Q hj; e ▸ hg j Q₀ h₀)
    have hΔl : (Δl.map (·.subst (up k))).map (·.subst (lift k θ)) =
        (Δl.map (·.subst θ)).map (·.subst (up k)) := by
      simp [List.map_map, Function.comp_def, Cn.up_lift]
    have hΓ : ((Γ.map (·.subst (up k))).map (·.subst (lift k θ))).map Scheme.tr =
        ((Γ.map (·.subst θ)).map Scheme.tr).map (·.subst (up k)) := by
      simp [List.map_map, Function.comp_def, Scheme.up_lift, Scheme.tr_subst]
    rw [hΔl, hΓ] at h₁
    simp only [Ty.subst, liftTy_eq]
    simpa using X.allLam h₁
  | ite _ _ _ ihc iha ihb =>
    intro _ _ _ _ hD hρ hg; exact X.ite (ihc hD hρ hg) (iha hD hρ hg) (ihb hD hρ hg)
  | caseOpt _ _ _ ihe ihn ihs =>
    intro _ _ _ _ hD hρ hg
    have hs := ihs hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at hs
    exact X.caseOpt (ihe hD hρ hg) (ihn hD hρ hg) hs
  | lam _ ih =>
    intro Δl Δ θ ρ hD hρ hg
    have hb' := ih hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at hb'
    exact X.lam hb'
  | let_ _ _ ih₁ ih₂ =>
    intro _ _ _ _ hD hρ hg
    have h₂ := ih₂ hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at h₂
    refine X.let_ (k := 0) (bd := []) ?_ (fun h => absurd rfl h) (.inr rfl) h₂
    simpa [TSch.subst_up_zero, Cn.subst_up_zero] using ih₁ hD hρ hg
  | @letSig Γ D σ e b t φ₁ φ e' b' he hp hφ _ ih₁ ih₂ =>
    intro Δl Δ θ ρ hD hρ hg; subst hD
    have hD' : σ.ctx ++ (Δl ++ Δ).map (·.subst (up σ.k)) =
        (σ.ctx ++ Δl.map (·.subst (up σ.k))) ++ Δ.map (·.subst (up σ.k)) := by simp
    have hΔ : ∀ (j : Nat) (Q : Cn), (Δ.map (·.subst (up σ.k)))[j]? = some Q →
        ∃ Q₀ : Cn, Δ[j]? = some Q₀ ∧ Q.subst (lift σ.k θ) = Q₀.subst θ := fun j Q hj => by
      rw [List.getElem?_map] at hj
      cases hQ : Δ[j]? with
      | none => rw [hQ] at hj; cases hj
      | some Q₀ =>
        rw [hQ] at hj; cases hj
        exact ⟨Q₀, rfl, by simp only [Cn.up_lift, Cn.subst_ground (hg j Q₀ hQ)]⟩
    have h₁ := ih₁ hD' (θ := lift σ.k θ) (ρ := ρ)
      (fun j Q hj => let ⟨Q₀, h₀, e⟩ := hΔ j Q hj; e ▸ hρ j Q₀ h₀)
      (fun j Q hj => let ⟨Q₀, h₀, e⟩ := hΔ j Q hj; e ▸ hg j Q₀ h₀)
    rw [List.map_append] at h₁
    have hd := X.lamD (q := σ.q.subst (lift σ.k θ)) h₁
      (fun h => by rw [hp (.inl fun e => h (by simp [e]))]; rfl)
    have hΔl : (Δl.map (·.subst (up σ.k))).map (·.subst (lift σ.k θ)) =
        (Δl.map (·.subst θ)).map (·.subst (up σ.k)) := by
      simp [List.map_map, Function.comp_def, Cn.up_lift]
    have hΓ : ((Γ.map (·.subst (up σ.k))).map (·.subst (lift σ.k θ))).map Scheme.tr =
        ((Γ.map (·.subst θ)).map Scheme.tr).map (·.subst (up σ.k)) := by
      simp [List.map_map, Function.comp_def, Scheme.up_lift, Scheme.tr_subst]
    rw [hΔl, hΓ, List.length_map, List.length_append, List.length_map] at hd
    have hφ' : φ₁.subst (lift σ.k θ) = .P ∨ φ₁.subst (lift σ.k θ) = φ.subst θ := by
      by_cases hk : σ.k = 0
      · rw [hk, lift_zero]; exact Stage.subst_or θ hφ
      · exact .inl (by rw [hp (.inr hk)]; rfl)
    exact X.let_ hd (fun h => by rw [hp (.inr h)]; rfl) hφ' (ih₂ rfl hρ hg)

/-- A skeleton of `n` holes, all outside the abstractions over dictionaries; its own
dictionaries name only the premises bound inside it (`L` of them around). -/
def Tm.Skel (n : Nat) : Nat → Tm → Prop
  | L, .hole i => L = 0 ∧ i < n
  | L, .dict d => d.Below L
  | L, .meth e _ => e.Skel n L
  | L, .app f a => f.Skel n L ∧ a.Skel n L
  | L, .lam b => b.Skel n L
  | L, .let_ e b => e.Skel n L ∧ b.Skel n L
  | L, .lamD l b => b.Skel n (l + L)
  | L, .ite c a b => c.Skel n L ∧ a.Skel n L ∧ b.Skel n L
  | L, .caseOpt e m s => e.Skel n L ∧ m.Skel n L ∧ s.Skel n L
  | L, .caseK e _ _ m b r => e.Skel n L ∧ b.Skel n (m + L) ∧ r.Skel n L
  | _, .var _ | _, .const _ | _, .con _ => True

/-- **Closing a filling is filling with the closed dictionaries.** -/
theorem fill_dsubst {ds : List Dict} {ρ : Nat → Dict} : ∀ {s : Tm} {L : Nat},
    s.Skel ds.length L → (s.fill ds).dsubst ρ L = s.fill (ds.map (·.subst ρ))
  | .hole i, L, ⟨hL, hi⟩ => by
    subst hL
    have : dsub 0 ρ = ρ := by funext k; simp [dsub]
    simp [Tm.fill, Tm.dsubst, this, args, hi]
  | .dict d, _, h => by simp [Tm.fill, Tm.dsubst, Dict.subst_below h]
  | .var _, _, _ | .const _, _, _ | .con _, _, _ => rfl
  | .meth e _, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := e) h]
  | .lam b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := b) h]
  | .lamD _ b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := b) h]
  | .app f a, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2]
  | .let_ e b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2]
  | .ite c a b, _, h => by
    simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2.1, fill_dsubst h.2.2]
  | .caseOpt e m s, _, h => by
    simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2.1, fill_dsubst h.2.2]
  | .caseK e _ _ _ b r, _, h => by
    simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2.1, fill_dsubst h.2.2]

theorem dsub_zero (ρ : Nat → Dict) : dsub 0 ρ = ρ := by funext k; simp [dsub]

theorem All2.at {α β : Type} {R : α → β → Prop} : ∀ {l : List α} {l' : List β}, All2 R l l' →
    ∀ (i : Nat) (a : α), l[i]? = some a → ∃ b, l'[i]? = some b ∧ R a b
  | _, _, .nil, _, _, h => by cases h
  | _, _, .cons r _, 0, _, h => by cases h; exact ⟨_, rfl, r⟩
  | _, _, .cons _ t, i + 1, a, h => All2.at t i a h

/-- The closed dictionaries of a derivation are typed in the empty context. -/
theorem closed_typed {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ)
    {qs : List Cn} {ds : List Dict} (h : Ent P Δ qs ds) :
    ∀ (i : Nat) (d : Dict), (ds.map (·.subst ρ))[i]? = some d → ∃ q, X.Has [] [] (.dict d) (.dict q) .P := by
  have := ent_close X (Γ := []) (Δl := []) (φ := .P) hρ h
  simp only [List.length_nil, dsub_zero, List.map_nil] at this
  intro i d hd
  obtain ⟨q, -, hq⟩ := this.at i d hd
  exact ⟨q, hq⟩

/-- **T6(b)** for the skeletons of a target: the hypothesis `Sim` is `Target.sim`. -/
theorem T6b_target {lt : Cn → Cn → Prop} (h : Coherent P lt) (s : Tm) {t : TTy} {φ : Stage}
    {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ) {qs : List Cn}
    {ds₁ ds₂ : List Dict} (h₁ : Ent P Δ qs ds₁) (h₂ : Ent P Δ qs ds₂)
    (o₁ : X.Has [] [] (s.fill (ds₁.map (·.subst ρ))) t φ)
    (o₂ : X.Has [] [] (s.fill (ds₂.map (·.subst ρ))) t φ) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      X.exprs.run (s, t, φ) (ds₁.map (·.subst ρ)) = X.exprs.run (s, t, φ) (ds₂.map (·.subst ρ)) :=
  T6b h (X.sim) (s, t, φ) hρ h₁ h₂ ⟨o₁, closed_typed X hρ h₁⟩ ⟨o₂, closed_typed X hρ h₂⟩

end Flick.Elab
