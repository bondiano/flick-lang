import Formal.CanonForms
import Formal.Substitution
import Formal.SchemeEquivalence
import Formal.VarySpine
import Formal.SubAdm

/-!
# Canonical form of a dictionary (L1)

A value of type `D_C ā` is a dictionary record, and each field its class declares
is there, typed at its scheme — including when the dictionary type has been coerced
along the class's parameter variances (`LeLive.dict`).
-/

namespace Flick

variable {S : Sig}

theorem List.lookup_map_snd {l : Label} (g : Scheme → Scheme) : ∀ fs : List (Label × Scheme),
    (fs.map fun f => (f.1, g f.2)).lookup l = (fs.lookup l).map g
  | [] => rfl
  | (l', s) :: fs => by
    simp only [List.map, List.lookup]
    split <;> simp_all [List.lookup_map_snd g fs]

/-- `us'` varies `us` as subtyping of dictionary `c` allows (`LeLive.dict`). -/
def Sig.DictVary (S : Sig) (c : Nat) (us us' : List Ty) : Prop :=
  us.length = us'.length ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
    S.dictVar c j = .pos → LeLive S u u') ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
    S.dictVar c j = .neg → LeLive S u' u ∧ Caps.Sub u.reach u'.reach) ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' →
    (S.dictVar c j = .inv ∨ S.dictVar c j = .bi) → Ty.Equiv u u')

theorem Sig.DictVary.of_equiv {c : Nat} {us vs : List Ty} (he : Ty.ArgsEquiv us vs) :
    S.DictVary c us vs :=
  ⟨he.length,
    fun j u v hu hv _ => .equiv (he.get j u v hu hv),
    fun j u v hu hv _ => ⟨.equiv (he.get j u v hu hv).symm,
      fun a ha => Eq.mp (congrFun (he.get j u v hu hv).reach a) ha⟩,
    fun j u v hu hv _ => he.get j u v hu hv⟩

theorem LeLive.dict_spine {c : Nat} {vs us : List Ty}
    (h : LeLive S (.dict c (Ty.spine vs)) (.dict c (Ty.spine us))) :
    S.DictVary c vs us := by
  generalize hX : Ty.dict c (Ty.spine vs) = X at h
  generalize hY : Ty.dict c (Ty.spine us) = Y at h
  cases h with
  | refl =>
    have hsp := Ty.spine_inj (Ty.dict.inj (hX.trans hY.symm)).2
    subst hsp
    exact .of_equiv (Ty.ArgsEquiv.refl vs)
  | equiv he =>
    rw [← hX, ← hY] at he
    cases he.form with
    | rows hr _ => cases hr
    | dict hU => exact .of_equiv hU.spine_args
  | dict hl hp hn hnr hi =>
    obtain ⟨rfl, h1⟩ := Ty.dict.inj hX
    obtain ⟨_, h2⟩ := Ty.dict.inj hY
    obtain rfl := Ty.spine_inj h1
    obtain rfl := Ty.spine_inj h2
    exact ⟨hl, hp, fun j u u' hu hu' hv => ⟨hn j u u' hu hu' hv, hnr j u u' hu hu' hv⟩, hi⟩
  | arrow => cases hX
  | eff => cases hX
  | rcons => cases hX
  | hide _ _ _ _ ha => exact absurd (hX.trans ha) Ty.dict_ne_tcon
  | data _ _ _ _ _ ha => exact absurd (hX.trans ha) Ty.dict_ne_tcon

private theorem instMix_zero (vs us : List Ty) : instMix vs us 0 = instT vs := by
  funext a; cases a <;> simp [instMix, instT]

private theorem instMix_end {k : Nat} {vs us : List Ty} (hvk : vs.length = k) (huk : us.length = k) :
    instMix vs us k = instT us := by
  funext a
  cases a with
  | v j =>
    by_cases hj : j < k
    · simp [instMix, hj]
    · simp [instMix, hj, instT, hvk, huk]
  | r p => rfl

private theorem liftT_instMix_agree (k : Nat) (vs us : List Ty) (m : Nat) :
    ∀ a, a ≠ .v (k + m) → liftT k (instMix vs us m) a = liftT k (instMix vs us (m + 1)) a := by
  intro a ha
  cases a with
  | r p => rfl
  | v j =>
    by_cases hjk : j < k
    · simp [liftT, hjk]
    · simp only [liftT, hjk, ↓reduceIte]
      congr 1
      have hne : j - k ≠ m := fun e => ha (by congr 1; omega)
      by_cases h1 : j - k < m
      · simp [instMix, h1, show j - k < m + 1 by omega]
      · simp [instMix, h1, show ¬ j - k < m + 1 by omega]

/-- Scheme obligations at a supertype dictionary hold at the underlying dictionary too:
for any non-invariant class parameter `i`, `Sig.dictVar_ok` forbids `s.cs` from mentioning it. -/
theorem dict_field_holds {c k : Nat} {fs : List (Label × Scheme)} {l : Label} {s : Scheme}
    {vs us ts : List Ty} {Θ : Facts} (hc : S.dict c = some (k, fs)) (hl : (l, s) ∈ fs)
    (hvk : vs.length = k) (hw : S.DictVary c vs us) (hH : (s.substT (instT us)).Holds Θ ts) :
    (s.substT (instT vs)).Holds Θ ts := by
  have huk : us.length = k := hw.1 ▸ hvk
  have step : ∀ m, m < k → (s.substT (instMix vs us (m + 1))).Holds Θ ts →
      (s.substT (instMix vs us m)).Holds Θ ts := by
    intro m hm hHm
    have hm1 : m < vs.length := hvk ▸ hm
    have hm2 : m < us.length := huk ▸ hm
    by_cases hinv : S.dictVar c m = .inv
    · have he := hw.2.2.2 m _ _ (List.getElem?_eq_getElem hm1) (List.getElem?_eq_getElem hm2) (.inl hinv)
      refine hHm.subst_equiv fun a => ?_
      cases a with
      | r p => exact .refl
      | v j =>
        by_cases hjm : j = m
        · subst hjm; simp [instMix, instT, hm1, hm2]; exact he.symm
        · by_cases h1 : j < m
          · simp [instMix, h1, show j < m + 1 by omega]; exact .refl
          · simp [instMix, h1, show ¬ j < m + 1 by omega]; exact .refl
    · have hcs := (S.dictVar_ok c k fs m hc hinv l s hl).1
      have heq := hcs.substT_eq (liftT_instMix_agree s.k vs us m)
      simpa only [Scheme.Holds, Scheme.substT, heq] using hHm
  have chain : ∀ m, m ≤ k → (s.substT (instMix vs us m)).Holds Θ ts →
      (s.substT (instMix vs us 0)).Holds Θ ts := by
    intro m; induction m with
    | zero => intro _ h; exact h
    | succ m ih => intro hm h; exact ih (by omega) (step m (by omega) h)
  rw [← instMix_zero vs us]
  exact chain k (Nat.le_refl _) (by rw [instMix_end hvk huk]; exact hH)

/-- A field type at the underlying dictionary is below the field type at a supertype dictionary. -/
theorem dict_field_vary {c k : Nat} {fs : List (Label × Scheme)} {l : Label} {s : Scheme}
    {vs us : List Ty} (hc : S.dict c = some (k, fs)) (hl : (l, s) ∈ fs)
    (hvk : vs.length = k) (hw : S.DictVary c vs us) :
    LeLive S (s.t.substT (liftT s.k (instT vs))) (s.t.substT (liftT s.k (instT us))) := by
  have huk : us.length = k := hw.1 ▸ hvk
  have step : ∀ m, m < k →
      LeLive S (s.t.substT (liftT s.k (instMix vs us m)))
               (s.t.substT (liftT s.k (instMix vs us (m + 1)))) := by
    intro m hm
    have hm1 : m < vs.length := hvk ▸ hm
    have hm2 : m < us.length := huk ▸ hm
    have g1 := List.getElem?_eq_getElem hm1
    have g2 := List.getElem?_eq_getElem hm2
    have hσ := liftT_instMix_agree s.k vs us m
    have ev : liftT s.k (instMix vs us m) (.v (s.k + m)) = vs[m].up s.k := by
      simp [liftT, instMix, instT, hm1, show ¬ s.k + m < s.k by omega]
    have ev' : liftT s.k (instMix vs us (m + 1)) (.v (s.k + m)) = us[m].up s.k := by
      simp [liftT, instMix, instT, hm2, show ¬ s.k + m < s.k by omega]
    cases hv : S.dictVar c m with
    | pos =>
      have hpos := hw.2.1 m _ _ g1 g2 hv
      have hup : LeLive S (vs[m].up s.k) (us[m].up s.k) := by
        simp only [Ty.up_def]; exact hpos.substT (shiftV s.k)
      have hr : Caps.Sub (liftT s.k (instMix vs us m) (.v (s.k + m))).reach
          (liftT s.k (instMix vs us (m + 1)) (.v (s.k + m))).reach := by
        rw [ev, ev']; exact hup.reach
      have hp := (S.dictVar_ok c k fs m hc (by rw [hv]; decide) l s hl).2.2 (.inl hv)
      rw [hv] at hp
      exact (field_varyD hp hσ hr (fun _ => by rw [ev, ev']; exact hup) (fun h => by cases h)).1 rfl
    | neg =>
      obtain ⟨hneg, hnr⟩ := hw.2.2.1 m _ _ g1 g2 hv
      have hup : LeLive S (us[m].up s.k) (vs[m].up s.k) := by
        simp only [Ty.up_def]; exact hneg.substT (shiftV s.k)
      have hr : Caps.Sub (liftT s.k (instMix vs us m) (.v (s.k + m))).reach
          (liftT s.k (instMix vs us (m + 1)) (.v (s.k + m))).reach := by
        rw [ev, ev']
        intro a ha
        obtain ⟨b, hb, hba⟩ := (Ty.reach_up vs[m] s.k a).mp ha
        exact (Ty.reach_up us[m] s.k a).mpr ⟨b, hnr b hb, hba⟩
      have hp := (S.dictVar_ok c k fs m hc (by rw [hv]; decide) l s hl).2.2 (.inr hv)
      rw [hv] at hp
      exact (field_varyD hp hσ hr (fun h => by cases h) (fun _ => by rw [ev, ev']; exact hup)).1 rfl
    | bi =>
      have hno := (S.dictVar_ok c k fs m hc (by rw [hv]; decide) l s hl).2.1 hv
      rw [hno.substT_eq hσ]; exact .refl
    | inv =>
      have he := hw.2.2.2 m _ _ g1 g2 (.inl hv)
      refine .equiv (s.t.substT_equiv fun a => ?_)
      by_cases ha : a = .v (s.k + m)
      · rw [ha, ev, ev']; exact he.up s.k
      · rw [hσ a ha]; exact .refl
  have chain : ∀ m, m ≤ k →
      LeLive S (s.t.substT (liftT s.k (instMix vs us 0)))
               (s.t.substT (liftT s.k (instMix vs us m))) := by
    intro m; induction m with
    | zero => intro _; exact .refl
    | succ m ih => intro hm; exact (ih (by omega)).trans (step m (by omega))
  rw [← instMix_zero vs us, ← instMix_end hvk huk]
  exact chain k (Nat.le_refl _)

private theorem Typ.drec_dict {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ c r c' U φ, e = .drec c r → J = .ex (.dict c' U) φ →
      ∃ k fs us, c = c' ∧ LeLive S (.dict c (Ty.spine us)) (.dict c U) ∧
        S.dict c = some (k, fs) ∧ us.length = k ∧
        Typ S Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) ∧ r.NoIns := by
  induction h with
  | conv _ he ih =>
    intro c r c' U φ hv hJ; cases hJ
    cases he.symm.form with
    | rows hr _ => cases hr
    | dict hU =>
      obtain ⟨k, fs, us, hC, hu, hc, hl, ht, hn⟩ := ih c r _ _ _ hv rfl
      exact ⟨k, fs, us, hC, hu.trans (.equiv (.dict hU.symm)), hc, hl, ht, hn⟩
  | drec hc hl hr hn hT =>
    intro c r c' U φ hv hJ; cases hv; cases hJ
    obtain ⟨U', hU', hle⟩ := hT.dict_left
    cases hU'
    exact ⟨_, _, _, rfl, hle, hc, hl, hr, hn⟩
  | allI | errVal | funcRef => intro c r c' U φ _ hJ; cases hJ
  | _ => intro c r c' U φ hv _; cases hv

/-- L1 for dictionaries. -/
theorem canonical_dict {Ψ Θ Γ v c U φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.dict c U) φ)) :
    ∃ r k fs us, v = .drec c r ∧ LeLive S (.dict c (Ty.spine us)) (.dict c U) ∧
      S.dict c = some (k, fs) ∧ us.length = k ∧
      Typ S Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) ∧ r.NoIns := by
  have hk := h.vkind hv rfl (by simp [Ty.isAll]) (by simp [Ty.kind])
  cases hv with
  | drec =>
    obtain ⟨k, fs, us, hC, hu, hc, hl, hr, hn⟩ := h.drec_dict _ _ c _ _ rfl rfl
    cases hC
    exact ⟨_, k, fs, us, rfl, hu, hc, hl, hr, hn⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- A field a dictionary record declares is there, at its scheme. -/
theorem Typ.dfs_field {Ψ Θ Γ l} : ∀ {r : Tm} {fs : List (Label × Scheme)} {s : Scheme},
    Typ S Ψ Θ Γ r (.dfs fs) → fs.lookup l = some s →
    ∃ w, r.field l = some w ∧ Typ S (Ψ.map (Ty.up s.k)) (Facts.union (Θ.substT (shiftV s.k)) s.cs)
      (Γ.map (Scheme.up s.k)) w (.ex s.t .pure) := by
  intro r
  induction r with
  | rnil => intro _ _ h hl; cases h; simp [List.lookup] at hl
  | rcons l' v r _ ih =>
    intro _ _ h hl
    cases h with
    | dcons hv hr =>
      simp only [List.lookup, Tm.field] at hl ⊢
      by_cases he : l' = l
      · subst he; simp at hl; subst hl; exact ⟨_, by simp, hv⟩
      · have : (l == l') = false := by simpa using Ne.symm he
        rw [this] at hl; simp only [he, if_false]; exact ih hr hl
  | _ => intro _ _ h; cases h

theorem Tm.has_of_field {l : Label} {x : Site} : ∀ {r w : Tm}, r.field l = some w → w.Has x → r.Has x := by
  intro r
  induction r with
  | rcons l' e r _ ih =>
    intro w h hw
    simp only [Tm.field] at h
    split at h
    · cases h; exact .inl hw
    · exact .inr (ih h hw)
  | drec c r ih => intro w h hw; exact ih h hw
  | _ => intro w h; simp [Tm.field] at h

/-- The field of a dictionary record at an instance of its scheme. -/
theorem dict_field {Ψ Θ v c us φ k fs l s ts w} (hv : Value v)
    (h : Typ S Ψ Θ [] v (.ex (.dict c (Ty.spine us)) φ)) (hc : S.dict c = some (k, fs))
    (hl : fs.lookup l = some s) (hts : ts.length = s.k) (hH : (s.substT (instT us)).Holds Θ ts)
    (hw : v.field l = some w) : Typ S Ψ Θ [] w (.ex ((s.substT (instT us)).inst ts) .pure) := by
  obtain ⟨r, _, _, vs, rfl, hU, hc', hvk, hr, hn⟩ := canonical_dict hv h
  rw [hc] at hc'; cases hc'
  have hdv := hU.dict_spine
  obtain ⟨w', hw', ht⟩ := hr.dfs_field (by rw [List.lookup_map_snd, hl]; rfl)
  simp only [Tm.field] at hw; rw [hw] at hw'; cases hw'
  have hH' : (s.substT (instT vs)).Holds Θ ts := dict_field_holds hc (List.lookup_mem' hl) hvk hdv hH
  have typed := Typ.instantiate (s := s.substT (instT vs)) (Ψ := Ψ) (Γ := []) hts hH' (fun h => hn (Tm.has_of_field hw h)) ht
  exact typed.sub ⟨(dict_field_vary hc (List.lookup_mem' hl) hvk hdv).substT (instT ts), Stage.le_refl _⟩

end Flick
