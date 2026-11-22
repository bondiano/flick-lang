import Formal.Equiv.DecBase

/-!
# Contextual equivalence: frame decomposition around the focus
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

/-- **A frame around the focus**: after the right side catches up, a related frame, the frame of
a left cell, or a special pair holding the focus. -/
theorem Sim.dec_frame_sz {F : Frame} {x : Tm} (hF : F.ok) {E₀ : List Frame} {r : Tm}
    (hxr : x = plug E₀ r)
    (ih : ∀ (W' : World) (μ : List Tm) (y : Tm), Sim W' .any x y →
      (W'.B → x.Closed ∧ y.Closed) → Dec W' μ y E₀ r) :
    ∀ (n : Nat) {W : World} {x' : Pos} {s t : Tm}, t.sz ≤ n →
      Sim W x' s t → x' = .any → s = F.plug x →
      ∀ (W' : World) (μ : List Tm), W.le W' → (W'.B → s.Closed ∧ t.Closed) →
        Dec W' μ t (E₀ ++ [F]) r := by
  have act : ∀ {W' : World} {μ : List Tm} {s b : Tm}, s = F.plug x → Act W' s b →
      Dec W' μ b (E₀ ++ [F]) r := fun hs h => Dec.act (by rw [plug_snoc, ← hxr, hs]) h
  intro n
  induction n with
  | zero =>
    intro W x' s t hsz
    cases t <;> simp [Tm.sz] at hsz
  | succ n ih' =>
    intro W x' s t hsz h hx hs W' μ hW hcl
    cases h with
    | @app _ f f' a a' _ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      · have hcl₁ : W'.B → x.Closed ∧ f'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨closed_frame_inner (F := .appL _) hca, closed_frame_inner (F := .appL _) hcb⟩
        exact Dec.frame (F' := .appL a') (fun _ hW' => FrS.appL (h₂.mono (hW.trans hW'))) trivial
          (ih W' μ f' (h₁.mono hW) hcl₁)
      · have hcl₁ : W'.B → f.Closed ∧ f'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
        obtain ⟨W₁, μ₁, f₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW hcl₁
        have hcl₂ : W₁.B → x.Closed ∧ a'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
          ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
        exact Dec.pre (r₁.frame (F := .appL a') trivial)
          (Dec.frame (F' := .appR f₁) (fun _ hW' => FrS.appR (s₁.mono hW')) v₁
            (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le)) hcl₂))
    | @rcons _ l e e' a a' _ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      · have hcl₁ : W'.B → x.Closed ∧ e'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨closed_frame_inner (F := .rconsL _ _) hca, closed_frame_inner (F := .rconsL _ _) hcb⟩
        exact Dec.frame (F' := .rconsL l a') (fun _ hW' => FrS.rconsL (h₂.mono (hW.trans hW')))
          trivial (ih W' μ e' (h₁.mono hW) hcl₁)
      · have hcl₁ : W'.B → e.Closed ∧ e'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
        obtain ⟨W₁, μ₁, e₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW hcl₁
        have hcl₂ : W₁.B → x.Closed ∧ a'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
          ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
        exact Dec.pre (r₁.frame (F := .rconsL l a') trivial)
          (Dec.frame (F' := .rconsR l e₁) (fun _ hW' => FrS.rconsR (s₁.mono hW')) v₁
            (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le)) hcl₂))
    | @update _ e e' l a a' _ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      · have hcl₁ : W'.B → x.Closed ∧ e'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨closed_frame_inner (F := .updL _ _) hca, closed_frame_inner (F := .updL _ _) hcb⟩
        exact Dec.frame (F' := .updL l a') (fun _ hW' => FrS.updL (h₂.mono (hW.trans hW'))) trivial
          (ih W' μ e' (h₁.mono hW) hcl₁)
      · have hcl₁ : W'.B → e.Closed ∧ e'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
        obtain ⟨W₁, μ₁, e₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hF W' μ hW hcl₁
        have hcl₂ : W₁.B → x.Closed ∧ a'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
          ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
        exact Dec.pre (r₁.frame (F := .updL l a') trivial)
          (Dec.frame (F' := .updR e₁ l) (fun _ hW' => FrS.updR (s₁.mono hW')) v₁
            (ih W₁ μ₁ a' (h₂.mono (hW.trans r₁.le)) hcl₂))
    | @dropC e c c' b' hb hn h' =>
      subst hb
      simp only [Tm.sz, sz_rename] at hsz
      have rj := RJ.drop (W := W') μ e c'
      have hcl₁ : (W'.addJ₂ μ.length).B → s.Closed ∧ c'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, rj.cl hcb⟩
      exact Dec.pre rj (ih' (by omega) h' rfl hs _ _ (hW.trans (World.le_addJ₂ _ _)) hcl₁)
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      exact Dec.pre (RJ.start μ e')
        (Dec.settle (.inr rfl) (ih' (by omega) h' rfl hs _ _ (hW.trans (World.le_addJ₂ _ _)) hcl))
    | @jR _ n a a' _ hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      exact Dec.settle (hW.J₂ _ hn) (ih' (by omega) h' rfl hs W' μ hW hcl)
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
        (ih' hsz' ((hc.mono hW₁).subst _ _ hsub) rfl hs _ _ (World.le.refl _) hcl₁)
    | jL _ hn h' =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.junkL (hW.J₁ _ hn) (ih W' μ _ (h'.mono hW) hcl)
    | dropA hb h' => exact act hs (.dropA hb (h'.mono hW))
    | dropB hn hb h' => exact act hs (.dropB (hW.J₁ _ hn) hb (h'.mono hW))
    | forceA h' => exact act hs (.forceA (h'.mono hW))
    | bornA hB hv hv' h' => exact act hs (.bornA (hW.B.1 hB) hv hv' (h'.mono hW))
    | bornB hB hv hv' h' => exact act hs (.bornB (hW.B.1 hB) hv hv' (h'.mono hW))
    | bornS hB hl hc hst => exact act hs (.bornS (hW.B.1 hB) (hW.L _ _ _ hl) hc hst)
    | bornV hB hl hc hc' hn => exact act hs (.bornV (hW.B.1 hB) (hW.L _ _ _ hl) hc hc' hn)
    | dead hB hn => exact act hs (.dead (hW.B.1 hB) hn)
    | o4B hB hbr hf' hpe hpe' he hb hne hnb =>
      exact act hs (.o4B (hW.B.1 hB) hbr hf' (hW.R ▸ hpe) (hW.R ▸ hpe') (he.mono hW) (hb.mono hW) hne hnb)
    | o4Alloc1 hB hr hs' hce hpe he hc hnc =>
      exact act hs (.o4Alloc1 (hW.B.1 hB) (hW.HR _ _ hr) hs' hce (hW.R ▸ hpe) (he.mono hW) (hc.mono hW) hnc)
    | o4Bind hB hc => exact act hs (.o4Bind (hW.B.1 hB) (hc.mono hW))
    | o4Settle hB hH hhl hce hct hpt =>
      exact act hs (.o4Settle (hW.B.1 hB) (hW.H _ _ hH) (hW.HL _ _ hhl) hce hct (hW.R ▸ hpt))
    | appK | appV | rconsK | rconsV | updK | updV => cases hx
    | let_ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .let_ _) (fun _ hW' => (FrS.let_ h₂).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .let_ _) a, closed_frame_inner (F := .let_ _) b⟩)
    | proj h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .proj _) (fun _ hW' => (FrS.proj).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .proj _) a, closed_frame_inner (F := .proj _) b⟩)
    | match_ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .match_ _) (fun _ hW' => (FrS.match_ h₂).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .match_ _) a, closed_frame_inner (F := .match_ _) b⟩)
    | await h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .await) (fun _ hW' => (FrS.await).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .await) a, closed_frame_inner (F := .await) b⟩)
    | force h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .force) (fun _ hW' => (FrS.force).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .force) a, closed_frame_inner (F := .force) b⟩)
    | raise h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .raise) (fun _ hW' => (FrS.raise).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .raise) a, closed_frame_inner (F := .raise) b⟩)
    | try_ h₁ h₂ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .try_ _) (fun _ hW' => (FrS.try_ h₂).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .try_ _) a, closed_frame_inner (F := .try_ _) b⟩)
    | inside h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .inside _) (fun _ hW' => (FrS.inside).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .inside _) a, closed_frame_inner (F := .inside _) b⟩)
    | hwith hl ha hr h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hwith _ _ _ _ _) (fun _ hW' => (FrS.hwith hl ha hr).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hwith _ _ _ _ _) a, closed_frame_inner (F := .hwith _ _ _ _ _) b⟩)
    | hkeep hl ha h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hkeep _ _ _ _) (fun _ hW' => (FrS.hkeep hl ha).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hkeep _ _ _ _) a, closed_frame_inner (F := .hkeep _ _ _ _) b⟩)
    | hkarm hl ha h₁ hc =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hkarmA _ _ _ _ _) (fun _ hW' => (FrS.hkarmA hl ha hc).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hkarmA _ _ _ _ _) a, closed_frame_inner (F := .hkarmA _ _ _ _ _) b⟩)
    | hctl hl hk ha hr h₁ hc =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hctlA _ _ _ _ _ _ _ _) (fun _ hW' => (FrS.hctlA hl hk ha hr hc).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) a, closed_frame_inner (F := .hctlA _ _ _ _ _ _ _ _) b⟩)
    | hres hl ha h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hresB _ _ _) (fun _ hW' => (FrS.hresB hl ha).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hresB _ _ _) a, closed_frame_inner (F := .hresB _ _ _) b⟩)
    | hdone h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .hdone _) (fun _ hW' => (FrS.hdone).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .hdone _) a, closed_frame_inner (F := .hdone _) b⟩)
    | settle hl h₁ =>
      subst hx
      cases F <;> simp only [Frame.plug] at hs <;> cases hs
      exact Dec.frame (F' := .settle _) (fun _ hW' => (FrS.settle hl).mono (hW.trans hW')) trivial
        (ih W' μ _ (h₁.mono hW) fun hB => let ⟨a, b⟩ := hcl hB;
          ⟨closed_frame_inner (F := .settle _) a, closed_frame_inner (F := .settle _) b⟩)
    | _ =>
      cases F <;> simp only [Frame.plug] at hs <;> cases hs

theorem Sim.dec_frame {F : Frame} {x : Tm} (hF : F.ok) {E₀ : List Frame} {r : Tm}
    (hxr : x = plug E₀ r)
    (ih : ∀ (W' : World) (μ : List Tm) (y : Tm), Sim W' .any x y →
      (W'.B → x.Closed ∧ y.Closed) → Dec W' μ y E₀ r)
    {x' : Pos} {s t : Tm} (h : Sim W x' s t) (hx : x' = .any) (hs : s = F.plug x)
    (W' : World) (μ : List Tm) (hW : W.le W') (hcl : W'.B → s.Closed ∧ t.Closed) :
    Dec W' μ t (E₀ ++ [F]) r :=
  Sim.dec_frame_sz hF hxr ih t.sz (Nat.le_refl _) h hx hs W' μ hW hcl

/-- **Decomposition**: the right side, caught up, around the focus of the left one. -/
theorem Sim.dec {r : Tm} (hrv : ¬ Value r) (hr : ∀ f a, r = .app f a → Value f ∧ Value a)
    (hrc : ∀ l e a, r ≠ .rcons l e a) (hal : ¬ IsAlts r) :
    ∀ (m : Nat) {E : List Frame} {t : Tm} (W : World), E.length = m → OkE E →
      Sim W .any (plug E r) t → ∀ μ, (W.B → (plug E r).Closed ∧ t.Closed) → Dec W μ t E r := by
  intro m
  induction m with
  | zero =>
    intro E t W hl _ h μ hcl
    cases E with
    | nil => exact h.dec_base rfl hrv hr hrc hal W μ (World.le.refl _) hcl
    | cons => cases hl
  | succ m ih =>
    intro E t W hl hE h μ hcl
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩
    · cases hl
    · rw [plug_snoc] at h hcl
      have hE₀ : OkE E₀ := fun G hG => hE G (List.mem_append_left _ hG)
      have hF : F.ok := hE F (List.mem_append_right _ (List.mem_singleton_self _))
      exact Sim.dec_frame hF rfl (fun W' μ y hy hcl' => ih W' (by simpa using hl) hE₀ hy μ hcl')
        h rfl rfl W μ (World.le.refl _) hcl

end Flick.Equiv
