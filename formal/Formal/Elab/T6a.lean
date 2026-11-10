import Formal.Elab.Target

/-!
# Elaboration: T6(a) (elaboration.md §4.2)

`ent_typed`: a derived dictionary has its dictionary type, `Δ ⊩ Q ⇝ d ⇒ ⟦Δ⟧ ⊢⁺ d : D_Q`.
`T6a`: an elaboration has the translated type, by induction on `Elab`. Elaboration adds no
term variables (`Elab.free`).
-/

namespace Flick.Elab

variable {P : Prog} (X : Target P)

theorem TTy.subst_dicts (σ : Nat → Ty) (q : Ty) (qs : List Cn) (t : TTy) :
    (dicts q qs t).subst σ = dicts (q.subst σ) (qs.map (·.subst σ)) (t.subst σ) := by
  induction qs with
  | nil => rfl
  | cons q qs ih =>
    simp only [dicts, List.foldr_cons, List.map_cons] at ih ⊢; simp [TTy.subst, Stage.subst, ih]

theorem Scheme.tr_subst (σ : Nat → Ty) (s : Scheme) : (s.subst σ).tr = s.tr.subst σ := by
  simp [Scheme.tr, Scheme.subst, TSch.subst, TTy.subst_dicts, TTy.subst]

theorem lift_up_zero (k : Nat) : lift k (up 0) = .var := by
  funext n
  simp only [lift, up, Nat.add_zero, Ty.subst]
  split <;> first | rfl | (congr 1; omega)

theorem Stage.subst_id (φ : Stage) : φ.subst .var = φ := by
  cases φ <;> simp [Stage.subst, Ty.subst_id]

theorem TTy.subst_id (t : TTy) : t.subst .var = t := by
  induction t with
  | mono t => simp [TTy.subst, Ty.subst_id]
  | arr φ a b q iha ihb => simp [TTy.subst, iha, ihb, Ty.subst_id, Stage.subst_id]
  | dict q => simp [TTy.subst, Cn.subst, Ty.subst_id]

theorem up_zero : up 0 = .var := by funext n; simp [up]

theorem TSch.subst_up_zero (s : TSch) : s.subst (up 0) = s := by
  simp [TSch.subst, lift_up_zero, TTy.subst_id]

theorem Cn.subst_up_zero (q : Cn) : q.subst (up 0) = q := by
  simp [Cn.subst, up_zero, Ty.subst_id]

/-- The dictionaries `d̄` have the types `D_Q̄`. -/
abbrev DTy (Γ : List TSch) (Δ : List Cn) (φ : Stage) : List Dict → List Cn → Prop :=
  All2 fun d q => X.Has Γ Δ (.dict d) (.dict q) φ

theorem dict_apps {Γ Δ φ f ds q qs t} (hf : X.Has Γ Δ (.dict f) (dicts q qs t) φ)
    (h : DTy X Γ Δ φ ds qs) : X.Has Γ Δ (.dict (apps f ds)) t φ := by
  induction h generalizing f with
  | nil => exact hf
  | cons hd _ ih => exact ih (X.dap hf hd)

theorem tm_dapps {Γ Δ φ f ds q qs t} (hf : X.Has Γ Δ f (dicts q qs t) φ)
    (h : DTy X Γ Δ φ ds qs) : X.Has Γ Δ (f.dapps ds) t φ := by
  induction h generalizing f with
  | nil => exact hf
  | cons hd _ ih => exact ih (X.dapp hf hd)

theorem clo_typed {Γ Δ φ d Q} (h : Clo P Δ d Q) : X.Has Γ Δ (.dict d) (.dict Q) φ := by
  induction h with
  | given hk => exact X.dvar hk
  | super _ hs ih => exact X.slot ih hs

/-- **Dictionaries are typed**: `Δ ⊩ Q̄ ⇝ d̄ ⇒ ⟦Δ⟧ ⊢⁺ d̄ : D_Q̄`. -/
theorem ent_typed {Γ Δ φ qs ds} (h : Ent P Δ qs ds) : DTy X Γ Δ φ ds qs := by
  induction h with
  | nil => exact .nil
  | cons _ _ ih₁ ih₂ => cases ih₁ with | cons r _ => exact .cons r ih₂
  | given hc => exact .cons (clo_typed X hc) .nil
  | @prim Q _ h₁ h₂ =>
    have := X.prim (Γ := Γ) (Δ := Δ) (φ := φ) (θ := .var) h₁ h₂
    rw [show Q.subst .var = Q by simp [Cn.subst, Ty.subst_id]] at this
    exact .cons this .nil
  | strct h₁ h₂ _ ih =>
    have := X.strct (Γ := Γ) (Δ := Δ) (φ := φ) (θ := .var) h₁ h₂
    simp only [Ty.subst_id] at this
    exact .cons (dict_apps X this ih) .nil
  | @inst Q I ins σ ds h₁ h₂ h₃ _ ih =>
    have hq : Q = ⟨ins.cls, ins.head.map (·.subst σ)⟩ := by cases Q; simp_all
    subst hq
    exact .cons (dict_apps X (X.inst h₁) ih) .nil

theorem Tm.free_dapps {f : Tm} {ds : List Dict} {x : Nat} (h : (f.dapps ds).Free x) : f.Free x := by
  induction ds generalizing f with
  | nil => exact h
  | cons d ds ih => simpa [Tm.Free] using ih h

/-- **Elaboration adds no term variables.** -/
theorem Elab.free {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') {x : Nat} (hx : e'.Free x) :
    e.Free x := by
  induction h generalizing x with
  | var => have := Tm.free_dapps hx; simpa [Tm.Free, Exp.Free] using this
  | meth => have := Tm.free_dapps hx; simp [Tm.Free] at this
  | const => have := Tm.free_dapps hx; simp [Tm.Free] at this
  | app _ _ _ ih₁ ih₂ => exact hx.imp ih₁ ih₂
  | lam _ ih | allLam _ ih => exact ih hx
  | allVar | varAll => simpa [Tm.Free, Exp.Free] using hx
  | let_ _ _ ih₁ ih₂ => exact hx.imp ih₁ ih₂
  | letSig _ _ _ _ ih₁ ih₂ => exact hx.imp ih₁ ih₂
  | ite _ _ _ ihc iha ihb => exact hx.imp ihc (Or.imp iha ihb)
  | caseOpt _ _ _ ihe ihn ihs => exact hx.imp ihe (Or.imp ihn ihs)
  | con => exact False.elim (Tm.free_dapps hx)
  | caseK _ _ _ _ _ _ _ _ ihe ihb ihr => exact hx.imp ihe (Or.imp ihb ihr)
  | caseKDead _ _ _ _ _ _ _ ihe ihr =>
    exact hx.imp ihe (Or.imp (fun h => Nat.noConfusion h) ihr)

theorem apps_below {L : Nat} : ∀ {f : Dict} {ds : List Dict}, f.Below L → (∀ d ∈ ds, d.Below L) →
    (apps f ds).Below L
  | _, [], hf, _ => hf
  | _, d :: _, hf, h => apps_below (f := .ap _ d) ⟨hf, h d (.head _)⟩ fun d' hd => h d' (.tail _ hd)

theorem Clo.dbelow {Δ d Q} (h : Clo P Δ d Q) : d.Below Δ.length := by
  induction h with
  | given hk => exact (List.getElem?_eq_some_iff.mp hk).1
  | super _ _ ih => exact ih

/-- A derived dictionary names only premises of `Δ`. -/
theorem Ent.dbelow {Δ qs ds} (h : Ent P Δ qs ds) : ∀ d ∈ ds, d.Below Δ.length := by
  induction h with
  | nil => intro d hd; cases hd
  | cons _ _ ih₁ ih₂ =>
    intro d hd
    rcases List.mem_cons.mp hd with rfl | hd
    · exact ih₁ _ (.head _)
    · exact ih₂ _ hd
  | given hc => intro d hd; simp only [List.mem_singleton] at hd; subst hd; exact hc.dbelow
  | prim => intro d hd; simp only [List.mem_singleton] at hd; subst hd; trivial
  | strct _ _ _ ih => intro d hd; simp only [List.mem_singleton] at hd; subst hd; exact apps_below trivial ih
  | inst _ _ _ _ ih => intro d hd; simp only [List.mem_singleton] at hd; subst hd; exact apps_below trivial ih

theorem Tm.dapps_DB {L : Nat} : ∀ {f : Tm} {ds : List Dict}, f.DB L → (∀ d ∈ ds, d.Below L) →
    (f.dapps ds).DB L
  | _, [], hf, _ => hf
  | _, d :: _, hf, h =>
    Tm.dapps_DB (f := .app _ (.dict d)) ⟨hf, h d (.head _)⟩ fun d' hd => h d' (.tail _ hd)

/-- **An elaboration names only premises of `Δ`.** -/
theorem Elab.dbelow {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') : e'.DB Δ.length := by
  induction h with
  | var _ _ he => exact Tm.dapps_DB trivial he.dbelow
  | meth _ _ _ _ h₀ he => exact Tm.dapps_DB (h₀.dbelow _ (.head _)) he.dbelow
  | const _ _ he => exact Tm.dapps_DB trivial he.dbelow
  | con _ _ he => exact Tm.dapps_DB trivial he.dbelow
  | app _ _ _ ih₁ ih₂ => exact ⟨ih₁, ih₂⟩
  | lam _ ih => exact ih
  | allLam _ ih => simpa using ih
  | allVar | varAll => trivial
  | let_ _ _ ih₁ ih₂ => exact ⟨ih₁, ih₂⟩
  | letSig _ _ _ _ ih₁ ih₂ => exact ⟨by simpa using ih₁, ih₂⟩
  | ite _ _ _ ih₁ ih₂ ih₃ => exact ⟨ih₁, ih₂, ih₃⟩
  | caseOpt _ _ _ ih₁ ih₂ ih₃ => exact ⟨ih₁, ih₂, ih₃⟩
  | caseK _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ => exact ⟨ih₁, by simpa [armΔ] using ih₂, ih₃⟩
  | caseKDead _ _ _ _ _ _ _ ih₁ ih₂ => exact ⟨ih₁, trivial, ih₂⟩

theorem armS_tr (c : Ctor) (θ : Nat → Ty) (Γ : List Scheme) :
    (armS c θ Γ).map Scheme.tr = armT c θ (Γ.map Scheme.tr) := by
  simp only [armS, armT, List.map_append, List.map_map, Function.comp_def, Scheme.tr_subst]
  rfl

/-- **T6(a) (types are preserved).** `Γ; Δ ⊢ e : τ ! φ ⇝ e'` gives
`⟦Γ⟧, ⟦Δ⟧ ⊢⁺ e' : τ ! φ` in any target. -/
theorem T6a {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') :
    X.Has (Γ.map Scheme.tr) Δ e' (.mono τ) φ := by
  induction h with
  | @var Γ Δ x σ ts ds φ hx hk he =>
    have hv := X.var (Γ := Γ.map Scheme.tr) (Δ := Δ) (φ := φ) (ts := ts)
      (by rw [List.getElem?_map, hx]; rfl) hk
    simp only [Scheme.tr, TTy.subst_dicts, TTy.subst] at hv
    exact tm_dapps X hv (ent_typed X he)
  | meth hm hc _ _ h₀ he =>
    cases ent_typed X h₀ with
    | cons r _ => exact tm_dapps X (X.meth hm hc r) (ent_typed X he)
  | @const Γ Δ c σ ts ds φ hc hk he =>
    have hv := X.const (Γ := Γ.map Scheme.tr) (Δ := Δ) (φ := φ) hc hk
    simp only [TTy.subst_dicts, TTy.subst] at hv
    exact tm_dapps X hv (ent_typed X he)
  | @allLam Γ Δ k b t ψ b' _ ih =>
    refine X.allLam ?_
    simpa only [List.map_map, Function.comp_def, Scheme.tr_subst] using ih
  | allVar hx hc hk =>
    refine X.allVar ?_ hk
    rw [List.getElem?_map, hx]; simp [Scheme.tr, hc, dicts]
  | varAll hx hc hts hA hus =>
    refine X.varAll ?_ hts hA hus
    rw [List.getElem?_map, hx]; simp [Scheme.tr, hc, dicts]
  | app _ _ hψ ih₁ ih₂ => exact X.app ih₁ ih₂ hψ
  | ite _ _ _ ihc iha ihb => exact X.ite ihc iha ihb
  | caseOpt _ _ _ ihe ihn ihs => exact X.caseOpt ihe ihn ihs
  | @con Γ Δ K c ts ds φ hK hk he =>
    have hv := X.con (Γ := Γ.map Scheme.tr) (Δ := Δ) (φ := φ) hK hk
    simp only [TTy.subst_dicts, TTy.subst] at hv
    exact tm_dapps X hv (ent_typed X he)
  | @caseK Γ Δ e K n b r T us t φ c θ e' b' r' _ hK hT hlen hn hU hb _ ihe ihb ihr =>
    have hΓ : (Γ.map Scheme.tr).map (·.subst (up 0)) = Γ.map Scheme.tr := by
      simp [TSch.subst_up_zero]
    have hΔ : Δ.map (·.subst (up 0)) = Δ := by simp [Cn.subst_up_zero]
    have hτ : ∀ τ : Ty, τ.subst (up 0) = τ := fun τ => by rw [up_zero, Ty.subst_id]
    have hφ : φ.subst (up 0) = φ := by rw [up_zero, Stage.subst_id]
    have h := X.caseK (Γ := Γ.map Scheme.tr) (Δl := Δ) (Δ := []) (σ := up 0)
      (ρ := fun _ => .inst 0) (e := e') (b := b') (r := r') (t := t) (φ := φ) (c := c) (θ := θ)
      (T := T) (us := us) (K := K) (n := n) (by rw [hΓ, hΔ, hτ, hφ]; exact ihe) hK hT hlen hn hU
      (by rw [List.append_nil, ← armS_tr]; exact ihb) (fun _ _ h => by simp at h)
      (fun _ _ h => by simp at h) (fun _ => trivial) (by rw [hΓ, hΔ, hτ, hφ]; exact ihr)
    rw [hΓ, hΔ, hτ, hφ, Tm.dsubst_of_DB (by simpa [armΔ] using hb.dbelow)] at h
    exact h
  | caseKDead _ hK hT hlen _ hC _ ihe ihr =>
    exact X.caseKDead (σ := .var) (by rw [Ty.subst_id]; exact ihe) hK hT hlen hC ihr
  | lam _ ih => exact X.lam ih
  | let_ _ _ ih₁ ih₂ =>
    refine X.let_ (k := 0) ?_ (fun h => absurd rfl h) (.inr rfl) ih₂
    simpa [TSch.subst_up_zero, Cn.subst_up_zero] using ih₁
  | @letSig Γ Δ σ e b t φ₁ φ e' b' he hp hφ _ ih₁ ih₂ =>
    have hd := X.lamD (q := σ.q) ih₁ (fun h => hp (.inl h))
    rw [List.map_map] at hd
    simp only [Function.comp_def, Scheme.tr_subst] at hd
    have hΓ : Γ.map (fun x => x.tr.subst (up σ.k)) = (Γ.map Scheme.tr).map (·.subst (up σ.k)) := by
      simp
    rw [hΓ] at hd
    exact X.let_ hd (fun h => hp (.inr h)) hφ ih₂

end Flick.Elab
