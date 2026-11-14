import Formal.Shapes
import Formal.SpineMatch
import Formal.Elab.CoreLaws
import Formal.Elab.CoreUnify

/-!
# λFlick⁺ as a `Target`: constructors and arms

A constructor is a constructor of the core, its dictionaries first (`has_con`). An arm on a
constructor is an arm of the core on the spine of its dictionaries and fields, then a last
arm `_`. A dead arm has no unifier in the core either (`has_caseKDead`).
-/

namespace Flick.Elab

variable {P : Prog} {S : Sig} {N : Names}

theorem trT_foldl_app (us : List Ty) (f : Ty) :
    trT N (us.foldl .app f) = (us.map (trT N)).foldl .tapp (trT N f) := by
  induction us generalizing f with
  | nil => rfl
  | cons u us ih => exact ih (.app f u)

theorem trT_tcon {T : Nat} (h2 : T ≠ 2) (h4 : T ≠ 4) (us : List Ty) :
    trT N (Ty.tcon T us) = Flick.Ty.tcon (N.con T) (us.map (trT N)) := by
  rw [Ty.tcon, trT_foldl_app]
  simp [trT, h2, h4, Flick.Ty.tcon]

theorem trT_ctor_ty (c : Ctor) :
    trT N c.ty = Flick.Ty.arrows (c.fields.map (trT N)) (trT N (Ty.tcon c.T c.res)) := by
  simp only [Ctor.ty]
  induction c.fields with
  | nil => rfl
  | cons a as ih => simp only [List.foldr_cons, trT, List.map_cons, Flick.Ty.arrows, ih]; rfl

theorem trTT_dicts_cnil (qs : List Cn) (t : TTy) :
    trTT N (dicts .cnil qs t) = Flick.Ty.arrows (qs.map (trD N)) (trTT N t) := by
  induction qs with
  | nil => rfl
  | cons q qs ih => simp only [dicts, List.foldr_cons, List.map_cons, Flick.Ty.arrows] at ih ⊢; simp [trTT, ih]; rfl

theorem arrows_append (as bs : List Flick.Ty) (r : Flick.Ty) :
    Flick.Ty.arrows (as ++ bs) r = Flick.Ty.arrows as (Flick.Ty.arrows bs r) := by
  induction as with
  | nil => rfl
  | cons a as ih => simp only [List.cons_append, Flick.Ty.arrows, ih]

theorem trTT_ctor (hR : Real P S N) {K : Nat} {c : Ctor} (hK : P.ctors K = some c) :
    trTT N (dicts .cnil c.ctx (.mono c.ty)) =
      Flick.Ty.arrows (c.ctx.map (trD N) ++ c.fields.map (trT N))
        (Flick.Ty.tcon (N.con c.T) (c.res.map (trT N))) := by
  obtain ⟨-, -, h2, h4⟩ := hR.ctor hK
  rw [trTT_dicts_cnil, trTT, trT_ctor_ty, trT_tcon h2 h4, arrows_append]

theorem has_con (hR : Real P S N) {Γ Δ K c ts φ} (hK : P.ctors K = some c) (hk : ts.length = c.k) :
    Has S N Γ Δ (.con K) ((dicts .cnil c.ctx (.mono c.ty)).subst (inst ts)) φ := by
  intro _ _ _ _ _ _
  obtain ⟨hS, hres, -, -⟩ := hR.ctor hK
  refine ⟨_, .con, .ctor hS (us := ts.map (trT N)) (by simpa using hk) ?_ ?_⟩
  · rw [hres]; exact hR.ctor_holds hK _ (by simpa using hk)
  · rw [hres, ← trTT_inst_tr, trTT_ctor hR hK, Flick.Ty.arrows_substT, Flick.Ty.tcon_substT]
    exact .refl

theorem tcon_subst (T : Nat) (us : List Ty) (σ : Nat → Ty) :
    (Ty.tcon T us).subst σ = Ty.tcon T (us.map (·.subst σ)) := by
  have key : ∀ (us : List Ty) (f : Ty),
      (us.foldl .app f).subst σ = (us.map (·.subst σ)).foldl .app (f.subst σ) := by
    intro us
    induction us with
    | nil => intro f; rfl
    | cons u us ih => intro f; exact ih (.app f u)
  exact key us _

theorem map_eq_zip {α β : Type} {f : α → Flick.Ty} {g : β → Flick.Ty} :
    ∀ {as : List α} {bs : List β}, as.map f = bs.map g → ∀ p ∈ as.zip bs, f p.1 = g p.2
  | [], _, _ => fun _ h => by cases h
  | _ :: _, [], h => by cases h
  | a :: as, b :: bs, h => by
    simp only [List.map_cons, List.cons.injEq] at h
    intro p hp
    rcases List.mem_cons.mp hp with rfl | hp
    · exact h.1
    · exact map_eq_zip h.2 p hp

theorem map_equiv_zip {α β : Type} {f : α → Flick.Ty} {g : β → Flick.Ty} :
    ∀ {as : List α} {bs : List β}, Flick.Ty.ArgsEquiv (as.map f) (bs.map g) →
      ∀ p ∈ as.zip bs, Flick.Ty.Equiv (f p.1) (g p.2)
  | [], _, _ => fun _ h => by cases h
  | _ :: _, [], h => by cases h
  | a :: as, b :: bs, h => by
    cases h with
    | cons he ht =>
      intro p hp
      rcases List.mem_cons.mp hp with rfl | hp
      · exact he
      · exact map_equiv_zip ht p hp

/-- A unifier in the core of an instance `σ` of the problem of an arm gives one of the open
problem, after `σ` lifted past the variables of the constructor. -/
theorem cunif_of (hR : Real P S N) {K : Nat} {c : Ctor} {us : List Ty} {σ : Nat → Ty}
    (hK : P.ctors K = some c) (_hlen : us.length = c.res.length) {θ'' : Atom → Flick.Ty}
    (hu : Flick.Ty.Equiv ((Flick.Ty.tcon (N.con c.T) (S.res (N.ctor K))).substT θ'')
      (((trT N ((Ty.tcon c.T us).subst σ)).up c.k).substT θ'')) :
    CUnif N (c.eqs us) fun a => (liftT c.k (trσ N σ) a).substT θ'' := by
  obtain ⟨hS, hres, h2, h4⟩ := hR.ctor hK
  have hcl := S.res_closed _ _ _ _ (liftT c.k (trσ N σ)) hS fun i hi => by simp [liftT, hi]
  rw [hres] at hu hcl
  rw [tcon_subst, trT_tcon h2 h4, Flick.Ty.up_def, Flick.Ty.tcon_substT, Flick.Ty.tcon_substT,
    Flick.Ty.tcon_substT] at hu
  have hl := hu.tcon_inv.2
  simp only [List.map_map] at hl
  have hF : ∀ x ∈ c.res, (trT N x).substT (fun a => (liftT c.k (trσ N σ) a).substT θ'') =
      (trT N x).substT θ'' := by
    intro x hx
    rw [← Flick.Ty.substT_comp]
    have e := List.map_inj_left.mp (show c.res.map (fun x => (trT N x).substT (liftT c.k (trσ N σ))) =
      c.res.map (fun x => trT N x) by simpa [Function.comp_def] using hcl) x hx
    rw [e]
  have hG : ∀ y : Ty, (trT N (y.subst (up c.k))).substT
      (fun a => (liftT c.k (trσ N σ) a).substT θ'') =
      ((trT N (y.subst σ)).substT (shiftV c.k)).substT θ'' := by
    intro y
    rw [trT_subst, ← Flick.Ty.substT_comp]
    have e₁ : (trT N y).substT (trσ N (up c.k)) = (trT N y).up c.k :=
      (Flick.Ty.substT_congr fun a => trσ_up N c.k a).trans (Flick.Ty.up_def _ _).symm
    rw [e₁, Flick.Ty.up_comm, ← trT_subst, Flick.Ty.up_def]
  have hm : Flick.Ty.ArgsEquiv
      (c.res.map (fun x => (trT N x).substT fun a => (liftT c.k (trσ N σ) a).substT θ''))
      (us.map (fun y => (trT N (y.subst (up c.k))).substT
        fun a => (liftT c.k (trσ N σ) a).substT θ'')) := by
    rw [List.map_congr_left hF]
    simp only [hG]
    exact hl
  intro p hp
  simp only [Ctor.eqs, List.zip_map_right, List.mem_map] at hp
  obtain ⟨q, hq, rfl⟩ := hp
  exact map_equiv_zip hm q hq

/-- The matched type of an arm in the core: `T` applied, with the name of `T`. -/
theorem trT_scrut (hR : Real P S N) {K : Nat} {c : Ctor} (hK : P.ctors K = some c) (us : List Ty)
    (σ : Nat → Ty) : (trT N ((Ty.tcon c.T us).subst σ)).hd = .adt (N.con c.T) := by
  obtain ⟨-, -, h2, h4⟩ := hR.ctor hK
  rw [tcon_subst, trT_tcon h2 h4]
  exact Flick.Ty.tcon_hd _ _

theorem has_caseKDead (hR : Real P S N) {Γ Δ e K n m r T us σ t φ c}
    (he : Has S N Γ Δ e (.mono ((Ty.tcon T us).subst σ)) φ) (hK : P.ctors K = some c) (hT : c.T = T)
    (hlen : us.length = c.res.length) (hC : Clash (c.eqs us)) (hr : Has S N Γ Δ r (.mono t) φ) :
    Has S N Γ Δ (.caseK e K n m .dead r) (.mono t) φ := by
  subst hT
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := he Ψ Θ Γc γ δ hc
  obtain ⟨r', hr', tr⟩ := hr Ψ Θ Γc γ δ hc
  obtain ⟨hS, -, -, -⟩ := hR.ctor hK
  have dead : ∀ θ'',
      Flick.Ty.Equiv ((Flick.Ty.tcon (N.con c.T) (S.res (N.ctor K))).substT θ'')
        (((trT N ((Ty.tcon c.T us).subst σ)).up c.k).substT θ'') → False :=
    fun θ'' hu => Clash.none (hR.con_inj hK) hC _ (cunif_of hR hK hlen hu)
  refine ⟨_, .caseK he' (.lam .var) hr', .match_ te
    (.aref (Δ := fun _ => []) hS (trT_scrut hR hK us σ) (fun h => by cases h)
      (fun i h => by simp [Flick.Tm.Free, ext] at h) (fun l h => by cases h)
      (fun θ _ hu => (dead θ hu).elim) (fun θ _ hu => (dead θ hu).elim) (.acons .wild tr .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩

end Flick.Elab
