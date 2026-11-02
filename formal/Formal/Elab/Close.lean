import Formal.Elab.CloseTy
import Formal.Elab.Run
import Formal.Elab.SugarBase

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
def dsub (L : Nat) (ρ : Nat → Dict) : Nat → Dict := fun k => if k < L then .var k else ρ (k - L)

/-- `e[ρ]` for the premises from `L` on. -/
def Tm.dsubst (ρ : Nat → Dict) : Nat → Tm → Tm
  | _, .var x => .var x
  | _, .hole i => .hole i
  | L, .dict d => .dict (d.subst (dsub L ρ))
  | L, .meth e m => .meth (e.dsubst ρ L) m
  | _, .const c => .const c
  | L, .app f a => .app (f.dsubst ρ L) (a.dsubst ρ L)
  | L, .lam b => .lam (b.dsubst ρ L)
  | L, .let_ e b => .let_ (e.dsubst ρ L) (b.dsubst ρ L)
  | L, .lamD l b => .lamD l (b.dsubst ρ (l + L))
  | L, .ite c a b => .ite (c.dsubst ρ L) (a.dsubst ρ L) (b.dsubst ρ L)
  | L, .caseOpt e n s => .caseOpt (e.dsubst ρ L) (n.dsubst ρ L) (s.dsubst ρ L)

theorem Tm.dsubst_dapps (ρ : Nat → Dict) (L : Nat) (f : Tm) (ds : List Dict) :
    (f.dapps ds).dsubst ρ L = (f.dsubst ρ L).dapps (ds.map (·.subst (dsub L ρ))) := by
  induction ds generalizing f with
  | nil => rfl
  | cons d ds ih => exact ih (.app f (.dict d))

theorem Tm.free_dsubst {ρ : Nat → Dict} {e : Tm} {L x : Nat} (h : (e.dsubst ρ L).Free x) :
    e.Free x := by
  induction e generalizing L x with
  | var | hole | dict | const => exact h
  | meth _ _ ih | lam _ ih | lamD _ _ ih => exact ih h
  | app _ _ ih₁ ih₂ | let_ _ _ ih₁ ih₂ => exact h.imp ih₁ ih₂
  | ite _ _ _ ih₁ ih₂ ih₃ | caseOpt _ _ _ ih₁ ih₂ ih₃ => exact h.imp ih₁ (Or.imp ih₂ ih₃)

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

/-- **T6(a) with the premises closed.** `Γ; Δl, Δ ⊢ e : τ ! φ ⇝ e'` and `Δ` closed by `θ`, `ρ`
to ground constraints give `⟦Γθ⟧, ⟦Δl θ⟧ ⊢⁺ e'[ρ] : τθ ! φ` in any target. -/
theorem T6a_close (hP : P.ConstsClosed) {Γ D e τ φ e'} (h : Elab P Γ D e τ φ e') :
    ∀ {Δl Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}, D = Δl ++ Δ → Closes P Δ θ ρ →
      (∀ (k : Nat) (Q : Cn), Δ[k]? = some Q → (Q.subst θ).Ground) →
      X.Has ((Γ.map (·.subst θ)).map Scheme.tr) (Δl.map (·.subst θ)) (e'.dsubst ρ Δl.length)
        (.mono (τ.subst θ)) φ := by
  induction h with
  | @var Γ D x σ ts ds φ hx hk he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    have hv := X.var (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (Δ := Δl.map (·.subst θ)) (φ := φ)
      (s := (σ.subst θ).tr) (ts := ts.map (·.subst θ))
      (by rw [List.getElem?_map, List.getElem?_map, hx]; rfl) (by simpa [Scheme.subst] using hk)
    rw [show (σ.subst θ).tr.ty.subst (inst (ts.map (·.subst θ))) =
        dicts ((σ.q.subst (inst ts)).subst θ) ((σ.ctx.map (·.subst (inst ts))).map (·.subst θ))
          (.mono ((σ.ty.subst (inst ts)).subst θ)) by
      simp only [Scheme.tr, Scheme.subst, TTy.subst_dicts, TTy.subst, List.map_map,
        Function.comp_def, Cn.lift_inst hk, Ty.lift_inst hk]] at hv
    rw [Tm.dsubst_dapps]
    exact tm_dapps X hv (ent_close X hρ he)
  | @meth Γ D m C σ c₀ rest ts d₀ ds φ hm hc _ _ h₀ he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    rw [Tm.dsubst_dapps]
    cases ent_close X (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (φ := φ) hρ h₀ with
    | cons r _ =>
      simp only [Cn.subst_comp] at r
      have hv := X.meth hm hc r
      rw [Ty.subst_comp]
      refine tm_dapps X hv ?_
      simpa [List.map_map, Function.comp_def, Cn.subst_comp] using ent_close X hρ he
  | @const Γ D c σ ts ds φ hc hk he =>
    intro Δl Δ θ ρ hD hρ _; subst hD
    have hv := X.const (Γ := (Γ.map (·.subst θ)).map Scheme.tr) (Δ := Δl.map (·.subst θ))
      (φ := φ) (ts := ts.map (·.subst θ)) hc (by simpa using hk)
    rw [const_inst hP hc hk θ] at hv
    rw [Tm.dsubst_dapps]
    exact tm_dapps X hv (ent_close X hρ he)
  | app _ _ hψ ih₁ ih₂ => intro _ _ _ _ hD hρ hg; exact X.app (ih₁ hD hρ hg) (ih₂ hD hρ hg) hψ
  | ite _ _ _ ihc iha ihb =>
    intro _ _ _ _ hD hρ hg; exact X.ite (ihc hD hρ hg) (iha hD hρ hg) (ihb hD hρ hg)
  | caseOpt _ _ _ ihe ihn ihs =>
    intro _ _ _ _ hD hρ hg
    have hs := ihs hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at hs
    exact X.caseOpt (ihe hD hρ hg) (ihn hD hρ hg) hs
  | @lam Γ D b a t q φ ψ b' hb hh ih =>
    intro Δl Δ θ ρ hD hρ hg
    have hb' := ih hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at hb'
    refine X.lam hb' fun n hn => ?_
    obtain ⟨m, hm, ho⟩ := heldS_subst (heldT_of (e := .lam b) (e' := .lam (b'.dsubst ρ Δl.length))
      (fun _ hx => hb.free (Tm.free_dsubst (e := b') hx)) hn)
    exact Ty.occurs_subst (hh m hm) ho
  | let_ _ _ ih₁ ih₂ =>
    intro _ _ _ _ hD hρ hg
    have h₂ := ih₂ hD hρ hg
    simp only [List.map_cons, Scheme.subst, lift_zero, List.map_nil, Ty.subst] at h₂
    refine X.let_ (k := 0) ?_ (fun h => absurd rfl h) (.inr rfl) h₂
    simpa [TSch.subst_up_zero, Cn.subst_up_zero] using ih₁ hD hρ hg
  | @letSig Γ D σ e b t φ₁ φ e' b' he hp hφ hq _ ih₁ ih₂ =>
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
      (fun h => hp (.inl fun e => h (by simp [e])))
      fun hc n hn => by
        obtain ⟨m, hm, ho⟩ := heldS_subst (heldT_of (e := e)
          (fun _ hx => he.free (Tm.free_dsubst hx)) hn)
        exact (hq (fun e => hc (by simp [e])) m hm).imp (Ty.occurs_subst · ho)
          (Ty.occurs_subst · ho)
    have hΔl : (Δl.map (·.subst (up σ.k))).map (·.subst (lift σ.k θ)) =
        (Δl.map (·.subst θ)).map (·.subst (up σ.k)) := by
      simp [List.map_map, Function.comp_def, Cn.up_lift]
    have hΓ : ((Γ.map (·.subst (up σ.k))).map (·.subst (lift σ.k θ))).map Scheme.tr =
        ((Γ.map (·.subst θ)).map Scheme.tr).map (·.subst (up σ.k)) := by
      simp [List.map_map, Function.comp_def, Scheme.up_lift, Scheme.tr_subst]
    rw [hΔl, hΓ, List.length_map, List.length_append, List.length_map] at hd
    exact X.let_ hd (fun h => hp (.inr h)) hφ (ih₂ rfl hρ hg)

/-- A dictionary whose premises lie below `L`. -/
def Dict.Below (L : Nat) : Dict → Prop
  | .var k => k < L
  | .slot d _ => d.Below L
  | .ap d e => d.Below L ∧ e.Below L
  | _ => True

theorem Dict.subst_below {L : Nat} {ρ : Nat → Dict} : ∀ {d : Dict}, d.Below L →
    d.subst (dsub L ρ) = d
  | .var k, h => by simp [Dict.subst, dsub, show k < L from h]
  | .slot d i, h => by simp [Dict.subst, Dict.subst_below (d := d) h]
  | .ap d e, h => by simp [Dict.subst, Dict.subst_below h.1, Dict.subst_below h.2]
  | .inst _, _ | .strct _ _, _ | .prim _ _, _ => rfl

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
  | _, .var _ | _, .const _ => True

/-- **Closing a filling is filling with the closed dictionaries.** -/
theorem fill_dsubst {ds : List Dict} {ρ : Nat → Dict} : ∀ {s : Tm} {L : Nat},
    s.Skel ds.length L → (s.fill ds).dsubst ρ L = s.fill (ds.map (·.subst ρ))
  | .hole i, L, ⟨hL, hi⟩ => by
    subst hL
    have : dsub 0 ρ = ρ := by funext k; simp [dsub]
    simp [Tm.fill, Tm.dsubst, this, args, hi]
  | .dict d, _, h => by simp [Tm.fill, Tm.dsubst, Dict.subst_below h]
  | .var _, _, _ | .const _, _, _ => rfl
  | .meth e _, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := e) h]
  | .lam b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := b) h]
  | .lamD _ b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst (s := b) h]
  | .app f a, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2]
  | .let_ e b, _, h => by simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2]
  | .ite c a b, _, h => by
    simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2.1, fill_dsubst h.2.2]
  | .caseOpt e m s, _, h => by
    simp [Tm.fill, Tm.dsubst, fill_dsubst h.1, fill_dsubst h.2.1, fill_dsubst h.2.2]

end Flick.Elab
