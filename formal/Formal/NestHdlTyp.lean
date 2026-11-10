import Formal.NestKept

/-!
# What the types of a handler's redex give `⊢ Λ`

What goes into a body or an arm from outside has a type without regions: the argument
of an operation, of `resume`, the value of a `tail` arm. What a handler gives back and
raises names no region younger than the handler.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

/-- The hole of a context, at a run-time stage, typed in the store of the context. -/
theorem hole_typ0 {Θ E r t φ} (hE : OkE E) (hv : ¬ Value r) (h : Typ S Ψ Θ [] (plug E r) (.ex t φ))
    (hnp : ∀ {K Θ' t'}, ¬ Typ S (Ψ.map (Ty.up K)) Θ' [] r (.ex t' .pure)) :
    ∃ Θ' t' φ', Typ S Ψ Θ' [] r (.ex t' φ') := by
  obtain ⟨K, Θ', t', φ', hr, -, hK, -⟩ := ctx_typing E hE hv h
  rcases hK with hp | ⟨rfl, -⟩
  · subst hp; exact (hnp hr).elim
  · exact ⟨Θ', t', φ', by simpa using hr⟩

theorem op_YF {E o : Name} {c : OpClass} {a b : Ty} (hop : S.op E o = some (c, a, b)) :
    a.YF 0 ∧ b.YF 0 :=
  ⟨(S.op_closed _ _ _ _ _ _ hop).1, (S.op_closed _ _ _ _ _ _ hop).2⟩

/-- The argument of an operation at the hole of a context has a type without regions. -/
theorem perform_arg {Θ Ei Ef o c v t φ} (hEi : OkE Ei)
    (h : Typ S Ψ Θ [] (plug Ei (.app (.const (.perform Ef o c)) v)) (.ex t φ)) :
    ∃ Θ' T φ', Typ S Ψ Θ' [] v (.ex T φ') ∧ T.YF 0 := by
  obtain ⟨K, Θ', t', φ', hr, -, hK, -⟩ := ctx_typing Ei hEi perform_not_value h
  cases hr with
  | app hf hx hψ _ =>
    cases hf with
    | perform hop hT =>
      obtain ⟨_, _, _, _, he, la, lψ, _, -⟩ := hT.arrow_left
      cases he
      rcases hK with hp | ⟨rfl, -⟩
      · exact absurd (Stage.le_trans lψ (hp ▸ hψ)) id
      · simp only [List.map_up_zero] at hx
        exact ⟨_, _, _, hx.sub ⟨la, Stage.le_refl _⟩, (op_YF hop).1⟩
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

/-- The body of a handler in the store of its context. -/
theorem hwith_body {Θ E p ℓ Ef al rf e t φ} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E (.hwith p ℓ Ef al rf e)) (.ex t φ)) :
    ∃ Θ' t' φ', Typ S Ψ Θ' [] e (.ex t' φ') := by
  obtain ⟨Θ', t', φ', hr⟩ := hole_typ0 hE (fun h => by cases h) h fun h => hwith_not_pure h
  cases hr with
  | hwith _ _ _ _ he _ _ _ _ => exact ⟨_, _, _, he⟩
  | errVal hv | allI hv => cases hv

/-- The arm of a handler in the store of its context. -/
theorem hctl_typ {Θ E p ℓ κ tl Ef al rf a k t φ} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E (.hctl p ℓ κ tl Ef al rf a k)) (.ex t φ)) :
    ∃ Θ' t' φ', Typ S Ψ Θ' [] (.hctl p ℓ κ tl Ef al rf a k) (.ex t' φ') :=
  hole_typ0 hE (fun h => by cases h) h fun h => hctl_not_pure h

/-- The body of a kept handler in the store of its context. -/
theorem hkeep_body {Θ E q ℓ Ef al e t φ} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E (.hkeep q ℓ Ef al e)) (.ex t φ)) :
    ∃ Θ' t' φ', Typ S Ψ Θ' [] e (.ex t' φ') := by
  obtain ⟨Θ', t', φ', hr⟩ := hole_typ0 hE (fun h => by cases h) h fun h => hkeep_not_pure h
  cases hr with
  | hkeep _ _ he _ _ _ _ _ _ => exact ⟨_, _, _, he⟩
  | errVal hv | allI hv => cases hv

theorem hkarm_not_pure {Θ q ℓ E al a k t} (h : Typ S Ψ Θ [] (.hkarm q ℓ E al a k) (.ex t .pure)) :
    False := by
  cases h with
  | hkarm _ _ _ _ _ _ _ _ _ _ hle hφ _ => exact Stage.le_trans hle hφ
  | errVal hv | allI hv => cases hv

/-- The value of an arm of a kept handler has a type without regions. -/
theorem kept_val {Θ E q ℓ Ef al w k t φ} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E (.hkarm q ℓ Ef al w k)) (.ex t φ)) :
    ∃ Θ' T φ', Typ S Ψ Θ' [] w (.ex T φ') ∧ T.YF 0 := by
  obtain ⟨Θ', t', φ', hr⟩ := hole_typ0 hE (fun h => by cases h) h fun h => hkarm_not_pure h
  cases hr with
  | hkarm _ _ hop _ _ _ _ _ _ ha _ _ _ => exact ⟨_, _, _, ha, (op_YF hop).2⟩
  | errVal hv | allI hv => cases hv

/-- What an arm of a kept handler raises has a type of the errors of its cell. -/
theorem kept_raise {Θ E q ℓ Ef al w k t φ} (hE : OkE E) (hw : Value w)
    (h : Typ S Ψ Θ [] (plug E (.hkarm q ℓ Ef al (.raise w) k)) (.ex t φ)) :
    ∃ Θ' a r ε t' C, Ψ[ℓ]? = some (.arrow a (.rt r ε) t' C) ∧
      Typ S Ψ Θ' [] w (.ex (.errors ε) .pure) := by
  obtain ⟨Θ', t', φ', hr⟩ := hole_typ0 hE (fun h => by cases h) h fun h => hkarm_not_pure h
  cases hr with
  | hkarm hℓ _ _ _ _ _ _ _ _ ha _ _ _ => exact ⟨_, _, _, _, _, _, hℓ, raise_errors hw ha⟩
  | errVal hv | allI hv => cases hv

/-- The value of a `tail` arm has a type without regions. -/
theorem tail_val {Θ p ℓ κ Ef al rf w k t φ} (h : Typ S Ψ Θ [] (.hctl p ℓ κ true Ef al rf w k) (.ex t φ)) :
    ∃ T φ', Typ S Ψ Θ [] w (.ex T φ') ∧ T.YF 0 := by
  cases h with
  | hctl _ _ _ _ hop _ htl _ _ _ _ ha _ _ _ _ =>
    exact ⟨_, _, by simpa using ha, (op_YF hop).2⟩
  | errVal hv | allI hv => cases hv

/-- The argument of `resume` has a type without regions. -/
theorem resume_arg {Θ p ℓ κ Ef al rf u k t φ} {A : List Frame} (hA : OkE A)
    (h : Typ S Ψ Θ [] (.hctl p ℓ κ false Ef al rf (plug A (.app (.kont p κ) u)) k) (.ex t φ)) :
    ∃ Θ' T φ', Typ S Ψ Θ' [] u (.ex T φ') ∧ T.YF 0 := by
  cases h with
  | hctl _ _ _ _ hop _ _ _ _ _ _ ha hl _ _ _ =>
    obtain ⟨C', hl₁, -⟩ := hl rfl
    rw [if_neg (by decide)] at ha
    obtain ⟨K, Θ', t', φ', hr, -, hK, -⟩ := ctx_typing A hA kont_not_value ha
    cases hr with
    | app hf hx hψ _ =>
      obtain ⟨hψp, hb⟩ := kont_fn (S := S) hl₁ hf
      rcases hK with hp | ⟨rfl, -⟩
      · exact (hψp (Stage.eq_pure (hp ▸ hψ))).elim
      · simp only [List.map_up_zero] at hx
        exact ⟨_, _, _, hx.sub ⟨(hb rfl).1, Stage.le_refl _⟩, (op_YF hop).2⟩
    | errVal hv | allI hv => exact (kont_not_value hv).elim
  | errVal hv | allI hv => cases hv

/-- What a resumed body gives back names no region younger than its handler. -/
theorem hres_val {Θ p ℓ a w t φ} (hc : HCell Ψ p ℓ) (h : Typ S Ψ Θ [] (.hres p ℓ a w) (.ex t φ)) :
    ∃ T φ', Typ S Ψ Θ [] w (.ex T φ') ∧ ∀ x, T.reach (.r x) → x < p := by
  cases h with
  | hres hℓ hb _ _ _ _ _ _ _ => exact ⟨_, _, hb, fun x hx => (hc.res hℓ).1.reach hx⟩
  | errVal hv | allI hv => cases hv

/-- What a resumed body raises names no region younger than its handler. -/
theorem hres_raise {Θ p ℓ a w t φ} (hc : HCell Ψ p ℓ) (hw : Value w)
    (h : Typ S Ψ Θ [] (.hres p ℓ a (.raise w)) (.ex t φ)) :
    ∃ T φ', Typ S Ψ Θ [] w (.ex T φ') ∧ ∀ x, T.reach (.r x) → x < p := by
  cases h with
  | hres hℓ hb _ _ _ _ _ _ _ =>
    obtain ⟨ε', hw', hε⟩ := raise_inv hw hb
    obtain ⟨_, _, he, -, hs⟩ := Stage.rt_le hε
    cases he
    exact ⟨_, _, hw', fun x hx => (hc.res hℓ).2.reach (hx.imp (hs _) (hs _))⟩
  | errVal hv | allI hv => cases hv

theorem hres_not_pure {Θ p ℓ a b t} (h : Typ S Ψ Θ [] (.hres p ℓ a b) (.ex t .pure)) : False := by
  cases h with
  | hres _ _ _ _ _ _ _ hφ _ => exact hφ
  | errVal hv | allI hv => cases hv

end Flick
