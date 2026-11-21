import Formal.Live.OrderHelpers

/-!
# Transitivity and substitution for `LeLive` (`Formal.Live.Order`)
-/

namespace Flick

theorem LeLive.trans_lt (n : Nat) : ∀ {b a c : Ty}, sizeOf b < n → LeLive S a b →
    LeLive S b c → LeLive S a c := by
  induction n with
  | zero => intro b a c hn; exact absurd hn (Nat.not_lt_zero _)
  | succ n ihn =>
    intro b a c hn h₁ h₂
    have ih : ∀ {m a c : Ty}, sizeOf m < sizeOf b → LeLive S a m → LeLive S m c →
        LeLive S a c := fun hm => ihn (by omega)
    by_cases hA : ∃ x φ y C, b = .arrow x φ y C
    · obtain ⟨x, φ, y, C, rfl⟩ := hA
      obtain ⟨_, _, _, _, rfl, ha, hφ, ht, hc⟩ := h₁.arrow_right
      obtain ⟨_, _, _, _, rfl, ga, gφ, gt, gc⟩ := h₂.arrow_left
      exact .arrow (ih (by simp; omega) ga ha) (Stage.le_trans hφ gφ)
        (ih (by simp; omega) ht gt) (hc.trans gc)
    by_cases hE : ∃ r ε t C, b = .eff r ε t C
    · obtain ⟨r, ε, t, C, rfl⟩ := hE
      obtain ⟨_, _, _, _, rfl, hφ, ht, hc⟩ := h₁.eff_right
      obtain ⟨_, _, _, _, rfl, gφ, gt, gc⟩ := h₂.eff_left
      exact .eff (Stage.le_trans hφ gφ) (ih (by simp; omega) ht gt)
        (hc.trans gc)
    by_cases hR : ∃ l t r, b = .rcons l t r
    · obtain ⟨l, t, r, rfl⟩ := hR
      obtain ⟨_, _, rfl, ht, hr, hc⟩ := h₁.rcons_right
      obtain ⟨_, _, rfl, gt, gr, gc⟩ := h₂.rcons_left
      exact .rcons (ih (by simp; omega) ht gt) (ih (by simp; omega) hr gr) (hc.trans gc)
    cases h₁ with
    | refl => exact h₂
    | equiv he =>
      cases h₂ with
      | refl => exact .equiv he
      | equiv he' => exact .equiv (he.trans he')
      | arrow => exact absurd ⟨_, _, _, _, rfl⟩ hA
      | eff => exact absurd ⟨_, _, _, _, rfl⟩ hE
      | rcons => exact absurd ⟨_, _, _, rfl⟩ hR
      | hide hi hl he' hr hb hc =>
        subst hb; subst hc
        obtain ⟨us, rfl, hu'⟩ := he.symm.tcon_left
        have hu := hu'.symm
        exact LeLive.data_trans ih hu.length
          (fun j u v h h' => by rw [(hu.get j u v h h').reach]; exact Caps.Sub.refl _)
          (fun j u v h h' _ _ => .equiv (hu.get j u v h h'))
          (fun j u v h h' _ _ => .equiv (hu.get j u v h h').symm)
          (fun j u v h h' _ _ => hu.get j u v h h') (.hide hi hl he' hr rfl rfl)
      | data hl hr hp hneg hinv hb hc =>
        subst hb; subst hc
        obtain ⟨us, rfl, hu'⟩ := he.symm.tcon_left
        have hu := hu'.symm
        exact LeLive.data_compose ih hu.length
          (fun j u v h h' => by rw [(hu.get j u v h h').reach]; exact Caps.Sub.refl _)
          (fun j u v h h' _ _ => .equiv (hu.get j u v h h'))
          (fun j u v h h' _ _ => .equiv (hu.get j u v h h').symm)
          (fun j u v h h' _ _ => hu.get j u v h h') hl hr hp hneg hinv
      | dict hl hp hneg hnr hinv =>
        cases he.symm.form with
        | rows hr _ => cases hr
        | dict hU =>
          obtain ⟨us, rfl, hu'⟩ := hU.spine_left
          have hu := hu'.symm
          exact LeLive.dict_compose ih hu.length
            (fun j u v h h' _ => .equiv (hu.get j u v h h'))
            (fun j u v h h' _ => .equiv (hu.get j u v h h').symm)
            (fun j u v h h' _ => by rw [(hu.get j u v h h').reach]; exact Caps.Sub.refl _)
            (fun j u v h h' _ => hu.get j u v h h') hl hp hneg hnr hinv
    | arrow => exact absurd ⟨_, _, _, _, rfl⟩ hA
    | eff => exact absurd ⟨_, _, _, _, rfl⟩ hE
    | rcons => exact absurd ⟨_, _, _, rfl⟩ hR
    | hide hi hl he hr ha hb => subst ha; rw [hb] at h₂; exact LeLive.hide_trans hi hl he hr h₂
    | data hl hr hp hneg hinv ha hb =>
      subst ha; subst hb
      exact LeLive.data_trans ih hl hr hp hneg hinv h₂
    | dict hl hp hneg hnr hinv =>
      exact LeLive.dict_trans ih hl hp hneg hnr hinv h₂

theorem LeLive.trans {a b c : Ty} (h₁ : LeLive S a b) (h₂ : LeLive S b c) : LeLive S a c :=
  LeLive.trans_lt (sizeOf b + 1) (by omega) h₁ h₂

theorem LeLive.substT {a b : Ty} (σ : Atom → Ty) (h : LeLive S a b) :
    LeLive S (a.substT σ) (b.substT σ) := by
  have sub : ∀ {x y : Ty}, Caps.Sub x.reach y.reach →
      Caps.Sub (x.substT σ).reach (y.substT σ).reach := fun hc a ha => by
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
    exact (Ty.reach_substT σ _ a).mpr ⟨b, hc b hb, h'⟩
  induction h with
  | refl => exact .refl
  | equiv he => exact .equiv (he.substT σ)
  | arrow _ hφ _ hc ih₁ ih₂ =>
    refine .arrow ih₁ ?_ ih₂ (sub hc)
    simpa only [Stage.use_substT, Ty.asStage_substS] using Stage.le_substT σ hφ
  | eff hφ _ hc ih =>
    refine .eff ?_ ih (sub hc)
    have := Stage.le_substT σ hφ
    rw [Stage.use_substT, Stage.use_substT] at this
    simpa only [Stage.substT, Ty.asRow_substT] using this
  | rcons _ _ hc ih₁ ih₂ => exact .rcons ih₁ ih₂ (sub hc)
  | hide hi hl he hr ha hb =>
    subst ha; subst hb
    rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .hide hi (by simpa using hl) (fun j hj => by simp [he j hj]) ?_ rfl rfl
    intro r r' h h'
    simp only [List.getElem?_map, Option.map_eq_some_iff] at h h'
    obtain ⟨u, hu, rfl⟩ := h
    obtain ⟨u', hu', rfl⟩ := h'
    exact sub (hr u u' hu hu')
  | data hl hr _ _ hinv ha hb ihp ihn =>
    subst ha; subst hb
    rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .data (by simpa using hl) (fun j x x' h h' => ?_) (fun j x x' h h' hj hv => ?_)
      (fun j x x' h h' hj hv => ?_) (fun j x x' h h' hj hv => ?_) rfl rfl
    all_goals
      simp only [List.getElem?_map, Option.map_eq_some_iff] at h h'
      obtain ⟨u, hu, rfl⟩ := h
      obtain ⟨u', hu', rfl⟩ := h'
    · exact sub (hr j u u' hu hu')
    · exact ihp j u u' hu hu' hj hv
    · exact ihn j u u' hu hu' hj hv
    · exact (hinv j u u' hu hu' hj hv).substT σ
  | dict hl _ _ hnr hinv ihp ihn =>
    simp only [Ty.substT, Ty.spine_substT]
    refine .dict (by simpa using hl) (fun j x x' h h' hv => ?_) (fun j x x' h h' hv => ?_)
      (fun j x x' h h' hv => ?_) (fun j x x' h h' hv => ?_)
    all_goals
      simp only [List.getElem?_map, Option.map_eq_some_iff] at h h'
      obtain ⟨u, hu, rfl⟩ := h
      obtain ⟨u', hu', rfl⟩ := h'
    · exact ihp j u u' hu hu' hv
    · exact ihn j u u' hu hu' hv
    · exact sub (hnr j u u' hu hu' hv)
    · exact (hinv j u u' hu hu' hv).substT σ

theorem LeLiveI.trans {a b c : Ty} (h : LeLiveI S a b)
    (hc : ∀ {x}, LeLive S x b → LeLive S x c) : LeLiveI S a c := by
  induction h with
  | le h => exact .le (hc h)
  | inst hus _ ih => exact .inst hus (ih hc)

theorem LeLiveI.substT {a b : Ty} (σ : Atom → Ty) (h : LeLiveI S a b) :
    LeLiveI S (a.substT σ) (b.substT σ) := by
  induction h with
  | le h => exact .le (h.substT σ)
  | @inst k t T us hus _ ih =>
    subst hus
    refine .inst (us := us.map (Ty.substT σ)) (by simp) ?_
    rw [Ty.inst_substT]; exact ih

theorem Row.outlives_substT {Θ r s} (σ : Atom → Ty)
    (h : Row.Outlives Θ r s) : Row.Outlives (Θ.substT σ) (r.substT σ) (s.substT σ) := by
  simpa [Row.Outlives, Ty.reach_caps, Ty.substT] using h.substT σ

theorem Const.ok_substT {Θ c ι} (σ : Atom → Ty)
    (h : Const.ok Θ c ι) : Const.ok (Θ.substT σ) c (ι.substT σ) := by
  cases c <;> simp only [Const.ok, CArg.substT] at h ⊢
  case addFin =>
    exact ⟨h.1.substT σ, Row.outlives_substT σ h.2.1, by
      rw [← Row.ofCaps_substT, Ty.asRow_substT]; exact h.2.2.substT σ⟩
  case spawn =>
    refine ⟨?_, Row.outlives_substT σ h.2.1, by
      rw [← Row.ofCaps_substT, Ty.asRow_substT]; exact h.2.2.substT σ⟩
    have hs := h.1.substT σ
    intro a ha
    apply hs a
    rcases ha with ⟨b, hb, h'⟩ | ha
    · exact ⟨b, .inl hb, h'⟩
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
      exact ⟨b, .inr hb, h'⟩
  case scopeRun => rw [← Row.ofCaps_substT, Ty.asRow_substT]; exact h.substT σ
  case refMake | refGet | refSet => rw [← Ty.reach_caps]; exact h.substT σ
  all_goals exact h

theorem Const.ok_mono {Θ Θ' c ι} (h : Const.ok Θ c ι)
    (hΘ : ∀ C s, Outl Θ C s → Outl Θ' C s) : Const.ok Θ' c ι := by
  cases c <;> simp only [Const.ok] at h ⊢
  case addFin | spawn => exact ⟨hΘ _ _ h.1, hΘ _ _ h.2.1, h.2.2⟩
  case refMake | refGet | refSet => exact hΘ _ _ h
  all_goals exact h

end Flick
