import Formal.Live.UseAtoms
import Formal.WithinChain

/-!
# The children and finalizers of the live discipline run at their owner's place

Each child and finalizer of a region `p` is typed at a stage whose region atoms are ancestors
of `p` (`LRgs`). A step keeps this: a new child of `p` uses `p`, regions older than `p` that its
registration uses, and the arms of handlers older than `p` (`kid_stage`), all ancestors of `p`
where the regions of the place form a chain.
-/

namespace Flick

variable {S : Sig}

/-- A term typed at a stage whose region atoms are in `A`. -/
def LTy (S : Sig) (Ψ : List Ty) (A : Nat → Prop) (k : Tm) : Prop :=
  ∃ t φ, TypM S .live Ψ Facts.age [] k (.ex t φ) ∧ ∀ x, φ.svc (.atm (.r x)) → A x

/-- The children and finalizers of a region `p` are typed at stages of ancestors of `p`. -/
def LRgs (S : Sig) (Ψ : List Ty) (anc : Nat → Nat → Prop) (Λ : List Rgn) : Prop :=
  ∀ (p : Nat) (g : Rgn), Λ[p]? = some g →
    (∀ k ∈ g.kids, LTy S Ψ (anc p) k) ∧ ∀ f ∈ g.fins, LTy S Ψ (anc p) f

theorem LTy.weaken {Ψ A k} (h : LTy S Ψ A k) (X : List Ty) : LTy S (Ψ ++ X) A k :=
  let ⟨t, φ, h, ha⟩ := h; ⟨t, φ, h.weakenStore X, ha⟩

theorem LTy.mono {Ψ A A' k} (h : LTy S Ψ A k) (hA : ∀ x, A x → A' x) : LTy S Ψ A' k :=
  let ⟨t, φ, h, ha⟩ := h; ⟨t, φ, h, fun x hx => hA x (ha x hx)⟩

/-- Renaming away regions not yet created can only remove stage atoms. -/
theorem LTy.kill {Ψ A k n} {t : Ty} {φ : Stage}
    (h : TypM S .live Ψ Facts.age [] k (.ex (t.substT (kill n)) (φ.substT (kill n))))
    (ha : ∀ x, φ.svc (.atm (.r x)) → A x) : LTy S Ψ A k :=
  ⟨_, _, h, fun x hx => ha x (Stage.kill_atm hx).1⟩

theorem LRgs.toRgs {Ψ anc Λ} (h : LRgs S Ψ anc Λ) : RgsTyM S .live Ψ Λ := fun p g hg =>
  ⟨fun k hk => let ⟨t, φ, h, _⟩ := (h p g hg).1 k hk; ⟨t, φ, h⟩,
    fun f hf => let ⟨t, φ, h, _⟩ := (h p g hg).2 f hf; ⟨t, φ, h⟩⟩

theorem LRgs.weaken {Ψ anc Λ} (h : LRgs S Ψ anc Λ) (X : List Ty) : LRgs S (Ψ ++ X) anc Λ :=
  fun p g hg => ⟨fun k hk => ((h p g hg).1 k hk).weaken X, fun f hf => ((h p g hg).2 f hf).weaken X⟩

theorem LRgs.anc {Ψ anc anc' Λ} (h : LRgs S Ψ anc Λ) (hanc : ∀ q, q < Λ.length → anc' q = anc q) :
    LRgs S Ψ anc' Λ := fun p g hg => by
  rw [hanc p (getElem?_lt hg)]; exact h p g hg

theorem LRgs.set {Ψ anc Λ p g g'} (h : LRgs S Ψ anc Λ) (hg : Λ[p]? = some g)
    (hk : ∀ k ∈ g'.kids, k ∈ g.kids ∨ LTy S Ψ (anc p) k)
    (hf : ∀ f ∈ g'.fins, f ∈ g.fins ∨ LTy S Ψ (anc p) f) : LRgs S Ψ anc (setRgn Λ p g') := by
  intro q g'' hq
  by_cases hpq : p = q
  · subst hpq
    rw [setRgn, List.getElem?_set_self (getElem?_lt hg)] at hq; cases hq
    obtain ⟨hk₀, hf₀⟩ := h p g hg
    exact ⟨fun k hk' => (hk k hk').elim (hk₀ k) id, fun f hf' => (hf f hf').elim (hf₀ f) id⟩
  · rw [setRgn, List.getElem?_set_ne hpq] at hq
    exact h q g'' hq

theorem LRgs.append {Ψ anc Λ} (h : LRgs S Ψ anc Λ) : LRgs S Ψ anc (Λ ++ [Rgn.new]) := by
  intro p g hg
  by_cases hp : p < Λ.length
  · rw [List.getElem?_append_left hp] at hg; exact h p g hg
  · rw [List.getElem?_append_right (by omega)] at hg
    cases hj : p - Λ.length with
    | zero => simp [hj, Rgn.new] at hg; subst hg; simp
    | succ => simp [hj] at hg

theorem LRgs.setKid {Ψ anc Λ p j k} (h : LRgs S Ψ anc Λ) (hk : LTy S Ψ (anc p) k) :
    LRgs S Ψ anc (setKid Λ p j k) := by
  intro q g hq
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  obtain ⟨hk₀, hf₀⟩ := h q g₀ hg₀
  by_cases hqp : q = p
  · subst hqp
    rw [h₁ rfl]
    refine ⟨fun k' hk' => ?_, hf₀⟩
    obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
    rw [List.getElem_set]
    split
    · exact hk
    · exact hk₀ _ (List.getElem_mem _)
  · rw [h₂ hqp]; exact ⟨hk₀, hf₀⟩

/-- The finalizer `open_` registers uses only its region. -/
theorem op_cap_live {Ψ} (p : Nat) : LTy S Ψ (· = p) (.app (.const .op) (.cap p)) := by
  refine ⟨_, _, .app (.const (ι := { (default : CArg) with s := .rg p }) trivial
    TypingMode.subtype.refl) .cap (Stage.le_refl _) TypingMode.subtype.refl, fun x hx => ?_⟩
  rcases hx with hx | hx
  · cases hx; rfl
  · exact hx.elim

/-- What a place of a thread offers a child registered there: a chain of regions holding the
atoms of the thread's stage, the regions of its frames and the handlers whose cells it keeps. -/
def Place (Ψ : List Ty) (anc : Nat → Nat → Prop) (φ : Stage) (E : List Frame)
    (H : Nat → Prop) : Prop :=
  Chain anc H ∧ (∀ x, φ.svc (.atm (.r x)) → H x) ∧
    (∀ F ∈ E, ∀ x, (F = .inside x ∨ F.rgn = some x) → H x) ∧
    ∀ F ∈ E, ∀ q ℓ, F.cell = some (q, ℓ) → H q ∧ ∀ r ε t,
      Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) → ∀ x, r (.atm (.r x)) → anc q x

/-- A child or a finalizer registered at a place runs at a stage of ancestors of its owner. -/
theorem kid_live {Ψ anc E c p w t φ H} (hE : OkE E) (hc : c = .addFin ∨ c = .spawn) (hv : Value w)
    (h : TypM S .live Ψ Facts.age [] (plug E (.app (.app (.const c) (.own p)) w)) (.ex t φ))
    (ha : (plug E (.app (.app (.const c) (.own p)) w)).Asc 0) (hP : Place Ψ anc φ E H) :
    LTy S Ψ (anc p) (withs p E (.await w)) := by
  obtain ⟨hch, hφ, hfr, hcl⟩ := hP
  obtain ⟨hA, -⟩ := asc_of_plug (fin_not_value hc) hE ha
  obtain ⟨t', R', ε', hk, hat⟩ := kid_stage hE hc hv h (Entails.refl _) hA
  have inH : ∀ x, (φ.svc (.atm (.r x)) ∨ ∃ F ∈ E, F = .inside x ∨ F.rgn = some x) → H x :=
    fun x hx => hx.elim (hφ x) fun ⟨F, hF, hd⟩ => hfr F hF x hd
  obtain ⟨K, Θ', t₁, ψ, hr, -, hψ⟩ := hole_atm hE (fin_not_value hc) h
  have hp : H p := inH p (hψ p (fin_atm hc hr))
  have hancp := hch p hp
  refine ⟨t', _, hk, fun x hx => ?_⟩
  rw [hancp]
  rcases hat x hx with ⟨hxp, hin⟩ | ⟨F, hF, q, ℓ, r, ε, t₂, hcF, hqp, hℓ, hr'⟩
  · exact ⟨inH x hin, hxp⟩
  · obtain ⟨hq, hrow⟩ := hcl F hF q ℓ hcF
    have hx' := hrow r ε t₂ hℓ x hr'
    rw [hch q hq] at hx'
    exact ⟨hx'.1, Nat.le_trans hx'.2 hqp⟩

/-- Changes at a runtime redex preserve the stages of stored work. -/
theorem LRgs.rhead {Ψ anc Λ μ r Λ' μ' r'} (hΛ : LRgs S Ψ anc Λ)
    (hself : ∀ p, anc p p) (hh : RHead Λ μ r Λ' μ' r') : LRgs S Ψ anc Λ' := by
  cases hh with
  | scope | hdl => exact hΛ.append
  | open_ hg =>
    refine hΛ.set hg (fun _ hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr ((op_cap_live _).mono fun x h => by subst x; exact hself _)
  | finRun hg _ _ _ hf =>
    refine hΛ.set hg (fun k hk => ?_) fun _ hf' =>
      .inl (by rw [hf]; exact List.mem_append_left _ hf')
    rcases List.mem_append.mp hk with hk | hk
    · exact .inl hk
    · simp at hk; subst hk
      exact .inr ((hΛ _ _ hg).2 _ (by rw [hf]; simp))
  | close hg => exact hΛ.set hg (fun _ hk => .inl hk) (fun _ hf => by simp at hf)
  | _ => exact hΛ

/-- Registration is the only thread step that adds new work. -/
theorem LRgs.step {root Ψ anc Λ μ e t φ l Λ' μ' e'} (hΛ : LRgs S Ψ anc Λ)
    (hself : ∀ p, anc p p) (h : TypM S .live Ψ Facts.age [] e (.ex t φ)) (ha : e.Asc 0)
    (hplace : ∀ E r, OkE E → e = plug E r → ¬ Value r → ∃ H, Place Ψ anc φ E H)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) : LRgs S Ψ anc Λ' := by
  cases hs with
  | addFin hE hg _ hv =>
    obtain ⟨H, hP⟩ := hplace _ _ hE rfl (fin_not_value (by simp))
    refine hΛ.set hg (fun _ hk => .inl hk) fun f hf => ?_
    rcases List.mem_append.mp hf with hf | hf
    · exact .inl hf
    · simp at hf; subst hf
      exact .inr (kid_live hE (.inl rfl) hv h ha hP)
  | spawn hE hg _ hv =>
    obtain ⟨H, hP⟩ := hplace _ _ hE rfl (fin_not_value (by simp))
    refine hΛ.set hg (fun k hk => ?_) fun _ hf => .inl hf
    rcases List.mem_append.mp hk with hk | hk
    · exact .inl hk
    · simp at hk; subst hk
      exact .inr (kid_live hE (.inr rfl) hv h ha hP)
  | rt _ hh => exact hΛ.rhead hself hh
  | _ => exact hΛ

end Flick
