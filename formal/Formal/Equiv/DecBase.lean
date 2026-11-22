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

theorem Act.sim {W' : World} (hW : W.le W') {a b : Tm} (h : Act W a b) : Act W' a b := by
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

theorem Dec.act {μ : List Tm} {E : List Frame} {r a b : Tm} (ha : a = plug E r) (h : Act W a b) :
    Dec W μ b E r :=
  .inl ⟨W, μ, E, [], a, b, [], RJ.refl _ _, by simp, ha, .nil, OkE.nil, h⟩

/-- **The focus**: after the right side catches up, a special pair holds the focus, or a
congruence relates it at `top`. -/
theorem Sim.dec_base_sz : ∀ (n : Nat) {W : World} {x : Pos} {r t : Tm}, t.sz ≤ n →
    Sim W x r t → x = .any → ¬ Value r →
    (∀ f a, r = .app f a → Value f ∧ Value a) → (∀ l e a, r ≠ .rcons l e a) → ¬ IsAlts r →
    ∀ (W' : World) (μ : List Tm), W.le W' → (W'.B → r.Closed ∧ t.Closed) → Dec W' μ t [] r := by
  intro n
  induction n with
  | zero =>
    intro W x r t hsz
    cases t <;> simp [Tm.sz] at hsz
  | succ n ih =>
    intro W x r t hsz h hx hrv hr hrc hal W' μ hW hcl
    cases h with
    | @app x f f' a a' _ h₁ h₂ =>
      subst hx
      obtain ⟨hf, ha⟩ := hr f a rfl
      have hcl₁ : W'.B → f.Closed ∧ f'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB
        ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
      obtain ⟨W₁, μ₁, f₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hf W' μ hW hcl₁
      have hcl₂ : W₁.B → a.Closed ∧ a'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
        ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
      obtain ⟨W₂, μ₂, a₁, r₂, v₂, s₂⟩ := h₂.vcu rfl ha W₁ μ₁ (hW.trans r₁.le) hcl₂
      exact .inr (.inl ⟨W₂, μ₂, [], .app f₁ a₁, (r₁.frame (F := .appL a') trivial).trans
        (r₂.frame (F := .appR f₁) v₁), .nil, OkE.nil,
        .app (by decide) ((s₁.mono r₂.le).value_any hf v₁ .top) (s₂.value_any ha v₂ .top)⟩)
    | rcons => exact (hrc _ _ _ rfl).elim
    | acons | aconsD | aguard => exact (hal trivial).elim
    | appK | appV | rconsK | rconsV | updK | updV => cases hx
    | @dropC e c c' b' hb hn h' =>
      subst hb
      simp only [Tm.sz, sz_rename] at hsz
      have rj := RJ.drop (W := W') μ e c'
      have hcl₁ : (W'.addJ₂ μ.length).B → r.Closed ∧ c'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, rj.cl hcb⟩
      exact Dec.pre rj (ih (by omega) h' rfl hrv hr hrc hal _ _ (hW.trans (World.le_addJ₂ _ _)) hcl₁)
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      exact Dec.pre (RJ.start μ e')
        (Dec.settle (.inr rfl) (ih (by omega) h' rfl hrv hr hrc hal _ _ (hW.trans (World.le_addJ₂ _ _)) hcl))
    | @jR x n a a' hx' hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      exact Dec.settle (hW.J₂ _ hn) (ih (by omega) h' rfl hrv hr hrc hal W' μ hW hcl)
    | @o4AllocR l e e' s c c' hB hl hs hcl' hpe' he hc =>
      subst hs
      simp only [Tm.sz] at hsz
      have hB' : W'.B := hW.B.1 hB
      have rj := RJ.o4AllocR (W := W') μ (c' := c') hB' (hW.HL _ _ hl) hcl' (hW.R ▸ hpe') (he.mono hW)
      have hW₁ := hW.trans rj.le
      have hsub : SubR ((W'.addHR μ.length e').addH l μ.length) (inst [.loc l]) (inst [.loc μ.length]) := by
        intro i; cases i with
        | zero =>
          exact ⟨fun y _ => .hloc hB' (.inr ⟨rfl, rfl⟩), by simp [inst, Tm.NoIns, Tm.Has],
            .inr .loc, .inr .loc⟩
        | succ k =>
          exact ⟨fun _ hy => .var hy, by simp [inst, Tm.NoIns, Tm.Has],
            .inl ⟨k, rfl⟩, .inl ⟨k, rfl⟩⟩
      have hcl₁ : ((W'.addHR μ.length e').addH l μ.length).B →
          (c.subst (inst [.loc l])).Closed ∧ (c'.subst (inst [.loc μ.length])).Closed := fun hB'' =>
        let ⟨hca, hcb⟩ := hcl hB''; ⟨hca, rj.cl hcb⟩
      have hsz' : (c'.subst (inst [.loc μ.length])).sz ≤ n := by rw [sz_subst_loc]; omega
      exact Dec.pre rj
        (ih hsz' ((hc.mono hW₁).subst _ _ hsub) rfl hrv hr hrc hal _ _ (World.le.refl _) hcl₁)
    | dropA hb h' => exact Dec.act rfl (.dropA hb (h'.mono hW))
    | dropB hn hb h' => exact Dec.act rfl (.dropB (hW.J₁ _ hn) hb (h'.mono hW))
    | forceA h' => exact Dec.act rfl (.forceA (h'.mono hW))
    | jL _ hn h' =>
      subst hx
      exact .inr (.inr ⟨W', μ, [], _, _, _, RJ.refl _ _, .nil, OkE.nil, rfl, hW.J₁ _ hn, h'.mono hW⟩)
    | bornA hB hv hv' h' => exact Dec.act rfl (.bornA (hW.B.1 hB) hv hv' (h'.mono hW))
    | bornB hB hv hv' h' => exact Dec.act rfl (.bornB (hW.B.1 hB) hv hv' (h'.mono hW))
    | bornS hB hl hc hs => exact Dec.act rfl (.bornS (hW.B.1 hB) (hW.L _ _ _ hl) hc hs)
    | bornV hB hl hc hc' hn => exact Dec.act rfl (.bornV (hW.B.1 hB) (hW.L _ _ _ hl) hc hc' hn)
    | dead hB hn => exact Dec.act rfl (.dead (hW.B.1 hB) hn)
    | o4A | o4Fix1 | o4Fix2 => exact (hrv .fix).elim
    | hloc => exact (hrv .loc).elim
    | o4B hB hbr hf' hpe hpe' he hb hne hnb =>
      exact Dec.act rfl (.o4B (hW.B.1 hB) hbr hf' (hW.R ▸ hpe) (hW.R ▸ hpe') (he.mono hW) (hb.mono hW) hne hnb)
    | o4Alloc1 hB hhr hs' hce hpe he hc hnc =>
      exact Dec.act rfl (.o4Alloc1 (hW.B.1 hB) (hW.HR _ _ hhr) hs' hce (hW.R ▸ hpe) (he.mono hW) (hc.mono hW) hnc)
    | o4Bind hB hc => exact Dec.act rfl (.o4Bind (hW.B.1 hB) (hc.mono hW))
    | o4Settle hB hH hhl hce hct hpt =>
      exact Dec.act rfl (.o4Settle (hW.B.1 hB) (hW.H _ _ hH) (hW.HL _ _ hhl) hce hct (hW.R ▸ hpt))
    | _ =>
      subst hx
      first
        | exact (hrv (by constructor)).elim
        | exact .inr (.inl ⟨W', μ, [], _, RJ.refl _ _, .nil, OkE.nil,
            Sim.mono hW (by constructor <;> first | assumption | decide)⟩)

theorem Sim.dec_base {x : Pos} {r t : Tm} (h : Sim W x r t) (hx : x = .any) (hrv : ¬ Value r)
    (hr : ∀ f a, r = .app f a → Value f ∧ Value a) (hrc : ∀ l e a, r ≠ .rcons l e a)
    (hal : ¬ IsAlts r) (W' : World) (μ : List Tm) (hW : W.le W')
    (hcl : W'.B → r.Closed ∧ t.Closed) : Dec W' μ t [] r :=
  Sim.dec_base_sz t.sz (Nat.le_refl _) h hx hrv hr hrc hal W' μ hW hcl


end Flick.Equiv
