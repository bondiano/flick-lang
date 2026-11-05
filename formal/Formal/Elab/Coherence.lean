import Formal.Elab.Basic

/-!
# Elaboration: coherence (elaboration.md §4.2)

`closed_rep`: after `ρ`, every derived dictionary is `≈` a closed derivation of the
substituted constraint. `slot_ok`: projecting a superclass slot from a closed dictionary is
`≈` a closed derivation of the superclass (well-founded induction by K5). `closed_unique`:
closed derivations are unique (K1, K5, K6). Lemma 6.1, Lemma 6.2 and T6(b) follow.
-/

namespace Flick.Elab

variable {P : Prog} {lt : Cn → Cn → Prop}

/-- Projecting a slot of a closed dictionary of `Q` gives a closed dictionary of the
superclass, up to `≈`. -/
def SlotOK (P : Prog) (Q : Cn) : Prop :=
  ∀ d₀ i S, Ent P [] [Q] [d₀] → P.super Q i = some S →
    ∃ d, Eqv P (.slot d₀ i) d ∧ Ent P [] [S] [d]

/-- `ρ` of Lemma 6.1: types `θ` and closed dictionaries `ρ(δ)` with `· ⊩ Q'θ ⇝ ρ(δ)`. -/
def Closes (P : Prog) (Δ : List Cn) (θ : Nat → Ty) (ρ : Nat → Dict) : Prop :=
  ∀ k Q, Δ[k]? = some Q → Ent P [] [Q.subst θ] [ρ k]

theorem All2.single {R : Dict → Dict → Prop} {a : Dict} {l : List Dict} (h : All2 R [a] l) :
    ∃ b, l = [b] ∧ R a b := by
  cases h with
  | cons r t => cases t; exact ⟨_, rfl, r⟩

theorem All2.trans_symm {a b c : List Dict} (h₁ : All2 (Eqv P) a b) (h₂ : All2 (Eqv P) c b) :
    All2 (Eqv P) a c := by
  induction h₁ generalizing c with
  | nil => cases h₂; exact .nil
  | cons r _ ih => cases h₂ with
    | cons r' t' => exact .cons (.trans r (.symm r')) (ih t')

theorem Ent.uncons {Δ : List Cn} {Q : Cn} {qs : List Cn} {ds : List Dict}
    (h : Ent P Δ (Q :: qs) ds) : ∃ d ds', ds = d :: ds' ∧ Ent P Δ [Q] [d] ∧ Ent P Δ qs ds' := by
  cases h with
  | cons h₁ h₂ => exact ⟨_, _, rfl, h₁, h₂⟩
  | given hc => exact ⟨_, _, rfl, .given hc, .nil⟩
  | prim h₁ h₂ => exact ⟨_, _, rfl, .prim h₁ h₂, .nil⟩
  | strct h₁ h₂ h₃ => exact ⟨_, _, rfl, .strct h₁ h₂ h₃, .nil⟩
  | inst h₁ h₂ h₃ h₄ => exact ⟨_, _, rfl, .inst h₁ h₂ h₃ h₄, .nil⟩

theorem clo_rep {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ)
    (ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ)) {d : Dict} {Q : Cn} (h : Clo P Δ d Q) :
    ∃ d', Eqv P (d.subst ρ) d' ∧ Ent P [] [Q.subst θ] [d'] := by
  induction h with
  | given hk => exact ⟨_, .refl, hρ _ _ hk⟩
  | super hc hs ih =>
    obtain ⟨d₀, he, hd₀⟩ := ih
    obtain ⟨d', h1, h2⟩ := ho _ _ hc d₀ _ _ hd₀ (Prog.super_subst _ hs)
    exact ⟨d', .trans (.slot he) h1, h2⟩

/-- After `ρ`, a derivation is `≈` a closed derivation of the substituted constraints. -/
theorem closed_rep (h : Coherent P lt) {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) (ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ))
    {qs : List Cn} {ds : List Dict} (he : Ent P Δ qs ds) :
    ∃ ds', Ent P [] (qs.map (·.subst θ)) ds' ∧ All2 (Eqv P) (ds.map (·.subst ρ)) ds' := by
  induction he with
  | nil => exact ⟨[], .nil, .nil⟩
  | cons _ _ ih₁ ih₂ =>
    obtain ⟨d₁, e₁, a₁⟩ := ih₁
    obtain ⟨ds₂, e₂, a₂⟩ := ih₂
    cases a₁ with
    | cons r a => cases a; exact ⟨_, .cons e₁ e₂, .cons r a₂⟩
  | given hc =>
    obtain ⟨d', h1, h2⟩ := clo_rep hρ ho hc
    exact ⟨[d'], h2, .cons h1 .nil⟩
  | prim h1 h2 => exact ⟨[_], .prim h1 (h.prims_subst h2), .cons (.primSub h1 h2) .nil⟩
  | @strct C τ ts ds h1 h2 _ ih =>
    obtain ⟨ds', e, a⟩ := ih
    refine ⟨[apps (.strct C (τ.subst θ)) ds'], ?_, .cons ?_ .nil⟩
    · have : (ts.map fun t => (⟨C, [t]⟩ : Cn)).map (·.subst θ) =
          (ts.map (·.subst θ)).map fun t => (⟨C, [t]⟩ : Cn) := by
        simp [List.map_map, Function.comp, Cn.subst]
      rw [this] at e
      exact .strct h1 (h.fields_subst h2) e
    · simp only [apps_subst]; exact Eqv.apps (.strctSub h1 h2) a
  | @inst Q I ins σ ds h1 h2 h3 _ ih =>
    obtain ⟨ds', e, a⟩ := ih
    refine ⟨[apps (.inst I) ds'], ?_, .cons ?_ .nil⟩
    · refine .inst (σ := fun n => (σ n).subst θ) h1 h2 ?_ ?_
      · show Q.args.map (·.subst θ) = _
        rw [h3, List.map_map]; exact List.map_congr_left fun t _ => Ty.subst_comp t σ θ
      · have : ins.ctx.map (fun c => c.subst fun n => (σ n).subst θ) =
            (ins.ctx.map (·.subst σ)).map (·.subst θ) := by
          rw [List.map_map]; exact List.map_congr_left fun c _ => (Cn.subst_comp c σ θ).symm
        rw [this]; exact e
    · simp only [apps_subst]; exact Eqv.apps .refl a

theorem map_eq_mem {α β : Type} {f g : α → β} {l : List α} (h : l.map f = l.map g) :
    ∀ x ∈ l, f x = g x := by
  induction l with
  | nil => intro x hx; cases hx
  | cons a l ih =>
    simp only [List.map_cons, List.cons.injEq] at h
    intro x hx; rcases List.mem_cons.1 hx with rfl | hx
    · exact h.1
    · exact ih h.2 x hx

/-- K5: equal heads fix the context. -/
theorem ctx_agree (h : Coherent P lt) {I : Nat} {ins : Inst} {σ σ' : Nat → Ty}
    (hI : P.insts I = some ins) (heq : ins.head.map (·.subst σ) = ins.head.map (·.subst σ')) :
    ins.ctx.map (·.subst σ) = ins.ctx.map (·.subst σ') := by
  refine List.map_congr_left fun c hc => ?_
  simp only [Cn.subst, Cn.mk.injEq, true_and]
  refine List.map_congr_left fun t ht => Ty.subst_agree fun n hn => ?_
  obtain ⟨hd, hmem, hocc⟩ := h.k5vars hI hc ht hn
  exact Ty.agree_of_eq (map_eq_mem heq hd hmem) n hocc

/-- Closed derivations are unique (K1, K5, K6). -/
theorem closed_unique (h : Coherent P lt) {qs : List Cn} {ds₁ : List Dict}
    (h₁ : Ent P [] qs ds₁) : ∀ {ds₂}, Ent P [] qs ds₂ → ds₁ = ds₂ := by
  induction h₁ with
  | nil => intro ds₂ h₂; cases h₂; rfl
  | cons _ _ ih₁ ih₂ =>
    intro ds₂ h₂
    obtain ⟨d, ds', rfl, e₁, e₂⟩ := h₂.uncons
    have := ih₁ e₁; cases this; rw [ih₂ e₂]
  | given hc => exact (hc.not_nil).elim
  | @prim Q p hp hpr =>
    intro ds₂ h₂
    obtain ⟨d₂, ds₀, rfl, h₂, hn⟩ := h₂.uncons
    cases hn
    rcases h₂.single with hc | ⟨p', -, hp', rfl⟩ | ⟨C, τ, ts, ds, rfl, hC, -⟩ | ⟨I, ins, σ, ds, hI, hcls, -⟩
    · exact (hc.not_nil).elim
    · rw [hpr] at hp'; cases hp'; rfl
    · rw [h.primStruct hp] at hC; cases hC
    · exact (h.k6prim hp hI hcls).elim
  | @strct C τ ts ds hC hf _ ih =>
    intro ds₂ h₂
    obtain ⟨d₂, ds₀, rfl, h₂, hn⟩ := h₂.uncons
    cases hn
    rcases h₂.single with hc | ⟨p', hp, -⟩ | ⟨C', τ', ts', ds', hq, -, hf', he', rfl⟩ |
        ⟨I, ins, σ, ds', hI, hcls, hargs, -⟩
    · exact (hc.not_nil).elim
    · rw [h.primStruct hp] at hC; cases hC
    · simp only [Cn.mk.injEq, List.cons.injEq, and_true] at hq; obtain ⟨rfl, rfl⟩ := hq
      rw [hf] at hf'; cases hf'; rw [ih he']
    · exact (h.k6struct hC hf hI hcls hargs).elim
  | @inst Q I ins σ ds hI hcls hargs _ ih =>
    intro ds₂ h₂
    obtain ⟨d₂, ds₀, rfl, h₂, hn⟩ := h₂.uncons
    cases hn
    rcases h₂.single with hc | ⟨p', hp, -⟩ | ⟨C', τ', ts', ds', rfl, hC, hf', -⟩ |
        ⟨I', ins', σ', ds', hI', hcls', hargs', he', rfl⟩
    · exact (hc.not_nil).elim
    · exact (h.k6prim hp hI hcls).elim
    · exact (h.k6struct hC hf' hI hcls hargs).elim
    · have hII := h.k1 hI hI' (hcls.trans hcls'.symm) (hargs.symm.trans hargs')
      subst hII; rw [hI] at hI'; cases hI'
      rw [ctx_agree h hI (hargs.symm.trans hargs')] at ih
      rw [ih he']

theorem clo_lt (h : Coherent P lt) {I : Nat} {ins : Inst} {σ : Nat → Ty} (hI : P.insts I = some ins)
    {d : Dict} {Q : Cn} (hc : Clo P ins.ctx d Q) :
    lt (Q.subst σ) ⟨ins.cls, ins.head.map (·.subst σ)⟩ := by
  induction hc with
  | given hk => exact h.k5inst hI (List.mem_of_getElem? hk)
  | super _ hs ih => exact h.lt_trans (h.k5super (Prog.super_subst σ hs)) ih

/-- A superclass slot of a closed dictionary is `≈` a closed dictionary of the superclass. -/
theorem slot_ok (h : Coherent P lt) (Q : Cn) : SlotOK P Q := by
  induction Q using h.lt_wf.induction with
  | _ Q ih =>
  intro d₀ i S hd hs
  rcases hd.single with hc | ⟨p, hp, -, rfl⟩ | ⟨C, τ, ts, ds, rfl, hC, hf, hds, rfl⟩ |
      ⟨I, ins, σ, ds, hI, hcls, hargs, hds, rfl⟩
  · exact (hc.not_nil).elim
  · simp [Prog.super, h.primSupers hp] at hs
  · unfold Prog.super at hs
    cases hS₀ : (P.supers C)[i]? with
    | none => rw [hS₀] at hs; cases hs
    | some S₀ =>
      rw [hS₀] at hs; simp only [Option.map_some, Option.some.injEq] at hs; subst hs
      obtain ⟨hSC, hSa⟩ := h.structSupers hC hS₀
      -- every field's slot, by the induction hypothesis
      have key : ∀ (us : List Ty) (es : List Dict), (∀ t ∈ us, t ∈ ts) →
          Ent P [] (us.map fun t => ⟨C, [t]⟩) es →
          ∃ es', Ent P [] (us.map fun t => ⟨S₀.cls, [t]⟩) es' ∧
            All2 (Eqv P) (es.map (.slot · i)) es' := by
        intro us
        induction us with
        | nil => intro es _ he; cases he; exact ⟨[], .nil, .nil⟩
        | cons u us ihu =>
          intro es hsub he
          obtain ⟨e, es₀, rfl, he₁, he₂⟩ := he.uncons
          have hsup : P.super ⟨C, [u]⟩ i = some ⟨S₀.cls, [u]⟩ := by
            simp [Prog.super, hS₀, Cn.subst, hSa, Ty.subst, params]
          obtain ⟨e', r, he'⟩ := ih _ (h.k5fields hC hf (hsub u List.mem_cons_self)) e i _ he₁ hsup
          obtain ⟨es', h₂, a⟩ := ihu es₀ (fun t ht => hsub t (List.mem_cons_of_mem _ ht)) he₂
          exact ⟨e' :: es', .cons he' h₂, .cons r a⟩
      obtain ⟨es', he', a⟩ := key ts ds (fun _ ht => ht) hds
      refine ⟨apps (.strct S₀.cls τ) es', .trans (.strctSlot hC hS₀ hf hds) (Eqv.apps .refl a), ?_⟩
      have : S₀.subst (params [τ]) = ⟨S₀.cls, [τ]⟩ := by
        simp [Cn.subst, hSa, Ty.subst, params]
      rw [this]; exact .strct hSC hf he'
  · -- the slot of an instance: K7, then the context
    have hQ : Q = Cn.subst σ ⟨ins.cls, ins.head⟩ := by
      cases Q with
      | mk c a => simp only [Cn.subst, Cn.mk.injEq]; exact ⟨hcls.symm, hargs⟩
    unfold Prog.super at hs
    cases hS₀ : (P.supers Q.cls)[i]? with
    | none => rw [hS₀] at hs; cases hs
    | some S₀ =>
      have hsup : P.super ⟨ins.cls, ins.head⟩ i = some (S₀.subst (params ins.head)) := by
        simp [Prog.super, hcls, hS₀]
      obtain ⟨s, hsl, hse⟩ := h.k7 hI hsup
      have hρ : Closes P ins.ctx σ (args ds) := fun k c hk =>
        hds.get.2 k _ (by simp [hk])
      have ho : ∀ d Q', Clo P ins.ctx d Q' → SlotOK P (Q'.subst σ) := fun d Q' hc =>
        ih _ (hQ ▸ clo_lt h hI hc)
      obtain ⟨ds', e, a⟩ := closed_rep h hρ ho hse
      obtain ⟨d', rfl, r⟩ := a.single
      refine ⟨_, .trans (.instSlot hI hsl hds) r, ?_⟩
      have hS : S = (S₀.subst (params ins.head)).subst σ := by
        rw [hS₀] at hs; simp only [Option.map_some, Option.some.injEq] at hs
        rw [← hs, Cn.subst_comp]; congr 1; funext n; rw [params_subst, ← hargs]
      rw [hS]; simpa using e

/-- **Lemma 6.1 (superclass paths).** Two derivations of one constraint agree after `ρ`. -/
theorem lemma_6_1 (h : Coherent P lt) {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) {Q : Cn} {d₁ d₂ : Dict} (h₁ : Ent P Δ [Q] [d₁])
    (h₂ : Ent P Δ [Q] [d₂]) : Eqv P (d₁.subst ρ) (d₂.subst ρ) := by
  have ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ) := fun _ _ _ => slot_ok h _
  obtain ⟨_, e₁, a₁⟩ := closed_rep h hρ ho h₁
  obtain ⟨_, e₂, a₂⟩ := closed_rep h hρ ho h₂
  obtain ⟨_, rfl, r₁⟩ := a₁.single
  obtain ⟨_, rfl, r₂⟩ := a₂.single
  cases closed_unique h e₁ e₂
  exact .trans r₁ (.symm r₂)

theorem Ty.subst_var (t : Ty) : t.subst .var = t := by
  induction t using Ty.ind
  case arr φ a b q iha ihb ihq ihφ => cases φ <;> simp_all [Ty.subst, Stage.subst, Stage.All]
  all_goals simp_all [Ty.subst]

/-- **Lemma 6.2 (cache).** A constraint without variables solved without premises: its
derivation in any `Δ` is, after `ρ`, `≈` that solution (`docs/formal/elaboration.md#lemma-62-needs-the-substitution`). -/
theorem lemma_6_2 (h : Coherent P lt) {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) {Q : Cn} (hg : ∀ t ∈ Q.args, t.Ground) {d₀ d : Dict}
    (h₀ : Ent P [] [Q] [d₀]) (he : Ent P Δ [Q] [d]) : Eqv P (d.subst ρ) d₀ := by
  have ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ) := fun _ _ _ => slot_ok h _
  obtain ⟨_, e, a⟩ := closed_rep h hρ ho he
  obtain ⟨_, rfl, r⟩ := a.single
  have hQ : Q.subst θ = Q := by
    cases Q with
    | mk C ts =>
      simp only [Cn.subst, Cn.mk.injEq, true_and]
      conv => rhs; rw [← List.map_id ts]
      refine List.map_congr_left fun t ht => ?_
      rw [show t = t.subst .var from (Ty.subst_var t).symm, Ty.subst_comp]
      exact Ty.subst_agree fun n hn => absurd hn (hg t ht n)
  simp only [List.map_cons, List.map_nil, hQ] at e
  cases closed_unique h e h₀
  exact r

/-- **T6(b) (coherence).** Two elaborations that differ only in the derivations of their
dictionaries agree after `ρ` up to `≈` in every dictionary position; with the simulation
`Sim`, they give one result. -/
theorem T6b (h : Coherent P lt) {X : Exprs} (hsim : Sim P X) (s : X.Skel) {Δ : List Cn}
    {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ) {qs : List Cn} {ds₁ ds₂ : List Dict}
    (h₁ : Ent P Δ qs ds₁) (h₂ : Ent P Δ qs ds₂) (o₁ : X.Ok s (ds₁.map (·.subst ρ)))
    (o₂ : X.Ok s (ds₂.map (·.subst ρ))) :
    All2 (Eqv P) (ds₁.map (·.subst ρ)) (ds₂.map (·.subst ρ)) ∧
      X.run s (ds₁.map (·.subst ρ)) = X.run s (ds₂.map (·.subst ρ)) := by
  have ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ) := fun _ _ _ => slot_ok h _
  obtain ⟨_, e₁, a₁⟩ := closed_rep h hρ ho h₁
  obtain ⟨_, e₂, a₂⟩ := closed_rep h hρ ho h₂
  cases closed_unique h e₁ e₂
  have a := All2.trans_symm a₁ a₂
  exact ⟨a, hsim s _ _ a o₁ o₂⟩

end Flick.Elab
