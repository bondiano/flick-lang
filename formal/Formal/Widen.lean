import Formal.Canonical

/-!
# Widening the hidden row of data

Subtyping may widen the hidden row of data with hidden storage and nothing else
(regions.md §2.2, `LeLive.hide`). The row stands in the fields only where widening it
widens them (`Ty.CovRow`), so a value built at a narrower row stores fields below the
fields at the wider one (`field_widen`), and its constructor builds the wider type too
(`Sig.builds_widen`).
-/

namespace Flick

variable {S : Sig}

/-- Substitutions that agree but at `v i` agree on what does not depend on it. -/
theorem Ty.NoV.substT_eq {i : Nat} {t : Ty} (h : t.NoV i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) : t.substT σ = t.substT σ' := by
  have h' : t.substT (dropV i) = t := h
  rw [← h', Ty.substT_comp, Ty.substT_comp]
  apply Ty.substT_congr
  intro a
  by_cases ha : a = .v i
  · subst ha; simp [dropV, Ty.substT]
  · simp only [dropV, ha, if_false]
    cases a <;> simp [Ty.ident, Ty.substT, hσ _ ha]

theorem Stage.NoV.substT_eq {i : Nat} {φ : Stage} (h : φ.NoV i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) : φ.substT σ = φ.substT σ' := by
  have h' : φ.substT (dropV i) = φ := h
  rw [← h', Stage.substT_comp, Stage.substT_comp]
  congr 1; funext a
  by_cases ha : a = .v i
  · subst ha; simp [dropV, Ty.substT]
  · simp only [dropV, ha, if_false]
    cases a <;> simp [Ty.ident, Ty.substT, hσ _ ha]

/-- Two instances that differ only at `i` agree at every other atom. -/
theorem instT_agree {i : Nat} {us us' : List Ty} (hl : us.length = us'.length)
    (heq : ∀ j, j ≠ i → us[j]? = us'[j]?) : ∀ a, a ≠ .v i → instT us a = instT us' a := by
  intro a ha
  cases a with
  | v j =>
    have hj : j ≠ i := fun h => ha (h ▸ rfl)
    have e := heq j hj
    by_cases h1 : j < us.length
    · have h2 : j < us'.length := hl ▸ h1
      rw [List.getElem?_eq_getElem h1, List.getElem?_eq_getElem h2] at e
      simp only [instT, h1, h2, dite_true]
      exact Option.some.inj e
    · have h2 : ¬ j < us'.length := hl ▸ h1
      simp only [instT, h2, dite_false, hl]
  | r p => rfl

theorem reach_pointwise {i : Nat} {σ σ' : Atom → Ty} (hσ : ∀ a, a ≠ .v i → σ a = σ' a)
    (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach) : ∀ b, Caps.Sub (σ b).reach (σ' b).reach := by
  intro b
  by_cases hb : b = .v i
  · subst hb; exact hr
  · rw [hσ b hb]; exact Caps.Sub.refl _

theorem Ty.reach_substT_mono {t : Ty} {σ σ' : Atom → Ty}
    (h : ∀ b, Caps.Sub (σ b).reach (σ' b).reach) :
    Caps.Sub (t.substT σ).reach (t.substT σ').reach := fun a ha => by
  obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ t a).mp ha
  exact (Ty.reach_substT σ' t a).mpr ⟨b, hb, h b a h'⟩

theorem Caps.substT_widen {C : Caps} {σ σ' : Atom → Ty}
    (h : ∀ b, Caps.Sub (σ b).reach (σ' b).reach) : Caps.Sub (C.substT σ) (C.substT σ') :=
  fun x ⟨b, hb, h'⟩ => ⟨b, hb, h b x h'⟩

/-- A field at a narrower hidden row is below the field at a wider one. -/
theorem field_widen {i : Nat} {f : Ty} (hf : f.CovRow S.hidden i) {σ σ' : Atom → Ty}
    (hσ : ∀ a, a ≠ .v i → σ a = σ' a) (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach) :
    LeLive S (f.substT σ) (f.substT σ') := by
  have hp := reach_pointwise hσ hr
  rcases hf with h | ⟨a, φ, b, C, rfl, ha, hφ, hb⟩ | ⟨r, e, t, C, rfl, hr', he, ht⟩ |
      ⟨T, us, j, rfl, hj, hus⟩
  · rw [h.substT_eq hσ]; exact .refl
  · simp only [Ty.substT]
    rw [ha.substT_eq hσ, hφ.substT_eq hσ, hb.substT_eq hσ]
    have hC := Caps.substT_widen (C := C) hp
    refine .arrow .refl (Stage.use_mono (Stage.le_refl _) hC) .refl fun x hx => ?_
    rcases hx with hx | hx | hx
    · exact .inl hx
    · exact .inr (.inl hx)
    · exact .inr (.inr (hC x hx))
  · simp only [Ty.substT]
    have er := hr'.substT_eq hσ
    have ee := he.substT_eq hσ
    simp only [Ty.substT, Ty.row.injEq] at er ee
    rw [er, ee, ht.substT_eq hσ]
    have hC := Caps.substT_widen (C := C) hp
    refine .eff (Stage.use_mono (Stage.le_refl _) hC) .refl fun x hx => ?_
    rcases hx with hx | hx
    · exact .inl hx
    · exact .inr (hC x hx)
  · rw [Ty.tcon_substT, Ty.tcon_substT]
    refine .hide hj (by simp) (fun l hl => ?_) (fun r r' h h' => ?_) rfl rfl
    · simp only [List.getElem?_map]
      cases hu : us[l]? with
      | none => rfl
      | some u => simp [(hus l u hu hl).substT_eq hσ]
    · simp only [List.getElem?_map] at h h'
      cases hu : us[j]? with
      | none => rw [hu] at h; cases h
      | some u =>
        rw [hu] at h h'; cases h; cases h'
        exact Ty.reach_substT_mono hp

/-- `us'` is `us` with the hidden row of `T` widened, or `us` itself. -/
def Sig.Widen (S : Sig) (T : Name) (us us' : List Ty) : Prop :=
  us = us' ∨ ∃ i, S.hidden T = some i ∧ us.length = us'.length ∧
    (∀ j, j ≠ i → us[j]? = us'[j]?) ∧
    ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach

theorem Sig.Widen.refl {T : Name} {us : List Ty} : S.Widen T us us := .inl rfl

theorem Sig.Widen.length {T : Name} {us us' : List Ty} (h : S.Widen T us us') :
    us.length = us'.length := by
  rcases h with rfl | ⟨_, _, hl, _⟩
  · rfl
  · exact hl

theorem Sig.Widen.trans {T : Name} {us vs ws : List Ty} (h₁ : S.Widen T us vs)
    (h₂ : S.Widen T vs ws) : S.Widen T us ws := by
  rcases h₁ with rfl | ⟨i, hi, hl, he, hs⟩
  · exact h₂
  rcases h₂ with rfl | ⟨i', hi', hl', he', hs'⟩
  · exact .inr ⟨i, hi, hl, he, hs⟩
  obtain rfl := Option.some.inj (hi.symm.trans hi')
  refine .inr ⟨i, hi, hl.trans hl', fun j hj => (he j hj).trans (he' j hj), fun r r' h h' => ?_⟩
  have hlt : i < vs.length := hl ▸ (List.getElem?_eq_some_iff.mp h).1
  exact (hs r _ h (List.getElem?_eq_getElem hlt)).trans (hs' _ r' (List.getElem?_eq_getElem hlt) h')

theorem Sig.Widen.nil {T : Name} {ws : List Ty} (h : S.Widen T [] ws) : ws = [] :=
  List.eq_nil_of_length_eq_zero h.length.symm

theorem Ty.tcon_up_substT (T : Name) (us : List Ty) (k : Nat) (ρ : Atom → Ty) :
    ((Ty.tcon T us).up k).substT ρ = Ty.tcon T (us.map fun u => (u.up k).substT ρ) := by
  simp only [Ty.up, Ty.substT_ren, Ty.tcon_substT]

/-- A constructor that builds data builds it with its hidden row widened: the row is a
variable of what it builds, and nothing else there depends on it. -/
theorem Sig.builds_widen {K T : Name} {us us' : List Ty} (hw : S.Widen T us us')
    (h : S.Builds K (Ty.tcon T us)) : S.Builds K (Ty.tcon T us') := by
  rcases hw with rfl | ⟨i, hi, hl, heq, -⟩
  · exact h
  intro k ts T₁ us₁ hK ht hlen
  obtain ⟨rfl, rfl⟩ := Ty.tcon_eq ht
  obtain ⟨ρ, hρ⟩ := h k ts T us hK rfl (hl.trans hlen)
  obtain ⟨hik, hri, hno, -⟩ := S.hidden_ok T i hi K k ts hK
  have hi_lt : i < us'.length := by rw [hlen]; exact (List.getElem?_eq_some_iff.mp hri).1
  let ρ' : Atom → Ty := fun a => if a = .v i then ((us'[i]'hi_lt).up k).substT ρ else ρ a
  have hag : ∀ a, a ≠ .v i → ρ a = ρ' a := fun a ha => by simp [ρ', ha]
  have hup : ∀ u : Ty, (u.up k).substT ρ = (u.up k).substT ρ' := fun u => by
    simp only [Ty.up, Ty.substT_ren]
    exact Ty.substT_congr fun a => hag _ (Atom.up_ne hik)
  refine ⟨ρ', ?_⟩
  rw [Ty.tcon_up_substT] at hρ ⊢
  rw [Ty.tcon_substT] at hρ ⊢
  obtain ⟨-, hρl⟩ := Ty.tcon_eq hρ
  congr 1
  apply List.ext_getElem?
  intro n
  simp only [List.getElem?_map]
  by_cases hn : n = i
  · subst hn
    rw [hri, List.getElem?_eq_getElem hi_lt]
    simp only [Option.map_some, Option.some.injEq, Ty.substT]
    rw [show ρ' (.v n) = ((us'[n]'hi_lt).up k).substT ρ from if_pos rfl]
    exact hup _
  · have e := congrArg (·[n]?) hρl
    simp only [List.getElem?_map] at e
    rw [← heq n hn]
    cases hrn : (S.res K)[n]? with
    | none =>
      rw [hrn] at e
      cases hu : us[n]? with
      | none => rfl
      | some u => rw [hu] at e; cases e
    | some u =>
      rw [hrn] at e
      cases hu : us[n]? with
      | none => rw [hu] at e; cases e
      | some w =>
        rw [hu] at e
        simp only [Option.map_some, Option.some.injEq] at e ⊢
        rw [← (hno n u hrn hn).substT_eq hag, e, hup]

/-- A spine type at a narrower hidden row: its parameters are below the other's and its
result widens to the other's. -/
inductive SpW (S : Sig) : Ty → Ty → Prop
  | refl {t : Ty} : SpW S t t
  | res {T : Name} {us us' : List Ty} : S.Widen T us us' → SpW S (Ty.tcon T us) (Ty.tcon T us')
  | arrow {z a zt t : Ty} : LeLive S z a → SpW S zt t →
      SpW S (.arrow z .pure zt Caps.none) (.arrow a .pure t Caps.none)

theorem SpW.arrows {i : Nat} {σ σ' : Atom → Ty} (hσ : ∀ a, a ≠ .v i → σ a = σ' a)
    (hr : Caps.Sub (σ (.v i)).reach (σ' (.v i)).reach) {R R' : Ty} (hR : SpW S R R') :
    ∀ fs : List Ty, (∀ f ∈ fs, f.CovRow S.hidden i) →
      SpW S (Ty.arrows (fs.map (Ty.substT σ)) R) (Ty.arrows (fs.map (Ty.substT σ')) R')
  | [], _ => hR
  | f :: fs, h => .arrow (field_widen (h f (.head _)) hσ hr)
      (SpW.arrows hσ hr hR fs fun g hg => h g (.tail _ hg))

/-- A constructor's type at a narrower instance of its hidden row. -/
theorem SpW.ctor {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k) (hw : S.Widen T us us') :
    SpW S (Sig.ctorTyR ts T (S.res K) us) (Sig.ctorTyR ts T (S.res K) us') := by
  rcases hw with rfl | ⟨i, hi, hl, heq, hsub⟩
  · exact .refl
  obtain ⟨hik, hri, hno, hcov⟩ := S.hidden_ok T i hi K k ts hK
  have hσ := instT_agree hl heq
  have h1 : i < us.length := hk ▸ hik
  have h2 : i < us'.length := hl ▸ h1
  have hr : Caps.Sub ((instT us) (.v i)).reach ((instT us') (.v i)).reach := by
    simp only [instT, h1, h2, dite_true]
    exact hsub _ _ (List.getElem?_eq_getElem h1) (List.getElem?_eq_getElem h2)
  refine SpW.arrows hσ hr (.res (.inr ⟨i, hi, by simp, fun j hj => ?_, fun r r' h h' => ?_⟩)) ts hcov
  · simp only [List.getElem?_map]
    cases hj' : (S.res K)[j]? with
    | none => rfl
    | some u => simp [(hno j u hj' hj).substT_eq hσ]
  · simp only [List.getElem?_map, hri, Option.map_some, Option.some.injEq] at h h'
    subst h; subst h'
    exact hr

/-- One step along a spine: the parameter of the value is below the pattern's, and the
rest of the spine is again a narrower spine below the value's rest. -/
theorem SpW.arrow_step {Z X a t : Ty} (hs : SpW S Z (.arrow a .pure t Caps.none))
    (hl : LeLive S Z X) :
    ∃ a' φ' t' C', X = .arrow a' φ' t' C' ∧ LeLive S a' a ∧ ∃ Z', LeLive S Z' t' ∧ SpW S Z' t := by
  generalize hY : Ty.arrow a .pure t Caps.none = Y at hs
  cases hs with
  | refl =>
    subst hY
    obtain ⟨_, _, _, _, rfl, la, -, lt, -⟩ := hl.arrow_left
    exact ⟨_, _, _, _, rfl, la, _, lt, .refl⟩
  | res _ => exact absurd hY.symm (Ty.tcon_not_arrow _ _ _ _ _ _)
  | arrow hza hzt =>
    cases hY
    obtain ⟨_, _, _, _, rfl, la, -, lt, -⟩ := hl.arrow_left
    exact ⟨_, _, _, _, rfl, la.trans hza, _, lt, hzt⟩

/-- The instance of an ordinary constructor widens where what it builds widens. -/
theorem Sig.Widen.of_map {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hj : Sig.Inj k (S.res K)) (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k)
    (hk' : us'.length = k)
    (h : S.Widen T ((S.res K).map (Ty.substT (instT us))) ((S.res K).map (Ty.substT (instT us')))) :
    S.Widen T us us' := by
  rcases h with h | ⟨i, hi, -, heq, hsub⟩
  · exact .inl (Sig.inst_inj hj hk hk' h)
  obtain ⟨hik, hri, -, -⟩ := S.hidden_ok T i hi K k ts hK
  refine .inr ⟨i, hi, hk.trans hk'.symm, fun j hji => ?_, fun r r' h h' => ?_⟩
  · by_cases hjk : j < k
    · obtain ⟨m, hm⟩ := hj j hjk
      have hmi : m ≠ i := by
        intro e; subst e; rw [hri] at hm; cases hm; exact hji rfl
      have e := heq m hmi
      have h1 : j < us.length := hk ▸ hjk
      have h2 : j < us'.length := hk' ▸ hjk
      simp only [List.getElem?_map, hm, Option.map_some, Option.some.injEq, Ty.substT, instT,
        h1, h2, dite_true] at e
      rw [List.getElem?_eq_getElem h1, List.getElem?_eq_getElem h2, e]
    · rw [List.getElem?_eq_none (by omega), List.getElem?_eq_none (by omega)]
  · obtain ⟨h1, rfl⟩ := List.getElem?_eq_some_iff.mp h
    obtain ⟨h2, rfl⟩ := List.getElem?_eq_some_iff.mp h'
    have := hsub ((Ty.tvar i).substT (instT us)) ((Ty.tvar i).substT (instT us'))
      (by simp [List.getElem?_map, hri]) (by simp [List.getElem?_map, hri])
    simpa [Ty.substT, instT, h1, h2] using this

/-- The instance of a constructor at which what it builds is a widening of what it built. -/
theorem Sig.Widen.lift {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us ws : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k)
    (hw : S.Widen T ((S.res K).map (Ty.substT (instT us))) ws) :
    ∃ us', us'.length = k ∧ (S.res K).map (Ty.substT (instT us')) = ws ∧ S.Widen T us us' := by
  rcases hw with rfl | ⟨i, hi, hl, heq, hsub⟩
  · exact ⟨us, hk, rfl, .refl⟩
  obtain ⟨hik, hri, hno, -⟩ := S.hidden_ok T i hi K k ts hK
  have hri' : i < (S.res K).length := (List.getElem?_eq_some_iff.mp hri).1
  have hil : i < ws.length := by rw [← hl]; simpa using hri'
  have h1 : i < us.length := hk ▸ hik
  let us' := us.set i (ws[i]'hil)
  have hl' : us.length = us'.length := by simp [us']
  have heq' : ∀ j, j ≠ i → us[j]? = us'[j]? := fun j hj => by
    simp [us', Ne.symm hj]
  have hσ := instT_agree hl' heq'
  refine ⟨us', by simp [us', hk], ?_, .inr ⟨i, hi, hl', heq', fun r r' h h' => ?_⟩⟩
  · apply List.ext_getElem?
    intro n
    simp only [List.getElem?_map]
    by_cases hn : n = i
    · subst hn
      rw [hri, List.getElem?_eq_getElem hil]
      simp [Ty.substT, instT, us', h1]
    · rw [← heq n hn, List.getElem?_map]
      cases hu : (S.res K)[n]? with
      | none => rfl
      | some u => simp [(hno n u hu hn).substT_eq hσ]
  · have e1 : us[i]? = some ((Ty.tvar i).substT (instT us)) := by
      simp [Ty.substT, instT, h1]
    have e2 : us'[i]? = some (ws[i]'hil) := by simp [us', h1]
    rw [e1] at h; rw [e2] at h'; cases h; cases h'
    exact hsub _ _ (by simp [List.getElem?_map, hri]) (List.getElem?_eq_getElem hil)

end Flick
