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

/-- Related contexts, the innermost frame first: related frames, and the frames of cells only
one side has. -/
inductive FrL (W : World) : List Frame → List Frame → Prop
  | nil : FrL W [] []
  | cons {F F' E E'} : FrS W F F' → FrL W E E' → FrL W (F :: E) (F' :: E')
  | junkL {n E E'} : W.J₁ n → FrL W E E' → FrL W (.settle n :: E) E'
  | junkR {n E E'} : W.J₂ n → FrL W E E' → FrL W E (.settle n :: E')

/-- Source code: no run-time form. -/
def _root_.Flick.Tm.Src : Tm → Prop
  | .var _ | .lit _ | .const _ | .ctor _ | .rnil | .anil | .todo | .glob _ => True
  | .lam b | .fix b | .task b | .await b | .lazy b | .force b | .raise b | .scope b | .proj b _ =>
    b.Src
  | .drec _ r => r.Src
  | .app f a | .let_ f a | .letrec f a | .rcons _ f a | .update f _ a | .match_ f a
  | .acons _ f a | .try_ f a | .handle f a => f.Src ∧ a.Src
  | .aguard _ g b r | .hdl _ g b r => g.Src ∧ b.Src ∧ r.Src
  | _ => False

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
  | bornA hv hv' h _ => exact .bornA hv hv' h
  | bornB hv hv' h _ => exact .bornB hv hv' h
  | bornS hl hc hs => exact .bornS hl hc hs
  | bornV hl hc hc' hn => exact .bornV hl hc hc' hn
  | dead hn => exact .dead hn
  | aconsD hd hn _ ih => exact .aconsD hd hn ih
  | _ => constructor <;> first | assumption | decide

theorem Sim.mono {W' : World} (hW : W.le W') {x : Pos} {a b : Tm} (h : Sim W x a b) :
    Sim W' x a b := by
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
  | bornS hl hc hs => exact .bornS (hW.L _ _ _ hl) hc hs
  | bornV hl hc hc' hn => exact .bornV (hW.L _ _ _ hl) hc hc' hn
  | dropA hb _ ih => exact .dropA hb ih
  | dropC hb hn _ ih => exact .dropC hb hn ih
  | forceA _ ih => exact .forceA ih
  | forceC hn _ ih => exact .forceC hn ih
  | bornA hv hv' _ ih => exact .bornA hv hv' ih
  | bornB hv hv' _ ih => exact .bornB hv hv' ih
  | dead hn => exact .dead hn
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
  induction h with
  | nil => exact .nil
  | cons hF _ ih => exact .cons (hF.mono hW) ih
  | junkL hn _ ih => exact .junkL (hW.J₁ _ hn) ih
  | junkR hn _ ih => exact .junkR (hW.J₂ _ hn) ih

end Flick.Equiv
