import Formal.Equiv.Sim

/-!
# Contextual equivalence: related frames, source code, and a growing world

Related frames (`FrS`) and contexts (`FrL`, up to the frames of cells only one side has); source
code is related to itself (`Sim.refl`); relations stay as the world grows (`Sim.mono`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

/-- Two frames of one kind with related parts. -/
inductive FrS (W : World) : Frame → Frame → Prop
  | appL {e e'} : Sim W .any e e' → FrS W (.appL e) (.appL e')
  | appR {v v'} : Sim W .any v v' → FrS W (.appR v) (.appR v')
  | let_ {b b'} : Sim W .any b b' → FrS W (.let_ b) (.let_ b')
  | match_ {al al'} : Sim W .top al al' → FrS W (.match_ al) (.match_ al')
  | rconsL {l r r'} : Sim W .any r r' → FrS W (.rconsL l r) (.rconsL l r')
  | rconsR {l v v'} : Sim W .any v v' → FrS W (.rconsR l v) (.rconsR l v')
  | proj {l} : FrS W (.proj l) (.proj l)
  | updL {l e e'} : Sim W .any e e' → FrS W (.updL l e) (.updL l e')
  | updR {v v' l} : Sim W .any v v' → FrS W (.updR v l) (.updR v' l)
  | await : FrS W .await .await
  | try_ {al al'} : Sim W .top al al' → FrS W (.try_ al) (.try_ al')
  | raise : FrS W .raise .raise
  | inside {p} : FrS W (.inside p) (.inside p)
  | hwith {p ℓ ℓ' k E al al' r r'} : W.L ℓ ℓ' k → Sim W .top al al' → Sim W .any r r' →
      FrS W (.hwith p ℓ E al r) (.hwith p ℓ' E al' r')
  | hctlA {p ℓ ℓ' k κ κ' tl E al al' r r' c c'} : W.L ℓ ℓ' k → KapR W tl κ κ' →
      Sim W .top al al' → Sim W .any r r' → Sim W .park c c' →
      FrS W (.hctlA p ℓ κ tl E al r c) (.hctlA p ℓ' κ' tl E al' r' c')
  | hresB {p ℓ ℓ' k a a'} : W.L ℓ ℓ' k → Sim W .park a a' → FrS W (.hresB p ℓ a) (.hresB p ℓ' a')
  | hdone {p} : FrS W (.hdone p) (.hdone p)
  | hkeep {q ℓ ℓ' k E al al'} : W.L ℓ ℓ' k → Sim W .top al al' →
      FrS W (.hkeep q ℓ E al) (.hkeep q ℓ' E al')
  | hkarmA {q ℓ ℓ' k E al al' c c'} : W.L ℓ ℓ' k → Sim W .top al al' → Sim W .park c c' →
      FrS W (.hkarmA q ℓ E al c) (.hkarmA q ℓ' E al' c')
  | force : FrS W .force .force
  | settle {ℓ ℓ'} : W.L ℓ ℓ' .sym → FrS W (.settle ℓ) (.settle ℓ')

/-- Related contexts, the innermost frame first: related frames, the frames of cells only
one side has, and coupled `settle` + `let` frames of O2a. -/
inductive FrL (W : World) : List Frame → List Frame → Prop
  | nil : FrL W [] []
  | cons {F F' E E'} : FrS W F F' → FrL W E E' → FrL W (F :: E) (F' :: E')
  | junkL {n E E'} : W.J₁ n → FrL W E E' → FrL W (.settle n :: E) E'
  | junkR {n E E'} : W.J₂ n → FrL W E E' → FrL W E (.settle n :: E')
  | againF {l l' : Nat} {c₀ : Tm} {σ σ' : Nat → Tm} {E E' : List Frame} :
      ¬ W.B → W.L l l' .sym → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      FrL W E E' →
      FrL W (.settle l :: .let_ (c₀.subst (subForce (.loc l) σ)) :: E)
        (.settle l' :: .let_ (c₀.subst (subVar σ')) :: E')
  | againR {l l' : Nat} {c₀ : Tm} {σ σ' : Nat → Tm} {E E' : List Frame} :
      ¬ W.B → W.L l l' .sym → c₀.Src →
      (∀ i y, y ≠ .park → Sim W y (σ i) (σ' i)) → (∀ i, (σ' i).NoIns) →
      (∀ i, (∃ j, σ i = .var j) ∨ Value (σ i)) → (∀ i, (∃ j, σ' i = .var j) ∨ Value (σ' i)) →
      FrL W E E' →
      FrL W (.settle l :: .let_ (c₀.subst (subVar σ)) :: E)
        (.settle l' :: .let_ (c₀.subst (subForce (.loc l') σ')) :: E')

theorem _root_.Flick.Tm.Src.noIns : ∀ {e : Tm}, e.Src → e.NoIns := by
  intro e
  induction e <;> intro h <;> simp only [Tm.Src] at h <;> simp only [Tm.NoIns, Tm.Has] <;>
    simp_all [Tm.NoIns]

variable {W : World}

theorem KapR.mono {W' : World} (hW : W.le W') {tl : Bool} {κ κ' : Nat} (h : KapR W tl κ κ') :
    KapR W' tl κ κ' := by
  rcases h with ⟨h₁, h₂⟩ | ⟨h₁, k, h₂⟩
  · exact .inl ⟨h₁, h₂⟩
  · exact .inr ⟨h₁, k, hW.L _ _ _ h₂⟩

theorem Sim.refl : ∀ {e : Tm}, e.Src → ∀ x, x ≠ .park → Sim W x e e := by
  intro e
  induction e <;> intro h x hx <;> simp only [Tm.Src] at h
  all_goals first
    | exact h.elim
    | constructor <;> first
      | exact hx
      | exact Tm.Src.noIns h
      | exact Tm.Src.noIns h.1
      | apply_assumption <;> first
        | decide
        | simp_all [Pos.fr]
        | (cases x <;> simp_all [Pos.fr])

theorem Sim.lower {x : Pos} {a b : Tm} (h : Sim W x a b) : Sim W .any a b := by
  induction h with
  | app _ _ _ ih₁ ih₂ => exact .app (by simp) ih₁ ih₂
  | appK _ _ _ ih₁ _ => exact .app (by simp) ih₁ ‹_›
  | appV _ _ _ ih₁ ih₂ => exact .app (by simp) ih₁ ih₂
  | rcons _ _ _ ih₁ ih₂ => exact .rcons (by simp) ih₁ ih₂
  | rconsK _ _ _ ih₁ _ => exact .rcons (by simp) ih₁ ‹_›
  | rconsV _ _ _ ih₁ ih₂ => exact .rcons (by simp) ih₁ ih₂
  | updK _ _ _ ih₁ _ => exact .update (by simp) ih₁ ‹_›
  | updV _ _ _ ih₁ ih₂ => exact .update (by simp) ih₁ ih₂
  | jL hx hn _ ih => exact .jL (by simp) hn ih
  | jR hx hn _ ih => exact .jR (by simp) hn ih
  | let_ _ _ ih _ => exact .let_ ih ‹_›
  | proj _ ih => exact .proj ih
  | match_ _ _ ih _ => exact .match_ ih ‹_›
  | await _ ih => exact .await ih
  | force _ ih => exact .force ih
  | raise _ ih => exact .raise ih
  | try_ _ _ ih _ => exact .try_ ih ‹_›
  | inside _ ih => exact .inside ih
  | hwith hl _ _ _ _ _ ih => exact .hwith hl ‹_› ‹_› ih
  | hkeep hl _ _ _ ih => exact .hkeep hl ‹_› ih
  | hkarm hl _ _ _ _ ih _ => exact .hkarm hl ‹_› ih ‹_›
  | hctl hl hk _ _ _ _ _ _ ih _ => exact .hctl hl hk ‹_› ‹_› ih ‹_›
  | hres hl _ _ _ ih => exact .hres hl ‹_› ih
  | hdone _ ih => exact .hdone ih
  | settle hl _ ih => exact .settle hl ih
  | update hx h₁ h₂ => exact .update (by simp) h₁ h₂
  | dropA hb h _ => exact .dropA hb h
  | dropB hn hb h _ => exact .dropB hn hb h
  | dropC hb hn h _ => exact .dropC hb hn h
  | forceA h _ => exact .forceA h
  | forceC hn h _ => exact .forceC hn h
  | bornA hB hv hv' h _ => exact .bornA hB hv hv' h
  | bornB hB hv hv' h _ => exact .bornB hB hv hv' h
  | bornS hB hl hc hs => exact .bornS hB hl hc hs
  | bornV hB hl hc hc' hn => exact .bornV hB hl hc hc' hn
  | dead hB hn => exact .dead hB hn
  | hloc hB hH => exact .hloc hB hH
  | o4A hB hf hbr hpe hpe' he hb hne hnb _ _ => exact .o4A hB hf hbr hpe hpe' he hb hne hnb
  | o4B hB hbr hf' hpe hpe' he hb hne hnb _ _ => exact .o4B hB hbr hf' hpe hpe' he hb hne hnb
  | o4Fix1 hB hr hf hf' hcl hpe he hb hnb _ _ => exact .o4Fix1 hB hr hf hf' hcl hpe he hb hnb
  | o4Fix2 hB hl hf hf' hcl hpe he hb hne hnb _ _ => exact .o4Fix2 hB hl hf hf' hcl hpe he hb hne hnb
  | o4Alloc1 hB hr hs' hcl hpe he hc hnc _ _ => exact .o4Alloc1 hB hr hs' hcl hpe he hc hnc
  | o4AllocR hB hl hs hcl hpe he hc _ _ => exact .o4AllocR hB hl hs hcl hpe he hc
  | o4Bind hB hc _ => exact .o4Bind hB hc
  | o4Settle hB hH hhl hce hct hpt => exact .o4Settle hB hH hhl hce hct hpt
  | doneF hB hd hv' hc' hn => exact .doneF hB hd hv' hc' hn
  | doneR hB hd hv hc => exact .doneR hB hd hv hc
  | againF1 hB hc he hev hev' hσ hσn hσv hσv' _ _ =>
    exact .againF1 hB hc he hev hev' hσ hσn hσv hσv'
  | againF2 _ hB hl _ hc hσ hσn hσv hσv' iht _ =>
    exact .againF2 (by simp) hB hl iht hc hσ hσn hσv hσv'
  | againF3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' _ _ =>
    exact .againF3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv'
  | againR1 hB hc he hev hev' hσ hσn hσv hσv' _ _ =>
    exact .againR1 hB hc he hev hev' hσ hσn hσv hσv'
  | againR2 _ hB hl _ hc hσ hσn hσv hσv' iht _ =>
    exact .againR2 (by simp) hB hl iht hc hσ hσn hσv hσv'
  | againR3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv' _ _ =>
    exact .againR3 hB hd hv hv' hcv hcv' hnv' hvv hc hσ hσn hσv hσv'
  | letRaise hB hv hv' hvv _ => exact .letRaise hB hv hv' hvv
  | letUnw hB hl ha _ => exact .letUnw hB hl ha
  | letTodo hB => exact .letTodo hB
  | aconsD hd hn _ ih => exact .aconsD hd hn ih
  | _ => constructor <;> first | assumption | decide

theorem Sim.mono {W' : World} (hW : W.le W') {x : Pos} {a b : Tm} (h : Sim W x a b) :
    Sim W' x a b := by
  have hnB : ¬ W.B → ¬ W'.B := fun h h' => h (hW.B.2 h')
  induction h with
  | loc hl => exact .loc (hW.L _ _ _ hl)
  | hwith hl _ _ _ ih₁ ih₂ ih₃ => exact .hwith (hW.L _ _ _ hl) ih₁ ih₂ ih₃
  | hkeep hl _ _ ih₁ ih₂ => exact .hkeep (hW.L _ _ _ hl) ih₁ ih₂
  | hkarm hl _ _ _ ih₁ ih₂ ih₃ => exact .hkarm (hW.L _ _ _ hl) ih₁ ih₂ ih₃
  | hctl hl hk _ _ _ _ ih₁ ih₂ ih₃ ih₄ => exact .hctl (hW.L _ _ _ hl) (hk.mono hW) ih₁ ih₂ ih₃ ih₄
  | hres hl _ _ ih₁ ih₂ => exact .hres (hW.L _ _ _ hl) ih₁ ih₂
  | unw hl _ ih => exact .unw (hW.L _ _ _ hl) ih
  | kont hl => exact .kont (hW.L _ _ _ hl)
  | settle hl _ ih => exact .settle (hW.L _ _ _ hl) ih
  | dropB hn hb _ ih => exact .dropB (hW.J₁ _ hn) hb ih
  | jL hx hn _ ih => exact .jL hx (hW.J₁ _ hn) ih
  | jR hx hn _ ih => exact .jR hx (hW.J₂ _ hn) ih
  | bornS hB hl hc hs => exact .bornS (hW.B.1 hB) (hW.L _ _ _ hl) hc hs
  | bornV hB hl hc hc' hn => exact .bornV (hW.B.1 hB) (hW.L _ _ _ hl) hc hc' hn
  | dropA hb _ ih => exact .dropA hb ih
  | dropC hb hn _ ih => exact .dropC hb hn ih
  | forceA _ ih => exact .forceA ih
  | forceC hn _ ih => exact .forceC hn ih
  | bornA hB hv hv' _ ih => exact .bornA (hW.B.1 hB) hv hv' ih
  | bornB hB hv hv' _ ih => exact .bornB (hW.B.1 hB) hv hv' ih
  | dead hB hn => exact .dead (hW.B.1 hB) hn
  | hloc hB hH => exact .hloc (hW.B.1 hB) (hW.H _ _ hH)
  | o4A hB hf hbr hpe hpe' _ _ hne hnb ihe ihb =>
    exact .o4A (hW.B.1 hB) hf hbr (hW.R ▸ hpe) (hW.R ▸ hpe') ihe ihb hne hnb
  | o4B hB hbr hf' hpe hpe' _ _ hne hnb ihe ihb =>
    exact .o4B (hW.B.1 hB) hbr hf' (hW.R ▸ hpe) (hW.R ▸ hpe') ihe ihb hne hnb
  | o4Fix1 hB hr hf hf' hcl hpe _ _ hnb ihe ihb =>
    exact .o4Fix1 (hW.B.1 hB) (hW.HR _ _ hr) hf hf' hcl (hW.R ▸ hpe) ihe ihb hnb
  | o4Fix2 hB hl hf hf' hcl hpe _ _ hne hnb ihe ihb =>
    exact .o4Fix2 (hW.B.1 hB) (hW.HL _ _ hl) hf hf' hcl (hW.R ▸ hpe) ihe ihb hne hnb
  | o4Alloc1 hB hr hs' hcl hpe _ _ hnc ihe ihc =>
    exact .o4Alloc1 (hW.B.1 hB) (hW.HR _ _ hr) hs' hcl (hW.R ▸ hpe) ihe ihc hnc
  | o4AllocR hB hl hs hcl hpe _ _ ihe ihc =>
    exact .o4AllocR (hW.B.1 hB) (hW.HL _ _ hl) hs hcl (hW.R ▸ hpe) ihe ihc
  | o4Bind hB _ ih => exact .o4Bind (hW.B.1 hB) ih
  | o4Settle hB hH hhl hce hct hpt =>
    exact .o4Settle (hW.B.1 hB) (hW.H _ _ hH) (hW.HL _ _ hhl) hce hct (hW.R ▸ hpt)
  | doneF hB hd hv' hc' hn => exact .doneF (hnB hB) (hW.D _ _ _ _ hd) hv' hc' hn
  | doneR hB hd hv hc => exact .doneR (hnB hB) (hW.D _ _ _ _ hd) hv hc
  | againF1 hB hc _ hev hev' _ hσn hσv hσv' ihe ihσ =>
    exact .againF1 (hnB hB) hc ihe hev hev' ihσ hσn hσv hσv'
  | againF2 hx hB hl _ hc _ hσn hσv hσv' iht ihσ =>
    exact .againF2 hx (hnB hB) (hW.L _ _ _ hl) iht hc ihσ hσn hσv hσv'
  | againF3 hB hd hv hv' hcv hcv' hnv' _ hc _ hσn hσv hσv' ihv ihσ =>
    exact .againF3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' ihv hc ihσ hσn hσv hσv'
  | againR1 hB hc _ hev hev' _ hσn hσv hσv' ihe ihσ =>
    exact .againR1 (hnB hB) hc ihe hev hev' ihσ hσn hσv hσv'
  | againR2 hx hB hl _ hc _ hσn hσv hσv' iht ihσ =>
    exact .againR2 hx (hnB hB) (hW.L _ _ _ hl) iht hc ihσ hσn hσv hσv'
  | againR3 hB hd hv hv' hcv hcv' hnv' _ hc _ hσn hσv hσv' ihv ihσ =>
    exact .againR3 (hnB hB) (hW.D _ _ _ _ hd) hv hv' hcv hcv' hnv' ihv hc ihσ hσn hσv hσv'
  | letRaise hB hv hv' _ ih => exact .letRaise (hnB hB) hv hv' ih
  | letUnw hB hl _ ih => exact .letUnw (hnB hB) (hW.L _ _ _ hl) ih
  | letTodo hB => exact .letTodo (hnB hB)
  | appK hs _ _ ih₁ ih₂ => exact .appK hs ih₁ ih₂
  | appV hv _ _ ih₁ ih₂ => exact .appV hv ih₁ ih₂
  | rconsK hs _ _ ih₁ ih₂ => exact .rconsK hs ih₁ ih₂
  | rconsV hv _ _ ih₁ ih₂ => exact .rconsV hv ih₁ ih₂
  | updK hs _ _ ih₁ ih₂ => exact .updK hs ih₁ ih₂
  | updV hv _ _ ih₁ ih₂ => exact .updV hv ih₁ ih₂
  | aconsD hd hn _ ih => exact .aconsD hd hn ih
  | _ => constructor <;> assumption

theorem FrS.mono {W' : World} (hW : W.le W') {F F' : Frame} (h : FrS W F F') : FrS W' F F' := by
  cases h with
  | hwith hl h₁ h₂ => exact .hwith (hW.L _ _ _ hl) (h₁.mono hW) (h₂.mono hW)
  | hctlA hl hk h₁ h₂ h₃ => exact .hctlA (hW.L _ _ _ hl) (hk.mono hW) (h₁.mono hW) (h₂.mono hW) (h₃.mono hW)
  | hresB hl h₁ => exact .hresB (hW.L _ _ _ hl) (h₁.mono hW)
  | hkeep hl h₁ => exact .hkeep (hW.L _ _ _ hl) (h₁.mono hW)
  | hkarmA hl h₁ h₂ => exact .hkarmA (hW.L _ _ _ hl) (h₁.mono hW) (h₂.mono hW)
  | settle hl => exact .settle (hW.L _ _ _ hl)
  | _ => constructor <;> apply Sim.mono hW <;> assumption

theorem FrL.mono {W' : World} (hW : W.le W') {E E' : List Frame} (h : FrL W E E') : FrL W' E E' := by
  have hnB : ¬ W.B → ¬ W'.B := fun h h' => h (hW.B.2 h')
  induction h with
  | nil => exact .nil
  | cons hF _ ih => exact .cons (hF.mono hW) ih
  | junkL hn _ ih => exact .junkL (hW.J₁ _ hn) ih
  | junkR hn _ ih => exact .junkR (hW.J₂ _ hn) ih
  | againF hB hl hc hσ hσn hσv hσv' _ ih =>
    exact .againF (hnB hB) (hW.L _ _ _ hl) hc (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv' ih
  | againR hB hl hc hσ hσn hσv hσv' _ ih =>
    exact .againR (hnB hB) (hW.L _ _ _ hl) hc (fun i y hy => (hσ i y hy).mono hW) hσn hσv hσv' ih

end Flick.Equiv
