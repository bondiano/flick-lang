import Formal.Equiv.ActDef

/-!
# Contextual equivalence: base decomposition at the focus
-/

namespace Flick.Equiv

open Flick.Elab (All2 IsAlts)

variable {W : World}

/-- **The focus**: after the right side catches up, a special pair holds the focus, or a
congruence relates it at `top`. -/
theorem Sim.dec_base_sz : ∀ (n : Nat) {W : World} {x : Pos} {r t : Tm}, t.sz ≤ n →
    Sim W x r t → x = .any → ¬ Value r →
    (∀ f a, r = .app f a → Value f ∧ Value a) → (∀ l e a, r ≠ .rcons l e a) → ¬ IsAlts r →
    (∀ a b, r = .let_ a b → Value a) →
    ∀ (W' : World) {μ₁ : List Tm} (μ : List Tm), W.le W' → StR W' μ₁ μ →
      (W'.B → r.Closed ∧ t.Closed) → Dec W' μ t [] r := by
  intro n
  induction n with
  | zero =>
    intro W x r t hsz
    cases t <;> simp [Tm.sz] at hsz
  | succ n ih =>
    intro W x r t hsz h hx hrv hr hrc hal hlet W' μ₁ μ hW hst hcl
    have hnB : ¬ W.B → ¬ W'.B := fun h h' => h (hW.B.2 h')
    cases h with
    | @app x f f' a a' _ h₁ h₂ =>
      subst hx
      obtain ⟨hf, ha⟩ := hr f a rfl
      have hcl₁ : W'.B → f.Closed ∧ f'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB
        ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
      obtain ⟨W₁, μ₁', f₁, r₁, v₁, s₁⟩ := h₁.vcu rfl hf W' μ hW hst hcl₁
      have hcl₂ : W₁.B → a.Closed ∧ a'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
        ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
      obtain ⟨W₂, μ₂, a₁, r₂, v₂, s₂⟩ :=
        h₂.vcu rfl ha W₁ μ₁' (hW.trans r₁.le) (r₁.st _ hst) hcl₂
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
      exact Dec.pre rj (ih (by omega) h' rfl hrv hr hrc hal hlet _ _
        (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl₁)
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      have rj := RJ.start (W := W') μ e'
      exact Dec.pre rj
        (Dec.settle (.inr rfl) (ih (by omega) h' rfl hrv hr hrc hal hlet _ _
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl))
    | @jR x n a a' hx' hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      exact Dec.settle (hW.J₂ _ hn) (ih (by omega) h' rfl hrv hr hrc hal hlet W' μ hW hst hcl)
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
        (ih hsz' ((hc.mono hW₁).subst _ _ hsub) rfl hrv hr hrc hal hlet _ _
          (World.le.refl _) (rj.st _ hst) hcl₁)
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
    | doneF hB hd hv' hc' hn => exact Dec.act rfl (.doneF (hnB hB) (hW.D _ _ _ _ hd) hv' hc' hn)
    | doneR _ _ hv => exact (hrv hv).elim
    | againF1 | againF2 | againR1 | againR2 | letRaise | letUnw | letTodo =>
      have := hlet _ _ rfl; cases this
    | againF3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' =>
      exact Dec.act rfl (.againF3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' (hvv.mono hW) hc
        (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv')
    | againR3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' =>
      exact Dec.act rfl (.againR3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' (hvv.mono hW) hc
        (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv')
    | _ =>
      subst hx
      first
        | exact (hrv (by constructor)).elim
        | exact .inr (.inl ⟨W', μ, [], _, RJ.refl _ _, .nil, OkE.nil,
            Sim.mono hW (by constructor <;> first | assumption | decide)⟩)

theorem Sim.dec_base {x : Pos} {r t : Tm} (h : Sim W x r t) (hx : x = .any) (hrv : ¬ Value r)
    (hr : ∀ f a, r = .app f a → Value f ∧ Value a) (hrc : ∀ l e a, r ≠ .rcons l e a)
    (hal : ¬ IsAlts r) (hlet : ∀ a b, r = .let_ a b → Value a)
    (W' : World) {μ₁ : List Tm} (μ : List Tm) (hW : W.le W') (hst : StR W' μ₁ μ)
    (hcl : W'.B → r.Closed ∧ t.Closed) : Dec W' μ t [] r :=
  Sim.dec_base_sz t.sz (Nat.le_refl _) h hx hrv hr hrc hal hlet W' μ hW hst hcl


end Flick.Equiv
