import Formal.Equiv.Values

/-!
# Contextual equivalence: the right side around the focus of the left one

A step of the left side happens at a focus `r` in an ordered context `E`. The right side, after
it catches up (`RJ`), is either a special pair whose left side holds the focus (`Act`), with
related frames around it, or related frames around a term related to the focus by a congruence,
at `top` (`Dec`, `Sim.dec`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

/-- The special pairs whose left side steps while the right side waits or follows. -/
inductive Act (W : World) : Tm → Tm → Prop
  | dropA {e b c c'} : b = c.rename (· + 1) → Sim W .any c c' → Act W (.let_ (.lazy e) b) c'
  | dropB {n b c c'} : W.J₁ n → b = c.rename (· + 1) → Sim W .any c c' →
      Act W (.let_ (.loc n) b) c'
  | forceA {e e'} : Sim W .any e e' → Act W (.force (.lazy e)) e'
  | bornA {v v'} : W.B → Value v → Value v' → Sim W .any v v' →
      Act W (.lazy v) (.let_ (.lazy v') (.let_ (.force (.var 0)) (.var 1)))
  | bornB {v v'} : W.B → Value v → Value v' → Sim W .any v v' →
      Act W (.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))) (.lazy v')
  | bornS {l l' v v' t} : W.B → W.L l l' (.born v v') → v.Closed → BornSt l v t → Act W t (.loc l')
  | bornV {l l' v v'} : W.B → W.L l l' (.born v v') → v.Closed → v'.Closed → v'.NoIns →
      Act W (.settle l v) v'
  | dead {b} : W.B → b.NoIns → Act W .todo b
  | o4B {e e' b b' br f'} : W.B → br = b.rename rot → f' = .let_ (.lazy (e'.rename (· + 2))) b' →
      PureOpen W.R e → PureOpen W.R e' → Sim W .any e e' → Sim W .any b b' →
      e'.NoIns → b'.NoIns →
      Act W (.let_ (.lazy e) (.fix br)) (.fix f')
  | o4Alloc1 {l' e e' c c' s'} : W.B → W.HR l' e' → s' = c'.subst (inst [.loc l']) →
      e.Closed → PureEval W.R e → Sim W .any e e' → Sim W .any c c' → c'.NoIns →
      Act W (.let_ (.lazy e) c) s'
  | o4Bind {l c c'} : W.B → Sim W .any (c.subst (inst [.loc l])) c' →
      Act W (.let_ (.loc l) c) c'
  | o4Settle {l l' e t} : W.B → W.H l l' → W.HL l e → e.Closed → t.Closed → PSteps W.R e t →
      Act W (.settle l t) (.force (.loc l'))
  | doneF {l l' : Nat} {v v' : Tm} : ¬ W.B → W.D l l' v v' → Value v' → v'.Closed → v'.NoIns →
      Act W (.force (.loc l)) v'
  | againF1 {c₀ e e' : Tm} {σ σ' : Nat → Tm} : ¬ W.B → c₀.Src →
      (∀ y, y ≠ .park → Sim W y e e') →
      ((∃ j, e = .var j) ∨ Value e) → ((∃ j, e' = .var j) ∨ Value e') →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ (.force e) (c₀.subst (subForce e σ)))
        (.let_ (.force e') (c₀.subst (subVar σ')))
  | againF2 {l l' : Nat} {t t' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.L l l' .sym →
      Final t → Sim W .any t t' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ (.settle l t) (c₀.subst (subForce (.loc l) σ)))
        (.let_ (.settle l' t') (c₀.subst (subVar σ')))
  | againF3 {l l' : Nat} {v v' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.D l l' v v' →
      Value v → Value v' → v.Closed → v'.Closed → v'.NoIns → Sim W .any v v' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ v (c₀.subst (subForce (.loc l) σ)))
        (.let_ v' (c₀.subst (subVar σ')))
  | againR1 {c₀ e e' : Tm} {σ σ' : Nat → Tm} : ¬ W.B → c₀.Src →
      (∀ y, y ≠ .park → Sim W y e e') →
      ((∃ j, e = .var j) ∨ Value e) → ((∃ j, e' = .var j) ∨ Value e') →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ (.force e) (c₀.subst (subVar σ)))
        (.let_ (.force e') (c₀.subst (subForce e' σ')))
  | againR2 {l l' : Nat} {t t' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.L l l' .sym →
      Final t → Sim W .any t t' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ (.settle l t) (c₀.subst (subVar σ)))
        (.let_ (.settle l' t') (c₀.subst (subForce (.loc l') σ')))
  | againR3 {l l' : Nat} {v v' c₀ : Tm} {σ σ' : Nat → Tm} : ¬ W.B → W.D l l' v v' →
      Value v → Value v' → v.Closed → v'.Closed → v'.NoIns → Sim W .any v v' → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      Act W (.let_ v (c₀.subst (subVar σ)))
        (.let_ v' (c₀.subst (subForce (.loc l') σ')))
  | letRaise {v v' b₁ b₂ : Tm} : ¬ W.B → Value v → Value v' → Sim W .any v v' →
      Act W (.let_ (.raise v) b₁) (.let_ (.raise v') b₂)
  | letUnw {p ℓ ℓ' : Nat} {k : Kd} {a a' b₁ b₂ : Tm} : ¬ W.B → W.L ℓ ℓ' k → Sim W .any a a' →
      Act W (.let_ (.unw p ℓ a) b₁) (.let_ (.unw p ℓ' a') b₂)
  | letTodo {b₁ b₂ : Tm} : ¬ W.B → Act W (.let_ .todo b₁) (.let_ .todo b₂)

/-- The right side, caught up around the focus `r` in `E` of the left one: a special pair holds
the focus; or a congruence relates it at `top`; or it is the frame of a left cell around a term
related to the right side. -/
def Dec (W : World) (μ : List Tm) (t : Tm) (E : List Frame) (r : Tm) : Prop :=
  (∃ W' μ' E₀ E₁ a b E₂, RJ W μ t W' μ' (plug E₂ b) ∧ E = E₀ ++ E₁ ∧ a = plug E₀ r ∧
      FrL W' E₁ E₂ ∧ OkE E₂ ∧ Act W' a b) ∨
    (∃ W' μ' E₂ r₂, RJ W μ t W' μ' (plug E₂ r₂) ∧ FrL W' E E₂ ∧ OkE E₂ ∧ Sim W' .top r r₂) ∨
    ∃ W' μ' E₂ n a b, RJ W μ t W' μ' (plug E₂ b) ∧ FrL W' E E₂ ∧ OkE E₂ ∧ r = .settle n a ∧
      W'.J₁ n ∧ Sim W' .any a b

theorem Dec.pre {W₁ : World} {μ μ₁ : List Tm} {t t₁ : Tm} {E : List Frame} {r : Tm}
    (h : RJ W μ t W₁ μ₁ t₁) (hd : Dec W₁ μ₁ t₁ E r) : Dec W μ t E r := by
  rcases hd with ⟨W', μ', E₀, E₁, a, b, E₂, hj, he, ha, hf, hk, hb⟩ |
    ⟨W', μ', E₂, r₂, hj, hf, hk, hs⟩ | ⟨W', μ', E₂, n, a, b, hj, hf, hk, hr, hn, hs⟩
  · exact .inl ⟨W', μ', E₀, E₁, a, b, E₂, h.trans hj, he, ha, hf, hk, hb⟩
  · exact .inr (.inl ⟨W', μ', E₂, r₂, h.trans hj, hf, hk, hs⟩)
  · exact .inr (.inr ⟨W', μ', E₂, n, a, b, h.trans hj, hf, hk, hr, hn, hs⟩)

/-- The frame of a right cell around the right side. -/
theorem Dec.settle {μ : List Tm} {t : Tm} {E : List Frame} {r : Tm} {n : Nat} (hn : W.J₂ n)
    (hd : Dec W μ t E r) : Dec W μ (.settle n t) E r := by
  rcases hd with ⟨W', μ', E₀, E₁, a, b, E₂, hj, he, ha, hf, hk, hb⟩ |
    ⟨W', μ', E₂, r₂, hj, hf, hk, hs⟩ | ⟨W', μ', E₂, m, a, b, hj, hf, hk, hr, hm, hs⟩
  · refine .inl ⟨W', μ', E₀, E₁, a, b, E₂ ++ [.settle n], ?_, he, ha, ?_, okE_snoc (F := .settle n) hk trivial, hb⟩
    · rw [plug_snoc]; exact hj.frame (F := .settle n) trivial
    · simpa using hf.append (FrL.junkR (hj.le.J₂ _ hn) .nil)
  · refine .inr (.inl ⟨W', μ', E₂ ++ [.settle n], r₂, ?_, ?_, okE_snoc (F := .settle n) hk trivial, hs⟩)
    · rw [plug_snoc]; exact hj.frame (F := .settle n) trivial
    · simpa using hf.append (FrL.junkR (hj.le.J₂ _ hn) .nil)
  · refine .inr (.inr ⟨W', μ', E₂ ++ [.settle n], m, a, b, ?_, ?_, okE_snoc (F := .settle n) hk trivial,
      hr, hm, hs⟩)
    · rw [plug_snoc]; exact hj.frame (F := .settle n) trivial
    · simpa using hf.append (FrL.junkR (hj.le.J₂ _ hn) .nil)

/-- The frame of a left cell around the focus. -/
theorem Dec.junkL {μ : List Tm} {t : Tm} {E : List Frame} {r : Tm} {n : Nat} (hn : W.J₁ n)
    (hd : Dec W μ t E r) : Dec W μ t (E ++ [.settle n]) r := by
  rcases hd with ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, ha, hf, hk, hb⟩ |
    ⟨W', μ', E₂, r₂, hj, hf, hk, hs⟩ | ⟨W', μ', E₂, m, a, b, hj, hf, hk, hr, hm, hs⟩
  · exact .inl ⟨W', μ', E₀, E₁ ++ [.settle n], a, b, E₂, hj, by simp, ha,
      by simpa using hf.append (FrL.junkL (hj.le.J₁ _ hn) .nil), hk, hb⟩
  · exact .inr (.inl ⟨W', μ', E₂, r₂, hj,
      by simpa using hf.append (FrL.junkL (hj.le.J₁ _ hn) .nil), hk, hs⟩)
  · exact .inr (.inr ⟨W', μ', E₂, m, a, b, hj,
      by simpa using hf.append (FrL.junkL (hj.le.J₁ _ hn) .nil), hk, hr, hm, hs⟩)

/-- A related frame around the right side. -/
theorem Dec.frame {μ : List Tm} {t : Tm} {E : List Frame} {r : Tm} {F F' : Frame}
    (hF : ∀ W', W.le W' → FrS W' F F') (hF' : F'.ok) (hd : Dec W μ t E r) :
    Dec W μ (F'.plug t) (E ++ [F]) r := by
  rcases hd with ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, ha, hf, hk, hb⟩ |
    ⟨W', μ', E₂, r₂, hj, hf, hk, hs⟩ | ⟨W', μ', E₂, m, a, b, hj, hf, hk, hr, hm, hs⟩
  · refine .inl ⟨W', μ', E₀, E₁ ++ [F], a, b, E₂ ++ [F'], ?_, by simp, ha,
      hf.append (.cons (hF _ hj.le) .nil), okE_snoc hk hF', hb⟩
    rw [plug_snoc]; exact hj.frame hF'
  · refine .inr (.inl ⟨W', μ', E₂ ++ [F'], r₂, ?_, hf.append (.cons (hF _ hj.le) .nil),
      okE_snoc hk hF', hs⟩)
    rw [plug_snoc]; exact hj.frame hF'
  · refine .inr (.inr ⟨W', μ', E₂ ++ [F'], m, a, b, ?_, hf.append (.cons (hF _ hj.le) .nil),
      okE_snoc hk hF', hr, hm, hs⟩)
    rw [plug_snoc]; exact hj.frame hF'

/-- A coupled `settle` + `let` pair of O2a around the right side. -/
theorem Dec.pair {μ : List Tm} {t : Tm} {E : List Frame} {r : Tm} {l l' : Nat} {F F' : Frame}
    (hp : ∀ W', W.le W' → FrL W' [.settle l, F] [.settle l', F']) (hF' : F'.ok)
    (hd : Dec W μ t E r) :
    Dec W μ (F'.plug (.settle l' t)) (E ++ [.settle l, F]) r := by
  have hok : ∀ {E₂ : List Frame}, OkE E₂ → OkE (E₂ ++ [.settle l', F']) := fun hk G hG => by
    rcases List.mem_append.mp hG with hG | hG
    · exact hk G hG
    · simp only [List.mem_cons, List.not_mem_nil, or_false] at hG
      rcases hG with rfl | rfl
      · trivial
      · exact hF'
  rcases hd with ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, ha, hf, hk, hb⟩ |
    ⟨W', μ', E₂, r₂, hj, hf, hk, hs⟩ | ⟨W', μ', E₂, m, a, b, hj, hf, hk, hr, hm, hs⟩
  · refine .inl ⟨W', μ', E₀, E₁ ++ [.settle l, F], a, b, E₂ ++ [.settle l', F'], ?_, by simp, ha,
      hf.append (hp _ hj.le), hok hk, hb⟩
    rw [plug_append]; exact (hj.frame (F := .settle l') trivial).frame hF'
  · refine .inr (.inl ⟨W', μ', E₂ ++ [.settle l', F'], r₂, ?_, hf.append (hp _ hj.le),
      hok hk, hs⟩)
    rw [plug_append]; exact (hj.frame (F := .settle l') trivial).frame hF'
  · refine .inr (.inr ⟨W', μ', E₂ ++ [.settle l', F'], m, a, b, ?_, hf.append (hp _ hj.le),
      hok hk, hr, hm, hs⟩)
    rw [plug_append]; exact (hj.frame (F := .settle l') trivial).frame hF'

theorem Act.sim {W' : World} (hW : W.le W') {a b : Tm} (h : Act W a b) : Act W' a b := by
  have hnB : ¬ W.B → ¬ W'.B := fun h h' => h (hW.B.2 h')
  cases h with
  | dropA hb h => exact .dropA hb (h.mono hW)
  | dropB hn hb h => exact .dropB (hW.J₁ _ hn) hb (h.mono hW)
  | forceA h => exact .forceA (h.mono hW)
  | bornA hB hv hv' h => exact .bornA (hW.B.1 hB) hv hv' (h.mono hW)
  | bornB hB hv hv' h => exact .bornB (hW.B.1 hB) hv hv' (h.mono hW)
  | bornS hB hl hc hs => exact .bornS (hW.B.1 hB) (hW.L _ _ _ hl) hc hs
  | bornV hB hl hc hc' hn => exact .bornV (hW.B.1 hB) (hW.L _ _ _ hl) hc hc' hn
  | dead hB hn => exact .dead (hW.B.1 hB) hn
  | o4B hB hbr hf' hpe hpe' he hb hne hnb =>
    exact .o4B (hW.B.1 hB) hbr hf' (hW.R ▸ hpe) (hW.R ▸ hpe') (he.mono hW) (hb.mono hW) hne hnb
  | o4Alloc1 hB hr hs' hce hpe he hc hnc =>
    exact .o4Alloc1 (hW.B.1 hB) (hW.HR _ _ hr) hs' hce (hW.R ▸ hpe) (he.mono hW) (hc.mono hW) hnc
  | o4Bind hB hc => exact .o4Bind (hW.B.1 hB) (hc.mono hW)
  | o4Settle hB hH hhl hce hct hpt =>
    exact .o4Settle (hW.B.1 hB) (hW.H _ _ hH) (hW.HL _ _ hhl) hce hct (hW.R ▸ hpt)
  | doneF hB hd hv' hc' hn => exact .doneF (hnB hB) (hW.D _ _ _ _ hd) hv' hc' hn
  | againF1 hB hc he hev hev' hσ hσn hσv hσv' =>
    exact .againF1 (hnB hB) hc (fun y hy => (he y hy).mono hW) hev hev'
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | againF2 hB hl hf ht hc hσ hσn hσv hσv' =>
    exact .againF2 (hnB hB) (hW.L _ _ _ hl) hf (ht.mono hW) hc
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | againF3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' =>
    exact .againF3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' (hvv.mono hW) hc
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | againR1 hB hc he hev hev' hσ hσn hσv hσv' =>
    exact .againR1 (hnB hB) hc (fun y hy => (he y hy).mono hW) hev hev'
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | againR2 hB hl hf ht hc hσ hσn hσv hσv' =>
    exact .againR2 (hnB hB) (hW.L _ _ _ hl) hf (ht.mono hW) hc
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | againR3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' =>
    exact .againR3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' (hvv.mono hW) hc
      (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv'
  | letRaise hB hv hv' hvv => exact .letRaise (hnB hB) hv hv' (hvv.mono hW)
  | letUnw hB hl ha => exact .letUnw (hnB hB) (hW.L _ _ _ hl) (ha.mono hW)
  | letTodo hB => exact .letTodo (hnB hB)

theorem Dec.act {μ : List Tm} {E : List Frame} {r a b : Tm} (ha : a = plug E r) (h : Act W a b) :
    Dec W μ b E r :=
  .inl ⟨W, μ, E, [], a, b, [], RJ.refl _ _, by simp, ha, .nil, OkE.nil, h⟩


end Flick.Equiv
