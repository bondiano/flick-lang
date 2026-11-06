import Formal.Frames
import Formal.Cut

/-!
# Typing through evaluation contexts (L3)
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
  have := h Ty.ident ⟨fun _ _ => rfl, fun _ b _ hb => ((Ty.reach_ident b _).mp hb).symm,
    fun _ b a hb => by rw [(Ty.reach_ident b a).mp hb]; cases a <;> rfl, fun _ => ident_noNames⟩ X r'
  simp only [List.map_substT_ident, liftT_ident, Ty.substT_ident, Stage.substT_ident,
    Facts.substT_ident] at this
  exact this h'

/-- A derivation of a part of a frame, after a substitution and in a grown store. -/
theorem Typ.sx {Ψ Θ e J} (h : Typ S Ψ Θ [] e J) {σ : Atom → Ty} (hσ : Fits σ e) (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X

theorem cell_sx {Ψ : List Ty} {σ : Atom → Ty} {ℓ : Nat} {T : Ty} (h : Ψ[ℓ]? = some T) (X : List Ty) :
    (Ψ.map (Ty.substT σ) ++ X)[ℓ]? = some (T.substT σ) := by
  rw [List.getElem?_append_left (by simpa using getElem?_lt h), List.getElem?_map, h]; rfl

theorem Typ.sx1 {Ψ Θ a e J} (h : Typ S Ψ Θ [.mono a] e J) {σ : Atom → Ty} (hσ : Fits σ e)
    (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [.mono (a.substT σ)] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X

/-- Services a frame provides have their types. -/
def FLook (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) : Prop :=
  ∀ N q v, F.lookup N = some (q, v) → Typ S Ψ Θ [] v (.ex (S.impl N) .pure)

theorem FLook.none {Ψ Θ F} (h : ∀ N, F.lookup N = none) : FLook S Ψ Θ F :=
  fun N _ _ hv => by rw [h N] at hv; cases hv

/-- One frame of L3: the hole is `k` type variables deep, and pure if `k ≠ 0`;
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

theorem bind_row_substT {ρ : Row} {N : Name} (σ : Atom → Ty) (hρ : ∀ n, ρ n ↔ n = .nm N) :
    ∀ n, ρ.substT σ n ↔ n = .nm N := by
  obtain rfl : ρ = Row.single N := funext fun n => propext (hρ n)
  rw [Row.single_substT]; exact fun _ => Iff.rfl

theorem join_row_substT {ρ ρ₁ ρ₂ : Row} (σ : Atom → Ty) (hρ : ∀ n, ρ n ↔ ρ₁ n ∨ ρ₂ n) :
    ∀ n, ρ.substT σ n ↔ ρ₁.substT σ n ∨ ρ₂.substT σ n := by
  obtain rfl : ρ = Row.union ρ₁ ρ₂ := funext fun n => propext (hρ n)
  rw [Row.union_substT]; exact fun _ => Iff.rfl

/-- A frame that handles `E` in its hole: the hole may also need `E`. -/
theorem FRel.installs {F : Frame} {E : Name} {φ₀ φ : Stage} {r ε : Row} (hE : F.installs E)
    (h₀ : φ₀.le (.rt (Row.union (Row.single E) r) ε)) (h : (Stage.rt r ε).le φ) : FRel F φ₀ φ := by
  obtain ⟨_, _, rfl, hr, he⟩ := Stage.rt_le h
  refine ⟨fun n hn => ?_, fun _ => (Stage.le_err h₀).trans he, fun hp => (by cases hp),
    fun _ _ h => (by cases h)⟩
  rcases Stage.le_svc h₀ n hn with h | h
  · subst h; exact .inr (.inr hE)
  · exact .inl (hr n h)

theorem frame_typing {Ψ Θ r t φ} (hr : ¬ Value r) (F : Frame) (hF : F.ok)
    (h : Typ S Ψ Θ [] (F.plug r) (.ex t φ)) : FrameTy S Ψ Θ F r t φ := by
  have nv := not_value_plug hr F
  have look : (∀ N, F.lookup N = none) → FLook S Ψ Θ F := FLook.none
  cases F <;> simp only [Frame.plug] at h nv
  case hwith p ℓ E al rf =>
    cases h with
    | hwith hℓ hal hrf hψ he hφ₀ hc hφ hT =>
      refine .mk0 ⟨_, _, he, .installs rfl hφ₀ hφ, fun σ hσ X _ h' => ?_, look fun _ => rfl⟩
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inl (.inr (.inl rfl)))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      exact .hwith (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (Stage.le_substT σ hψ) h'
        (by simpa [Stage.substT, Row.union_substT] using Stage.le_substT σ hφ₀)
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
    | errVal hv => exact (nv hv).elim
  case hctlA p ℓ κ tl E al rf k =>
    cases h with
    | hctl hℓ hal hrf hψ hop hc htl hE hk hp' hφ₀ ha hl hcm hφ hT =>
      refine .mk0 ⟨_, _, ha, .of_le hφ, fun σ hσ X _ h' => ?_, look fun _ => rfl⟩
      subst hk
      obtain ⟨-, hb⟩ := S.op_closed _ _ _ _ _ σ hop
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inl (.inr (.inl rfl)))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      have h₃ := hp'.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inr (.inr
        (plug_has_of_nosite (by simp [Tm.Has]) h)))))) X
      refine .hctl (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (Stage.le_substT σ hψ)
        hop hc htl hE rfl h₃ (by simpa [Stage.substT, Row.union_substT] using Stage.le_substT σ hφ₀) ?_
        (fun h => ?_) ⟨hcm.1, fun h => .inl (hcm.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
      · cases tl <;> simpa [hb] using h'
      · obtain ⟨C', hl₁, hl₂⟩ := hl h
        refine ⟨C'.substT σ, ?_, ⟨_, hl₂, by simp [hp]⟩⟩
        rw [List.getElem?_append_left (by simpa using getElem?_lt hl₁), List.getElem?_map, hl₁]
        simp [Ty.substT, Stage.substT, hb]
    | errVal hv => exact (nv hv).elim
  case hresB p ℓ a =>
    cases h with
    | hres hℓ hb hA ha hp' hl hφ hT =>
      refine .mk0 ⟨_, _, hb, .of_le hφ, fun σ hσ X _ h' => ?_, look fun _ => rfl⟩
      subst ha
      have h₂ := hp'.sx (hσ.sub fun s h => by
        rcases plug_has_iff.mp h with h | h
        · exact .inr (.inr (.inl (plug_has_iff.mpr (.inl h))))
        · simp only [Tm.Has] at h; rcases h with h | h
          · subst h; exact .inr (.inl rfl)
          · exact h.elim) X
      simp only [Flick.J.substT, Stage.substT] at h₂
      exact .hres (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h' hA rfl h₂
        (by rw [List.getElem?_append_left (by simpa using getElem?_lt hl), List.getElem?_map, hl]; rfl)
        (Stage.le_substT σ hφ) (hT.substT σ)
    | errVal hv => exact (nv hv).elim
  case hdone p =>
    cases h with
    | hdone ha =>
      exact .mk0 ⟨_, _, ha, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .hdone h', look fun _ => rfl⟩
    | errVal hv => exact (nv hv).elim
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
    | dproj h₁ hc hk hl hts hH hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ _ _ h' => .dproj_at h' hc hk hl hts hH hT,
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
    | provideL h₁ h₂ hb h₀ hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .provideL h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (by simpa [Stage.substT] using Stage.le_substT σ hb) (h₀.substT σ)
          (by simpa [Stage.substT] using Stage.le_substT σ hφ), look fun _ => rfl⟩
    | provideE h₁ h₂ hb h₀ hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .provideE h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (by simpa [Stage.substT] using Stage.le_substT σ hb) (h₀.substT σ)
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
  case with_ q θ =>
    cases h with
    | @with_ _ _ _ _ ρ _ _ _ _ _ _ _ hθv hθ h₂ hb hφ =>
      have henv := canonical_env hθv hθ
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₂, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp), fun _ _ h => (by cases h)⟩,
        fun σ hσ X _ h' => .with_ hθv (hθ.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X) h'
          (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hb)
            (Row.withP_substT (hσ.1 _ (.inr (.inr (.inl rfl)))) (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
          (by simpa [Stage.substT] using Stage.le_substT σ hφ), ?_⟩
      · have dom : ∀ N, ρ (.nm N) → (Frame.with_ q θ).lookup N ≠ none := fun N h => by
          obtain ⟨w, hw⟩ := env_lookup_some henv hθ h
          simp [Frame.lookup, hw]
        have h := Stage.le_svc hb n hn
        cases n with
        | nm N =>
          by_cases hN : ρ (.nm N)
          · exact .inr (.inl (by simpa [Option.isSome_iff_ne_none] using dom N hN))
          · exact .inl (hr' _ (h.2 hN))
        | ob N a =>
          by_cases hN : ρ (.nm N)
          · exact .inr (.inl (by simpa [Option.isSome_iff_ne_none] using dom N hN))
          · exact .inl (hr' _ (h.2 hN))
        | atm a | need _ a | rv a | obv _ a => exact .inl (hr' _ h)
      · exact (Stage.le_err hb).trans he'
      · intro N q' v hv
        simp only [Frame.lookup, Option.map_eq_some_iff] at hv
        obtain ⟨w, hw, h'⟩ := hv
        cases h'
        exact env_lookup_typ hθ hw
    | errVal hv => exact (nv hv).elim
  case inside p =>
    cases h with
    | inside hF h₁ hφ' ht hε hφ hT =>
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine ⟨0, _, _, _, by simpa using h₁, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp),
        fun _ _ h => (by cases h)⟩, .inr ⟨rfl, Ext.inside hF, fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => ?_,
        look fun _ => rfl⟩
      · rcases Stage.le_svc hφ' n hn with h | h | h | h
        · cases n with
          | atm a => exact .inr ⟨p, rfl, h⟩
          | _ => exact h.elim
        · exact .inl (hr' n h)
        · cases n with
          | ob N a => exact .inr (.inr ⟨p, rfl, h.1⟩)
          | obv v a => exact .inr ⟨p, rfl, h.1⟩
          | _ => exact h.elim
        · cases n with
          | need N a => exact .inr ⟨p, rfl, h⟩
          | _ => exact h.elim
      · exact (Stage.le_err hφ').trans he'
      · have hp : σ (.r p) = .rg p := hσ.1 _ (.inl (.inl rfl))
        have hav := hσ.2.1 (.inr (.inl rfl))
        rw [List.map_up_zero, liftT_zero, Facts.union_substT] at h'
        refine .inside ?_ h' ?_ (fun h => ht ?_) (Ty.no_rg_substT (t := .errors _) hav hε)
          (Stage.le_substT σ hφ) (hT.substT σ)
        · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
          obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
          exact ⟨by simp [Ty.substT, hp], fun q ⟨b, hb, h''⟩ => by rw [hav b q h''] at hb; exact hC q hb⟩
        · have := Stage.le_substT σ hφ'
          simp only [Stage.substT, Row.union_substT, Row.scope_substT, Ty.substT, hp] at this
          refine Stage.le_rt_sub this ?_
          rintro x (hx | hx | hx | hx)
          · exact .inl hx
          · exact .inr (.inl hx)
          · exact .inr (.inr (.inl (Row.disch_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
          · exact .inr (.inr (.inr (Row.needs_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
        · obtain ⟨b, hb, h''⟩ := (Ty.reach_substT σ _ _).mp h
          rwa [hav b _ h''] at hb
    | errVal hv => exact (nv hv).elim

end Flick
