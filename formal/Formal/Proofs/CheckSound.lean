import Formal.Proofs.Check

/-!
# `check_sound`: what the checker accepts holds

By induction on the certificate: each rule's conclusion holds when its
premises do. `refl` rests on `norm_sound`, `cong` and the splits on the
fundamental lemma, `funext` on the equality of functions, and a lemma on
its own soundness at the values its arguments evaluate to.
-/

namespace Flick.PT

open Classical in
/-- **Soundness of the checker.** -/
theorem check_sound {P : Prog} (hP : P.WF) {k : Nat} {L : List Stmt}
    (hL : ∀ l ∈ L, l.Holds P.bodies ∧ l.Typed P.schemes) :
    ∀ (c : Cert) (s : Stmt), check P k L s c = true → s.Holds P.bodies := by
  have hG := hP.globOK
  intro c
  induction c with
  | refl =>
    intro s h
    simp only [check, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨ht, hn⟩ := h
    obtain ⟨hl, hr, _⟩ := Stmt.typed?_sound ht
    intro θ γ hV _
    exact same_normal_form hP hl hr k hn θ γ γ hV.rs
  | symm p ih =>
    intro s h θ γ hV hF
    exact (ih _ h θ γ hV hF).symm
  | trans m p q ihp ihq =>
    intro s h θ γ hV hF
    simp only [check, Bool.and_eq_true] at h
    exact (ihp _ h.1 θ γ hV hF).trans (ihq _ h.2 θ γ hV hF)
  | cong c t a b p ih =>
    intro s h θ γ hV hF
    simp only [check, Bool.and_eq_true, beq_iff_eq] at h
    obtain ⟨⟨⟨hl, hr⟩, hc⟩, hp⟩ := h
    have hab := ih _ hp θ γ hV hF
    rw [hl, hr, Tm.inst_subst, Tm.inst_subst]
    exact fundamental hG (infer_sound hc) θ _ _ (hV.rs.cons hab)
  | funext p ih =>
    intro s h θ γ hV hF
    cases hty : s.ty with
    | arrow a b =>
      simp only [check, hty, Bool.and_eq_true] at h
      obtain ⟨ht, hp⟩ := h
      obtain ⟨hl, hr, _⟩ := Stmt.typed?_sound ht
      rw [hty] at hl hr
      obtain ⟨f, f', ef, ef', rf⟩ := fundamental hG hl θ γ γ hV.rs
      obtain ⟨g, g', eg, eg', rg⟩ := fundamental hG hr θ γ γ hV.rs
      cases ef.det ef'; cases eg.det eg'
      simp only [Ty.subst] at rf rg ⊢
      obtain ⟨_, fb, _, _, rfl, hf₂, _⟩ := R.arrow_lam rf
      cases hf₂
      obtain ⟨_, gb, _, _, rfl, hg₂, hgb⟩ := R.arrow_lam rg
      cases hg₂
      refine ⟨_, _, ef, eg, ?_⟩
      simp only [R]
      intro v w hvw
      have hvv : R P.bodies (a.subst θ) v v := hvw.trans hvw.symm
      have back : ∀ e : Tm, (e.ren .succ).subst (scons v γ) = e.subst γ := fun e => by
        rw [Tm.ren_subst]; rfl
      have hfacts : Stmt.FactsHold P.bodies
          { ctx := a :: s.ctx, facts := shiftFacts s.facts,
            lhs := .app (s.lhs.ren .succ) (.var 0), rhs := .app (s.rhs.ren .succ) (.var 0),
            ty := b } θ (scons v γ) := by
        intro fct hm
        simp only [shiftFacts, List.mem_map] at hm
        obtain ⟨f₀, hf₀, rfl⟩ := hm
        simp only [back]
        exact hF f₀ hf₀
      obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ := ih _ hp θ (scons v γ) (hV.cons hvv) hfacts
      simp only [Tm.subst, back] at eu₁ eu₂
      cases eu₁ with
      | app hf hx hb =>
        cases ef.det hf
        cases hvv.value.1.eval.det hx
        cases eu₂ with
        | app hg' hx' hb' =>
          cases eg.det hg'
          cases hvv.value.1.eval.det hx'
          obtain ⟨u₂', u₃, e₂, e₃, r₂₃⟩ := hgb v w hvw
          cases hb'.det e₂
          exact ⟨u₁, u₃, hb, e₃, ru.trans r₂₃⟩
    | _ => simp [check, hty] at h
  | split x p q ihp ihq =>
    intro s h
    simp only [check, Bool.and_eq_true] at h
    obtain ⟨ht, h⟩ := h
    have hs := Stmt.typed?_sound ht
    -- the value of `x` is that of the case: the rest keep theirs
    have keep : ∀ {θ : Nat → Ty} {γ γ' : Nat → Tm} {t : Tm} {j : Nat}, Vals P.bodies θ s.ctx γ →
        (∀ i, i ≠ x → (Tm.var (i + j)).subst γ' = γ i) →
        (∀ T, s.ctx[x]? = some T → E P.bodies (T.subst θ) (γ x) (t.subst γ')) →
        RS P.bodies θ s.ctx γ (fun i => (at? x t j i).subst γ') := by
      intro θ γ γ' t j hV hrest hx n T hn
      by_cases hnx : n = x
      · subst hnx; simp only [at?]; exact hx T hn
      · simp only [at?, if_neg hnx, hrest n hnx]; exact hV.rs n T hn
    cases hx : s.ctx[x]? with
    | none => simp [hx] at h
    | some T =>
      cases T with
      | unit =>
        simp only [hx] at h
        refine Stmt.holds_of_cases hP hs (σ₁ := at? x .unit 0) (c₁ := s.ctx) ?_ (ihp _ h) (ihp _ h)
        intro θ γ hV
        refine .inl ⟨γ, hV, keep hV (fun i _ => rfl) fun T hT => ?_⟩
        rw [hx] at hT; cases hT
        have hu := hV x .unit hx
        rw [R.unit_eq hu]
        exact ⟨_, _, .unit, .unit, by simp only [Ty.subst, R]⟩
      | prod a b =>
        simp only [hx] at h
        refine Stmt.holds_of_cases hP hs (σ₁ := at? x (.pair (.var 1) (.var 0)) 2)
          (c₁ := b :: a :: s.ctx) ?_ (ihp _ h) (ihp _ h)
        intro θ γ hV
        have hp := hV x _ hx
        simp only [Ty.subst] at hp
        obtain ⟨v₁, v₂, _, _, hγx, _, r₁, r₂⟩ := R.prod_eq hp
        refine .inl ⟨scons v₂ (scons v₁ γ), (hV.cons (by rw [hγx] at hp; exact hp.1)).cons
          (by rw [hγx] at hp; exact hp.2), keep hV (fun i _ => rfl) fun T hT => ?_⟩
        rw [hx] at hT; cases hT
        have := (hV x _ hx).E
        rw [hγx] at this ⊢
        exact this
      | sum a b =>
        simp only [hx, Bool.and_eq_true] at h
        refine Stmt.holds_of_cases hP hs (σ₁ := at? x (.inl (.var 0) b) 1) (c₁ := a :: s.ctx)
          (σ₂ := at? x (.inr a (.var 0)) 1) (c₂ := b :: s.ctx) ?_ (ihp _ h.1) (ihq _ h.2)
        intro θ γ hV
        have hsx := hV x _ hx
        simp only [Ty.subst] at hsx
        rcases R.sum_eq hsx with ⟨v₁, _, d₁, _, hγx, _, _⟩ | ⟨v₁, _, d₁, _, hγx, _, _⟩
        · rw [hγx] at hsx
          refine .inl ⟨scons v₁ γ, hV.cons hsx, keep hV (fun i _ => rfl) fun T hT => ?_⟩
          rw [hx] at hT; cases hT
          rw [hγx]
          exact R.E (G := P.bodies) (t := (Ty.sum a b).subst θ) (by simp only [Ty.subst]; exact hsx)
        · rw [hγx] at hsx
          refine .inr ⟨scons v₁ γ, hV.cons hsx, keep hV (fun i _ => rfl) fun T hT => ?_⟩
          rw [hx] at hT; cases hT
          rw [hγx]
          exact R.E (G := P.bodies) (t := (Ty.sum a b).subst θ) (by simp only [Ty.subst]; exact hsx)
      | var | arrow => simp [hx] at h
  | fact n =>
    intro s h θ γ _ hF
    simp only [check, beq_iff_eq] at h
    exact hF _ (List.mem_of_getElem? h)
  | lemma i ts args =>
    intro s h θ γ hV _
    cases hl : L[i]? with
    | none => simp [check, hl] at h
    | some l =>
      simp only [check, hl, Bool.and_eq_true, beq_iff_eq, List.all_eq_true, List.mem_range,
        List.isEmpty_iff] at h
      obtain ⟨⟨⟨⟨⟨hnf, hlen⟩, hlhs⟩, hrhs⟩, hty⟩, hargs⟩ := h
      obtain ⟨hHolds, hlt, hrt, _⟩ := hL l (List.mem_of_getElem? hl)
      let θ' : Nat → Ty := fun n => (tyInst ts n).subst θ
      -- each argument evaluates to a value of its type
      have arg : ∀ j t, l.ctx[j]? = some t →
          ∃ v, Eval P.bodies ((argSubst args j).subst γ) v ∧ R P.bodies (t.subst θ') v v := by
        intro j t hj
        have hjl : j < args.length := hlen ▸ (List.getElem?_eq_some_iff.mp hj).1
        have hT := infer_sound (hargs j hjl)
        have hgd : l.ctx.getD j .unit = t := by simp [List.getD, hj]
        rw [hgd] at hT
        obtain ⟨v, v', ev, ev', r⟩ := fundamental hG hT θ γ γ hV.rs
        have hsub : argSubst args j = args.getD j .unit := by
          simp [argSubst, List.getD, List.getElem?_eq_getElem hjl]
        rw [Ty.subst_subst] at r
        cases ev.det ev'
        exact ⟨v, hsub ▸ ev, r⟩
      let va : Nat → Tm := fun j =>
        if hj : j < l.ctx.length then
          Classical.choose (arg j l.ctx[j] (List.getElem?_eq_getElem hj))
        else .var j
      have hva : ∀ j t, l.ctx[j]? = some t →
          Eval P.bodies ((argSubst args j).subst γ) (va j) ∧ R P.bodies (t.subst θ') (va j) (va j) := by
        intro j t hj
        obtain ⟨hlt, hget⟩ := List.getElem?_eq_some_iff.mp hj
        subst hget
        simp only [va, dif_pos hlt]
        exact Classical.choose_spec (arg j l.ctx[j] (List.getElem?_eq_getElem hlt))
      have hVa : Vals P.bodies θ' l.ctx va := fun j t hj => (hva j t hj).2
      have hrel : RS P.bodies θ' l.ctx (fun j => (argSubst args j).subst γ) va :=
        fun j t hj => E.of_value (hva j t hj).1 (hva j t hj).2 |>.symm
      have hc := hHolds θ' va hVa (by intro f hf; rw [hnf] at hf; cases hf)
      rw [hlhs, hrhs, hty, Tm.subst_subst, Tm.subst_subst, Ty.subst_subst]
      exact ((fundamental hG hlt θ' _ _ hrel).trans hc).trans
        (fundamental hG hrt θ' _ _ hrel).symm

/-! ## Programs and lists of statements -/

/-- Each body checks at its scheme with the globals before it. -/
def Prog.wf? (P : Prog) : Bool :=
  (List.range P.length).all fun g =>
    match P[g]? with
    | some (b, σ) => infer (P.schemes.take g) [] b == some σ
    | none => false

theorem Prog.wf?_sound {P : Prog} (h : P.wf? = true) : P.WF := by
  intro g hg
  simp only [Prog.wf?, List.all_eq_true, List.mem_range] at h
  have := h g hg
  rw [List.getElem?_eq_getElem hg] at this
  simp only [beq_iff_eq] at this
  exact infer_sound this

/-- Statements checked in order: each, once checked, is a lemma of the
later ones. -/
def checkAll (P : Prog) (k : Nat) : List Stmt → List (Stmt × Cert) → Bool
  | _, [] => true
  | L, (s, c) :: rest => s.typed? P.schemes && check P k L s c && checkAll P k (L ++ [s]) rest

theorem checkAll_sound {P : Prog} (hP : P.WF) {k : Nat} :
    ∀ (items : List (Stmt × Cert)) (L : List Stmt),
    (∀ l ∈ L, l.Holds P.bodies ∧ l.Typed P.schemes) → checkAll P k L items = true →
    ∀ s ∈ items.map Prod.fst, s.Holds P.bodies ∧ s.Typed P.schemes := by
  intro items
  induction items with
  | nil => intro _ _ _ s hs; simp at hs
  | cons item rest ih =>
    intro L hL h s hs
    obtain ⟨s₀, c₀⟩ := item
    simp only [checkAll, Bool.and_eq_true] at h
    obtain ⟨⟨ht, hc⟩, hrest⟩ := h
    have h₀ : s₀.Holds P.bodies ∧ s₀.Typed P.schemes :=
      ⟨check_sound hP hL c₀ s₀ hc, Stmt.typed?_sound ht⟩
    have hL' : ∀ l ∈ L ++ [s₀], l.Holds P.bodies ∧ l.Typed P.schemes := by
      intro l hl
      rcases List.mem_append.mp hl with hl | hl
      · exact hL l hl
      · simp at hl; subst hl; exact h₀
    simp only [List.map_cons, List.mem_cons] at hs
    rcases hs with rfl | hs
    · exact h₀
    · exact ih (L ++ [s₀]) hL' hrest s hs

end Flick.PT
