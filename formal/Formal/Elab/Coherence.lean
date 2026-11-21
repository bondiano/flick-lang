import Formal.Elab.CoherenceBase

/-!
# Uniqueness of closed entailments, superclass slots, and T6b (`Formal.Elab.Coherence`)
-/

namespace Flick.Elab

variable {P : Prog} {lt : Cn → Cn → Prop}

/-- **Uniqueness under a common query (K1', K5, K6)**: two closed derivations whose constraints
both serve a common query `Q₀` yield `≈`-equivalent dictionaries. -/
theorem closed_unique_common (h : Coherent P lt) (Q₀ : Cn) :
    ∀ {Q₁ Q₂ : Cn} {d₁ d₂ : Dict},
      Cn.Serves P Q₀ Q₁ → Cn.Serves P Q₀ Q₂ →
      Ent P [] [Q₁] [d₁] → Ent P [] [Q₂] [d₂] → Eqv P d₁ d₂ := by
  induction Q₀ using h.lt_wf.induction with
  | _ Q₀ ih =>
    intro Q₁ Q₂ d₁ d₂ hs₁ hs₂ h₁ h₂
    rcases h₁.single with hc₁ | ⟨p₁, hp₁, hpr₁, rfl⟩ | ⟨C₁, τ₁, ts₁, ds₁, rfl, hC₁, hf₁, hds₁, rfl⟩ |
        ⟨I₁, ins₁, σ₁, ds₁, hI₁, hserv₁, hds₁, rfl⟩
    · exact (hc₁.not_nil).elim
    · rcases h₂.single with hc₂ | ⟨p₂, hp₂, hpr₂, rfl⟩ | ⟨C₂, τ₂, ts₂, ds₂, rfl, hC₂, hf₂, -, rfl⟩ |
          ⟨I₂, ins₂, σ₂, ds₂, hI₂, hserv₂, -, rfl⟩
      · exact (hc₂.not_nil).elim
      · have hinv : ∀ j, P.clsVar Q₀.cls j = .inv ∨ P.clsVar Q₀.cls j = .bi := fun j =>
          .inl (hs₁.1 ▸ h.primVar hp₁ j)
        obtain rfl := Cn.Serves.eq_of_inv hinv hs₁
        obtain rfl := Cn.Serves.eq_of_inv hinv hs₂
        rw [hpr₁] at hpr₂; cases hpr₂; exact .refl
      · have hcc : C₂ = Q₁.cls := hs₂.1.trans hs₁.1.symm
        subst hcc; rw [h.primStruct hp₁] at hC₂; cases hC₂
      · exact (h.k6prim hp₁ hI₂ ((hserv₂.1.trans hs₂.1).trans hs₁.1.symm)).elim
    · rcases h₂.single with hc₂ | ⟨p₂, hp₂, -, rfl⟩ | ⟨C₂, τ₂, ts₂, ds₂, rfl, hC₂, hf₂, hds₂, rfl⟩ |
          ⟨I₂, ins₂, σ₂, ds₂, hI₂, hserv₂, -, rfl⟩
      · exact (hc₂.not_nil).elim
      · have hcc : C₁ = Q₂.cls := hs₁.1.trans hs₂.1.symm
        subst hcc; rw [h.primStruct hp₂] at hC₁; cases hC₁
      · cases Q₀ with
        | mk C₀ args₀ =>
          have hC10 : C₁ = C₀ := hs₁.1
          have hC20 : C₂ = C₀ := hs₂.1
          subst C₁; subst C₂
          cases args₀ with
          | nil => exact absurd hs₁.2.1 (Nat.zero_ne_add_one 0)
          | cons τ₀ rest =>
            cases rest with
            | cons _ _ => exact absurd (Nat.succ.inj hs₁.2.1) (Nat.succ_ne_zero _)
            | nil =>
              obtain ⟨hlen, hfs⟩ := h.fields_serves hC₁ hf₁ hf₂ hs₁ hs₂
              have key : ∀ (us vs : List Ty) (es₁ es₂ : List Dict),
                  us.length = vs.length →
                  (∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
                    ∃ q, lt q ⟨C₀, [τ₀]⟩ ∧ Cn.Serves P q ⟨C₀, [u]⟩ ∧ Cn.Serves P q ⟨C₀, [v]⟩) →
                  Ent P [] (us.map fun t => ⟨C₀, [t]⟩) es₁ →
                  Ent P [] (vs.map fun t => ⟨C₀, [t]⟩) es₂ →
                  All2 (Eqv P) es₁ es₂ := by
                intro us
                induction us with
                | nil =>
                  intro vs es₁ es₂ hlu _ he₁ he₂
                  cases vs with
                  | nil => cases he₁; cases he₂; exact .nil
                  | cons _ _ => exact absurd hlu (Nat.zero_ne_add_one _)
                | cons u us ihu =>
                  intro vs es₁ es₂ hlu hq he₁ he₂
                  cases vs with
                  | nil => exact absurd hlu (Nat.add_one_ne_zero _)
                  | cons v vs =>
                    obtain ⟨e₁, es₁', rfl, ha₁, hb₁⟩ := he₁.uncons
                    obtain ⟨e₂, es₂', rfl, ha₂, hb₂⟩ := he₂.uncons
                    obtain ⟨q, hlt, hq₁, hq₂⟩ := hq 0 u v rfl rfl
                    exact .cons (ih q hlt hq₁ hq₂ ha₁ ha₂)
                      (ihu vs es₁' es₂' (Nat.succ.inj hlu)
                        (fun j x y hx hy => hq (j + 1) x y hx hy)
                        hb₁ hb₂)
              have a := key ts₁ ts₂ ds₁ ds₂ hlen hfs hds₁ hds₂
              exact Eqv.apps (.strctLe hC₁ hf₁ hf₂ ⟨τ₀, hs₁, hs₂⟩) a
      · exact (h.k6struct hC₁ hf₁ hI₂ hs₁ (hs₂.trans hserv₂)).elim
    · rcases h₂.single with hc₂ | ⟨p₂, hp₂, -, rfl⟩ | ⟨C₂, τ₂, ts₂, ds₂, rfl, hC₂, hf₂, -, rfl⟩ |
          ⟨I₂, ins₂, σ₂, ds₂, hI₂, hserv₂, hds₂, rfl⟩
      · exact (hc₂.not_nil).elim
      · exact (h.k6prim hp₂ hI₁ ((hserv₁.1.trans hs₁.1).trans hs₂.1.symm)).elim
      · exact (h.k6struct hC₂ hf₂ hI₁ hs₂ (hs₁.trans hserv₁)).elim
      · have hsI₁ : Cn.Serves P Q₀ ⟨ins₁.cls, ins₁.head.map (·.subst σ₁)⟩ := hs₁.trans hserv₁
        have hsI₂ : Cn.Serves P Q₀ ⟨ins₂.cls, ins₂.head.map (·.subst σ₂)⟩ := hs₂.trans hserv₂
        have hII := h.k1' hI₁ hI₂ hsI₁ hsI₂
        subst hII; rw [hI₁] at hI₂; cases hI₂
        have hmono := h.k5mono hI₁ hsI₁ hsI₂
        have key : ∀ (cs : List Cn) (es₁ es₂ : List Dict),
            (∀ c ∈ cs, ∃ q, lt q Q₀ ∧ Cn.Serves P q (c.subst σ₁) ∧ Cn.Serves P q (c.subst σ₂)) →
            Ent P [] (cs.map (·.subst σ₁)) es₁ →
            Ent P [] (cs.map (·.subst σ₂)) es₂ →
            All2 (Eqv P) es₁ es₂ := by
          intro cs
          induction cs with
          | nil => intro es₁ es₂ _ he₁ he₂; cases he₁; cases he₂; exact .nil
          | cons c cs ihc =>
            intro es₁ es₂ hm he₁ he₂
            obtain ⟨e₁, es₁', rfl, ha₁, hb₁⟩ := he₁.uncons
            obtain ⟨e₂, es₂', rfl, ha₂, hb₂⟩ := he₂.uncons
            obtain ⟨q, hlt, hq₁, hq₂⟩ := hm c List.mem_cons_self
            exact .cons (ih q hlt hq₁ hq₂ ha₁ ha₂)
              (ihc es₁' es₂' (fun c' hc' => hm c' (List.mem_cons_of_mem _ hc')) hb₁ hb₂)
        exact Eqv.apps .refl (key ins₁.ctx ds₁ ds₂ hmono hds₁ hds₂)

/-- Closed derivations are unique up to `≈` (K1', K5, K6). -/
theorem closed_unique (h : Coherent P lt) {qs : List Cn} {ds₁ ds₂ : List Dict}
    (h₁ : Ent P [] qs ds₁) (h₂ : Ent P [] qs ds₂) : All2 (Eqv P) ds₁ ds₂ := by
  induction qs generalizing ds₁ ds₂ with
  | nil => cases h₁; cases h₂; exact .nil
  | cons Q qs ih =>
    obtain ⟨d₁, ds₁', rfl, e₁, t₁⟩ := h₁.uncons
    obtain ⟨d₂, ds₂', rfl, e₂, t₂⟩ := h₂.uncons
    exact .cons (closed_unique_common h Q (.refl Q) (.refl Q) e₁ e₂) (ih t₁ t₂)

theorem clo_lt (h : Coherent P lt) {I : Nat} {ins : Inst} {σ : Nat → Ty} {Q : Cn}
    (hI : P.insts I = some ins) (hserv : Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩)
    {d : Dict} {Q' : Cn} (hc : Clo P ins.ctx d Q') : lt (Q'.subst σ) Q := by
  induction hc with
  | given hk => exact h.k5inst hI (List.mem_of_getElem? hk) hserv
  | super _ hs ih => exact h.lt_trans (h.k5super (Prog.super_subst σ hs)) ih

/-- A superclass slot of a closed dictionary is `≈` a closed dictionary of the superclass. -/
theorem slot_ok (h : Coherent P lt) (Q : Cn) : SlotOK P Q := by
  induction Q using h.lt_wf.induction with
  | _ Q ih =>
  intro d₀ i S hd hs
  rcases hd.single with hc | ⟨p, hp, -, rfl⟩ | ⟨C, τ, ts, ds, rfl, hC, hf, hds, rfl⟩ |
      ⟨I, ins, σ, ds, hI, hserv, hds, rfl⟩
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
  · -- the slot of an instance under B1: K7 on head, then transport along `super_serves`
    have hcls : ins.cls = Q.cls := hserv.1
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
        ih _ (clo_lt h hI hserv hc)
      obtain ⟨ds', e, a⟩ := closed_rep h hρ ho hse
      obtain ⟨d', rfl, r⟩ := a.single
      have hsupH : P.super ⟨ins.cls, ins.head.map (·.subst σ)⟩ i =
          some ((S₀.subst (params ins.head)).subst σ) :=
        Prog.super_subst σ hsup
      have hsupQ : P.super Q i = some S := by unfold Prog.super; exact hs
      have hSS : Cn.Serves P S ((S₀.subst (params ins.head)).subst σ) :=
        h.super_serves hserv hsupQ hsupH
      obtain ⟨d'', r', hd''⟩ := ent_serves h _ (by simpa using e) hSS
      exact ⟨d'', .trans (.instSlot hI hsl hds) (.trans r r'), hd''⟩

/-- **Lemma 6.1 (superclass paths).** Two derivations of one constraint agree after `ρ`. -/
theorem lemma_6_1 (h : Coherent P lt) {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) {Q : Cn} {d₁ d₂ : Dict} (h₁ : Ent P Δ [Q] [d₁])
    (h₂ : Ent P Δ [Q] [d₂]) : Eqv P (d₁.subst ρ) (d₂.subst ρ) := by
  have ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ) := fun _ _ _ => slot_ok h _
  obtain ⟨_, e₁, a₁⟩ := closed_rep h hρ ho h₁
  obtain ⟨_, e₂, a₂⟩ := closed_rep h hρ ho h₂
  obtain ⟨_, rfl, r₁⟩ := a₁.single
  obtain ⟨_, rfl, r₂⟩ := a₂.single
  obtain ⟨_, hb, r₁₂⟩ := (closed_unique h e₁ e₂).single
  cases hb
  exact .trans r₁ (.trans r₁₂ (.symm r₂))

theorem Ty.subst_var (t : Ty) : t.subst .var = t := Ty.subst_id t

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
  obtain ⟨_, hb, r'⟩ := (closed_unique h e h₀).single
  cases hb
  exact .trans r r'

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
  have a := All2.trans a₁ (All2.trans_symm (closed_unique h e₁ e₂) a₂)
  exact ⟨a, hsim s _ _ a o₁ o₂⟩

end Flick.Elab
