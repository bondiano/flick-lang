import Formal.Elab.CoreArm
import Formal.Elab.CoreDSubst
import Formal.Elab.CoreDict

/-!
# λFlick⁺ as a `Target`: the law of an arm on a constructor

The arm is typed under the most general unifier `θ` of the open problem, before the premises
`Δ` are closed by the types `σ` and the dictionaries `ρ`. In the core, each unifier `θ''` of
the closed problem, after `σ` lifted past the variables of the constructor, is a unifier `κ`
of the open one (`cunif_of`), so `⟦θ⟧` followed by `κ` (`Unify.most`). The body, typed under
`⟦θ⟧` in the core context of the open arm (`canon`) with no store and no facts, is typed under
`κ` (T1), in the store and with the facts of the arm (`weakenStore`, `cut`); then its variables
go to those of the arm and its closed premises to their dictionaries (`Typ.subst`), which is
the translation of the closed body (`Tr.dsubst`).
-/

namespace Flick.Elab

variable {P : Prog} {S : Sig} {N : Names}

/-- A substitution of variables by variables of the same scheme, or by closed terms of the
scheme without run-time sites. -/
theorem SubOk.of_get {Ψ Θ Γ₁ Γ₂} {τ : Nat → Flick.Tm}
    (h : ∀ i s, Γ₁[i]? = some s → (∃ j, τ i = .var j ∧ Γ₂[j]? = some s) ∨
      ((∀ x, ¬ (τ i).Free x) ∧ (∀ a, ¬ (τ i).Has a) ∧
        TypM S .live (Ψ.map (Flick.Ty.up s.k)) (Θ.at s) (Γ₂.map (Flick.Scheme.up s.k)) (τ i)
          (.ex s.t .pure) ∧ s.k = 0 ∧ ∃ c u, s.t = .dict c u)) :
    SubOkM S .live Ψ Θ Γ₁ Γ₂ τ := by
  intro i s hi
  rcases h i s hi with ⟨j, hj, hs⟩ | ⟨hf, hn, ht, hd⟩
  · rw [hj]
    exact ⟨TypM.var_self hs, fun h => False.elim h, trivial, .inr (.inl ⟨_, rfl, hs⟩)⟩
  · exact ⟨ht, hn .ins, trivial, .inr (.inr (.inr hd))⟩

/-- The core context of `Γ` and `Δ`, the premises first. -/
def canonC (Γ : List TSch) (Δ : List Cn) : List Flick.Scheme :=
  Δ.map (fun q => Flick.Scheme.mono (trD N q)) ++ Γ.map (trS N)

/-- Its variables; the variables and the premises of nothing go apart. -/
def cγ (Γ : List TSch) (Δ : List Cn) (x : Nat) : Nat :=
  if x < Γ.length then Δ.length + x else Δ.length + Γ.length + 2 * x

def cδ (Γ : List TSch) (Δ : List Cn) (k : Nat) : Nat :=
  if k < Δ.length then k else Δ.length + Γ.length + 2 * k + 1

theorem Corr.canon (Γ : List TSch) (Δ : List Cn) :
    Corr N (canonC (N := N) Γ Δ) Γ Δ (cγ Γ Δ) (cδ Γ Δ) where
  var x := by
    unfold cγ
    split
    · rename_i hx
      rw [canonC, List.getElem?_append_right (by simp), List.getElem?_map]
      simp
    · rename_i hx
      have h₁ : (canonC (N := N) Γ Δ)[Δ.length + Γ.length + 2 * x]? = none :=
        List.getElem?_eq_none (by simp only [canonC, List.length_append, List.length_map]; omega)
      have h₂ : Γ[x]? = none := List.getElem?_eq_none (by omega)
      rw [h₁, h₂]; rfl
  dict k := by
    unfold cδ
    split
    · rename_i hk
      rw [canonC, List.getElem?_append_left (by simpa), List.getElem?_map]
    · rename_i hk
      have h₁ : (canonC (N := N) Γ Δ)[Δ.length + Γ.length + 2 * k + 1]? = none :=
        List.getElem?_eq_none (by simp only [canonC, List.length_append, List.length_map]; omega)
      have h₂ : Δ[k]? = none := List.getElem?_eq_none (by omega)
      rw [h₁, h₂]; rfl

theorem clo_core (hR : Real P S N) {Γ Δ φ d Q} (h : Clo P Δ d Q) :
    Has S N Γ Δ (.dict d) (.dict Q) φ := by
  induction h with
  | given hk => exact has_dvar hk
  | super _ hs ih => exact has_slot hR ih hs

theorem dapps_core {Γ Δ φ f ds q qs t} (hf : Has S N Γ Δ (.dict f) (dicts q qs t) φ)
    (h : All2 (fun d q => Has S N Γ Δ (.dict d) (.dict q) φ) ds qs) :
    Has S N Γ Δ (.dict (apps f ds)) t φ := by
  induction h generalizing f with
  | nil => exact hf
  | cons hd _ ih => exact ih (has_dap hf hd)

/-- Derived dictionaries are typed in the core. -/
theorem ent_core (hR : Real P S N) {Γ Δ φ qs ds} (h : Ent P Δ qs ds) :
    All2 (fun d q => Has S N Γ Δ (.dict d) (.dict q) φ) ds qs := by
  induction h with
  | nil => exact .nil
  | cons _ _ ih₁ ih₂ => cases ih₁ with | cons r _ => exact .cons r ih₂
  | given hc => exact .cons (clo_core hR hc) .nil
  | @prim Q _ h₁ h₂ =>
    have := has_prim (Γ := Γ) (Δ := Δ) (φ := φ) (θ := .var) hR h₁ h₂
    rw [show Q.subst .var = Q by simp [Cn.subst, Ty.subst_id]] at this
    exact .cons this .nil
  | strct h₁ h₂ _ ih =>
    have := has_strct (Γ := Γ) (Δ := Δ) (φ := φ) (θ := .var) hR h₁ h₂
    simp only [Ty.subst_id] at this
    exact .cons (dapps_core this ih) .nil
  | @inst Q I ins σ ds h₁ h₂ h₃ _ ih =>
    have hq : Q = ⟨ins.cls, ins.head.map (·.subst σ)⟩ := by cases Q; simp_all
    subst hq
    exact .cons (dapps_core (has_inst hR h₁) ih) .nil

theorem trDt_noFree {δ : Nat → Nat} : ∀ {d : Dict}, d.Below 0 → ∀ x, ¬ (trDt N δ d).Free x
  | .var k, h => absurd h (Nat.not_lt_zero k)
  | .slot d _, h => fun x hx => trDt_noFree (d := d) h x hx
  | .ap d e, h => fun x hx => hx.elim (trDt_noFree (d := d) h.1 x) (trDt_noFree (d := e) h.2 x)
  | .inst _, _ | .strct _ _, _ | .prim _ _, _ => fun _ hx => hx

/-- A closed dictionary of `Q`, at an instance of `⟦Q⟧`, in any context of the core. -/
theorem closed_dict (hR : Real P S N) {Q : Cn} {d : Dict} (h : Ent P [] [Q] [d])
    (μ : Atom → Flick.Ty) (Ψ : List Flick.Ty) (Θ : Facts) (Γc : List Flick.Scheme) :
    TypM S .live Ψ Θ Γc (trDt N (fun _ => 0) d) (.ex ((trD N Q).substT μ) .pure) := by
  have hb : d.Below 0 := by simpa using Ent.dbelow h d (.head _)
  cases ent_core (Γ := []) (φ := .P) hR h with
  | cons hd _ =>
    obtain ⟨d', hd', td⟩ := hd [] Facts.none [] (fun x => x) (fun k => k)
      ⟨fun _ => rfl, fun _ => rfl⟩
    cases hd' with
    | dict hd' =>
      rw [hd'.eq, trDt_below (δ' := fun _ => 0) hb] at td
      have hns : ∀ s, ¬ (trDt N (fun _ => 0) d).Has s := fun s => (TrD.total (N := N) _ d).has N s
      have t₁ := td.substT μ ⟨fun p hp => (hns _ hp).elim,
        fun hi => (hns _ hi).elim, fun hi => (hns _ hi).elim, fun hi => (hns _ hi).elim⟩
      simp only [List.map_nil, Facts.none_substT, Flick.J.substT, trTT, trStage_P] at t₁
      have t₂ := (t₁.weakenStore Ψ).cut (Θ₂ := Θ) (Entails.none Θ)
      simp only [List.nil_append] at t₂
      have t₃ := t₂.rename (Γ' := Γc) (ξ := id) fun _ _ h => by simp at h
      rwa [Flick.Tm.rename_fix _ _ fun x hx => absurd hx (trDt_noFree hb x)] at t₃

end Flick.Elab
