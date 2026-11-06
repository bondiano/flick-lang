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

/-- The cell `ℓ` of the handler `p` names no region created after `p`: what the
handler gives back and raises is older than whatever its arm opens. -/
def HCell (p ℓ : Nat) : Prop := ∃ t, Ψ[ℓ]? = some t ∧ t.YF p

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
  -- a handler is at the place of its `inside` form
  | .hwith p ℓ _ al r e, A => A p ∧ al.Within A ∧ r.Within A ∧ e.Within A ∧ al.NoIns ∧ r.NoIns ∧
    (∀ x, A x ↔ anc p x) ∧ HCell Ψ p ℓ
  -- the arm and the body it parked are at the same place, and share no region
  | .hctl p ℓ _ _ _ al r a k, A =>
    A p ∧ al.Within A ∧ r.Within A ∧ a.Within A ∧ k.Within A ∧ al.NoIns ∧ r.NoIns ∧
      (∀ x, a.Has (.frame x) → ¬ k.Has (.frame x)) ∧ (∀ x, A x ↔ anc p x) ∧ HCell Ψ p ℓ
  | .hres p ℓ a b, A => A p ∧ a.Within A ∧ b.Within A ∧ (∀ x, b.Has (.frame x) → ¬ a.Has (.frame x)) ∧
    HCell Ψ p ℓ
  | .hdone p a, A => A p ∧ a.Within A
  -- what runs once the body unwound runs at the handler's place, and holds no region
  | .unw p _ a, A => A p ∧ a.Within (anc p) ∧ (∀ x, ¬ a.Has (.frame x)) ∧ ∀ x, anc p x → A x
  | .kont p _, A => A p
  | .hole p, A => A p

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
  | .hwith p ℓ _ al r => A p ∧ al.Within Ψ B anc A ∧ r.Within Ψ B anc A ∧ al.NoIns ∧ r.NoIns ∧
    (∀ x, A x ↔ anc p x) ∧ HCell Ψ p ℓ
  -- the body a handler parked while its arm runs is at the same place
  | .hctlA p ℓ _ _ _ al r k =>
    A p ∧ al.Within Ψ B anc A ∧ r.Within Ψ B anc A ∧ k.Within Ψ B anc A ∧ al.NoIns ∧ r.NoIns ∧
      (∀ x, A x ↔ anc p x) ∧ HCell Ψ p ℓ
  | .hresB p ℓ a => A p ∧ a.Within Ψ B anc A ∧ HCell Ψ p ℓ
  | .hdone p => A p

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
theorem HCell.ext {X : List Ty} {p ℓ : Nat} : HCell Ψ p ℓ → HCell (Ψ ++ X) p ℓ :=
  fun ⟨t, ht, hy⟩ => ⟨t, by rw [List.getElem?_append_left (List.getElem?_eq_some_iff.mp ht).1]; exact ht, hy⟩

theorem Tm.Within.mono {Ψ' : List Ty} {B' anc' : Nat → Nat → Prop}
    (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l) (hΨ : ∀ p ℓ, HCell Ψ p ℓ → HCell Ψ' p ℓ) :
    ∀ {e : Tm} {A : Nat → Prop}, e.Within Ψ B anc A →
      (∀ q, (e.Has (.frame q) ∨ e.Has .ins ∧ e.Has (.rgn q)) → anc' q = anc q) →
        e.Within Ψ' B' anc' A := by
  intro e
  induction e <;> intro A h ha <;> simp only [Tm.Within, Tm.Has] at h ha ⊢ <;> grind

/-- Without `inside` forms and handlers, a term is at its place for any ancestors. -/
theorem Tm.Within.anyAnc {anc' : Nat → Nat → Prop} {e : Tm} {A : Nat → Prop} (hn : e.NoIns)
    (h : e.Within Ψ B anc A) : e.Within Ψ B anc' A :=
  h.mono (fun _ _ h => h) (fun _ _ h => h) fun q hq => hq.elim (fun h => (hn.frame q h).elim)
    fun h => (hn h.1).elim

theorem Frame.Within.plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.Within Ψ B anc A)
    (hr : r.Within Ψ B anc (F.hole A)) (hd : ∀ x, r.Has (.frame x) → ¬ (F.plug (.var 0)).Has (.frame x)) :
    (F.plug r).Within Ψ B anc A := by
  cases F <;> simp only [Frame.Within, Frame.hole, Frame.plug, Tm.Within, Tm.Has] at hF hr hd ⊢ <;> grind

theorem Frame.within_of_plug {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Within Ψ B anc A) : F.Within Ψ B anc A ∧ r.Within Ψ B anc (F.hole A) := by
  cases F <;> simp only [Frame.Within, Frame.hole, Frame.plug, Frame.ok, Tm.Within] at hF h ⊢ <;> grind

theorem within_of_plug {A : Nat → Prop} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Within Ψ B anc A →
      WithinE Ψ B anc A E ∧ r.Within Ψ B anc (holeE A E)
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := within_of_plug (r := F.plug r) (not_value_plug hv F) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.within_of_plug hF hv h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

/-- A region of a term in a frame is no region of the rest of the frame: an `inside`
form is not inside itself, and a handler's arm and the body it parks share none. -/
theorem Frame.disj {F : Frame} {A : Nat → Prop} {r : Tm} (hF : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Within Ψ B anc A) {x : Nat} (hx : r.Has (.frame x)) :
    ¬ (F.plug (.var 0)).Has (.frame x) := by
  have hFW := (Frame.within_of_plug hF hv h).1
  intro h'
  cases F with
  | inside p =>
    simp only [Frame.plug, Tm.Has, Tm.Within] at h h'
    rcases h' with h' | h' | h' | h'
    · cases h'
    · cases h'
    · cases h'; exact (h.2.2.frame hx).1 (.inr rfl)
    · exact h'
  | _ =>
    simp only [Frame.plug, Tm.Has, Tm.Within, Frame.Within] at h h' hFW <;>
      first
      | grind [Tm.has_frame_ins]
      | (rcases h' with h' | h'
         · exact absurd (Tm.has_frame_ins h') (hFW.2.noIns hFW.1)
         · exact h'.elim)
      | grind [Tm.has_frame_ins, Tm.NoIns]

theorem plug_mono {s : Site} {x y : Tm} (hxy : ∀ s, x.Has s → y.Has s) :
    ∀ {E : List Frame}, (plug E x).Has s → (plug E y).Has s
  | [], h => hxy s h
  | F :: E, h => plug_mono (E := E) (x := F.plug x) (y := F.plug y)
      (fun s h => by cases F <;> simp only [Frame.plug, Tm.Has] at h ⊢ <;> grind) h

/-- What a context writes around its hole it writes around any term. -/
theorem plug_of_hole {s : Site} {r : Tm} {E : List Frame} (h : (plug E (.var 0)).Has s) : (plug E r).Has s :=
  plug_mono (fun _ h => by simp [Tm.Has] at h) h

/-- A region of a term in a context is no region of the rest of the context. -/
theorem within_disj {A : Nat → Prop} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Within Ψ B anc A → ∀ {x}, r.Has (.frame x) →
      ¬ (plug E (.var 0)).Has (.frame x)
  | [], _, _, _, _, h => h
  | F :: E, hE, h, x, hx, h' => by
    obtain ⟨hF, hE'⟩ := hE.cons
    have hW := (within_of_plug (r := F.plug r) (not_value_plug hv F) hE' h).2
    rcases plug_has_inv (r := .var 0) E h' with h' | h'
    · exact within_disj (not_value_plug hv F) hE' h (F.plug_has hx) h'
    · exact Frame.disj hF hv hW hx h'

/-- A term in a context replaced by one at the place of its hole whose regions are
the old term's or new. -/
theorem Tm.Within.replug {A : Nat → Prop} {r₀ r : Tm} (hv : ¬ Value r₀) :
    ∀ {E : List Frame}, OkE E → (plug E r₀).Within Ψ B anc A → r.Within Ψ B anc (holeE A E) →
      (∀ x, r.Has (.frame x) → r₀.Has (.frame x) ∨ ¬ (plug E r₀).Has (.frame x)) →
      (plug E r).Within Ψ B anc A
  | [], _, _, hr, _ => hr
  | F :: E, hE, h, hr, hd => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨-, hW⟩ := within_of_plug (r := F.plug r₀) (not_value_plug hv F) hE' h
    have hFW := (Frame.within_of_plug hF hv hW).1
    refine Tm.Within.replug (E := E) (r₀ := F.plug r₀) (not_value_plug hv F) hE' h
      (hFW.plug hr fun x hx h' => ?_) fun x hx => ?_
    · rcases hd x hx with hx | hx
      · exact Frame.disj hF hv hW hx h'
      · exact hx (plug_has E (F.has_of_hole h'))
    · rcases F.plug_has_inv (r := .var 0) hx with hx | hx
      · exact .inl (F.has_of_hole hx)
      · exact (hd x hx).imp_left F.plug_has

theorem holeE_iff {A : Nat → Prop} {x : Nat} : ∀ {E : List Frame}, holeE A E x ↔ A x ∨ .inside x ∈ E
  | [] => by simp [holeE]
  | F :: E => by
    have := holeE_iff (A := A) (x := x) (E := E)
    cases F <;> simp [holeE, Frame.hole, this] <;> grind

/-- An `inside` form of `q` in a context: `q` is new at its place, and its ancestors
are regions of the hole or of the context: a body or an arm a handler parks holds
its own `inside` forms. -/
theorem WithinE.frameAnc {A : Nat → Prop} {q : Nat} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → (plug E (.var 0)).Has (.frame q) →
      ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧
        ∀ x, anc q x → holeE A E x ∨ (plug E (.var 0)).Has (.frame x)
  | [], _, h => by simp [plug, Tm.Has] at h
  | F :: E, ⟨hE, hF⟩, h => by
    have up : ∀ x, holeE A E x → holeE A (F :: E) x := fun x hx => by
      cases F <;> simp [holeE, Frame.hole, hx]
    have upA : ∀ x, A x → holeE A E x := fun x hx => holeE_iff.mpr (.inl hx)
    -- the `inside` forms of a part of `F` that is at the place of its hole
    have part : ∀ {t : Tm}, t.Within Ψ B anc (holeE A E) → t.Has (.frame q) →
        (∀ x, t.Has (.frame x) → (F.plug (.var 0)).Has (.frame x)) →
        ¬ A q ∧ anc q q ∧ (∀ x, A x → anc q x) ∧
          ∀ x, anc q x → holeE A (F :: E) x ∨ (plug (F :: E) (.var 0)).Has (.frame x) := by
      intro t ht hq hs
      obtain ⟨h₁, h₂, h₃, h₄⟩ := ht.frame hq
      refine ⟨fun h' => h₁ (upA _ h'), h₂, fun x hx => h₃ x (upA x hx), fun x hx => ?_⟩
      exact (h₄ x hx).imp (up x) fun h => plug_has E (hs x h)
    rcases plug_has_inv (r := .var 0) E h with h | h
    · obtain ⟨h₁, h₂, h₃, h₄⟩ := WithinE.frameAnc hE h
      exact ⟨h₁, h₂, h₃, fun x hx => (h₄ x hx).imp (up x) (plug_of_hole (r := F.plug (.var 0)))⟩
    cases F with
    | inside p =>
      simp only [Frame.plug, Tm.Has, Frame.Within] at h hF
      rcases h with h | h | h | h
      · cases h
      · cases h
      · cases h
        refine ⟨fun h' => hF.1 (upA _ h'), by rw [hF.2]; simp, fun x hx => ?_, ?_⟩
        · rw [hF.2]; exact .inl (upA x hx)
        · intro x hx; rw [hF.2] at hx; exact .inl (by simp [holeE, Frame.hole, hx])
      · simp at h
    | hctlA p ℓ κ tl E' al r k =>
      simp only [Frame.Within] at hF
      refine part hF.2.2.2.1 ?_ fun x hx => by simp [Frame.plug, Tm.Has, hx]
      simp only [Frame.plug, Tm.Has] at h
      grind [Tm.has_frame_ins, Tm.NoIns]
    | hresB p ℓ a =>
      simp only [Frame.Within] at hF
      refine part hF.2.1 ?_ fun x hx => by simp [Frame.plug, Tm.Has, hx]
      simp only [Frame.plug, Tm.Has] at h
      grind
    | _ =>
      simp only [Frame.plug, Tm.Has, Frame.Within] at h hF <;>
        first
        | grind [Tm.has_frame_ins, Tm.NoIns]
        | (rcases h with h | h
           · exact absurd (Tm.has_frame_ins h) (hF.2.noIns hF.1)
           · exact h.elim)

end Flick
