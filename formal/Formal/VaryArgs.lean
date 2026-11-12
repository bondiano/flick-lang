import Formal.Vary

/-!
# Varying the arguments of data

`Sig.Vary` is what `LeLive.data` asks of the arguments of data (regions.md §2.2.1). A
parameter that may change is an ordinary variable of every constructor, or one a constructor
fixes to a type subtyping cannot move (A2, `Ty.Closed`), so a constructor that builds data
builds every variation of it (`Sig.builds_vary`).
-/

namespace Flick

variable {S : Sig}

/-- `us'` varies `us` as subtyping of data allows (regions.md §2.2.1, `LeLive.data`): each
argument keeps what it reaches, and one that is not the hidden row moves by the variance of
its parameter. -/
def Sig.Vary (S : Sig) (T : Name) (us us' : List Ty) : Prop :=
  us.length = us'.length ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach) ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .pos → LeLive S u u') ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .neg → LeLive S u' u) ∧
  (∀ (j : Nat) (u u' : Ty), us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
    S.var T j = .inv → u = u')

/-- Arguments of the same position. -/
theorem List.getElem?_pair {us us' : List Ty} {j : Nat} {u : Ty} (hl : us.length = us'.length)
    (h : us[j]? = some u) : ∃ u', us'[j]? = some u' :=
  ⟨_, List.getElem?_eq_getElem (hl ▸ (List.getElem?_eq_some_iff.mp h).1)⟩

/-- Equal positions vary as nothing. -/
theorem Sig.Vary.of_agree {T : Name} {us us' : List Ty} (hl : us.length = us'.length)
    (P : Nat → Prop) (hP : ∀ j, ¬ P j → us[j]? = us'[j]?)
    (hr : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → Caps.Sub u.reach u'.reach)
    (hp : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .pos → LeLive S u u')
    (hn : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .neg → LeLive S u' u)
    (hi : ∀ j u u', P j → us[j]? = some u → us'[j]? = some u' → S.hidden T ≠ some j →
      S.var T j = .inv → u = u') : S.Vary T us us' := by
  refine ⟨hl, fun j u u' h h' => ?_, fun j u u' h h' hh hv => ?_, fun j u u' h h' hh hv => ?_,
    fun j u u' h h' hh hv => ?_⟩
  all_goals by_cases hj : P j
  any_goals rw [hP j hj, h'] at h; cases h
  · exact hr j u u' hj h h'
  · exact Caps.Sub.refl _
  · exact hp j u u' hj h h' hh hv
  · exact .refl
  · exact hn j u u' hj h h' hh hv
  · exact .refl
  · exact hi j u u' hj h h' hh hv
  · rfl

theorem Sig.Vary.refl {T : Name} {us : List Ty} : S.Vary T us us :=
  .of_agree rfl (fun _ => False) (fun _ _ => rfl) (fun _ _ _ h => h.elim)
    (fun _ _ _ h => h.elim) (fun _ _ _ h => h.elim) (fun _ _ _ h => h.elim)

theorem Sig.Vary.length {T : Name} {us us' : List Ty} (h : S.Vary T us us') :
    us.length = us'.length := h.1

theorem Sig.Vary.nil {T : Name} {ws : List Ty} (h : S.Vary T [] ws) : ws = [] :=
  List.eq_nil_of_length_eq_zero h.length.symm

theorem Sig.Vary.trans {T : Name} {us vs ws : List Ty} (h₁ : S.Vary T us vs)
    (h₂ : S.Vary T vs ws) : S.Vary T us ws := by
  obtain ⟨l₁, r₁, p₁, n₁, i₁⟩ := h₁
  obtain ⟨l₂, r₂, p₂, n₂, i₂⟩ := h₂
  refine ⟨l₁.trans l₂, fun j u w h h' => ?_, fun j u w h h' hh hv => ?_,
    fun j u w h h' hh hv => ?_, fun j u w h h' hh hv => ?_⟩
  all_goals obtain ⟨v, hv'⟩ := List.getElem?_pair l₁ h
  · exact (r₁ j u v h hv').trans (r₂ j v w hv' h')
  · exact (p₁ j u v h hv' hh hv).trans (p₂ j v w hv' h' hh hv)
  · exact (n₂ j v w hv' h' hh hv).trans (n₁ j u v h hv' hh hv)
  · exact (i₁ j u v h hv' hh hv).trans (i₂ j v w hv' h' hh hv)

/-- Widening the hidden row is a variation. -/
theorem Sig.Vary.of_hide {T : Name} {i : Nat} {us us' : List Ty} (hi : S.hidden T = some i)
    (hl : us.length = us'.length) (heq : ∀ j, j ≠ i → us[j]? = us'[j]?)
    (hsub : ∀ r r', us[i]? = some r → us'[i]? = some r' → Caps.Sub r.reach r'.reach) :
    S.Vary T us us' :=
  .of_agree hl (· = i) heq (fun j u u' hj h h' => by subst hj; exact hsub u u' h h')
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)
    (fun j _ _ hj _ _ hh => by subst hj; exact absurd hi hh)

/-- A parameter that may change: not invariant, or the hidden row. -/
def Sig.Varies (S : Sig) (T : Name) (j : Nat) : Prop := S.var T j ≠ .inv ∨ S.hidden T = some j

theorem Ty.tcon_cons_tapp (T : Name) (x : Ty) (xs : List Ty) :
    ∃ g b, Ty.tcon T (x :: xs) = .tapp g b := by
  have : ∀ (xs : List Ty) (acc : Ty), (∃ g b, acc = Ty.tapp g b) →
      ∃ g b, xs.foldl Ty.tapp acc = Ty.tapp g b := by
    intro xs; induction xs with
    | nil => intro acc h; exact h
    | cons y ys ih => intro acc _; exact ih _ ⟨_, _, rfl⟩
  exact this xs (.tapp (.adt T) x) ⟨_, _, rfl⟩

theorem Ty.tcon_shape (T : Name) (us : List Ty) :
    Ty.tcon T us = .adt T ∨ ∃ g b, Ty.tcon T us = .tapp g b := by
  cases us with
  | nil => exact .inl rfl
  | cons x xs => exact .inr (Ty.tcon_cons_tapp T x xs)

theorem Ty.Closed.substT {c : Ty} (h : c.Closed) (σ : Atom → Ty) : c.substT σ = c := by
  cases c <;> simp_all [Ty.Closed, Ty.substT]

theorem Ty.Closed.noV {c : Ty} (h : c.Closed) (i : Nat) : c.NoV i := h.substT _

theorem Ty.Closed.tcon {T : Name} {us : List Ty} (h : (Ty.tcon T us).Closed) : us = [] := by
  cases us with
  | nil => rfl
  | cons x xs =>
    obtain ⟨g, b, e⟩ := Ty.tcon_cons_tapp T x xs
    rw [e] at h; exact h.elim

/-- Subtyping cannot move a closed type up. -/
theorem Ty.Closed.le_eq {c u : Ty} (h : c.Closed) (hl : LeLive S c u) : u = c := by
  cases hl with
  | refl => rfl
  | arrow => exact h.elim
  | eff => exact h.elim
  | hide _ hl _ _ ha hb | data hl _ _ _ _ ha hb =>
    subst ha; subst hb
    obtain rfl := h.tcon
    rw [List.eq_nil_of_length_eq_zero hl.symm]

/-- Subtyping cannot move a closed type down. -/
theorem Ty.Closed.ge_eq {c u : Ty} (h : c.Closed) (hl : LeLive S u c) : u = c := by
  cases hl with
  | refl => rfl
  | arrow => exact h.elim
  | eff => exact h.elim
  | hide _ hl _ _ ha hb | data hl _ _ _ _ ha hb =>
    subst ha; subst hb
    obtain rfl := h.tcon
    rw [List.eq_nil_of_length_eq_zero hl]

/-- What a substitution sends to a closed type is a variable or the type itself, and
subtyping moves neither. -/
theorem Ty.Closed.inst_eq {c u u' : Ty} {σ : Atom → Ty} (h : c.Closed) (e : u.substT σ = c)
    (hl : LeLive S u u' ∨ LeLive S u' u) : u' = u := by
  have atom : ∀ {x : Ty}, (∀ T us, x ≠ Ty.tcon T us) → (∀ a φ b C, x ≠ .arrow a φ b C) →
      (∀ r e t C, x ≠ .eff r e t C) → (LeLive S x u' ∨ LeLive S u' x) → u' = x :=
    fun ht ha he hl => by
      rcases hl with hl | hl
      · cases hl with
        | refl => rfl
        | arrow => exact absurd rfl (ha _ _ _ _)
        | eff => exact absurd rfl (he _ _ _ _)
        | hide _ _ _ _ hx _ | data _ _ _ _ _ hx _ => exact absurd hx (ht _ _)
      · cases hl with
        | refl => rfl
        | arrow => exact absurd rfl (ha _ _ _ _)
        | eff => exact absurd rfl (he _ _ _ _)
        | hide _ _ _ _ _ hx | data _ _ _ _ _ _ hx => exact absurd hx (ht _ _)
  have nt : ∀ {x : Ty}, (∀ g b, x ≠ .tapp g b) → (∀ T, x ≠ .adt T) → ∀ T us, x ≠ Ty.tcon T us :=
    fun hg hd T us hx => by
      rcases Ty.tcon_shape T us with e' | ⟨g, b, e'⟩
      · exact hd T (hx.trans e')
      · exact hg g b (hx.trans e')
  cases u with
  | tvar | rg =>
    exact atom (nt (fun _ _ h => by cases h) (fun _ h => by cases h)) (fun _ _ _ _ h => by cases h)
      (fun _ _ _ _ h => by cases h) hl
  | base | adt =>
    simp only [Ty.substT] at e; subst e
    rcases hl with hl | hl
    · exact h.le_eq hl
    · exact h.ge_eq hl
  | _ => subst e; simp [Ty.substT, Ty.Closed] at h

/-- A parameter that may change is fixed by nothing else a constructor builds. Either it is
an ordinary variable of the constructor, or (A2) the constructor fixes it to a closed type,
does not use it, and it is neither the hidden row nor unused. -/
theorem Sig.varies_ok {T : Name} {j : Nat} (h : S.Varies T j) {K : Name} {k : Nat}
    {ts : List Ty} (hK : S.ctor K = some (k, ts, T)) :
    (∀ m u, (S.res K)[m]? = some u → m ≠ j → u.NoV j) ∧
      ((j < k ∧ (S.res K)[j]? = some (.tvar j)) ∨
        (S.hidden T ≠ some j ∧ (S.var T j = .pos ∨ S.var T j = .neg) ∧
          (∃ c, (S.res K)[j]? = some c ∧ c.Closed) ∧ ∀ f ∈ ts, f.NoV j)) := by
  by_cases hh : S.hidden T = some j
  · obtain ⟨h1, h2, h3, -⟩ := S.hidden_ok T j hh K k ts hK; exact ⟨h3, .inl ⟨h1, h2⟩⟩
  · rcases h with h | h
    · obtain ⟨h3, h4⟩ := S.var_ok T j h K k ts hK
      refine ⟨h3, ?_⟩
      rcases h4 with ⟨h1, h2, -⟩ | ⟨hb, hc, hf⟩
      · exact .inl ⟨h1, h2⟩
      · refine .inr ⟨hh, ?_, hc, hf⟩
        cases hv : S.var T j with
        | pos => exact .inl rfl
        | neg => exact .inr rfl
        | inv => exact absurd hv h
        | bi => exact absurd hv hb
    · exact absurd h hh

/-- A parameter that may change and stands as a variable of a constructor is one of its
variables. -/
theorem Sig.varies_var {T : Name} {j : Nat} (h : S.Varies T j) {K : Name} {k : Nat}
    {ts : List Ty} (hK : S.ctor K = some (k, ts, T)) (hr : (S.res K)[j]? = some (.tvar j)) :
    j < k := by
  rcases (Sig.varies_ok h hK).2 with ⟨h1, -⟩ | ⟨-, -, ⟨c, hc, hcl⟩, -⟩
  · exact h1
  · rw [hr] at hc; cases hc; exact hcl.elim

/-- The arguments at a parameter fixed to a closed type do not vary. -/
theorem Sig.Vary.eq_of_closed {T : Name} {us us' : List Ty} (hw : S.Vary T us us') {j : Nat}
    (hh : S.hidden T ≠ some j) (hv : S.var T j = .pos ∨ S.var T j = .neg) {c u u' : Ty}
    {σ : Atom → Ty} (hc : c.Closed) (hu : us[j]? = some u) (hu' : us'[j]? = some u')
    (e : u.substT σ = c) : u' = u := by
  refine hc.inst_eq (S := S) e ?_
  rcases hv with hv | hv
  · exact .inl (hw.2.2.1 j u u' hu hu' hh hv)
  · exact .inr (hw.2.2.2.1 j u u' hu hu' hh hv)

theorem Sig.Vary.eq_of_not_varies {T : Name} {us us' : List Ty} (hw : S.Vary T us us')
    {j : Nat} (hj : ¬ S.Varies T j) : us[j]? = us'[j]? := by
  have hinv : S.var T j = .inv := by
    exact Classical.byContradiction fun h => hj (.inl h)
  have hh : S.hidden T ≠ some j := fun h => hj (.inr h)
  cases hu : us[j]? with
  | none =>
    cases hu' : us'[j]? with
    | none => rfl
    | some u' =>
      obtain ⟨u, hu₀⟩ := List.getElem?_pair hw.length.symm hu'
      rw [hu] at hu₀; cases hu₀
  | some u =>
    obtain ⟨u', hu'⟩ := List.getElem?_pair hw.length hu
    rw [hu', hw.2.2.2.2 j u u' hu hu' hh hinv]

theorem Ty.tcon_up_substT (T : Name) (us : List Ty) (k : Nat) (ρ : Atom → Ty) :
    ((Ty.tcon T us).up k).substT ρ = Ty.tcon T (us.map fun u => (u.up k).substT ρ) := by
  simp only [Ty.up, Ty.substT_ren, Ty.tcon_substT]

/-- A constructor that builds data builds every variation of it: what may change is a
variable of what it builds or fixed to a closed type, and nothing else there depends on it. -/
theorem Sig.builds_vary {K T : Name} {us us' : List Ty} (hw : S.Vary T us us')
    (h : S.Builds K (Ty.tcon T us)) : S.Builds K (Ty.tcon T us') := by
  classical
  intro k ts T₁ us₁ hK ht hlen
  obtain ⟨rfl, rfl⟩ := Ty.tcon_eq ht
  obtain ⟨ρ, hρ⟩ := h k ts T us hK rfl (hw.length.trans hlen)
  let ρ' : Atom → Ty := fun a => match a with
    | .v j => if hj : (S.Varies T j ∧ (S.res K)[j]? = some (.tvar j)) ∧ j < us'.length then
        ((us'[j]'hj.2).up k).substT ρ else ρ (.v j)
    | .r p => ρ (.r p)
  have hhi : ∀ j, k ≤ j → ρ' (.v j) = ρ (.v j) := fun j hj => by
    simp only [ρ']
    split
    · rename_i h; exact absurd (Sig.varies_var h.1.1 hK h.1.2) (by omega)
    · rfl
  have hup : ∀ u : Ty, (u.up k).substT ρ' = (u.up k).substT ρ := fun u => by
    simp only [Ty.up, Ty.substT_ren]
    apply Ty.substT_congr
    intro a
    cases ha : a.up k with
    | v j =>
      have : k ≤ j := Classical.byContradiction fun hj =>
        Atom.up_ne (a := a) (by omega : j < k) ha
      exact hhi j this
    | r p => rfl
  refine ⟨ρ', ?_⟩
  rw [Ty.tcon_up_substT] at hρ ⊢
  rw [Ty.tcon_substT] at hρ ⊢
  obtain ⟨-, hρl⟩ := Ty.tcon_eq hρ
  simp only [hup]
  congr 1
  apply List.ext_getElem?
  intro n
  simp only [List.getElem?_map]
  by_cases hn : S.Varies T n ∧ (S.res K)[n]? = some (.tvar n)
  · have hnl : n < us'.length := by rw [hlen]; exact (List.getElem?_eq_some_iff.mp hn.2).1
    rw [hn.2, List.getElem?_eq_getElem hnl]
    simp [Ty.substT, ρ', hn, hnl]
  · have e := congrArg (·[n]?) hρl
    simp only [List.getElem?_map] at e
    cases hrn : (S.res K)[n]? with
    | none =>
      rw [hrn] at e
      have : ¬ n < us'.length := fun h' => by
        rw [hlen] at h'; rw [List.getElem?_eq_getElem h'] at hrn; cases hrn
      rw [List.getElem?_eq_none (Nat.le_of_not_lt this)]; rfl
    | some u =>
      rw [hrn] at e
      have hu_same : u.substT ρ' = u.substT ρ := by
        refine Ty.substT_eq_upto k (fun a ha => ?_) (fun j hj hne => ?_)
        · cases a with
          | v j => exact hhi j (Classical.byContradiction fun h => ha j (by omega) rfl)
          | r p => rfl
        · have hvj : S.Varies T j ∧ (S.res K)[j]? = some (.tvar j) :=
            Classical.byContradiction fun hv => hne (by simp [ρ', hv])
          exact (Sig.varies_ok hvj.1 hK).1 n u hrn fun e => hn (e ▸ hvj)
      have hlt : n < us.length := by
        rw [hw.length, hlen]; exact (List.getElem?_eq_some_iff.mp hrn).1
      have hlt' : n < us'.length := hw.length ▸ hlt
      have hus : us'[n] = us[n] := by
        by_cases hv : S.Varies T n
        · rcases (Sig.varies_ok hv hK).2 with ⟨-, h2⟩ | ⟨hh, hpn, ⟨c, hc, hcl⟩, -⟩
          · exact absurd ⟨hv, h2⟩ hn
          · rw [hrn] at hc; cases hc
            rw [List.getElem?_eq_getElem hlt] at e
            simp only [Option.map_some, Option.some.injEq] at e
            rw [hcl.substT, Ty.up, Ty.substT_ren] at e
            exact hw.eq_of_closed hh hpn hcl (List.getElem?_eq_getElem hlt)
              (List.getElem?_eq_getElem hlt') e.symm
        · have e' := hw.eq_of_not_varies hv
          rw [List.getElem?_eq_getElem hlt, List.getElem?_eq_getElem hlt'] at e'
          exact (Option.some.inj e').symm
      rw [List.getElem?_eq_getElem hlt'] at ⊢
      rw [List.getElem?_eq_getElem hlt] at e
      simp only [Option.map_some, Option.some.injEq] at e ⊢
      rw [hu_same, e, hus]

end Flick
