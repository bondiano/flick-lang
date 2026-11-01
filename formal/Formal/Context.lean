import Formal.Frames
import Formal.Cut

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

/-- The hole after a step, in a store grown by `X`, refills the frame; so it does
after a substitution `σ` that fits the term, which renames the regions not yet
created out of the whole derivation (T4). -/
def Refill0 (S : Sig) (Ψ : List Ty) (Θ : Facts) (t' : Ty) (φ' : Stage) (C : Tm → Tm) (t : Ty)
    (φ : Stage) (r : Tm) : Prop :=
  ∀ σ, Fits σ (C r) → ∀ X r', Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] r'
      (.ex (t'.substT σ) (φ'.substT σ)) →
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] (C r') (.ex (t.substT σ) (φ.substT σ))

/-- As `Refill0`, for a hole `k` type variables deep: under (Let-Gen) the hole is
typed with the store and the facts shifted past the generalized variables, and
the facts of the scheme assumed (`Θ'`). -/
def Refill (S : Sig) (Ψ : List Ty) (Θ : Facts) (k : Nat) (Θ' : Facts) (t' : Ty) (φ' : Stage)
    (C : Tm → Tm) (t : Ty) (φ : Stage) (r : Tm) : Prop :=
  ∀ σ, Fits σ (C r) → ∀ X r', Typ S ((Ψ.map (Ty.substT σ) ++ X).map (Ty.up k))
      (Θ'.substT (liftT k σ)) [] r' (.ex (t'.substT (liftT k σ)) (φ'.substT (liftT k σ))) →
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] (C r') (.ex (t.substT σ) (φ.substT σ))

theorem List.map_substT_ident (Ψ : List Ty) : Ψ.map (Ty.substT Ty.ident) = Ψ := by
  induction Ψ with
  | nil => rfl
  | cons t Ψ ih => simp [Ty.substT_ident, ih]

/-- `Refill` without a substitution. -/
theorem Refill.id {Ψ Θ k Θ' t' φ' C t φ r} (h : Refill S Ψ Θ k Θ' t' φ' C t φ r) :
    ∀ X r', Typ S ((Ψ ++ X).map (Ty.up k)) Θ' [] r' (.ex t' φ') →
      Typ S (Ψ ++ X) Θ [] (C r') (.ex t φ) := by
  intro X r' h'
  have := h Ty.ident ⟨fun _ _ => rfl, fun _ b _ hb => ((Ty.reach_ident b _).mp hb).symm⟩ X r'
  simp only [List.map_substT_ident, liftT_ident, Ty.substT_ident, Stage.substT_ident,
    Facts.substT_ident] at this
  exact this h'

/-- A derivation of a part of a frame, after a substitution and in a grown store. -/
theorem Typ.sx {Ψ Θ e J} (h : Typ S Ψ Θ [] e J) {σ : Atom → Ty} (hσ : Fits σ e) (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X

theorem Typ.sx1 {Ψ Θ a e J} (h : Typ S Ψ Θ [.mono a] e J) {σ : Atom → Ty} (hσ : Fits σ e)
    (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [.mono (a.substT σ)] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X

/-- Services a frame provides have their types. -/
def FLook (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) : Prop :=
  ∀ N v, F.lookup N = some v → Typ S Ψ Θ [] v (.ex (S.impl N) .pure)

theorem FLook.none {Ψ Θ F} (h : ∀ N, F.lookup N = none) : FLook S Ψ Θ F :=
  fun N _ hv => by rw [h N] at hv; cases hv

/-- One frame of Л3: the hole is `k` type variables deep, and pure if `k ≠ 0`;
at depth `0` it has the facts of the frame. -/
def FrameTy (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) (r : Tm) (t : Ty) (φ : Stage) : Prop :=
  ∃ k Θ' t' φ', Typ S (Ψ.map (Ty.up k)) Θ' [] r (.ex t' φ') ∧ FRel F φ' φ ∧
    (φ' = .pure ∨ k = 0 ∧ Ext Θ Θ' ∧ (¬ F.isInside → Θ' = Θ)) ∧ Refill S Ψ Θ k Θ' t' φ' F.plug t φ r ∧
      FLook S Ψ Θ F

theorem FrameTy.mk0 {Ψ Θ F r t φ}
    (h : ∃ t' φ', Typ S Ψ Θ [] r (.ex t' φ') ∧ FRel F φ' φ ∧ Refill0 S Ψ Θ t' φ' F.plug t φ r ∧
      FLook S Ψ Θ F) : FrameTy S Ψ Θ F r t φ :=
  let ⟨t', φ', h, rel, back, look⟩ := h
  ⟨0, Θ, t', φ', by simpa using h, rel, .inr ⟨rfl, Ext.refl _, fun _ => rfl⟩,
    fun σ hσ X r' h' => back σ hσ X r' (by rw [List.map_up_zero, liftT_zero] at h'; exact h'), look⟩

theorem bind_row_substT {ρ : Row} {N : Name} (σ : Atom → Ty) (hρ : ∀ n, ρ n → n = .nm N) :
    ∀ n, ρ.substT σ n → n = .nm N := by
  intro n hn
  cases n with
  | nm N' => exact hρ _ hn
  | atm a => obtain ⟨b, hb, _⟩ := hn; cases hρ _ hb

theorem join_row_substT {ρ ρ₁ ρ₂ : Row} (σ : Atom → Ty) (hρ : ∀ n, ρ n → ρ₁ n ∨ ρ₂ n) :
    ∀ n, ρ.substT σ n → ρ₁.substT σ n ∨ ρ₂.substT σ n := by
  intro n hn
  cases n with
  | nm N' => exact hρ _ hn
  | atm a =>
    obtain ⟨b, hb, h'⟩ := hn
    rcases hρ _ hb with h | h
    · exact .inl ⟨b, h, h'⟩
    · exact .inr ⟨b, h, h'⟩

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
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (Stage.le_substT σ hψ)
        (hT.substT σ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case appR =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app (h₁.sx (hσ.sub fun _ h => .inl h) X) h' (Stage.le_substT σ hψ)
        (hT.substT σ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case let_ =>
    cases h with
    | let_ h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .let_ h' (h₂.sx1 (hσ.sub fun _ h => .inr h) X),
        look fun _ => rfl⟩
    | letGen h₁ h₂ =>
      exact ⟨_, _, _, _, h₁, ⟨fun _ h => h.elim, fun _ _ h => h.elim, fun _ => rfl,
        fun _ h => (h rfl).elim⟩, .inl rfl, fun σ hσ X _ h' => .letGen (by
          simpa [Facts.union_substT, Facts.up_comm] using h')
          (by simpa [Scheme.substT] using (h₂.substT σ (hσ.sub fun _ h => .inr h)).weakenStore X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case match_ =>
    cases h with
    | match_ h₁ h₂ hc =>
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .match_ h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (.inl fun v hs => hx v (Shp.of_substT hs)),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsL =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl) h'
          (h₂.sx (hσ.sub fun _ h => .inr h) X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case rconsR =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl)
          (h₁.sx (hσ.sub fun _ h => .inl h) X) h',
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case proj =>
    cases h with
    | proj h₁ hR hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .proj h' (Ty.field_substT σ hR) (hT.substT σ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updL =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update h' (Ty.field_substT σ hR) (h₂.sx (hσ.sub fun _ h => .inr h) X),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case updR =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update (h₁.sx (hσ.sub fun _ h => .inl h) X) (Ty.field_substT σ hR) h',
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case await =>
    cases h with
    | await h₁ hφ hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .await h' (Stage.le_substT σ hφ) (hT.substT σ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case raise =>
    cases h with
    | raise h₁ hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raise h'
        (by simpa [Stage.substT] using Stage.le_substT σ hφ), look fun _ => rfl⟩
    | raiseAdt h₁ hU hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raiseAdt h'
        (Ty.hd_substT σ hU) (by simpa [Stage.substT, Row.err_substT] using Stage.le_substT σ hφ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case bind =>
    cases h with
    | bind h₁ hρ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .bind (by simpa [S.impl_closed] using h')
        (bind_row_substT σ hρ), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinL =>
    cases h with
    | join h₁ h₂ hρ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .join h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (join_row_substT σ hρ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case joinR =>
    cases h with
    | join h₁ h₂ hρ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .join (h₁.sx (hσ.sub fun _ h => .inl h) X) h' (join_row_substT σ hρ),
        look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case provide =>
    cases h with
    | provideL h₁ h₂ hb hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .provideL h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (by simpa [Stage.substT] using Stage.le_substT σ hb)
          (by simpa [Stage.substT] using Stage.le_substT σ hφ), look fun _ => rfl⟩
    | provideE h₁ h₂ hb hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .provideE h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (by simpa [Stage.substT] using Stage.le_substT σ hb)
          (by simpa [Stage.substT] using Stage.le_substT σ hφ), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case try_ =>
    cases h with
    | tryR h₁ h₂ hφ hs =>
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₁, ⟨fun n hn => .inl (hr' n hn), fun h => (h trivial).elim,
        fun hp => (by cases hp), fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => .tryR h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (Stage.le_substT σ hφ)
          (by simpa using hs.substT σ), look fun _ => rfl⟩
    | tryP h₁ h₂ =>
      refine .mk0 ⟨_, _, h₁, ⟨fun _ h => h.elim, fun h => (h trivial).elim, fun _ => rfl,
        fun h => (h trivial).elim⟩, fun σ hσ X _ h' => .tryP h' (by
          have := h₂.sx (hσ.sub fun _ h => .inr h) X
          simp only [Flick.J.substT, Row.none_substT] at this; exact this), look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
  case with_ θ =>
    cases h with
    | with_ hθv hθ h₂ hb hφ =>
      have henv := canonical_env hθv hθ
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₂, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp), fun _ _ h => (by cases h)⟩,
        fun σ hσ X _ h' => .with_ hθv (hθ.sx (hσ.sub fun _ h => .inl h) X) h'
          (by simpa [Stage.substT] using Stage.le_substT σ hb)
          (by simpa [Stage.substT] using Stage.le_substT σ hφ), ?_⟩
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
    | inside hF h₁ hφ' ht hε hφ hT =>
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine ⟨0, _, _, _, by simpa using h₁, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp),
        fun _ _ h => (by cases h)⟩, .inr ⟨rfl, Ext.inside hF, fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => ?_,
        look fun _ => rfl⟩
      · rcases Stage.le_svc hφ' n hn with h | h
        · cases n with
          | nm => exact h.elim
          | atm a => exact .inr ⟨p, rfl, h⟩
        · exact .inl (hr' n h)
      · exact (Stage.le_err hφ').trans he'
      · have hp : σ (.r p) = .rg p := hσ.1 _ (.inl rfl)
        have hav := hσ.2 (.inr (.inl rfl))
        rw [List.map_up_zero, liftT_zero, Facts.union_substT] at h'
        refine .inside ?_ h' ?_ (fun h => ht ?_) (fun ⟨b, hb, h''⟩ => hε (by rwa [hav b _ h''] at hb))
          (Stage.le_substT σ hφ) (hT.substT σ)
        · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
          obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
          exact ⟨by simp [Ty.substT, hp], fun q ⟨b, hb, h''⟩ => by rw [hav b q h''] at hb; exact hC q hb⟩
        · simpa [Stage.substT, Row.union_substT, Row.scope_substT, Ty.substT, hp] using
            Stage.le_substT σ hφ'
        · obtain ⟨b, hb, h''⟩ := (Ty.reach_substT σ _ _).mp h
          rwa [hav b _ h''] at hb
    | errVal hv => exact (nv hv).elim

end Flick
