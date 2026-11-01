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
def Refill (S : Sig) (Ψ : List Ty) (t' : Ty) (φ' : Stage) (C : Tm → Tm) (t : Ty) (φ : Stage) :
    Prop :=
  ∀ X r', Typ S (Ψ ++ X) [] r' (.ex t' φ') → Typ S (Ψ ++ X) [] (C r') (.ex t φ)

/-- Services a frame provides have their types. -/
def FLook (S : Sig) (Ψ : List Ty) (F : Frame) : Prop :=
  ∀ N v, F.lookup N = some v → Typ S Ψ [] v (.ex (S.impl N) .pure)

theorem FLook.none {Ψ F} (h : ∀ N, F.lookup N = none) : FLook S Ψ F :=
  fun N _ hv => by rw [h N] at hv; cases hv

/-- One frame of Л3. -/
def FrameTy (S : Sig) (Ψ : List Ty) (F : Frame) (r : Tm) (t : Ty) (φ : Stage) : Prop :=
  ∃ t' φ', Typ S Ψ [] r (.ex t' φ') ∧ FRel F φ' φ ∧ Refill S Ψ t' φ' F.plug t φ ∧ FLook S Ψ F

theorem frame_typing {Ψ r t φ} (hr : ¬ Value r) (F : Frame) (hF : F.ok)
    (h : Typ S Ψ [] (F.plug r) (.ex t φ)) : FrameTy S Ψ F r t φ := by
  have nv := not_value_plug hr F
  have look : (∀ N, F.lookup N = none) → FLook S Ψ F := FLook.none
  cases F <;> simp only [Frame.plug] at h nv
  case appL =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .app h' (h₂.weakenStore X) hψ hT,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case appR =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact ⟨_, _, h₂, .of_le (Stage.le_refl _), fun X _ h' => .app (h₁.weakenStore X) h' hψ hT,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case let_ =>
    cases h with
    | let_ h₁ h₂ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .let_ h' (h₂.weakenStore X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case match_ =>
    cases h with
    | match_ h₁ h₂ hc =>
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .match_ h' (h₂.weakenStore X) (.inl hx), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsL =>
    cases h with
    | rcons hl h₁ h₂ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .rcons hl h' (h₂.weakenStore X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsR =>
    cases h with
    | rcons hl h₁ h₂ =>
      exact ⟨_, _, h₂, .of_le (Stage.le_refl _), fun X _ h' => .rcons hl (h₁.weakenStore X) h',
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case proj =>
    cases h with
    | proj h₁ hR hT =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .proj h' hR hT, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updL =>
    cases h with
    | update h₁ hR h₂ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .update h' hR (h₂.weakenStore X), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updR =>
    cases h with
    | update h₁ hR h₂ =>
      exact ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun X _ h' => .update (h₁.weakenStore X) hR h', look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case await =>
    cases h with
    | await h₁ hφ hT =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .await h' hφ hT, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case raise =>
    cases h with
    | raise h₁ hφ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .raise h' hφ, look fun _ => rfl⟩
    | raiseAdt h₁ hφ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .raiseAdt h' hφ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case bind =>
    cases h with
    | bind h₁ hρ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ h' => .bind h' hρ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinL =>
    cases h with
    | join h₁ h₂ hρ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _), fun X _ h' => .join h' (h₂.weakenStore X) hρ,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinR =>
    cases h with
    | join h₁ h₂ hρ =>
      exact ⟨_, _, h₂, .of_le (Stage.le_refl _), fun X _ h' => .join (h₁.weakenStore X) h' hρ,
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case provide =>
    cases h with
    | provideL h₁ h₂ hb hφ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .provideL h' (h₂.weakenStore X) hb hφ, look fun _ => rfl⟩
    | provideE h₁ h₂ hb hφ =>
      exact ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun X _ h' => .provideE h' (h₂.weakenStore X) hb hφ, look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case try_ =>
    cases h with
    | tryR h₁ h₂ hφ hs =>
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine ⟨_, _, h₁, ⟨fun n hn => .inl (hr' n hn), fun h => (h trivial).elim,
        fun hp => (by cases hp), fun h => (h trivial).elim⟩,
        fun X _ h' => .tryR h' (h₂.weakenStore X) hφ hs, look fun _ => rfl⟩
    | tryP h₁ h₂ =>
      refine ⟨_, _, h₁, ⟨fun _ h => h.elim, fun h => (h trivial).elim, fun _ => rfl,
        fun h => (h trivial).elim⟩, fun X _ h' => .tryP h' (h₂.weakenStore X), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case with_ θ =>
    cases h with
    | with_ hθv hθ h₂ hb hφ =>
      have henv := canonical_env hθv hθ
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine ⟨_, _, h₂, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp), fun _ _ h => (by cases h)⟩,
        fun X _ h' => .with_ hθv (hθ.weakenStore X) h' hb hφ, ?_⟩
      · rcases Stage.le_svc hb n hn with h | h
        · obtain ⟨w, hw⟩ := env_lookup_some henv hθ h
          exact .inr (by simp [Frame.dom, Frame.lookup, hw])
        · exact .inl (hr' n h)
      · exact (Stage.le_err hb).trans he'
      · intro N v hv
        exact env_lookup_typ hθ hv
    | errVal hv => exact (nv hv).elim

end Flick
