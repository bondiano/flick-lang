import Formal.Lemma4

/-!
# Where a run-time region may be written (`⊢ Λ`, regions.md §3.4)

A place in a configuration has the regions it may name: the regions in which the
work at that place was created. In a thread these are the regions of its base
(none for the main thread, the ancestors `anc ϱ` of the region `ϱ` for a child
or finalizer of `ϱ`), together with the regions of the `inside` forms around the
place. `e.Within A` says that `cap π` and `own π` of `e` name a region of their
place, that `inside π` names a new region whose ancestors are the regions of its
place and `π`, and that a location `ℓ` of `e` sees through its cell exactly the
regions `B ℓ`: those of its place that `⌈Ψ(ℓ)⌉` names. `inside` forms are only
where the term evaluates next, so none is lost or copied by a step.
-/

namespace Flick

section
variable (Ψ : List Ty) (B : Nat → Nat → Prop) (anc : Nat → Nat → Prop)

/-- The location `ℓ` at a place with the regions `A`. -/
def LocOk (A : Nat → Prop) (l : Nat) : Prop :=
  ∃ t, Ψ[l]? = some t ∧ B l = fun x => A x ∧ t.reach (.r x)

/-- `e` at a place with the regions `A`. -/
def Tm.Within : Tm → (Nat → Prop) → Prop
  | .var _, _ => True
  | .lam b, A => b.Within A ∧ b.NoIns
  | .app f a, A => f.Within A ∧ a.Within A ∧ (a.NoIns ∨ Value f)
  | .let_ e b, A => e.Within A ∧ b.Within A ∧ b.NoIns
  | .letrec f b, A => f.Within A ∧ b.Within A ∧ f.NoIns ∧ b.NoIns
  | .fix f, A => f.Within A ∧ f.NoIns
  | .lit _, _ => True
  | .loc l, A => LocOk Ψ B A l
  | .const _, _ => True
  | .ctor _, _ => True
  | .rnil, _ => True
  | .rcons _ e r, A => e.Within A ∧ r.Within A ∧ (r.NoIns ∨ Value e)
  | .proj e _, A => e.Within A
  | .update e _ e', A => e.Within A ∧ e'.Within A ∧ (e'.NoIns ∨ Value e)
  | .match_ e al, A => e.Within A ∧ al.Within A ∧ al.NoIns
  | .anil, _ => True
  | .acons _ b r, A => b.Within A ∧ r.Within A
  | .aguard _ g b r, A => g.Within A ∧ b.Within A ∧ r.Within A
  | .todo, _ => True
  | .task e, A => e.Within A ∧ e.NoIns
  | .await e, A => e.Within A
  | .raise e, A => e.Within A
  | .try_ e al, A => e.Within A ∧ al.Within A ∧ al.NoIns
  | .handle v al, A => v.Within A ∧ al.Within A ∧ v.NoIns ∧ al.NoIns
  | .provide l e, A => l.Within A ∧ e.Within A ∧ e.NoIns
  | .with_ _ θ e, A => θ.NoIns ∧ θ.Within (fun _ => False) ∧ e.Within A
  | .enil, _ => True
  | .econs _ v θ, A => v.Within A ∧ θ.Within A
  | .bind _ e, A => e.Within A
  | .join e₁ e₂, A => e₁.Within A ∧ e₂.Within A ∧ (e₂.NoIns ∨ Value e₁)
  | .scope e, A => e.Within A ∧ e.NoIns
  | .inside p e, A => ¬ A p ∧ anc p = (fun x => A x ∨ x = p) ∧ e.Within (fun x => A x ∨ x = p)
  | .cap p, A => A p
  | .own p, A => A p
  | .drec _ r, A => r.Within A ∧ r.NoIns
  | .glob _, _ => True
  | .hdl _ al r e, A => al.Within A ∧ r.Within A ∧ e.Within A ∧ al.NoIns ∧ r.NoIns ∧ e.NoIns
  | .hwith p _ al r e, A => A p ∧ al.Within A ∧ r.Within A ∧ e.Within A ∧ al.NoIns ∧ r.NoIns
  | .hctl p _ _ al r a k, A =>
    A p ∧ al.Within A ∧ r.Within A ∧ a.Within A ∧ k.Within A ∧ al.NoIns ∧ r.NoIns
  | .hres p a b, A => A p ∧ a.Within A ∧ b.Within A
  | .hdone p a, A => A p ∧ a.Within A
  | .hab p a e, A => A p ∧ a.Within A ∧ e.Within A
  | .unw p, A => A p
  | .kont p, A => A p
  | .hole, _ => True

/-- The frames of a context at a place with the regions `A`: what the frame
holds besides the hole is there, and does not evaluate before the hole. -/
def Frame.Within (A : Nat → Prop) : Frame → Prop
  | .appL a => a.Within Ψ B anc A ∧ a.NoIns
  | .appR v => v.Within Ψ B anc A ∧ Value v
  | .let_ b => b.Within Ψ B anc A ∧ b.NoIns
  | .match_ al => al.Within Ψ B anc A ∧ al.NoIns
  | .rconsL _ r => r.Within Ψ B anc A ∧ r.NoIns
  | .rconsR _ v => v.Within Ψ B anc A ∧ Value v
  | .proj _ => True
  | .updL _ e' => e'.Within Ψ B anc A ∧ e'.NoIns
  | .updR v _ => v.Within Ψ B anc A ∧ Value v
  | .await => True
  | .try_ al => al.Within Ψ B anc A ∧ al.NoIns
  | .provide e => e.Within Ψ B anc A ∧ e.NoIns
  | .bind _ => True
  | .joinL e => e.Within Ψ B anc A ∧ e.NoIns
  | .joinR v => v.Within Ψ B anc A ∧ Value v
  | .with_ _ θ => θ.NoIns ∧ θ.Within Ψ B anc (fun _ => False)
  | .raise => True
  | .inside p => ¬ A p ∧ anc p = fun x => A x ∨ x = p

/-- The regions of the hole of a frame. -/
def Frame.hole (A : Nat → Prop) : Frame → Nat → Prop
  | .inside p => fun x => A x ∨ x = p
  | _ => A

/-- The regions of the hole of a context; the list starts at the innermost frame. -/
def holeE (A : Nat → Prop) : List Frame → Nat → Prop
  | [] => A
  | F :: E => F.hole (holeE A E)

def WithinE (A : Nat → Prop) : List Frame → Prop
  | [] => True
  | F :: E => WithinE A E ∧ F.Within Ψ B anc (holeE A E)

end

variable {Ψ : List Ty} {B : Nat → Nat → Prop} {anc : Nat → Nat → Prop}

/-- A value at a place holds no `inside` form. -/
theorem Value.noIns {e : Tm} {A : Nat → Prop} (hv : Value e) (h : e.Within Ψ B anc A) : e.NoIns := by
  induction hv generalizing A with
  | lam => exact h.2
  | fix => exact h.2
  | task => exact h.2
  | rcons _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | ctorApp _ _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | constApp _ _ ih => rintro (h' | h'); cases h'; exact ih h.2.1 h'
  | constApp2 _ _ _ _ ih₁ ih₂ =>
    rintro ((h' | h') | h'); cases h'; exact ih₁ h.1.2.1 h'; exact ih₂ h.2.1 h'
  | econs _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2 h'
  | drec => exact h.2
  | _ => intro h'; cases h'

/-- A term without `inside` forms is at a place when what it writes is. -/
theorem Tm.Within.of_has {A : Nat → Prop} : ∀ {e : Tm}, e.NoIns → (∀ p, e.Has (.rgn p) → A p) →
    (∀ l, e.Has (.loc l) → LocOk Ψ B A l) → e.Within Ψ B anc A := by
  intro e
  induction e <;> intro hn hr hl <;> simp only [Tm.Within] <;> (try and_intros) <;>
    first
    | trivial
    | (exfalso; apply hn; simp [Tm.Has]; done)
    | exact hl _ rfl
    | exact hr _ rfl
    | exact .inl fun h => hn (by simp [Tm.Has, h])
    | exact fun h => hn (by simp [Tm.Has, h])
    | (apply_assumption
       · exact fun h => hn (by simp [Tm.Has, h])
       · exact fun p h => hr p (by simp [Tm.Has, h])
       · exact fun l h => hl l (by simp [Tm.Has, h]))

/-- A region written in a term is one of its place or of an `inside` form in it. -/
theorem Tm.Within.rgn {p : Nat} : ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A → e.Has (.rgn p) →
    A p ∨ e.Has (.frame p) := by
  intro e
  induction e <;> intro A h hs <;> simp only [Tm.Within, Tm.Has] at h hs ⊢ <;> grind

/-- A location of a term is at a place between its place and the `inside` forms in it. -/
theorem Tm.Within.loc {l : Nat} : ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A → e.Has (.loc l) →
    ∃ A', (∀ x, A' x → A x ∨ e.Has (.frame x)) ∧ LocOk Ψ B A' l := by
  intro e
  induction e <;> intro A h hs <;> simp only [Tm.Within, Tm.Has] at h hs ⊢ <;> grind

/-- A location of a term without run-time forms is at the place of the term. -/
theorem Tm.Within.loc0 {l : Nat} : ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A → e.NoIns →
    e.Has (.loc l) → LocOk Ψ B A l := by
  intro e
  induction e <;> intro A h hn hs <;> simp only [Tm.NoIns, Tm.Has, not_or] at hn <;>
    simp only [Tm.Within, Tm.Has] at h hs ⊢ <;> (try simp only [Tm.NoIns] at *) <;> grind

/-- The `inside` form of `q` in a term: `q` is new at its place, and its ancestors
are between the place of the term and the `inside` forms in it. -/
theorem Tm.Within.frame {q : Nat} : ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A → e.Has (.frame q) →
    ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧ ∀ x, anc q x → A x ∨ e.Has (.frame x) := by
  intro e
  induction e with
  | with_ p θ e _ ih =>
    intro A h hs
    rcases hs with hs | hs | hs | hs
    · cases hs
    · cases hs
    · exact absurd (Tm.has_frame_ins hs) h.1
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := ih h.2.2 hs
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp_right fun h => .inr (.inr (.inr h))⟩
  | _ => intro A h hs; simp only [Tm.Within, Tm.Has] at h hs ⊢ <;> grind

/-- A term stays at its place when the ancestors of its `inside` forms and its
locations do. -/
theorem Tm.Within.mono {Ψ' : List Ty} {B' anc' : Nat → Nat → Prop}
    (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l) :
    ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A → (∀ q, e.Has (.frame q) → anc' q = anc q) →
      e.Within Ψ' B' anc' A := by
  intro e
  induction e <;> intro A h ha <;> simp only [Tm.Within, Tm.Has] at h ha ⊢ <;> grind

theorem Frame.Within.plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.Within Ψ B anc A)
    (hr : r.Within Ψ B anc (F.hole A)) : (F.plug r).Within Ψ B anc A := by
  cases F <;> simp only [Frame.Within, Frame.hole, Frame.plug, Tm.Within] at hF hr ⊢ <;> grind

theorem Frame.within_of_plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Within Ψ B anc A) : F.Within Ψ B anc A ∧ r.Within Ψ B anc (F.hole A) := by
  cases F <;> simp only [Frame.Within, Frame.hole, Frame.plug, Frame.ok, Tm.Within] at hF h ⊢ <;> grind

theorem WithinE.fill {A : Nat → Prop} {r : Tm} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → r.Within Ψ B anc (holeE A E) → (plug E r).Within Ψ B anc A
  | [], _, hr => hr
  | F :: _, ⟨hE, hF⟩, hr => WithinE.fill (r := F.plug r) hE (hF.plug hr)

theorem within_of_plug {A : Nat → Prop} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Within Ψ B anc A →
      WithinE Ψ B anc A E ∧ r.Within Ψ B anc (holeE A E)
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := within_of_plug (r := F.plug r) (not_value_plug hv F) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.within_of_plug hF hv h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

theorem holeE_iff {A : Nat → Prop} {x : Nat} : ∀ {E : List Frame}, holeE A E x ↔ A x ∨ .inside x ∈ E
  | [] => by simp [holeE]
  | F :: E => by
    have := holeE_iff (A := A) (x := x) (E := E)
    cases F <;> simp [holeE, Frame.hole, this] <;> grind

/-- The `inside` forms of a term in a context are those of the context and of the term. -/
theorem WithinE.frames {A : Nat → Prop} {r : Tm} {q : Nat} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → (plug E r).Has (.frame q) → .inside q ∈ E ∨ r.Has (.frame q)
  | [], _, h => .inr h
  | F :: E, ⟨hE, hF⟩, h => by
    rcases WithinE.frames (r := F.plug r) hE h with h | h
    · exact .inl (List.mem_cons_of_mem _ h)
    · cases F <;> simp only [Frame.plug, Tm.Has, Frame.Within] at h hF <;>
        first
        | (rcases h with h | h
           · exact absurd (Tm.has_frame_ins h) (by first | exact hF.2 | exact (hF.2.noIns hF.1))
           · exact .inr h)
        | (rcases h with h | h
           · exact .inr h
           · exact absurd (Tm.has_frame_ins h) (by first | exact hF.2 | exact (hF.2.noIns hF.1)))
        | exact .inr h
        | (rcases h with h | h | h | h
           · cases h
           · cases h
           · cases h; exact .inl (by simp)
           · exact .inr h)
        | (rcases h with h | h | h | h
           · cases h
           · cases h
           · exact absurd (Tm.has_frame_ins h) hF.1
           · exact .inr h)

/-- The `inside` form of `q` in a context: `q` is new there and its ancestors are
regions of the hole. -/
theorem WithinE.frameAnc {A : Nat → Prop} {q : Nat} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → .inside q ∈ E →
      ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧ ∀ x, anc q x → holeE A E x
  | [], _, h => by simp at h
  | F :: E, ⟨hE, hF⟩, h => by
    have up : ∀ x, holeE A E x → holeE A (F :: E) x := fun x hx => by
      cases F <;> simp [holeE, Frame.hole, hx]
    rcases List.mem_cons.mp h with rfl | h
    · simp only [Frame.Within] at hF
      refine ⟨fun h' => hF.1 (holeE_iff.mpr (.inl h')), by rw [hF.2]; simp, fun x hx => ?_, ?_⟩
      · rw [hF.2]; exact .inl (holeE_iff.mpr (.inl hx))
      · intro x hx; rw [hF.2] at hx; simp [holeE, Frame.hole, hx]
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := WithinE.frameAnc hE h
      exact ⟨h₁, h₂, h₃, fun x hx => up x (h₄ x hx)⟩

/-- Regions whose ancestors are the regions before them. -/
def Chain (anc : Nat → Nat → Prop) (A : Nat → Prop) : Prop :=
  ∀ x, A x → anc x = fun y => A y ∧ y ≤ x

theorem Chain.inside {A : Nat → Prop} {p : Nat} (h : Chain anc A) (hp : ¬ A p)
    (ha : anc p = fun x => A x ∨ x = p) (hle : ∀ x, anc p x → x ≤ p) :
    Chain anc (fun x => A x ∨ x = p) := by
  have lt : ∀ x, A x → x < p := fun x hx =>
    Nat.lt_of_le_of_ne (hle x (by rw [ha]; exact .inl hx)) fun h' => hp (h' ▸ hx)
  rintro x (hx | rfl)
  · rw [h x hx]; funext y
    have := lt x hx
    apply propext; constructor
    · rintro ⟨h₁, h₂⟩; exact ⟨.inl h₁, h₂⟩
    · rintro ⟨h₁ | rfl, h₂⟩; exact ⟨h₁, h₂⟩; omega
  · rw [ha]; funext y
    apply propext; constructor
    · rintro (h₁ | rfl); exact ⟨.inl h₁, Nat.le_of_lt (lt y h₁)⟩; exact ⟨.inr rfl, Nat.le_refl _⟩
    · exact fun h => h.1

theorem WithinE.chain {A : Nat → Prop} (hle : ∀ q x, anc q x → x ≤ q) :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → Chain anc A → Chain anc (holeE A E)
  | [], _, h => h
  | F :: E, ⟨hE, hF⟩, h => by
    have h' := WithinE.chain hle hE h
    cases F <;> simp only [holeE, Frame.hole] <;> try exact h'
    exact h'.inside hF.1 hF.2 (hle _)

/-- What a term reaches through the store is a region of its place or of an
`inside` form in it, when every cell holds only what its locations see. -/
theorem Tm.Within.reaches {μ : List Tm}
    (hμ : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Within Ψ B anc (B l) ∧ v.NoIns) {e : Tm} {p : Nat}
    (hr : Reaches μ e p) : ∀ {A : Nat → Prop}, e.Within Ψ B anc A → A p ∨ e.Has (.frame p) := by
  induction hr with
  | here hp => exact fun h => h.rgn hp
  | cell hl hv _ ih =>
    intro A h
    obtain ⟨hw, hn⟩ := hμ _ _ hv
    obtain ⟨A', h₂, t, _, hB⟩ := h.loc hl
    rcases ih hw with h' | h'
    · rw [hB] at h'; exact h₂ _ h'.1
    · exact absurd h' (hn.frame _)

end Flick
