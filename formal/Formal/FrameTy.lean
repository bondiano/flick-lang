import Formal.Frames
import Formal.Cut

/-!
# One frame of a context: the typing of its hole (L3)

How the stage of a hole relates to the stage around a frame (`FRelM`), what
refills it after a step (`RefillM`), and the typing of a frame (`FrameTyM`).
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

/-- How the stage of a hole relates to the stage around one frame. -/
structure FRelM (mode : TypingMode) (F : Frame) (φ' φ : Stage) : Prop where
  svc : Row.Sub φ'.svc (Row.union φ.svc (mode.dom F))
  err : ¬ F.isTry → Row.Sub φ'.err φ.err
  pure : φ = .pure → φ' = .pure
  rt : ¬ F.isTry → φ' ≠ .pure → φ ≠ .pure

abbrev FRel (F : Frame) (φ' φ : Stage) : Prop := FRelM .captures F φ' φ

theorem FRelM.of_le {mode F φ' φ} (h : φ'.le φ) : FRelM mode F φ' φ where
  svc := fun n hn => .inl (Stage.le_svc h n hn)
  err := fun _ => Stage.le_err h
  pure := fun hp => by subst hp; exact Stage.eq_pure h
  rt := fun _ hp hφ => by subst hφ; exact hp (Stage.eq_pure h)

theorem FRel.of_le {F φ' φ} (h : φ'.le φ) : FRel F φ' φ := FRelM.of_le h

/-- A hole that uses what the frame provides: in the live discipline a runtime form of a
handler stands for its own region, so the hole may use it. -/
theorem FRelM.use_rgn {mode F φ ψ} {C : Caps}
    (hF : mode = .live → ∀ a, C a → mode.dom F (.atm a)) (h : ψ.le (mode.use φ C)) :
    FRelM mode F ψ φ where
  svc := fun n hn => by
    rcases TypingMode.use_svc mode φ C (Stage.le_svc h n hn) with h' | ⟨hm, a, ha, rfl⟩
    · exact .inl h'
    · exact .inr (hF hm a ha)
  err := fun _ => by simpa only [TypingMode.use_err] using Stage.le_err h
  pure := fun hp => by
    subst hp; simp only [TypingMode.use_pure] at h; exact Stage.eq_pure h
  rt := fun _ hp hφ => by subst hφ; simp only [TypingMode.use_pure] at h; exact hp (Stage.eq_pure h)

/-- The hole after a step, in a store grown by `X`, refills the frame; so it does
after a substitution `σ` that fits the term, which renames the regions not yet
created out of the whole derivation (T4). -/
def Refill0M (S : Sig) (mode : TypingMode) (Ψ : List Ty) (Θ : Facts) (t' : Ty) (φ' : Stage) (C : Tm → Tm) (t : Ty)
    (φ : Stage) (r : Tm) : Prop :=
  ∀ σ, Fits σ (C r) → ∀ X r', TypM S mode (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] r'
      (.ex (t'.substT σ) (φ'.substT σ)) →
    TypM S mode (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] (C r') (.ex (t.substT σ) (φ.substT σ))

/-- As `Refill0`, for a hole `k` type variables deep: under (Let-Gen) the hole is
typed with the store and the facts shifted past the generalized variables, and
the facts of the scheme assumed (`Θ'`). -/
def RefillM (S : Sig) (mode : TypingMode) (Ψ : List Ty) (Θ : Facts) (k : Nat) (Θ' : Facts) (t' : Ty) (φ' : Stage)
    (C : Tm → Tm) (t : Ty) (φ : Stage) (r : Tm) : Prop :=
  ∀ σ, Fits σ (C r) → ∀ X r', TypM S mode ((Ψ.map (Ty.substT σ) ++ X).map (Ty.up k))
      (Θ'.substT (liftT k σ)) [] r' (.ex (t'.substT (liftT k σ)) (φ'.substT (liftT k σ))) →
    TypM S mode (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] (C r') (.ex (t.substT σ) (φ.substT σ))

theorem List.map_substT_ident (Ψ : List Ty) : Ψ.map (Ty.substT Ty.ident) = Ψ := by
  induction Ψ with
  | nil => rfl
  | cons t Ψ ih => simp [Ty.substT_ident, ih]

/-- `Refill` without a substitution. -/
theorem RefillM.id {Ψ Θ k Θ' t' φ' C t φ r} (h : RefillM S mode Ψ Θ k Θ' t' φ' C t φ r) :
    ∀ X r', TypM S mode ((Ψ ++ X).map (Ty.up k)) Θ' [] r' (.ex t' φ') →
      TypM S mode (Ψ ++ X) Θ [] (C r') (.ex t φ) := by
  intro X r' h'
  have := h Ty.ident ⟨fun _ _ => rfl, fun _ b _ hb => ((Ty.reach_ident b _).mp hb).symm,
    fun _ b a hb => by rw [(Ty.reach_ident b a).mp hb]; cases a <;> rfl, fun _ => ident_noNames⟩ X r'
  simp only [List.map_substT_ident, liftT_ident, Ty.substT_ident, Stage.substT_ident,
    Facts.substT_ident] at this
  exact this h'

/-- A derivation of a part of a frame, after a substitution and in a grown store. -/
theorem TypM.sx {Ψ Θ e J} (h : TypM S mode Ψ Θ [] e J) {σ : Atom → Ty} (hσ : Fits σ e) (X : List Ty) :
    TypM S mode (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X



theorem cell_sx {Ψ : List Ty} {σ : Atom → Ty} {ℓ : Nat} {T : Ty} (h : Ψ[ℓ]? = some T) (X : List Ty) :
    (Ψ.map (Ty.substT σ) ++ X)[ℓ]? = some (T.substT σ) := by
  rw [List.getElem?_append_left (by simpa using getElem?_lt h), List.getElem?_map, h]; rfl

theorem TypM.sx1 {Ψ Θ a e J} (h : TypM S mode Ψ Θ [.mono a] e J) {σ : Atom → Ty} (hσ : Fits σ e)
    (X : List Ty) :
    TypM S mode (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [.mono (a.substT σ)] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X



/-- One frame of L3: the hole is `k` type variables deep, and pure if `k ≠ 0`;
at depth `0` it has the facts of the frame. -/
def FrameTyM (S : Sig) (mode : TypingMode) (Ψ : List Ty) (Θ : Facts) (F : Frame) (r : Tm) (t : Ty) (φ : Stage) : Prop :=
  ∃ k Θ' t' φ', TypM S mode (Ψ.map (Ty.up k)) Θ' [] r (.ex t' φ') ∧ FRelM mode F φ' φ ∧
    (φ' = .pure ∨ k = 0 ∧ Ext Θ Θ' ∧ (¬ F.isInside → Θ' = Θ)) ∧ RefillM S mode Ψ Θ k Θ' t' φ' F.plug t φ r

theorem FrameTyM.mk0 {Ψ Θ F r t φ}
    (h : ∃ t' φ', TypM S mode Ψ Θ [] r (.ex t' φ') ∧ FRelM mode F φ' φ ∧ Refill0M S mode Ψ Θ t' φ' F.plug t φ r) : FrameTyM S mode Ψ Θ F r t φ :=
  let ⟨t', φ', h, rel, back⟩ := h
  ⟨0, Θ, t', φ', by simpa using h, rel, .inr ⟨rfl, Ext.refl _, fun _ => rfl⟩,
    fun σ hσ X r' h' => back σ hσ X r' (by rw [List.map_up_zero, liftT_zero] at h'; exact h')⟩

/-- A frame that handles `E` in its hole: the hole may also need `E`, and obligations about
it. -/
theorem FRelM.installs {mode : TypingMode} {F : Frame} {E : Name} {φ₀ φ : Stage} {r ε : Row}
    {Θ : Facts} {q : Nat}
    (hE : F.installs E) (h₀ : φ₀.le (.rt (Row.hP Θ q E r) ε)) (h : (Stage.rt r ε).le φ) :
    FRelM mode F φ₀ φ := by
  obtain ⟨_, _, rfl, hr, he⟩ := Stage.rt_le h
  refine ⟨fun n hn => ?_, fun _ => (Stage.le_err h₀).trans he, fun hp => (by cases hp),
    fun _ _ h => (by cases h)⟩
  have h₁ := Stage.le_svc h₀ n hn
  cases n with
  | nm N =>
    rcases h₁ with rfl | h₁
    · exact .inr (mode.dom_of hE)
    · exact .inl (hr _ h₁)
  | ob N a =>
    by_cases hN : N = E
    · subst hN; exact .inr (mode.dom_of (.inl hE))
    · exact .inl (hr _ (h₁.2 hN))
  | atm a | rv a | obv v a => exact .inl (hr _ h₁)

theorem FRel.installs {F : Frame} {E : Name} {φ₀ φ : Stage} {r ε : Row} {Θ : Facts} {q : Nat}
    (hE : F.installs E) (h₀ : φ₀.le (.rt (Row.hP Θ q E r) ε)) (h : (Stage.rt r ε).le φ) :
    FRel F φ₀ φ := FRelM.installs hE h₀ h

abbrev Refill0 (S : Sig) := Refill0M S .captures
abbrev Refill (S : Sig) := RefillM S .captures
abbrev FrameTy (S : Sig) := FrameTyM S .captures

theorem Refill.id {Ψ Θ k Θ' t' φ' C t φ r} (h : Refill S Ψ Θ k Θ' t' φ' C t φ r) :
    ∀ X r', Typ S ((Ψ ++ X).map (Ty.up k)) Θ' [] r' (.ex t' φ') →
      Typ S (Ψ ++ X) Θ [] (C r') (.ex t φ) := RefillM.id h

theorem Typ.sx {Ψ Θ e J} (h : Typ S Ψ Θ [] e J) {σ : Atom → Ty} (hσ : Fits σ e) (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] e (J.substT σ) := TypM.sx h hσ X

theorem Typ.sx1 {Ψ Θ a e J} (h : Typ S Ψ Θ [.mono a] e J) {σ : Atom → Ty} (hσ : Fits σ e)
    (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [.mono (a.substT σ)] e (J.substT σ) :=
  TypM.sx1 h hσ X

theorem FrameTy.mk0 {Ψ Θ F r t φ}
    (h : ∃ t' φ', Typ S Ψ Θ [] r (.ex t' φ') ∧ FRel F φ' φ ∧ Refill0 S Ψ Θ t' φ' F.plug t φ r) :
    FrameTy S Ψ Θ F r t φ := FrameTyM.mk0 h

end Flick
