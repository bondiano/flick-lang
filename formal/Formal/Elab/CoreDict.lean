import Formal.Elab.CoreLaws
import Formal.Elab.Basic
import Formal.Elab.SugarBase

/-!
# λFlick⁺ as a `Target`: the laws of dictionaries

`lamD l` is `l` nested `λ`s of the core, each over a premise `D_Q` (`lams_typ`); a method and
a superclass slot are `Typ.dproj` on a dictionary record; instances, constants of Γ₀ and the
compiler's dictionaries are `Typ.glob` at their declared schemes (`Real`).
-/

namespace Flick.Elab

variable {P : Prog} {S : Sig} {N : Names}

/-- `λδ₁ … λδₗ. b` over the premises `qs`, each arrow with the qualifier `q`. -/
theorem lams_typ {Ψ Θ} {q : Ty} {T : TTy} {b' : Flick.Tm} (hns : ∀ s, ¬ b'.Has s) :
    ∀ (qs : List Cn) (Γc : List Flick.Scheme), qs ≠ [] →
    Typ S Ψ Θ (dctx N qs ++ Γc) b' (.ex (trTT N T) .pure) →
    ∀ φ, Typ S Ψ Θ Γc (lamsC qs.length b') (.ex (trTT N (dicts q qs T)) φ) := by
  intro qs
  induction qs with
  | nil => intro _ h; exact absurd rfl h
  | cons c qs ih =>
    intro Γc _ hb φ
    have hb' : Typ S Ψ Θ (dctx N qs ++ (Flick.Scheme.mono (trD N c) :: Γc)) b'
        (.ex (trTT N T) .pure) := by
      simpa [dctx, List.append_assoc] using hb
    have inner : Typ S Ψ Θ (Flick.Scheme.mono (trD N c) :: Γc) (lamsC qs.length b')
        (.ex (trTT N (dicts q qs T)) .pure) := by
      cases qs with
      | nil => exact hb'
      | cons c' qs' => exact ih _ (List.cons_ne_nil _ _) hb' .pure
    exact .lam inner (fun h => hns _ (lamsC_has.1 h)) .refl

theorem has_lamD {Γ Δ qs e t q φ} (he : Has S N Γ (qs ++ Δ) e t φ) (hφ : qs ≠ [] → φ = .P) :
    Has S N Γ Δ (.lamD qs.length e) (dicts q qs t) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨b', hb', tb⟩ := he Ψ Θ _ _ _ (hc.lamD N qs)
  refine ⟨_, .lamD hb', ?_⟩
  by_cases hq : qs = []
  · subst hq; exact tb
  · have := hφ hq; subst this
    exact lams_typ (hb'.has N) qs Γc hq tb _

/-- The field of a method at the instance `θ` of its class parameters and own variables. -/
theorem rot_inst (T : Flick.Ty) {p kk : Nat} (θ : Nat → Ty) :
    ((T.substT (rot p kk)).substT (liftT kk (instT (((vars p).map (·.subst θ)).map (trT N))))).substT
        (instT ((List.range kk).map fun j => trT N (θ (j + p)))) =
      T.substT (trσ N fun j => if j < p + kk then θ j else .var (j - (p + kk))) := by
  simp only [Flick.Ty.substT_comp]
  apply Flick.Ty.substT_congr
  intro a
  cases a with
  | r q => rfl
  | v j =>
    simp only [rot]
    by_cases h1 : j < p
    · rw [if_pos h1]
      simp only [Flick.Ty.substT, liftT, if_neg (by omega : ¬ j + kk < kk), Nat.add_sub_cancel]
      rw [Flick.Ty.up_substT _ _ _ (by simp)]
      simp [instT, vars, trσ, Ty.subst, h1, show j < p + kk by omega]
    · rw [if_neg h1]
      by_cases h2 : j < p + kk
      · rw [if_pos h2]
        simp [Flick.Ty.substT, liftT, instT, trσ, h2, show j - p < kk by omega,
          Nat.sub_add_cancel (by omega : p ≤ j)]
      · rw [if_neg h2]
        simp only [Flick.Ty.substT, liftT, if_neg (by omega : ¬ j < kk), instT, List.length_map,
          vars, List.length_range, dif_neg (by omega : ¬ j - kk < p), Flick.Ty.up_def, shiftV,
          dif_neg (by omega : ¬ j - kk - p + kk < kk), trσ, if_neg h2, trT]
        congr 1; omega

/-- `rot_inst` at one atom. -/
theorem rot_inst_atom (a : Atom) {p kk : Nat} (θ : Nat → Ty) :
    ((rot p kk a).substT (liftT kk (instT (((vars p).map (·.subst θ)).map (trT N))))).substT
        (instT ((List.range kk).map fun j => trT N (θ (j + p)))) =
      trσ N (fun j => if j < p + kk then θ j else .var (j - (p + kk))) a := by
  cases a with
  | v n => exact rot_inst (.tvar n) θ
  | r q => exact rot_inst (.rg q) θ

theorem has_meth (hR : Real P S N) {Γ Δ e m C s c₀ rest θ φ} (hm : P.methods m = some (C, s))
    (hc₀ : s.ctx = c₀ :: rest) (hb : Bounds.Hold (s.bd.map (Bound.subst θ)))
    (he : Has S N Γ Δ e (.dict (c₀.subst θ)) φ) :
    Has S N Γ Δ (.meth e m) (dicts .cnil (rest.map (·.subst θ)) (.mono (s.ty.subst θ))) φ := by
  obtain ⟨p, rest', hctx, hp, hcl, hbd, hfs⟩ := hR.meth hm
  rw [hctx] at hc₀; cases hc₀
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := he Ψ Θ Γc γ δ hc
  obtain ⟨fs, hd⟩ := hR.dict C p
  have hlen : ((vars p).map (·.subst θ)).length = p := by simp [vars]
  have te' : Typ S Ψ Θ Γc e'
      (.ex (.dict (N.cls C p) (Flick.Ty.spine (((vars p).map (·.subst θ)).map (trT N)))) (trStage N φ)) := by
    have := te; simp only [trTT, trD, Cn.subst, hlen] at this; exact this
  -- the value bounds of the field hold at the instance: it is `θ` on the scheme's variables
  have hθ : ∀ j, j < s.k → (if j < p + (s.k - p) then θ j else .var (j - (p + (s.k - p)))) = θ j :=
    fun j hj => by rw [if_pos (by omega)]
  have holds : (Flick.Scheme.substT (instT (((vars p).map (·.subst θ)).map (trT N)))
      ⟨s.k - p, (trBd N s.bd).substT (rot p (s.k - p)),
        (trTT N (methTy rest s.ty)).substT (rot p (s.k - p))⟩).Holds Θ
      ((List.range (s.k - p)).map fun j => trT N (θ (j + p))) := by
    rintro C r ⟨C₁, r₁, ⟨C₀, r₀, h₀, rfl, rfl⟩, rfl, rfl⟩
    simp only [Caps.substT_comp, Flick.Ty.substT_comp]
    refine trBd_holds N (fun a => ?_) (by rw [hbd _ θ hθ]; exact hb) Θ C₀ r₀ h₀
    rw [← Flick.Ty.substT_comp]; exact rot_inst_atom a θ
  refine ⟨_, .meth he', .dproj te' hd (by simp [vars]) (hfs fs hd)
    (ts := (List.range (s.k - p)).map fun j => trT N (θ (j + p))) (by simp) holds ?_⟩
  simp only [Flick.Scheme.inst, Flick.Scheme.substT]
  rw [rot_inst, ← trTT_subst, Nat.add_sub_cancel' hp,
    hcl _ θ fun j hj => by simp [hj]]
  simp only [methTy, TTy.subst_dicts, TTy.subst, Ty.subst]
  exact .refl

theorem has_slot (hR : Real P S N) {Γ Δ d q i Sup φ} (hd : Has S N Γ Δ (.dict d) (.dict q) φ)
    (hs : P.super q i = some Sup) : Has S N Γ Δ (.dict (.slot d i)) (.dict Sup) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨d', hd', td⟩ := hd Ψ Θ Γc γ δ hc
  cases hd' with | dict hD =>
  obtain ⟨fs, hfs⟩ := hR.dict q.cls q.args.length
  obtain ⟨Sup', hs', hSup⟩ : ∃ Sup', P.super ⟨q.cls, vars q.args.length⟩ i = some Sup' ∧
      Sup = Sup'.subst (inst q.args) := by
    unfold Prog.super at hs
    cases h : (P.supers q.cls)[i]? with
    | none => rw [h] at hs; cases hs
    | some S₀ =>
      refine ⟨S₀.subst (params (vars q.args.length)), by simp [Prog.super, h], ?_⟩
      have h₂ := Prog.super_subst (P := P) (inst q.args) (Q := ⟨q.cls, vars q.args.length⟩)
        (S := S₀.subst (params (vars q.args.length))) (i := i) (by simp [Prog.super, h])
      have hv := vars_inst (as := q.args) [] rfl
      rw [List.append_nil] at hv
      simp only [Cn.subst, hv] at h₂
      unfold Prog.super at h₂
      rw [hs] at h₂; exact Option.some.inj h₂
  refine ⟨_, .dict (.slot hD), .dproj td hfs (by simp) (hR.slot hfs hs') (ts := []) rfl
    holds_none ?_⟩
  simp only [Flick.Scheme.inst, Flick.Scheme.substT, liftT_zero, substT_instT_nil]
  rw [hSup, trTT, trD_subst]
  rw [Flick.Ty.substT_congr fun a => (trσ_inst N q.args a)]
  exact .refl

/-- A global at a closed scheme, at the instance `σ`. -/
theorem has_glob {Γ Δ D g k} {t : TTy} (hg : S.glob g = some ⟨k, Facts.none, trTT N t⟩)
    (hcl : t.Closed k) (σ : Nat → Ty) {φ} (hD : ∀ γ δ, Tr N γ δ (.dict D) (.glob g)) :
    Has S N Γ Δ (.dict D) (t.subst σ) φ := fun _ _ _ γ δ _ =>
  ⟨_, hD γ δ, .glob hg (ts := (List.range k).map fun j => trT N (σ j)) (by simp)
    (fun _ _ h => (h : False).elim) (by rw [Flick.Scheme.inst, trTT_inst N hcl]; exact .refl)⟩

theorem has_inst (hR : Real P S N) {Γ Δ I ins σ φ} (hI : P.insts I = some ins) :
    Has S N Γ Δ (.dict (.inst I))
      (dicts .cnil (ins.ctx.map (·.subst σ)) (.dict ⟨ins.cls, ins.head.map (·.subst σ)⟩)) φ := by
  obtain ⟨k, hg, hcl⟩ := hR.inst hI
  have := has_glob (Γ := Γ) (Δ := Δ) (φ := φ) hg hcl σ fun _ _ => .dict .inst
  simpa [instTy, TTy.subst_dicts, TTy.subst, Cn.subst, Ty.subst] using this

/-- `struct_C^τ` at an instance `θ` of its closed scheme (`docs/formal/elaboration.md#structural-dictionaries-were-typed-only-at-their-own-record-type`). -/
theorem has_strct (hR : Real P S N) {Γ Δ C τ ts θ φ} (hC : P.structC C = true)
    (hf : P.fields τ = some ts) :
    Has S N Γ Δ (.dict (.strct C τ))
      (dicts .cnil (ts.map fun t => ⟨C, [t.subst θ]⟩) (.dict ⟨C, [τ.subst θ]⟩)) φ := by
  obtain ⟨k, hg, hcl⟩ := hR.strct hC hf
  have := has_glob (Γ := Γ) (Δ := Δ) (φ := φ) hg hcl θ fun _ _ => .dict .strct
  simpa [strctTy, TTy.subst_dicts, TTy.subst, Cn.subst, Ty.subst] using this

theorem has_prim (hR : Real P S N) {Γ Δ q p θ φ} (hq : P.primC q.cls = true)
    (hp : P.prims q = some p) : Has S N Γ Δ (.dict (.prim p q)) (.dict (q.subst θ)) φ :=
  let ⟨_, hg, hcl⟩ := hR.prim hq hp
  has_glob (Γ := Γ) (Δ := Δ) (φ := φ) (t := .dict q) hg hcl θ fun _ _ => .dict .prim

end Flick.Elab
