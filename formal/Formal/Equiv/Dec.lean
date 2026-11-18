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
  | bornA {v v'} : Value v → Value v' → Sim W .any v v' →
      Act W (.lazy v) (.let_ (.lazy v') (.let_ (.force (.var 0)) (.var 1)))
  | bornB {v v'} : Value v → Value v' → Sim W .any v v' →
      Act W (.let_ (.lazy v) (.let_ (.force (.var 0)) (.var 1))) (.lazy v')
  | bornS {l l' v v' t} : W.L l l' (.born v v') → v.Closed → BornSt l v t → Act W t (.loc l')
  | bornV {l l' v v'} : W.L l l' (.born v v') → v.Closed → v'.Closed → v'.NoIns →
      Act W (.settle l v) v'
  | dead {b} : b.NoIns → Act W .todo b

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
  | bornA hv hv' h => exact .bornA hv hv' (h.mono hW)
  | bornB hv hv' h => exact .bornB hv hv' (h.mono hW)
  | bornS hl hc hs => exact .bornS (hW.L _ _ _ hl) hc hs
  | bornV hl hc hc' hn => exact .bornV (hW.L _ _ _ hl) hc hc' hn
  | dead hn => exact .dead hn

theorem Dec.act {μ : List Tm} {E : List Frame} {r a b : Tm} (ha : a = plug E r) (h : Act W a b) :
    Dec W μ b E r :=
  .inl ⟨W, μ, E, [], a, b, [], RJ.refl _ _, by simp, ha, .nil, OkE.nil, h⟩

/-- **The focus**: after the right side catches up, a special pair holds the focus, or a
congruence relates it at `top`. -/
theorem Sim.dec_base {x : Pos} {r t : Tm} (h : Sim W x r t) (hx : x = .any) (hrv : ¬ Value r)
    (hr : ∀ f a, r = .app f a → Value f ∧ Value a) (hrc : ∀ l e a, r ≠ .rcons l e a)
    (hal : ¬ IsAlts r) : ∀ (W' : World) (μ : List Tm), W.le W' → Dec W' μ t [] r := by
  induction h with
  | @app x f f' a a' _ h₁ h₂ _ _ =>
    intro W' μ hW
    subst hx
    obtain ⟨hf, ha⟩ := hr f a rfl
    obtain ⟨W₁, μ₁, f₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hf W' μ hW
    obtain ⟨W₂, μ₂, a₁, r₂, v₂, s₂⟩ := h₂.vcu rfl ha W₁ μ₁ (hW.trans r₁.le)
    exact .inr (.inl ⟨W₂, μ₂, [], .app f₁ a₁, (r₁.frame (F := .appL a') trivial).trans
      (r₂.frame (F := .appR f₁) v₁), .nil, OkE.nil,
      .app (by decide) ((s₁.mono r₂.le).value_any hf v₁ .top) (s₂.value_any ha v₂ .top)⟩)
  | rcons => exact (hrc _ _ _ rfl).elim
  | acons | aguard => exact (hal trivial).elim
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | dropC hb hn _ ih =>
    intro W' μ hW; subst hb
    exact Dec.pre (RJ.drop μ _ _) (ih rfl hrv hr hrc hal _ _ (hW.trans (World.le_addJ₂ _ _)))
  | forceC hn _ ih =>
    intro W' μ hW
    exact Dec.pre (RJ.start μ _)
      (Dec.settle (.inr rfl) (ih rfl hrv hr hrc hal _ _ (hW.trans (World.le_addJ₂ _ _))))
  | jR hx' hn _ ih =>
    intro W' μ hW; subst hx
    exact Dec.settle (hW.J₂ _ hn) (ih rfl hrv hr hrc hal W' μ hW)
  | dropA hb h' _ => intro W' μ hW; exact Dec.act rfl (.dropA hb (h'.mono hW))
  | dropB hn hb h' _ => intro W' μ hW; exact Dec.act rfl (.dropB (hW.J₁ _ hn) hb (h'.mono hW))
  | forceA h' _ => intro W' μ hW; exact Dec.act rfl (.forceA (h'.mono hW))
  | jL _ hn h' _ =>
    intro W' μ hW; subst hx
    exact .inr (.inr ⟨W', μ, [], _, _, _, RJ.refl _ _, .nil, OkE.nil, rfl, hW.J₁ _ hn, h'.mono hW⟩)
  | bornA hv hv' h' _ => intro W' μ hW; exact Dec.act rfl (.bornA hv hv' (h'.mono hW))
  | bornB hv hv' h' _ => intro W' μ hW; exact Dec.act rfl (.bornB hv hv' (h'.mono hW))
  | bornS hl hc hs => intro W' μ hW; exact Dec.act rfl (.bornS (hW.L _ _ _ hl) hc hs)
  | bornV hl hc hc' hn => intro W' μ hW; exact Dec.act rfl (.bornV (hW.L _ _ _ hl) hc hc' hn)
  | dead hn => intro W' μ hW; exact Dec.act rfl (.dead hn)
  | _ =>
    intro W' μ hW
    subst hx
    first
      | exact (hrv (by constructor)).elim
      | exact .inr (.inl ⟨W', μ, [], _, RJ.refl _ _, .nil, OkE.nil,
          Sim.mono hW (by constructor <;> first | assumption | decide)⟩)

/-- **A frame around the focus**: after the right side catches up, a related frame, the frame of
a left cell, or a special pair holding the focus. -/
theorem Sim.dec_frame {F : Frame} {x : Tm} (hF : F.ok) {E₀ : List Frame} {r : Tm}
    (hxr : x = plug E₀ r)
    (ih : ∀ (W' : World) (μ : List Tm) (y : Tm), Sim W' .any x y → Dec W' μ y E₀ r) :
    ∀ {x' : Pos} {s t : Tm}, Sim W x' s t → x' = .any → s = F.plug x →
      ∀ (W' : World) (μ : List Tm), W.le W' → Dec W' μ t (E₀ ++ [F]) r := by
  have act : ∀ {W' : World} {μ : List Tm} {s b : Tm}, s = F.plug x → Act W' s b →
      Dec W' μ b (E₀ ++ [F]) r := fun hs h => Dec.act (by rw [plug_snoc, ← hxr, hs]) h
  intro x' s t h
  induction h with
  | @app x f f' a a' _ h₁ h₂ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    · exact Dec.frame (F' := .appL a') (fun _ hW' => FrS.appL (h₂.mono (hW.trans hW'))) trivial
        (ih W' μ f' (h₁.mono hW))
    · obtain ⟨W₁, μ₁, f₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW
      exact Dec.pre (r₁.frame (F := .appL a') trivial)
        (Dec.frame (F' := .appR f₁) (fun _ hW' => FrS.appR (s₁.mono hW')) v₁
          (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le))))
  | @rcons x l e e' a a' _ h₁ h₂ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    · exact Dec.frame (F' := .rconsL l a') (fun _ hW' => FrS.rconsL (h₂.mono (hW.trans hW')))
        trivial (ih W' μ e' (h₁.mono hW))
    · obtain ⟨W₁, μ₁, e₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW
      exact Dec.pre (r₁.frame (F := .rconsL l a') trivial)
        (Dec.frame (F' := .rconsR l e₁) (fun _ hW' => FrS.rconsR (s₁.mono hW')) v₁
          (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le))))
  | @update x e e' l a a' _ h₁ h₂ =>
    intro hx hs W' μ hW
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    · exact Dec.frame (F' := .updL l a') (fun _ hW' => FrS.updL (h₂.mono (hW.trans hW'))) trivial
        (ih W' μ e' (h₁.mono hW))
    · obtain ⟨W₁, μ₁, e₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW
      exact Dec.pre (r₁.frame (F := .updL l a') trivial)
        (Dec.frame (F' := .updR e₁ l) (fun _ hW' => FrS.updR (s₁.mono hW')) v₁
          (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le))))
  | dropC hb hn _ ih' =>
    intro hx hs W' μ hW; subst hb
    exact Dec.pre (RJ.drop μ _ _) (ih' rfl hs _ _ (hW.trans (World.le_addJ₂ _ _)))
  | forceC hn _ ih' =>
    intro hx hs W' μ hW
    exact Dec.pre (RJ.start μ _)
      (Dec.settle (.inr rfl) (ih' rfl hs _ _ (hW.trans (World.le_addJ₂ _ _))))
  | jR hx' hn _ ih' =>
    intro hx hs W' μ hW; subst hx
    exact Dec.settle (hW.J₂ _ hn) (ih' rfl hs W' μ hW)
  | jL _ hn h' _ =>
    intro hx hs W' μ hW; subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.junkL (hW.J₁ _ hn) (ih W' μ _ (h'.mono hW))
  | dropA hb h' _ => intro _ hs W' μ hW; exact act hs (.dropA hb (h'.mono hW))
  | dropB hn hb h' _ => intro _ hs W' μ hW; exact act hs (.dropB (hW.J₁ _ hn) hb (h'.mono hW))
  | forceA h' _ => intro _ hs W' μ hW; exact act hs (.forceA (h'.mono hW))
  | bornA hv hv' h' _ => intro _ hs W' μ hW; exact act hs (.bornA hv hv' (h'.mono hW))
  | bornB hv hv' h' _ => intro _ hs W' μ hW; exact act hs (.bornB hv hv' (h'.mono hW))
  | bornS hl hc hst => intro _ hs W' μ hW; exact act hs (.bornS (hW.L _ _ _ hl) hc hst)
  | bornV hl hc hc' hn => intro _ hs W' μ hW; exact act hs (.bornV (hW.L _ _ _ hl) hc hc' hn)
  | dead hn => intro _ hs W' μ hW; exact act hs (.dead hn)
  | appK | appV | rconsK | rconsV | updK | updV => intro hx; cases hx
  | let_ h₁ h₂ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .let_ _) (fun _ hW' => (FrS.let_ h₂).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | proj h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .proj _) (fun _ hW' => (FrS.proj).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | match_ h₁ h₂ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .match_ _) (fun _ hW' => (FrS.match_ h₂).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | await h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .await) (fun _ hW' => (FrS.await).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | force h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .force) (fun _ hW' => (FrS.force).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | raise h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .raise) (fun _ hW' => (FrS.raise).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | try_ h₁ h₂ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .try_ _) (fun _ hW' => (FrS.try_ h₂).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | inside h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .inside _) (fun _ hW' => (FrS.inside).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hwith hl ha hr h₁ _ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hwith _ _ _ _ _) (fun _ hW' => (FrS.hwith hl ha hr).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hkeep hl ha h₁ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hkeep _ _ _ _) (fun _ hW' => (FrS.hkeep hl ha).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hkarm hl ha h₁ hc _ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hkarmA _ _ _ _ _) (fun _ hW' => (FrS.hkarmA hl ha hc).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hctl hl hk ha hr h₁ hc _ _ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hctlA _ _ _ _ _ _ _ _) (fun _ hW' => (FrS.hctlA hl hk ha hr hc).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hres hl ha h₁ _ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hresB _ _ _) (fun _ hW' => (FrS.hresB hl ha).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | hdone h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .hdone _) (fun _ hW' => (FrS.hdone).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | settle hl h₁ _ =>
    intro hx hs W' μ hW
    subst hx
    cases F <;> simp only [Frame.plug] at hs <;> cases hs
    exact Dec.frame (F' := .settle _) (fun _ hW' => (FrS.settle hl).mono (hW.trans hW')) trivial
      (ih W' μ _ (h₁.mono hW))
  | _ =>
    intro _ hs
    cases F <;> simp only [Frame.plug] at hs <;> cases hs

/-- **Decomposition**: the right side, caught up, around the focus of the left one. -/
theorem Sim.dec {r : Tm} (hrv : ¬ Value r) (hr : ∀ f a, r = .app f a → Value f ∧ Value a)
    (hrc : ∀ l e a, r ≠ .rcons l e a) (hal : ¬ IsAlts r) :
    ∀ (m : Nat) {E : List Frame} {t : Tm} (W : World), E.length = m → OkE E →
      Sim W .any (plug E r) t → ∀ μ, Dec W μ t E r := by
  intro m
  induction m with
  | zero =>
    intro E t W hl _ h μ
    cases E with
    | nil => exact h.dec_base rfl hrv hr hrc hal W μ (World.le.refl _)
    | cons => cases hl
  | succ m ih =>
    intro E t W hl hE h μ
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
    · cases hl
    · rw [plug_snoc] at h
      have hE₀ : OkE E₀ := fun G hG => hE G (List.mem_append_left _ hG)
      have hF : F.ok := hE F (List.mem_append_right _ (List.mem_singleton_self _))
      exact Sim.dec_frame hF rfl (fun W' μ y hy => ih W' (by simpa using hl) hE₀ hy μ) h rfl rfl
        W μ (World.le.refl _)

end Flick.Equiv
