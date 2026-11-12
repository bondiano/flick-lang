import Formal.Vary

/-!
# Spines at a variation of an instance

A constructor's type at an instance is below its type at a variation of the instance
(`SpW.ctor`): each field built at the one is below the field read at the other
(`field_vary_all`), and what it builds varies with it (`Sig.Vary.map_res`).
-/

namespace Flick

variable {S : Sig}

/-- A spine type at a narrower instance: its parameters are below the other's and its
result varies to the other's. -/
inductive SpW (S : Sig) : Ty → Ty → Prop
  | refl {t : Ty} : SpW S t t
  | res {T : Name} {us us' : List Ty} : S.Vary T us us' → SpW S (Ty.tcon T us) (Ty.tcon T us')
  | arrow {z a zt t : Ty} : LeLive S z a → SpW S zt t →
      SpW S (.arrow z .pure zt Caps.none) (.arrow a .pure t Caps.none)

theorem SpW.arrows {σ σ' : Atom → Ty} {R R' : Ty} (hR : SpW S R R') :
    ∀ fs : List Ty, (∀ f ∈ fs, LeLive S (f.substT σ) (f.substT σ')) →
      SpW S (Ty.arrows (fs.map (Ty.substT σ)) R) (Ty.arrows (fs.map (Ty.substT σ')) R')
  | [], _ => hR
  | f :: fs, h => .arrow (h f (.head _)) (SpW.arrows hR fs fun g hg => h g (.tail _ hg))

/-- The instance that takes the first `m` variables from `us'` and the rest from `us`. -/
def instMix (us us' : List Ty) (m : Nat) : Atom → Ty
  | .v j => if j < m then instT us' (.v j) else instT us (.v j)
  | .r p => .rg p

/-- A field of a constructor at an instance is below the field at a variation of it. -/
theorem field_vary_all {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k) (hw : S.Vary T us us') {f : Ty}
    (hf : f ∈ ts) : LeLive S (f.substT (instT us)) (f.substT (instT us')) := by
  have hk' : us'.length = k := hw.length ▸ hk
  have h0 : f.substT (instT us) = f.substT (instMix us us' 0) :=
    Ty.substT_congr fun a => by cases a <;> simp [instMix, instT]
  have hK' : f.substT (instMix us us' k) = f.substT (instT us') :=
    Ty.substT_congr fun a => by
      cases a with
      | v j =>
        by_cases hj : j < k
        · simp [instMix, hj]
        · simp [instMix, hj, instT, hk, hk']
      | r p => rfl
  rw [h0, ← hK']
  have step : ∀ m, m < k →
      LeLive S (f.substT (instMix us us' m)) (f.substT (instMix us us' (m + 1))) := by
    intro m hm
    have hσ : ∀ a, a ≠ .v m → instMix us us' m a = instMix us us' (m + 1) a := fun a ha => by
      cases a with
      | v j =>
        have : j ≠ m := fun e => ha (e ▸ rfl)
        by_cases h1 : j < m
        · simp [instMix, h1, show j < m + 1 by omega]
        · simp [instMix, h1, show ¬ j < m + 1 by omega]
      | r p => rfl
    have hm1 : m < us.length := hk ▸ hm
    have hm2 : m < us'.length := hk' ▸ hm
    have ev : instMix us us' m (.v m) = us[m] := by simp [instMix, instT, hm1]
    have ev' : instMix us us' (m + 1) (.v m) = us'[m] := by simp [instMix, instT, hm2]
    have g1 := List.getElem?_eq_getElem hm1
    have g2 := List.getElem?_eq_getElem hm2
    have hr : Caps.Sub (instMix us us' m (.v m)).reach (instMix us us' (m + 1) (.v m)).reach := by
      rw [ev, ev']; exact hw.2.1 m _ _ g1 g2
    by_cases hh : S.hidden T = some m
    · obtain ⟨-, -, -, hcov⟩ := S.hidden_ok T m hh K k ts hK
      exact field_widen (hcov f hf) hσ hr
    · cases hv : S.var T m with
      | pos =>
        obtain ⟨-, -, -, hpol⟩ := S.var_ok T m (by rw [hv]; decide) K k ts hK
        have hp := (hpol f hf).2 (.inl hv)
        rw [hv] at hp
        exact (field_vary hp hσ hr (fun _ => by rw [ev, ev']; exact hw.2.2.1 m _ _ g1 g2 hh hv)
          (fun h => by cases h)).1 rfl
      | neg =>
        obtain ⟨-, -, -, hpol⟩ := S.var_ok T m (by rw [hv]; decide) K k ts hK
        have hp := (hpol f hf).2 (.inr hv)
        rw [hv] at hp
        exact (field_vary hp hσ hr (fun h => by cases h)
          (fun _ => by rw [ev, ev']; exact hw.2.2.2.1 m _ _ g1 g2 hh hv)).1 rfl
      | bi =>
        obtain ⟨-, -, -, hpol⟩ := S.var_ok T m (by rw [hv]; decide) K k ts hK
        rw [((hpol f hf).1 hv).substT_eq hσ]; exact .refl
      | inv =>
        have e := hw.2.2.2.2 m _ _ g1 g2 hh hv
        have hmix : ∀ a, instMix us us' m a = instMix us us' (m + 1) a := fun a => by
          by_cases ha : a = .v m
          · rw [ha, ev, ev', e]
          · exact hσ a ha
        rw [Ty.substT_congr hmix]
        exact .refl
  have chain : ∀ m, m ≤ k →
      LeLive S (f.substT (instMix us us' 0)) (f.substT (instMix us us' m)) := by
    intro m; induction m with
    | zero => intro _; exact .refl
    | succ m ih => intro hm; exact (ih (by omega)).trans (step m (by omega))
  exact chain k (Nat.le_refl _)

/-- The arguments of an instance of what a constructor builds vary with the instance. -/
theorem Sig.Vary.map_res {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k) (hw : S.Vary T us us') :
    S.Vary T ((S.res K).map (Ty.substT (instT us))) ((S.res K).map (Ty.substT (instT us'))) := by
  have hk' : us'.length = k := hw.length ▸ hk
  have same : ∀ n u, (S.res K)[n]? = some u → ¬ S.Varies T n →
      u.substT (instT us) = u.substT (instT us') := fun n u hu hn =>
    Ty.substT_eq_upto k (fun a ha => by
        cases a with
        | v j =>
          have : ¬ j < k := fun h => ha j h rfl
          simp [instT, hk, hk', this]
        | r p => rfl)
      fun j hj hne => by
        have hvj : S.Varies T j := Classical.byContradiction fun hv => by
          apply hne
          have e := hw.eq_of_not_varies hv
          rw [List.getElem?_eq_getElem (hk ▸ hj), List.getElem?_eq_getElem (hk' ▸ hj)] at e
          simp [instT, hk, hk', hj, Option.some.inj e]
        exact (Sig.varies_ok hvj hK).2.2 n u hu fun e => hn (e ▸ hvj)
  have get : ∀ {τ : Atom → Ty} {n : Nat} {x : Ty}, ((S.res K).map (Ty.substT τ))[n]? = some x →
      ∃ u, (S.res K)[n]? = some u ∧ x = u.substT τ := fun h => by
    simp only [List.getElem?_map, Option.map_eq_some_iff] at h
    obtain ⟨u, hu, rfl⟩ := h
    exact ⟨u, hu, rfl⟩
  have atv : ∀ n, S.Varies T n → ∀ x x', ((S.res K).map (Ty.substT (instT us)))[n]? = some x →
      ((S.res K).map (Ty.substT (instT us')))[n]? = some x' → us[n]? = some x ∧ us'[n]? = some x' :=
    fun n hn x x' h h' => by
      obtain ⟨hnk, hrn, -⟩ := Sig.varies_ok hn hK
      obtain ⟨u, hu, rfl⟩ := get h
      obtain ⟨u', hu', rfl⟩ := get h'
      rw [hrn] at hu hu'; cases hu; cases hu'
      simp [Ty.substT, instT, hk ▸ hnk, hk' ▸ hnk]
  refine .of_agree (by simp) (S.Varies T) (fun n hn => ?_) (fun n x x' hn h h' => ?_)
    (fun n x x' hn h h' hh hv => ?_) (fun n x x' hn h h' hh hv => ?_)
    (fun n x x' hn h h' hh hv => ?_)
  · simp only [List.getElem?_map]
    cases hu : (S.res K)[n]? with
    | none => rfl
    | some u => simp [same n u hu hn]
  all_goals obtain ⟨g1, g2⟩ := atv n hn x x' h h'
  · exact hw.2.1 n x x' g1 g2
  · exact hw.2.2.1 n x x' g1 g2 hh hv
  · exact hw.2.2.2.1 n x x' g1 g2 hh hv
  · exact hw.2.2.2.2 n x x' g1 g2 hh hv

/-- A constructor's type at a variation of its instance. -/
theorem SpW.ctor {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k) (hw : S.Vary T us us') :
    SpW S (Sig.ctorTyR ts T (S.res K) us) (Sig.ctorTyR ts T (S.res K) us') := by
  simp only [Sig.ctorTyR]
  exact SpW.arrows (.res (hw.map_res hK hk)) ts fun f hf => field_vary_all hK hk hw hf

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

/-- The instance of an ordinary constructor varies where what it builds varies. -/
theorem Sig.Vary.of_map {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us us' : List Ty}
    (hj : Sig.Inj k (S.res K)) (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k)
    (hk' : us'.length = k)
    (h : S.Vary T ((S.res K).map (Ty.substT (instT us))) ((S.res K).map (Ty.substT (instT us')))) :
    S.Vary T us us' := by
  have at_ : ∀ n, n < k → ∀ m, (S.res K)[m]? = some (Ty.tvar n) →
      ((S.res K).map (Ty.substT (instT us)))[m]? = us[n]? ∧
      ((S.res K).map (Ty.substT (instT us')))[m]? = us'[n]? := fun n hn m hm => by
    rw [List.getElem?_map, List.getElem?_map, hm, List.getElem?_eq_getElem (hk ▸ hn),
      List.getElem?_eq_getElem (hk' ▸ hn)]
    simp [Ty.substT, instT, hk ▸ hn, hk' ▸ hn]
  refine .of_agree (hk.trans hk'.symm) (S.Varies T) (fun n hn => ?_)
    (fun n x x' hn g1 g2 => ?_) (fun n x x' hn g1 g2 hh hv => ?_)
    (fun n x x' hn g1 g2 hh hv => ?_) (fun n x x' hn g1 g2 hh hv => ?_)
  · by_cases hnk : n < k
    · obtain ⟨m, hm⟩ := hj n hnk
      have hmv : ¬ S.Varies T m := fun hv => by
        obtain ⟨-, hrm, -⟩ := Sig.varies_ok hv hK
        rw [hrm] at hm; cases hm; exact hn hv
      obtain ⟨e1, e2⟩ := at_ n hnk m hm
      rw [← e1, ← e2]; exact h.eq_of_not_varies hmv
    · rw [List.getElem?_eq_none (by omega), List.getElem?_eq_none (by omega)]
  all_goals
    obtain ⟨hnk, hrn, -⟩ := Sig.varies_ok hn hK
    obtain ⟨e1, e2⟩ := at_ n hnk n hrn
    rw [← e1] at g1; rw [← e2] at g2
  · exact h.2.1 n x x' g1 g2
  · exact h.2.2.1 n x x' g1 g2 hh hv
  · exact h.2.2.2.1 n x x' g1 g2 hh hv
  · exact h.2.2.2.2 n x x' g1 g2 hh hv

/-- The instance of a constructor at which what it builds is a variation of what it built. -/
theorem Sig.Vary.lift {K : Name} {k : Nat} {ts : List Ty} {T : Name} {us ws : List Ty}
    (hK : S.ctor K = some (k, ts, T)) (hk : us.length = k)
    (hw : S.Vary T ((S.res K).map (Ty.substT (instT us))) ws) :
    ∃ us', us'.length = k ∧ (S.res K).map (Ty.substT (instT us')) = ws ∧ S.Vary T us us' := by
  classical
  have hwl : (S.res K).length = ws.length := by simpa using hw.length
  let us' : List Ty := (List.range k).map fun j =>
    if hj : S.Varies T j ∧ j < ws.length then ws[j]'hj.2 else instT us (.v j)
  have hl' : us'.length = k := by simp [us']
  have el : ∀ j (hj : j < k), us'[j]? = some
      (if hj' : S.Varies T j ∧ j < ws.length then ws[j]'hj'.2 else instT us (.v j)) :=
    fun j hj => by simp [us', hj]
  have eo : ∀ j, j < k → ¬ S.Varies T j → us'[j]? = us[j]? := fun j hj hv => by
    rw [el j hj, List.getElem?_eq_getElem (hk ▸ hj)]; simp [hv, instT, hk ▸ hj]
  have same : ∀ n u, (S.res K)[n]? = some u → ¬ S.Varies T n →
      u.substT (instT us') = u.substT (instT us) := fun n u hu hn =>
    Ty.substT_eq_upto k (fun a ha => by
        cases a with
        | v j =>
          have : ¬ j < k := fun h => ha j h rfl
          simp [instT, hk, hl', this]
        | r p => rfl)
      fun j hj hne => by
        have hvj : S.Varies T j := Classical.byContradiction fun hv => by
          apply hne
          have e := eo j hj hv
          rw [List.getElem?_eq_getElem (hl' ▸ hj), List.getElem?_eq_getElem (hk ▸ hj)] at e
          simp [instT, hk, hl', hj, Option.some.inj e]
        exact (Sig.varies_ok hvj hK).2.2 n u hu fun e => hn (e ▸ hvj)
  refine ⟨us', hl', ?_, ?_⟩
  · apply List.ext_getElem?
    intro n
    simp only [List.getElem?_map]
    by_cases hn : S.Varies T n
    · obtain ⟨hnk, hrn, -⟩ := Sig.varies_ok hn hK
      have hnw : n < ws.length := by rw [← hwl]; exact (List.getElem?_eq_some_iff.mp hrn).1
      rw [hrn, List.getElem?_eq_getElem hnw]
      simp [Ty.substT, instT, hnk, us', hn, hnw]
    · cases hrn : (S.res K)[n]? with
      | none =>
        have : ¬ n < ws.length := fun h => by
          rw [← hwl] at h; rw [List.getElem?_eq_getElem h] at hrn; cases hrn
        simp [List.getElem?_eq_none (Nat.le_of_not_lt this)]
      | some u =>
        simp only [Option.map_some]
        rw [same n u hrn hn]
        have e := hw.eq_of_not_varies hn
        simp only [List.getElem?_map, hrn, Option.map_some] at e
        exact e
  · refine .of_agree (hk.trans hl'.symm) (S.Varies T) (fun n hn => ?_)
      (fun n x x' hn g1 g2 => ?_) (fun n x x' hn g1 g2 hh hv => ?_)
      (fun n x x' hn g1 g2 hh hv => ?_) (fun n x x' hn g1 g2 hh hv => ?_)
    · by_cases hnk : n < k
      · exact (eo n hnk hn).symm
      · rw [List.getElem?_eq_none (by omega), List.getElem?_eq_none (by omega)]
    all_goals
      obtain ⟨hnk, hrn, -⟩ := Sig.varies_ok hn hK
      have hnw : n < ws.length := by rw [← hwl]; exact (List.getElem?_eq_some_iff.mp hrn).1
      have e1 : ((S.res K).map (Ty.substT (instT us)))[n]? = some x := by
        rw [List.getElem?_map, hrn]
        simp [Ty.substT, instT, hk ▸ hnk]
        rw [List.getElem?_eq_getElem (hk ▸ hnk)] at g1; exact Option.some.inj g1
      have e2 : ws[n]? = some x' := by
        rw [el n hnk] at g2; simp [hn, hnw] at g2; rw [← g2, List.getElem?_eq_getElem hnw]
    · exact hw.2.1 n x x' e1 e2
    · exact hw.2.2.1 n x x' e1 e2 hh hv
    · exact hw.2.2.2.1 n x x' e1 e2 hh hv
    · exact hw.2.2.2.2 n x x' e1 e2 hh hv

end Flick
