import Formal.WithinChain

/-!
# The run-time forms of a place, whatever its values name

`e.Fw A` is `e.Within A` without what a term names: a location, a capability, an owner
and `resume` are anywhere, and the cell of a handler names in its arms' row only
ancestors of the handler (`LCell`). What remains is where the `inside` forms and the
forms of handlers of a term are, which a step keeps whatever the typing discipline.
-/

namespace Flick

/-- The cell of the handler of region `q`: its arms' row names only ancestors of `q`. -/
def LCell (Ψ : List Ty) (anc : Nat → Nat → Prop) (q ℓ : Nat) : Prop :=
  ∃ r ε t, Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧ ∀ x, r (.atm (.r x)) → anc q x

section
variable (Ψ : List Ty) (anc : Nat → Nat → Prop)

/-- The run-time forms of `e` at a place with the regions `A`. -/
def Tm.Fw : Tm → (Nat → Prop) → Prop
  | .var _, _ => True
  | .lam b, A => b.Fw A ∧ b.NoIns
  | .app f a, A => f.Fw A ∧ a.Fw A ∧ (a.NoIns ∨ Value f)
  | .let_ e b, A => e.Fw A ∧ b.Fw A ∧ b.NoIns
  | .letrec f b, A => f.Fw A ∧ b.Fw A ∧ f.NoIns ∧ b.NoIns
  | .fix f, A => f.Fw A ∧ f.NoIns
  | .lit _, _ => True
  | .loc _, _ => True
  | .const _, _ => True
  | .ctor _, _ => True
  | .rnil, _ => True
  | .rcons _ e r, A => e.Fw A ∧ r.Fw A ∧ (r.NoIns ∨ Value e)
  | .proj e _, A => e.Fw A
  | .update e _ e', A => e.Fw A ∧ e'.Fw A ∧ (e'.NoIns ∨ Value e)
  | .match_ e al, A => e.Fw A ∧ al.Fw A ∧ al.NoIns
  | .anil, _ => True
  | .acons _ b r, A => b.Fw A ∧ r.Fw A
  | .aguard _ g b r, A => g.Fw A ∧ b.Fw A ∧ r.Fw A
  | .todo, _ => True
  | .task e, A => e.Fw A ∧ e.NoIns
  | .await e, A => e.Fw A
  | .raise e, A => e.Fw A
  | .try_ e al, A => e.Fw A ∧ al.Fw A ∧ al.NoIns
  | .handle v al, A => v.Fw A ∧ al.Fw A ∧ v.NoIns ∧ al.NoIns
  | .scope e, A => e.Fw A ∧ e.NoIns
  | .inside p e, A => ¬ A p ∧ anc p = (fun x => A x ∨ x = p) ∧ e.Fw (fun x => A x ∨ x = p)
  | .cap _, _ => True
  | .own _, _ => True
  | .drec _ r, A => r.Fw A ∧ r.NoIns
  | .glob _, _ => True
  | .hdl _ al r e, A => al.Fw A ∧ r.Fw A ∧ e.Fw A ∧ al.NoIns ∧ r.NoIns ∧ e.NoIns
  | .hwith p ℓ _ al r e, A => A p ∧ al.Fw A ∧ r.Fw A ∧ e.Fw A ∧ al.NoIns ∧ r.NoIns ∧
    (∀ x, A x ↔ anc p x) ∧ LCell Ψ anc p ℓ
  | .hctl p ℓ _ _ _ al r a k, A =>
    A p ∧ al.Fw A ∧ r.Fw A ∧ a.Fw A ∧ k.Fw A ∧ al.NoIns ∧ r.NoIns ∧
      (∀ x, a.Has (.frame x) → ¬ k.Has (.frame x)) ∧ (∀ x, A x ↔ anc p x) ∧ LCell Ψ anc p ℓ
  | .hres p ℓ a b, A => A p ∧ a.Fw A ∧ b.Fw A ∧ (∀ x, b.Has (.frame x) → ¬ a.Has (.frame x)) ∧
    LCell Ψ anc p ℓ
  | .hdone p a, A => A p ∧ a.Fw A
  | .unw p _ a, A => A p ∧ a.Fw (anc p) ∧ (∀ x, ¬ a.Has (.frame x)) ∧ ∀ x, anc p x → A x
  | .kont _ _, _ => True
  | .hole p, A => A p
  | .hkeep q ℓ _ al e, A => A q ∧ al.Fw (anc q) ∧ e.Fw A ∧ al.NoIns ∧
    (∀ x, anc q x → A x) ∧ LCell Ψ anc q ℓ
  | .hkarm q ℓ _ al a k, A => A q ∧ al.Fw (anc q) ∧ a.Fw A ∧ k.Fw A ∧ al.NoIns ∧
    (∀ x, a.Has (.frame x) → ¬ k.Has (.frame x)) ∧ (∀ x, anc q x → A x) ∧ LCell Ψ anc q ℓ

/-- The frames of a context at a place with the regions `A`. -/
def Frame.Fw (A : Nat → Prop) : Frame → Prop
  | .appL a => a.Fw Ψ anc A ∧ a.NoIns
  | .appR v => v.Fw Ψ anc A ∧ Value v
  | .let_ b => b.Fw Ψ anc A ∧ b.NoIns
  | .match_ al => al.Fw Ψ anc A ∧ al.NoIns
  | .rconsL _ r => r.Fw Ψ anc A ∧ r.NoIns
  | .rconsR _ v => v.Fw Ψ anc A ∧ Value v
  | .proj _ => True
  | .updL _ e' => e'.Fw Ψ anc A ∧ e'.NoIns
  | .updR v _ => v.Fw Ψ anc A ∧ Value v
  | .await => True
  | .try_ al => al.Fw Ψ anc A ∧ al.NoIns
  | .raise => True
  | .inside p => ¬ A p ∧ anc p = fun x => A x ∨ x = p
  | .hwith p ℓ _ al r => A p ∧ al.Fw Ψ anc A ∧ r.Fw Ψ anc A ∧ al.NoIns ∧ r.NoIns ∧
    (∀ x, A x ↔ anc p x) ∧ LCell Ψ anc p ℓ
  | .hctlA p ℓ _ _ _ al r k =>
    A p ∧ al.Fw Ψ anc A ∧ r.Fw Ψ anc A ∧ k.Fw Ψ anc A ∧ al.NoIns ∧ r.NoIns ∧
      (∀ x, A x ↔ anc p x) ∧ LCell Ψ anc p ℓ
  | .hresB p ℓ a => A p ∧ a.Fw Ψ anc A ∧ LCell Ψ anc p ℓ
  | .hdone p => A p
  | .hkeep q ℓ _ al => A q ∧ al.Fw Ψ anc (anc q) ∧ al.NoIns ∧ (∀ x, anc q x → A x) ∧
    LCell Ψ anc q ℓ
  | .hkarmA q ℓ _ al k => A q ∧ al.Fw Ψ anc (anc q) ∧ k.Fw Ψ anc A ∧ al.NoIns ∧
    (∀ x, anc q x → A x) ∧ LCell Ψ anc q ℓ

def FwE (A : Nat → Prop) : List Frame → Prop
  | [] => True
  | F :: E => FwE A E ∧ F.Fw Ψ anc (holeE A E)

end

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

/-- A value at a place holds no `inside` form. -/
theorem Value.noInsF {e : Tm} {A : Nat → Prop} (hv : Value e) (h : e.Fw Ψ anc A) : e.NoIns := by
  induction hv generalizing A with
  | lam => exact h.2
  | fix => exact h.2
  | task => exact h.2
  | rcons _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | ctorApp _ _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | constApp _ _ ih => rintro (h' | h'); cases h'; exact ih h.2.1 h'
  | drec => exact h.2
  | _ => intro h'; cases h'

/-- A term without run-time forms is at any place. -/
theorem Tm.Fw.of_noIns {A : Nat → Prop} : ∀ {e : Tm}, e.NoIns → e.Fw Ψ anc A := by
  intro e
  induction e <;> intro hn <;> simp only [Tm.Fw] <;> (try and_intros) <;>
    first
    | trivial
    | (exfalso; apply hn; simp [Tm.Has]; done)
    | exact .inl fun h => hn (by simp [Tm.Has, h])
    | exact fun h => hn (by simp [Tm.Has, h])
    | (apply_assumption; exact fun h => hn (by simp [Tm.Has, h]))

/-- The `inside` form of `q` in a term: `q` is new at its place, and its ancestors
are between the place of the term and the `inside` forms in it. -/
theorem Tm.Fw.frame {q : Nat} : ∀ {e : Tm} {A : Nat → Prop}, e.Fw Ψ anc A → e.Has (.frame q) →
    ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧ ∀ x, anc q x → A x ∨ e.Has (.frame x) := by
  intro e
  induction e with
  | hkeep p ℓ E' al e _ ih =>
    intro A h hs
    rcases hs with hs | hs | hs | hs
    · cases hs
    · cases hs
    · exact absurd (Tm.has_frame_ins hs) h.2.2.2.1
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := ih h.2.2.1 hs
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp_right fun h => .inr (.inr (.inr h))⟩
  | hkarm p ℓ E' al a k _ ih₁ ih₂ =>
    intro A h hs
    rcases hs with hs | hs | hs | hs | hs
    · cases hs
    · cases hs
    · exact absurd (Tm.has_frame_ins hs) h.2.2.2.2.1
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := ih₁ h.2.2.1 hs
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp_right fun h => .inr (.inr (.inr (.inl h)))⟩
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := ih₂ h.2.2.2.1 hs
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp_right fun h => .inr (.inr (.inr (.inr h)))⟩
  | _ => intro A h hs; simp only [Tm.Fw, Tm.Has] at h hs ⊢ <;> grind

theorem LCell.mono {Ψ' : List Ty} {anc' : Nat → Nat → Prop} {q ℓ : Nat}
    (hΨ : ∀ (l : Nat) (t : Ty), Ψ[l]? = some t → Ψ'[l]? = some t) (h : LCell Ψ anc q ℓ) (ha : anc' q = anc q) :
    LCell Ψ' anc' q ℓ :=
  let ⟨r, ε, t, h₁, h₂⟩ := h; ⟨r, ε, t, hΨ _ _ h₁, fun x hx => ha ▸ h₂ x hx⟩

/-- A term stays at its place in a larger store, with the ancestors of the regions of its
place and of its `inside` forms kept. -/
theorem Tm.Fw.mono {Ψ' : List Ty} {anc' : Nat → Nat → Prop}
    (hΨ : ∀ (l : Nat) (t : Ty), Ψ[l]? = some t → Ψ'[l]? = some t) :
    ∀ {e : Tm} {A : Nat → Prop}, e.Fw Ψ anc A → (∀ q, A q ∨ e.Has (.frame q) → anc' q = anc q) →
      e.Fw Ψ' anc' A := by
  intro e
  induction e <;> intro A h ha <;> simp only [Tm.Fw, Tm.Has] at h ha ⊢ <;> grind [LCell.mono]

theorem Frame.Fw.plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.Fw Ψ anc A)
    (hr : r.Fw Ψ anc (F.hole A)) (hd : ∀ x, r.Has (.frame x) → ¬ (F.plug (.var 0)).Has (.frame x)) :
    (F.plug r).Fw Ψ anc A := by
  cases F <;> simp only [Frame.Fw, Frame.hole, Frame.plug, Tm.Fw, Tm.Has] at hF hr hd ⊢ <;> grind

theorem Frame.fw_of_plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Fw Ψ anc A) : F.Fw Ψ anc A ∧ r.Fw Ψ anc (F.hole A) := by
  cases F <;> simp only [Frame.Fw, Frame.hole, Frame.plug, Frame.ok, Tm.Fw] at hF h ⊢ <;> grind

theorem fw_of_plug {A : Nat → Prop} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Fw Ψ anc A → FwE Ψ anc A E ∧ r.Fw Ψ anc (holeE A E)
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := fw_of_plug (r := F.plug r) (not_value_plug hv F) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.fw_of_plug hF hv h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

end Flick
