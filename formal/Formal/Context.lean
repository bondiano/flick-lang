import Formal.Frames

/-!
# Typing through evaluation contexts (Л3)
-/

namespace Flick

variable {S : Sig}

/-- How the stage of a hole relates to the stage around one frame. -/
structure FRel (F : Frame) (φ' φ : Stage) : Prop where
  svc : Row.Sub φ'.svc (Row.union φ.svc F.dom)
  err : ¬ F.isTry → Row.Sub φ'.err φ.err
  pure : φ = .pure → φ' = .pure
  rt : ¬ F.isTry → φ' ≠ .pure → φ ≠ .pure

theorem FRel.of_le {F φ' φ} (h : φ'.le φ) : FRel F φ' φ where
  svc := fun n hn => .inl (Stage.le_svc h n hn)
  err := fun _ => Stage.le_err h
  pure := fun hp => by subst hp; exact Stage.eq_pure h
  rt := fun _ hp hφ => by subst hφ; exact hp (Stage.eq_pure h)

/-- The hole after a step, in a store grown by `X`, refills the frame. -/
def Refill0 (S : Sig) (Ψ : List Ty) (Θ : Facts) (t' : Ty) (φ' : Stage) (C : Tm → Tm) (t : Ty)
    (φ : Stage) : Prop :=
  ∀ X r', Typ S (Ψ ++ X) Θ [] r' (.ex t' φ') → Typ S (Ψ ++ X) Θ [] (C r') (.ex t φ)

/-- As `Refill0`, for a hole `k` type variables deep: under (Let-Gen) the hole is
typed with the store and the facts shifted past the generalized variables, and
the facts of the scheme assumed (`Θ'`). -/
def Refill (S : Sig) (Ψ : List Ty) (Θ : Facts) (k : Nat) (Θ' : Facts) (t' : Ty) (φ' : Stage)
    (C : Tm → Tm) (t : Ty) (φ : Stage) : Prop :=
  ∀ X r', Typ S ((Ψ ++ X).map (Ty.up k)) Θ' [] r' (.ex t' φ') → Typ S (Ψ ++ X) Θ [] (C r') (.ex t φ)

/-- Services a frame provides have their types. -/
def FLook (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) : Prop :=
  ∀ N v, F.lookup N = some v → Typ S Ψ Θ [] v (.ex (S.impl N) .pure)

theorem FLook.none {Ψ Θ F} (h : ∀ N, F.lookup N = none) : FLook S Ψ Θ F :=
  fun N _ hv => by rw [h N] at hv; cases hv

/-- One frame of Л3: the hole is `k` type variables deep, and pure if `k ≠ 0`;
at depth `0` it has the facts of the frame. -/
def FrameTy (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) (r : Tm) (t : Ty) (φ : Stage) : Prop :=
  ∃ k Θ' t' φ', Typ S (Ψ.map (Ty.up k)) Θ' [] r (.ex t' φ') ∧ FRel F φ' φ ∧
    (φ' = .pure ∨ k = 0 ∧ Θ' = Θ) ∧ Refill S Ψ Θ k Θ' t' φ' F.plug t φ ∧ FLook S Ψ Θ F

theorem FrameTy.mk0 {Ψ Θ F r t φ}
    (h : ∃ t' φ', Typ S Ψ Θ [] r (.ex t' φ') ∧ FRel F φ' φ ∧ Refill0 S Ψ Θ t' φ' F.plug t φ ∧
      FLook S Ψ Θ F) : FrameTy S Ψ Θ F r t φ :=
  let ⟨t', φ', h, rel, back, look⟩ := h
  ⟨0, Θ, t', φ', by simpa using h, rel, .inr ⟨rfl, rfl⟩,
    fun X r' h' => back X r' (by simpa using h'), look⟩

theorem env_no_atm {Ψ Θ Γ θ ρ φ} (hθ : IsEnv θ) (h : Typ S Ψ Θ Γ θ (.ex (.env ρ) φ)) (a : Atom) :
    ¬ ρ (.atm a) := by
  induction hθ generalizing ρ with
  | enil => cases h with | enil hρ => exact hρ _
  | econs _ _ ih =>
    cases h with
    | econs _ _ _ hθ' hρ =>
      intro ha
      rcases hρ _ ha with h | h
      · cases h
      · exact ih hθ' h

theorem frame_typing {Ψ Θ r t φ} (hr : ¬ Value r) (F : Frame) (hF : F.ok)
    (h : Typ S Ψ Θ [] (F.plug r) (.ex t φ)) : FrameTy S Ψ Θ F r t φ := by
  have nv := not_value_plug hr F
  have look : (∀ N, F.lookup N = none) → FLook S Ψ Θ F := FLook.none
  cases F <;> simp only [Frame.plug] at h nv
  case appL =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .app h' (h₂.weakenStore X) hψ hT,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case appR =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun X _ h' => .app (h₁.weakenStore X) h' hψ hT,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case let_ =>
    cases h with
    | let_ h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .let_ h' (h₂.weakenStore X),
        look fun _ => rfl⟩
    | letGen h₁ h₂ =>
      exact ⟨_, _, _, _, h₁, ⟨fun _ h => h.elim, fun _ _ h => h.elim, fun _ => rfl,
        fun _ h => (h rfl).elim⟩, .inl rfl, fun X _ h' => .letGen h' (h₂.weakenStore X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case match_ =>
    cases h with
    | match_ h₁ h₂ hc =>
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .match_ h' (h₂.weakenStore X) (.inl hx), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsL =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .rcons hR hl h' (h₂.weakenStore X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsR =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun X _ h' => .rcons hR hl (h₁.weakenStore X) h',
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case proj =>
    cases h with
    | proj h₁ hR hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .proj h' hR hT, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updL =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .update h' hR (h₂.weakenStore X), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updR =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun X _ h' => .update (h₁.weakenStore X) hR h', look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case await =>
    cases h with
    | await h₁ hφ hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .await h' hφ hT, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case raise =>
    cases h with
    | raise h₁ hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .raise h' hφ, look fun _ => rfl⟩
    | raiseAdt h₁ hU hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .raiseAdt h' hU hφ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case bind =>
    cases h with
    | bind h₁ hρ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .bind h' hρ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinL =>
    cases h with
    | join h₁ h₂ hρ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .join h' (h₂.weakenStore X) hρ,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinR =>
    cases h with
    | join h₁ h₂ hρ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun X _ h' => .join (h₁.weakenStore X) h' hρ,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case provide =>
    cases h with
    | provideL h₁ h₂ hb hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .provideL h' (h₂.weakenStore X) hb hφ, look fun _ => rfl⟩
    | provideE h₁ h₂ hb hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .provideE h' (h₂.weakenStore X) hb hφ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case try_ =>
    cases h with
    | tryR h₁ h₂ hφ hs =>
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₁, ⟨fun n hn => .inl (hr' n hn), fun h => (h trivial).elim,
        fun hp => (by cases hp), fun h => (h trivial).elim⟩,
        fun X _ h' => .tryR h' (h₂.weakenStore X) hφ hs, look fun _ => rfl⟩
    | tryP h₁ h₂ =>
      refine .mk0 ⟨_, _, h₁, ⟨fun _ h => h.elim, fun h => (h trivial).elim, fun _ => rfl,
        fun h => (h trivial).elim⟩, fun X _ h' => .tryP h' (h₂.weakenStore X), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case with_ θ =>
    cases h with
    | with_ hθv hθ h₂ hb hφ =>
      have henv := canonical_env hθv hθ
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₂, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp), fun _ _ h => (by cases h)⟩,
        fun X _ h' => .with_ hθv (hθ.weakenStore X) h' hb hφ, ?_⟩
      · rcases Stage.le_svc hb n hn with h | h
        · cases n with
          | nm N =>
            obtain ⟨w, hw⟩ := env_lookup_some henv hθ h
            exact .inr (by simp [Frame.dom, Frame.lookup, hw])
          | atm a => exact (env_no_atm henv hθ a h).elim
        · exact .inl (hr' n h)
      · exact (Stage.le_err hb).trans he'
      · intro N v hv
        exact env_lookup_typ hθ hv
    | errVal hv => exact (nv hv).elim
  case inside p =>
    cases h with
    | inside h₁ hφ' hφ hT =>
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₁, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp),
        fun _ _ h => (by cases h)⟩, fun _ _ h' => .inside h' hφ' hφ hT, look fun _ => rfl⟩
      · rcases Stage.le_svc hφ' n hn with h | h
        · cases n with
          | nm => exact h.elim
          | atm a => exact .inr ⟨p, rfl, h⟩
        · exact .inl (hr' n h)
      · exact (Stage.le_err hφ').trans he'
    | errVal hv => exact (nv hv).elim

end Flick
