import Formal.Live.Views
import Formal.DictOps

/-!
# Structural laws for typing by use

`LeLive` compares effective use stages, including qualifiers.
-/

namespace Flick

/-- Widening a hidden row, then anything. -/
theorem LeLive.hide_trans {T : Name} {i : Nat} {us us' : List Ty} {c : Ty}
    (hi : S.hidden T = some i) (hl : us.length = us'.length)
    (he : ∀ j, j ≠ i → us[j]? = us'[j]?)
    (hr : ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach)
    (h₂ : LeLive S (Ty.tcon T us') c) : LeLive S (Ty.tcon T us) c := by
  generalize hx : Ty.tcon T us' = x at h₂
  cases h₂ with
  | refl => subst hx; exact .hide hi hl he hr rfl rfl
  | equiv heq =>
    rw [← hx] at heq
    obtain ⟨ws, rfl, hw⟩ := heq.tcon_left
    refine .data (hl.trans hw.length) (fun j u w hu hw' => ?_)
      (fun j u w hu hw' hj _ => ?_) (fun j u w hu hw' hj _ => ?_)
      (fun j u w hu hw' hj _ => ?_) rfl rfl
    all_goals
      obtain ⟨v, hv⟩ : ∃ v, us'[j]? = some v := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp hu).1
        exact ⟨us'[j]'(hl ▸ this), by simp⟩
    · rw [← (hw.get j v w hv hw').reach]
      exact LeLive.hide_args he hr j u v hu hv
    all_goals
      have hji : j ≠ i := fun e => hj (e ▸ hi)
      rw [he j hji, hv] at hu
      cases hu
    · exact .equiv (hw.get j _ w hv hw')
    · exact .equiv (hw.get j _ w hv hw').symm
    · exact hw.get j _ w hv hw'
  | arrow => exact absurd hx.symm Ty.arrow_ne_tcon
  | eff => exact absurd hx.symm Ty.eff_ne_tcon
  | rcons => exact absurd hx.symm Ty.rcons_ne_tcon
  | dict => exact absurd hx.symm Ty.dict_ne_tcon
  | hide hi' hl' he' hr' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    rw [hi] at hi'; cases hi'
    refine .hide hi (hl.trans hl') (fun j hj => (he j hj).trans (he' j hj)) ?_ rfl rfl
    intro r r' h h'
    obtain ⟨m, hm⟩ : ∃ m, us'[i]? = some m := by
      have : i < us.length := (List.getElem?_eq_some_iff.mp h).1
      exact ⟨us'[i]'(hl ▸ this), by simp⟩
    exact (hr r m h hm).trans (hr' m r' hm h')
  | data hl' hr' hp' hn' hi' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    have hs := LeLive.hide_args he hr
    refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
      (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
    all_goals
      obtain ⟨m, hm⟩ : ∃ m, us'[j]? = some m := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
        exact ⟨us'[j]'(hl ▸ this), by simp⟩
    · exact (hs j u m h hm).trans (hr' j m u' hm h')
    all_goals
      have hji : j ≠ i := fun e => hj (e ▸ hi)
      rw [he j hji, hm] at h; cases h
    · exact hp' j _ u' hm h' hj hv
    · exact hn' j _ u' hm h' hj hv
    · exact hi' j _ u' hm h' hj hv

theorem Ty.sizeOf_lt_foldl : ∀ {us : List Ty} {f : Ty},
    sizeOf f < sizeOf (us.foldl .tapp f) + 1 ∧
    ∀ {j : Nat} {u : Ty}, us[j]? = some u → sizeOf u < sizeOf (us.foldl .tapp f)
  | [], f => ⟨by simp, by simp⟩
  | x :: xs, f => by
    obtain ⟨h1, h2⟩ := Ty.sizeOf_lt_foldl (us := xs) (f := Ty.tapp f x)
    simp only [List.foldl] at h1 h2 ⊢
    have e : sizeOf (Ty.tapp f x) = 1 + sizeOf f + sizeOf x := by simp
    have hx : sizeOf x < sizeOf (xs.foldl Ty.tapp (Ty.tapp f x)) := by omega
    refine ⟨by omega, fun {j u} h => ?_⟩
    cases j with
    | zero => simp at h; subst h; exact hx
    | succ j => exact h2 (by simpa using h)

theorem Ty.sizeOf_lt_tcon {T : Name} {us : List Ty} {j : Nat} {u : Ty} (h : us[j]? = some u) :
    sizeOf u < sizeOf (Ty.tcon T us) := (Ty.sizeOf_lt_foldl).2 h

theorem Ty.sizeOf_lt_spine : ∀ {us : List Ty} {j : Nat} {u : Ty}, us[j]? = some u →
    sizeOf u < sizeOf (Ty.spine us)
  | [], _, _, h => by simp at h
  | x :: xs, 0, u, h => by simp at h; subst h; simp [Ty.spine]; omega
  | x :: xs, j + 1, u, h => by
    have ih := Ty.sizeOf_lt_spine (us := xs) (j := j) (u := u) (by simpa using h)
    simp only [Ty.spine, Ty.tapp.sizeOf_spec]
    omega

theorem Ty.sizeOf_lt_dict {c : Nat} {us : List Ty} {j : Nat} {u : Ty} (h : us[j]? = some u) :
    sizeOf u < sizeOf (Ty.dict c (Ty.spine us)) := by
  have := Ty.sizeOf_lt_spine h
  simp only [Ty.dict.sizeOf_spec]
  omega

/-- Composition of dictionary arguments by class parameter variance. -/
theorem LeLive.dict_compose {c : Nat} {us vs ws : List Ty}
    (ih : ∀ {m a b : Ty}, sizeOf m < sizeOf (Ty.dict c (Ty.spine vs)) → LeLive S a m →
      LeLive S m b → LeLive S a b)
    (hl : us.length = vs.length)
    (hp : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .pos → LeLive S u v)
    (hn : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .neg → LeLive S v u)
    (hnr : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .neg → Caps.Sub u.reach v.reach)
    (hi : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      (S.dictVar c j = .inv ∨ S.dictVar c j = .bi) → Ty.Equiv u v)
    (hl' : vs.length = ws.length)
    (hp' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w →
      S.dictVar c j = .pos → LeLive S v w)
    (hn' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w →
      S.dictVar c j = .neg → LeLive S w v)
    (hnr' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w →
      S.dictVar c j = .neg → Caps.Sub v.reach w.reach)
    (hi' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w →
      (S.dictVar c j = .inv ∨ S.dictVar c j = .bi) → Ty.Equiv v w) :
    LeLive S (.dict c (Ty.spine us)) (.dict c (Ty.spine ws)) := by
  refine .dict (hl.trans hl') (fun j u w h h' hv => ?_) (fun j u w h h' hv => ?_)
    (fun j u w h h' hv => ?_) (fun j u w h h' hv => ?_)
  all_goals
    obtain ⟨m, hm⟩ : ∃ m, vs[j]? = some m := by
      have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
      exact ⟨vs[j]'(hl ▸ this), by simp⟩
  · exact ih (Ty.sizeOf_lt_dict hm) (hp j u m h hm hv) (hp' j m w hm h' hv)
  · exact ih (Ty.sizeOf_lt_dict hm) (hn' j m w hm h' hv) (hn j u m h hm hv)
  · exact (hnr j u m h hm hv).trans (hnr' j m w hm h' hv)
  · exact (hi j u m h hm hv).trans (hi' j m w hm h' hv)

theorem LeLive.dict_trans {c : Nat} {us vs : List Ty} {b : Ty}
    (ih : ∀ {m a b : Ty}, sizeOf m < sizeOf (Ty.dict c (Ty.spine vs)) → LeLive S a m →
      LeLive S m b → LeLive S a b)
    (hl : us.length = vs.length)
    (hp : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .pos → LeLive S u v)
    (hn : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .neg → LeLive S v u)
    (hnr : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      S.dictVar c j = .neg → Caps.Sub u.reach v.reach)
    (hi : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v →
      (S.dictVar c j = .inv ∨ S.dictVar c j = .bi) → Ty.Equiv u v)
    (h₂ : LeLive S (.dict c (Ty.spine vs)) b) : LeLive S (.dict c (Ty.spine us)) b := by
  generalize hx : Ty.dict c (Ty.spine vs) = x at h₂
  cases h₂ with
  | refl => subst hx; exact .dict hl hp hn hnr hi
  | arrow | eff | rcons => cases hx
  | hide _ _ _ _ ha => exact absurd (hx.trans ha) Ty.dict_ne_tcon
  | data _ _ _ _ _ ha => exact absurd (hx.trans ha) Ty.dict_ne_tcon
  | dict hl' hp' hn' hnr' hi' =>
    injection hx with hc hsp
    subst hc
    have hvs : vs = _ := Ty.spine_inj hsp
    subst hvs
    exact LeLive.dict_compose ih hl hp hn hnr hi hl' hp' hn' hnr' hi'
  | equiv he =>
    rw [← hx] at he
    cases he.form with
    | rows hr _ => cases hr
    | dict hU =>
      obtain ⟨ws, rfl, hw⟩ := hU.spine_left
      exact LeLive.dict_compose ih hl hp hn hnr hi hw.length
        (fun j v w hv hw' _ => .equiv (hw.get j v w hv hw'))
        (fun j v w hv hw' _ => .equiv (hw.get j v w hv hw').symm)
        (fun j v w hv hw' _ => by rw [(hw.get j v w hv hw').reach]; exact Caps.Sub.refl _)
        (fun j v w hv hw' _ => hw.get j v w hv hw')

/-- Composition of nominal arguments, including equality at invariant parameters. -/
theorem LeLive.data_compose {T : Name} {us vs ws : List Ty}
    (ih : ∀ {m a c : Ty}, sizeOf m < sizeOf (Ty.tcon T vs) → LeLive S a m → LeLive S m c →
      LeLive S a c)
    (hl : us.length = vs.length)
    (hr : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v → Caps.Sub u.reach v.reach)
    (hp : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S u v)
    (hn : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S v u)
    (hi : ∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v → S.hidden T ≠ some j →
      S.var T j = .inv → Ty.Equiv u v)
    (hl' : vs.length = ws.length)
    (hr' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w → Caps.Sub v.reach w.reach)
    (hp' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S v w)
    (hn' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S w v)
    (hi' : ∀ (j : Nat) (v w : Ty), vs[j]? = some v → ws[j]? = some w → S.hidden T ≠ some j →
      S.var T j = .inv → Ty.Equiv v w) :
    LeLive S (Ty.tcon T us) (Ty.tcon T ws) := by
  refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
    (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
  all_goals
    obtain ⟨m, hm⟩ : ∃ m, vs[j]? = some m := by
      have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
      exact ⟨vs[j]'(hl ▸ this), by simp⟩
  · exact (hr j u m h hm).trans (hr' j m u' hm h')
  · exact ih (Ty.sizeOf_lt_tcon hm) (hp j u m h hm hj hv) (hp' j m u' hm h' hj hv)
  · exact ih (Ty.sizeOf_lt_tcon hm) (hn' j m u' hm h' hj hv) (hn j u m h hm hj hv)
  · exact (hi j u m h hm hj hv).trans (hi' j m u' hm h' hj hv)

/-- Two steps of data, argument by argument, given transitivity below the middle type. -/
theorem LeLive.data_trans {T : Name} {us vs : List Ty} {c : Ty}
    (ih : ∀ {m a c : Ty}, sizeOf m < sizeOf (Ty.tcon T vs) → LeLive S a m → LeLive S m c →
      LeLive S a c)
    (hl : us.length = vs.length)
    (hr : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → Caps.Sub u.reach u'.reach)
    (hp : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S u u')
    (hn : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S u' u)
    (hi : ∀ (j : Nat) (u u' : Ty), us[j]? = some u → vs[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .inv → Ty.Equiv u u')
    (h₂ : LeLive S (Ty.tcon T vs) c) : LeLive S (Ty.tcon T us) c := by
  generalize hx : Ty.tcon T vs = x at h₂
  cases h₂ with
  | refl => subst hx; exact .data hl hr hp hn hi rfl rfl
  | arrow => exact absurd hx.symm Ty.arrow_ne_tcon
  | eff => exact absurd hx.symm Ty.eff_ne_tcon
  | rcons => exact absurd hx.symm Ty.rcons_ne_tcon
  | dict => exact absurd hx.symm Ty.dict_ne_tcon
  | hide hi' hl' he' hr' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    refine .data (hl.trans hl') (fun j u u' h h' => ?_) (fun j u u' h h' hj hv => ?_)
      (fun j u u' h h' hj hv => ?_) (fun j u u' h h' hj hv => ?_) rfl rfl
    all_goals
      obtain ⟨m, hm⟩ : ∃ m, vs[j]? = some m := by
        have : j < us.length := (List.getElem?_eq_some_iff.mp h).1
        exact ⟨vs[j]'(hl ▸ this), by simp⟩
    · exact (hr j u m h hm).trans (LeLive.hide_args he' hr' j m u' hm h')
    all_goals
      have hji : j ≠ _ := fun e => hj (e ▸ hi')
      have hv' : vs[j]? = some u' := (he' j hji).trans h'
    · exact hp j u u' h hv' hj hv
    · exact hn j u u' h hv' hj hv
    · exact hi j u u' h hv' hj hv
  | data hl' hr' hp' hn' hi' ha hb =>
    subst hb
    obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (hx.trans ha)
    exact LeLive.data_compose ih hl hr hp hn hi hl' hr' hp' hn' hi'
  | equiv he =>
    rw [← hx] at he
    obtain ⟨ws, rfl, hw⟩ := he.tcon_left
    exact LeLive.data_compose ih hl hr hp hn hi hw.length
      (fun j v w hv hw' => by rw [(hw.get j v w hv hw').reach]; exact Caps.Sub.refl _)
      (fun j v w hv hw' _ _ => .equiv (hw.get j v w hv hw'))
      (fun j v w hv hw' _ _ => .equiv (hw.get j v w hv hw').symm)
      (fun j v w hv hw' _ _ => hw.get j v w hv hw')

end Flick
