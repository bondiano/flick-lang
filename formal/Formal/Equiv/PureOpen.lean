import Formal.Equiv.Pure

/-!
# Contextual equivalence: free-variable helpers and `PureOpen`
-/

namespace Flick.Equiv

theorem free_liftR_inv {n : Nat} {ξ : Nat → Nat} {i j : Nat} (h : liftR n ξ i = j) :
    (i < n ∧ i = j) ∨ (n ≤ i ∧ ∃ k, i = k + n ∧ j = ξ k + n) := by
  unfold liftR at h
  split at h
  · exact .inl ⟨‹_›, h⟩
  · refine .inr ⟨by omega, i - n, by omega, by omega⟩

theorem free_rename_inv : ∀ (e : Tm) (ξ : Nat → Nat) (j : Nat),
    (e.rename ξ).Free j → ∃ i, e.Free i ∧ ξ i = j := by
  have bind : ∀ {n : Nat} {b : Tm} {ξ : Nat → Nat} {j : Nat},
      (∀ ξ' j', (b.rename ξ').Free j' → ∃ i, b.Free i ∧ ξ' i = j') →
      (b.rename (liftR n ξ)).Free (j + n) → ∃ i, b.Free (i + n) ∧ ξ i = j := by
    intro n b ξ j ih h
    obtain ⟨i, hi, heq⟩ := ih (liftR n ξ) (j + n) h
    rcases free_liftR_inv heq with ⟨hlt, he⟩ | ⟨_, k, rfl, he⟩
    · omega
    · exact ⟨k, hi, by omega⟩
  intro e
  induction e with
  | var k => intro ξ j h; exact ⟨k, rfl, h⟩
  | lam b ih | fix b ih | scope b ih =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢; exact bind ih h
  | let_ a b ih₁ ih₂ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, rfl⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := bind ih₂ h; exact ⟨i, .inr hi, rfl⟩
  | letrec a b ih₁ ih₂ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, rfl⟩ := bind ih₁ h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := bind ih₂ h; exact ⟨i, .inr hi, rfl⟩
  | acons p b r ih₁ ih₂ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, rfl⟩ := bind ih₁ h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₂ ξ j h; exact ⟨i, .inr hi, rfl⟩
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h
    · obtain ⟨i, hi, rfl⟩ := bind ih₁ h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := bind ih₂ h; exact ⟨i, .inr (.inl hi), rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₃ ξ j h; exact ⟨i, .inr (.inr hi), rfl⟩
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | hres _ _ _ _ ih₁ ih₂ | hkeep _ _ _ _ _ ih₁ ih₂ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, rfl⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₂ ξ j h; exact ⟨i, .inr hi, rfl⟩
  | proj _ _ ih | task _ ih | await _ ih | lazy _ ih | force _ ih | raise _ ih | inside _ _ ih
  | drec _ _ ih | hdone _ _ ih | unw _ _ _ ih | settle _ _ ih =>
    intro ξ j h; exact ih ξ j h
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ _ ih₁ ih₂ ih₃ | hkarm _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h
    · obtain ⟨i, hi, rfl⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₂ ξ j h; exact ⟨i, .inr (.inl hi), rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₃ ξ j h; exact ⟨i, .inr (.inr hi), rfl⟩
  | hctl _ _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro ξ j h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h | h
    · obtain ⟨i, hi, rfl⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₂ ξ j h; exact ⟨i, .inr (.inl hi), rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₃ ξ j h; exact ⟨i, .inr (.inr (.inl hi)), rfl⟩
    · obtain ⟨i, hi, rfl⟩ := ih₄ ξ j h; exact ⟨i, .inr (.inr (.inr hi)), rfl⟩
  | _ => intro ξ j h; simp [Tm.rename, Tm.Free] at h

theorem free_rename_fwd : ∀ (e : Tm) (ξ : Nat → Nat) (i : Nat),
    e.Free i → (e.rename ξ).Free (ξ i) := by
  have bind : ∀ {n : Nat} {b : Tm} {ξ : Nat → Nat} {i : Nat},
      (∀ ξ' k, b.Free k → (b.rename ξ').Free (ξ' k)) →
      b.Free (i + n) → (b.rename (liftR n ξ)).Free (ξ i + n) := by
    intro n b ξ i ih h
    have := ih (liftR n ξ) (i + n) h
    have hl : liftR n ξ (i + n) = ξ i + n := by
      unfold liftR; split
      · omega
      · rw [Nat.add_sub_cancel]
    rwa [hl] at this
  intro e
  induction e with
  | var k => intro ξ i h; subst h; rfl
  | lam b ih | fix b ih | scope b ih =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢; exact bind ih h
  | let_ a b ih₁ ih₂ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · exact .inl (ih₁ ξ i h)
    · exact .inr (bind ih₂ h)
  | letrec a b ih₁ ih₂ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · exact .inl (bind ih₁ h)
    · exact .inr (bind ih₂ h)
  | acons p b r ih₁ ih₂ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · exact .inl (bind ih₁ h)
    · exact .inr (ih₂ ξ i h)
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h
    · exact .inl (bind ih₁ h)
    · exact .inr (.inl (bind ih₂ h))
    · exact .inr (.inr (ih₃ ξ i h))
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | hres _ _ _ _ ih₁ ih₂ | hkeep _ _ _ _ _ ih₁ ih₂ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h
    · exact .inl (ih₁ ξ i h)
    · exact .inr (ih₂ ξ i h)
  | proj _ _ ih | task _ ih | await _ ih | lazy _ ih | force _ ih | raise _ ih | inside _ _ ih
  | drec _ _ ih | hdone _ _ ih | unw _ _ _ ih | settle _ _ ih =>
    intro ξ i h; exact ih ξ i h
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ _ ih₁ ih₂ ih₃ | hkarm _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h
    · exact .inl (ih₁ ξ i h)
    · exact .inr (.inl (ih₂ ξ i h))
    · exact .inr (.inr (ih₃ ξ i h))
  | hctl _ _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro ξ i h; simp only [Tm.rename, Tm.Free] at h ⊢
    rcases h with h | h | h | h
    · exact .inl (ih₁ ξ i h)
    · exact .inr (.inl (ih₂ ξ i h))
    · exact .inr (.inr (.inl (ih₃ ξ i h)))
    · exact .inr (.inr (.inr (ih₄ ξ i h)))
  | _ => intro ξ i h; simp [Tm.Free] at h

theorem free_subst_inv : ∀ (e : Tm) (σ : Nat → Tm) (j : Nat),
    (e.subst σ).Free j → ∃ i, e.Free i ∧ (σ i).Free j := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm} {j : Nat},
      (∀ σ' j', (b.subst σ').Free j' → ∃ i, b.Free i ∧ (σ' i).Free j') →
      (b.subst (liftS n σ)).Free (j + n) → ∃ i, b.Free (i + n) ∧ (σ i).Free j := by
    intro n b σ j ih h
    obtain ⟨i, hi, hsi⟩ := ih (liftS n σ) (j + n) h
    unfold liftS at hsi
    split at hsi
    · simp only [Tm.Free] at hsi; omega
    · obtain ⟨k, hk, heq⟩ := free_rename_inv _ _ _ hsi
      have : k = j := by omega
      subst this
      exact ⟨i - n, by rwa [Nat.sub_add_cancel (by omega)], hk⟩
  intro e
  induction e with
  | var k => intro σ j h; exact ⟨k, rfl, h⟩
  | lam b ih | fix b ih | scope b ih =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢; exact bind ih h
  | let_ a b ih₁ ih₂ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, hs⟩ := ih₁ σ j h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := bind ih₂ h; exact ⟨i, .inr hi, hs⟩
  | letrec a b ih₁ ih₂ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, hs⟩ := bind ih₁ h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := bind ih₂ h; exact ⟨i, .inr hi, hs⟩
  | acons p b r ih₁ ih₂ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, hs⟩ := bind ih₁ h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₂ σ j h; exact ⟨i, .inr hi, hs⟩
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h | h
    · obtain ⟨i, hi, hs⟩ := bind ih₁ h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := bind ih₂ h; exact ⟨i, .inr (.inl hi), hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₃ σ j h; exact ⟨i, .inr (.inr hi), hs⟩
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | hres _ _ _ _ ih₁ ih₂ | hkeep _ _ _ _ _ ih₁ ih₂ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h
    · obtain ⟨i, hi, hs⟩ := ih₁ σ j h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₂ σ j h; exact ⟨i, .inr hi, hs⟩
  | proj _ _ ih | task _ ih | await _ ih | lazy _ ih | force _ ih | raise _ ih | inside _ _ ih
  | drec _ _ ih | hdone _ _ ih | unw _ _ _ ih | settle _ _ ih =>
    intro σ j h; exact ih σ j h
  | hdl _ _ _ _ ih₁ ih₂ ih₃ | hwith _ _ _ _ _ _ ih₁ ih₂ ih₃ | hkarm _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h | h
    · obtain ⟨i, hi, hs⟩ := ih₁ σ j h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₂ σ j h; exact ⟨i, .inr (.inl hi), hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₃ σ j h; exact ⟨i, .inr (.inr hi), hs⟩
  | hctl _ _ _ _ _ _ _ _ _ ih₁ ih₂ ih₃ ih₄ =>
    intro σ j h; simp only [Tm.subst, Tm.Free] at h ⊢
    rcases h with h | h | h | h
    · obtain ⟨i, hi, hs⟩ := ih₁ σ j h; exact ⟨i, .inl hi, hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₂ σ j h; exact ⟨i, .inr (.inl hi), hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₃ σ j h; exact ⟨i, .inr (.inr (.inl hi)), hs⟩
    · obtain ⟨i, hi, hs⟩ := ih₄ σ j h; exact ⟨i, .inr (.inr (.inr hi)), hs⟩
  | _ => intro σ j h; simp [Tm.subst, Tm.Free] at h

theorem closed_subst_all {e : Tm} {σ : Nat → Tm} (hσ : ∀ i, (σ i).Closed) : (e.subst σ).Closed :=
  fun j hj => let ⟨i, _, hi⟩ := free_subst_inv e σ j hj; hσ i j hi

theorem closed_fix_let_lazy_up2 {e b : Tm}
    (h : (Tm.fix (.let_ (.lazy (e.rename (· + 2))) b)).Closed) : e.Closed :=
  fun i hi => h i (Or.inl (free_rename_fwd e (· + 2) i hi))

theorem closed_let_lazy {e b : Tm} (h : (Tm.let_ (.lazy e) b).Closed) : e.Closed :=
  fun i hi => h i (Or.inl hi)

theorem closed_let_subst_loc {v b : Tm} (h : (Tm.let_ v b).Closed) (l : Nat) :
    (b.subst (inst [.loc l])).Closed := fun j hj => by
  obtain ⟨i, hi, hs⟩ := free_subst_inv b (inst [.loc l]) j hj
  cases i with
  | zero => simp [inst, Tm.Free] at hs
  | succ k =>
    simp only [inst] at hs
    subst hs
    exact h k (.inr hi)

theorem closed_frame_of {F : Frame} {a b : Tm} (hF : (F.plug a).Closed) (hb : b.Closed) :
    (F.plug b).Closed := by
  cases F <;> intro i hi <;> simp only [Frame.plug, Tm.Free] at hF hi
  all_goals first
    | exact hb i hi
    | (rcases hi with h | h; exact hb i h; exact hF i (.inr h))
    | (rcases hi with h | h; exact hF i (.inl h); exact hb i h)
    | (rcases hi with h | h | h; exact hF i (.inl h); exact hF i (.inr (.inl h)); exact hb i h)
    | (rcases hi with h | h | h; exact hF i (.inl h); exact hb i h; exact hF i (.inr (.inr h)))
    | (rcases hi with h | h | h | h; exact hF i (.inl h); exact hF i (.inr (.inl h));
       exact hb i h; exact hF i (.inr (.inr (.inr h))))

theorem closed_frame_inner {F : Frame} {a : Tm} (hF : (F.plug a).Closed) : a.Closed := by
  cases F <;> intro i hi <;> simp only [Frame.plug] at hF
  all_goals first
    | exact hF i hi
    | exact hF i (.inl hi)
    | exact hF i (.inr hi)
    | exact hF i (.inr (.inl hi))
    | exact hF i (.inr (.inr hi))
    | exact hF i (.inr (.inr (.inl hi)))

theorem closed_plug_of : ∀ {E : List Frame} {a b : Tm}, (plug E a).Closed → b.Closed →
    (plug E b).Closed
  | [], _, _, _, hb => hb
  | F :: E, a, b, h, hb => by
    simp only [Flick.plug] at h ⊢
    exact closed_plug_of h (closed_frame_of (closed_plug_inner_aux h) hb)
where
  closed_plug_inner_aux : ∀ {E : List Frame} {a : Tm}, (plug E a).Closed → a.Closed
    | [], _, h => h
    | _ :: _, _, h => closed_frame_inner (closed_plug_inner_aux h)

/-- An open term whose closed-value instantiations all evaluate purely. -/
def PureOpen (root : Roots) (e : Tm) : Prop :=
  ∀ σ : Nat → Tm, (∀ i, Value (σ i) ∧ (σ i).Closed) → PureEval root (e.subst σ)

theorem PureOpen.of_closed {root : Roots} {e : Tm} (hc : e.Closed) (hp : PureEval root e) :
    PureOpen root e :=
  fun σ _ => by rw [hc.subst]; exact hp

theorem PureOpen.eval {root : Roots} {e : Tm} (h : PureOpen root e) (hc : e.Closed) :
    PureEval root e := by
  have := h (fun _ => .lit .unit) (fun _ => ⟨.lit, fun _ hi => hi⟩)
  rwa [hc.subst] at this

theorem PureOpen.rename {root : Roots} {e : Tm} (h : PureOpen root e) (ξ : Nat → Nat) :
    PureOpen root (e.rename ξ) :=
  fun σ hσ => by rw [Tm.subst_rename]; exact h (σ ∘ ξ) (fun i => hσ (ξ i))

theorem PureOpen.subst {root : Roots} {e : Tm} (h : PureOpen root e) {σ : Nat → Tm}
    (hσ : ∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) : PureOpen root (e.subst σ) := by
  intro τ hτ
  rw [Tm.subst_subst]
  refine h (fun i => (σ i).subst τ) fun i => ?_
  dsimp only
  rcases hσ i with ⟨j, hj⟩ | hv
  · rw [hj]; exact hτ j
  · exact ⟨hv.subst, closed_subst_all (fun k => (hτ k).2)⟩


end Flick.Equiv
