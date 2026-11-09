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

namespace TypM
export Typ (sx)
end TypM

theorem cell_sx {Ψ : List Ty} {σ : Atom → Ty} {ℓ : Nat} {T : Ty} (h : Ψ[ℓ]? = some T) (X : List Ty) :
    (Ψ.map (Ty.substT σ) ++ X)[ℓ]? = some (T.substT σ) := by
  rw [List.getElem?_append_left (by simpa using getElem?_lt h), List.getElem?_map, h]; rfl

theorem Typ.sx1 {Ψ Θ a e J} (h : Typ S Ψ Θ [.mono a] e J) {σ : Atom → Ty} (hσ : Fits σ e)
    (X : List Ty) :
    Typ S (Ψ.map (Ty.substT σ) ++ X) (Θ.substT σ) [.mono (a.substT σ)] e (J.substT σ) := by
  simpa using (h.substT σ hσ).weakenStore X

namespace TypM
export Typ (sx1)
end TypM

/-- One frame of L3: the hole is `k` type variables deep, and pure if `k ≠ 0`;
at depth `0` it has the facts of the frame. -/
def FrameTy (S : Sig) (Ψ : List Ty) (Θ : Facts) (F : Frame) (r : Tm) (t : Ty) (φ : Stage) : Prop :=
  ∃ k Θ' t' φ', Typ S (Ψ.map (Ty.up k)) Θ' [] r (.ex t' φ') ∧ FRel F φ' φ ∧
    (φ' = .pure ∨ k = 0 ∧ Ext Θ Θ' ∧ (¬ F.isInside → Θ' = Θ)) ∧ Refill S Ψ Θ k Θ' t' φ' F.plug t φ r

theorem FrameTy.mk0 {Ψ Θ F r t φ}
    (h : ∃ t' φ', Typ S Ψ Θ [] r (.ex t' φ') ∧ FRel F φ' φ ∧ Refill0 S Ψ Θ t' φ' F.plug t φ r) : FrameTy S Ψ Θ F r t φ :=
  let ⟨t', φ', h, rel, back⟩ := h
  ⟨0, Θ, t', φ', by simpa using h, rel, .inr ⟨rfl, Ext.refl _, fun _ => rfl⟩,
    fun σ hσ X r' h' => back σ hσ X r' (by rw [List.map_up_zero, liftT_zero] at h'; exact h')⟩

/-- A frame that handles `E` in its hole: the hole may also need `E`, and obligations about
it. -/
theorem FRel.installs {F : Frame} {E : Name} {φ₀ φ : Stage} {r ε : Row} {Θ : Facts} {q : Nat}
    (hE : F.installs E) (h₀ : φ₀.le (.rt (Row.hP Θ q E r) ε)) (h : (Stage.rt r ε).le φ) :
    FRel F φ₀ φ := by
  obtain ⟨_, _, rfl, hr, he⟩ := Stage.rt_le h
  refine ⟨fun n hn => ?_, fun _ => (Stage.le_err h₀).trans he, fun hp => (by cases hp),
    fun _ _ h => (by cases h)⟩
  have h₁ := Stage.le_svc h₀ n hn
  cases n with
  | nm N =>
    rcases h₁ with rfl | h₁
    · exact .inr hE
    · exact .inl (hr _ h₁)
  | ob N a =>
    by_cases hN : N = E
    · subst hN; exact .inr (.inl hE)
    · exact .inl (hr _ (h₁.2 hN))
  | atm a | rv a | obv v a => exact .inl (hr _ h₁)

theorem frame_typing {Ψ Θ r t φ} (hr : ¬ Value r) (F : Frame) (hF : F.ok)
    (h : Typ S Ψ Θ [] (F.plug r) (.ex t φ)) : FrameTy S Ψ Θ F r t φ := by
  have nv := not_value_plug hr F
  cases F <;> simp only [Frame.plug] at h nv
  case hwith p ℓ E al rf =>
    cases h with
    | hwith hℓ hal hrf hψ he hφ₀ hc hφ hT =>
      refine .mk0 ⟨_, _, he, .installs rfl hφ₀ hφ, fun σ hσ X _ h' => ?_⟩
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      exact .hwith (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (Stage.le_substT σ hψ) h'
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
    | errVal hv | allI hv => exact (nv hv).elim
  case hctlA p ℓ κ tl E al rf k =>
    cases h with
    | hctl hℓ hal hrf hψ hop hc htl hE hk hp' hφ₀ ha hl hcm hφ hT =>
      refine .mk0 ⟨_, _, ha, .of_le hφ, fun σ hσ X _ h' => ?_⟩
      subst hk
      obtain ⟨-, hb⟩ := S.op_closed _ _ _ _ _ σ hop
      have hp : σ (.r p) = .rg p := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      have h₃ := hp'.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inr (.inr
        (plug_has_of_nosite (by simp [Tm.Has]) h)))))) X
      refine .hctl (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ (hrf.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inl h)))) X) (Stage.le_substT σ hψ)
        hop hc htl hE rfl h₃ (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl)))) ?_
        (fun h => ?_) ⟨hcm.1, fun h => .inl (hcm.2 h)⟩ (Stage.le_substT σ hφ) (hT.substT σ)
      · cases tl <;> simpa [hb] using h'
      · obtain ⟨C', hl₁, hl₂⟩ := hl h
        refine ⟨C'.substT σ, ?_, ⟨_, hl₂, by simp [hp]⟩⟩
        rw [List.getElem?_append_left (by simpa using getElem?_lt hl₁), List.getElem?_map, hl₁]
        simp [Ty.substT, Stage.substT, hb]
    | errVal hv | allI hv => exact (nv hv).elim
  case hkeep q ℓ E al =>
    cases h with
    | hkeep hℓ hal he hφ₀ hc hnt hle hφ hT =>
      refine .mk0 ⟨_, _, he, .installs rfl hφ₀ hφ, fun σ hσ X _ h' => ?_⟩
      have hp : σ (.r q) = .rg q := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      exact .hkeep (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ h'
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (fun h => .inl (hnt h))
        (by simpa [Stage.substT] using Stage.le_substT σ hle) (Stage.le_substT σ hφ) (hT.substT σ)
    | errVal hv | allI hv => exact (nv hv).elim
  case hkarmA q ℓ E al k =>
    cases h with
    | hkarm hℓ hal hop hE hk hp' hφ₀ hc hnt ha hle hφ hT =>
      refine .mk0 ⟨_, _, ha, .of_le (Stage.le_trans hle hφ), fun σ hσ X _ h' => ?_⟩
      subst hk
      obtain ⟨-, hb⟩ := S.op_closed _ _ _ _ _ σ hop
      have hp : σ (.r q) = .rg q := hσ.1 _ (.inr (.inl rfl))
      have h₁ := hal.sx (hσ.sub fun _ h => .inr (.inr (.inl h))) X
      simp only [Flick.J.substT, Ty.substT, hp] at h₁
      have h₃ := hp'.sx (hσ.sub fun _ h => .inr (.inr (.inr (.inr
        (plug_has_of_nosite (by simp [Tm.Has]) h))))) X
      exact .hkarm (by simpa [Ty.substT, Stage.substT] using cell_sx (σ := σ) hℓ X) h₁ hop hE rfl h₃
        (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ hφ₀)
          (Row.hP_substT hp (hσ.2.2.1 (.inl rfl)) (hσ.2.2.2 (.inl rfl))))
        ⟨hc.1, fun h => .inl (hc.2 h)⟩ (fun h => .inl (hnt h)) (by simpa [hb] using h')
        (by simpa [Stage.substT] using Stage.le_substT σ hle) (Stage.le_substT σ hφ) (hT.substT σ)
    | errVal hv | allI hv => exact (nv hv).elim
  case hresB p ℓ a =>
    cases h with
    | hres hℓ hb hA ha hp' hl hφ hT =>
      refine .mk0 ⟨_, _, hb, .of_le hφ, fun σ hσ X _ h' => ?_⟩
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
    | errVal hv | allI hv => exact (nv hv).elim
  case hdone p =>
    cases h with
    | hdone ha =>
      exact .mk0 ⟨_, _, ha, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .hdone h'⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case appL =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (Stage.le_substT σ hψ)
        (hT.substT σ)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case appR =>
    cases h with
    | app h₁ h₂ hψ hT =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .app (h₁.sx (hσ.sub fun _ h => .inl h) X) h' (Stage.le_substT σ hψ)
        (hT.substT σ)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case let_ =>
    cases h with
    | let_ h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ hσ X _ h' => .let_ h' (h₂.sx1 (hσ.sub fun _ h => .inr h) X)⟩
    | letGen h₁ h₂ =>
      exact ⟨_, _, _, _, h₁, ⟨fun _ h => h.elim, fun _ _ h => h.elim, fun _ => rfl,
        fun _ h => (h rfl).elim⟩, .inl rfl, fun σ hσ X _ h' => .letGen (by
          simpa [Facts.union_substT, Facts.up_comm] using h')
          (by simpa [Scheme.substT] using (h₂.substT σ (hσ.sub fun _ h => .inr h)).weakenStore X)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case match_ =>
    cases h with
    | match_ h₁ h₂ hc =>
      have hx : Exh S _ _ := hc.resolve_right fun ⟨hv, _⟩ => hr hv
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .match_ h' (h₂.sx (hσ.sub fun _ h => .inr h) X)
          (.inl fun v hs => hx v (Shp.of_substT hs))⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case rconsL =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl) h'
          (h₂.sx (hσ.sub fun _ h => .inr h) X)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case rconsR =>
    cases h with
    | rcons hR hl h₁ h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl)
          (h₁.sx (hσ.sub fun _ h => .inl h) X) h'⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case proj =>
    cases h with
    | proj h₁ hR hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .proj h' (Ty.field_substT σ hR) (hT.substT σ)⟩
    | dproj h₁ hc hk hl hts hH hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun _ _ _ _ h' => .dproj_at h' hc hk hl hts hH hT⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case updL =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update h' (Ty.field_substT σ hR) (h₂.sx (hσ.sub fun _ h => .inr h) X)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case updR =>
    cases h with
    | update h₁ hR h₂ =>
      exact .mk0 ⟨_, _, h₂, .of_le (Stage.le_refl _),
        fun σ hσ X _ h' => .update (h₁.sx (hσ.sub fun _ h => .inl h) X) (Ty.field_substT σ hR) h'⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case await =>
    cases h with
    | await h₁ hφ hT =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .await h' (Stage.le_substT σ hφ) (hT.substT σ)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case raise =>
    cases h with
    | raise h₁ hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raise h'
        (by simpa [Stage.substT] using Stage.le_substT σ hφ)⟩
    | raiseAdt h₁ hU hφ =>
      exact .mk0 ⟨_, _, h₁, .of_le (Stage.le_refl _), fun σ _ _ _ h' => .raiseAdt h'
        (Ty.hd_substT σ hU) (by simpa [Stage.substT, Row.err_substT] using Stage.le_substT σ hφ)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case try_ =>
    cases h with
    | tryR h₁ h₂ hφ hs =>
      obtain ⟨_, _, rfl, hr', _⟩ := Stage.rt_le hφ
      refine .mk0 ⟨_, _, h₁, ⟨fun n hn => .inl (hr' n hn), fun h => (h trivial).elim,
        fun hp => (by cases hp), fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => .tryR h' (h₂.sx (hσ.sub fun _ h => .inr h) X) (Stage.le_substT σ hφ)
          (by simpa using hs.substT σ)⟩
    | tryP h₁ h₂ =>
      refine .mk0 ⟨_, _, h₁, ⟨fun _ h => h.elim, fun h => (h trivial).elim, fun _ => rfl,
        fun h => (h trivial).elim⟩, fun σ hσ X _ h' => .tryP h' (by
          have := h₂.sx (hσ.sub fun _ h => .inr h) X
          simp only [Flick.J.substT, Row.none_substT] at this; exact this)⟩
    | errVal hv | allI hv => exact (nv hv).elim
  case inside p =>
    cases h with
    | inside hF h₁ hφ' ht hε hφ hT =>
      obtain ⟨_, _, rfl, hr', he'⟩ := Stage.rt_le hφ
      refine ⟨0, _, _, _, by simpa using h₁, ⟨fun n hn => ?_, fun _ => ?_, fun hp => (by cases hp),
        fun _ _ h => (by cases h)⟩, .inr ⟨rfl, Ext.inside hF, fun h => (h trivial).elim⟩,
        fun σ hσ X _ h' => ?_⟩
      · rcases Stage.le_svc hφ' n hn with h | h | h
        · cases n with
          | atm a => exact .inr ⟨p, rfl, h⟩
          | _ => exact h.elim
        · exact .inl (hr' n h)
        · cases n with
          | ob N a => exact .inr (.inr ⟨p, rfl, h.1⟩)
          | obv v a => exact .inr ⟨p, rfl, h.1⟩
          | _ => exact h.elim
      · exact (Stage.le_err hφ').trans he'
      · have hp : σ (.r p) = .rg p := hσ.1 _ (.inl rfl)
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
          rintro x (hx | hx | hx)
          · exact .inl hx
          · exact .inr (.inl hx)
          · exact .inr (.inr (Row.disch_substT (fun y hy => by rw [hp] at hy; exact hy) x hx))
        · obtain ⟨b, hb, h''⟩ := (Ty.reach_substT σ _ _).mp h
          rwa [hav b _ h''] at hb
    | errVal hv | allI hv => exact (nv hv).elim

end Flick
