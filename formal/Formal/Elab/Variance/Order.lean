import Formal.Elab.Variance.Spine

/-!
# Order laws for `RowLe`, `Stage.UseLe`, `Ty.Le`, and `Cn.Serves` (`Formal.Elab.Variance.Order`)
-/

namespace Flick.Elab

/-! ### `RowLe`, `Stage.UseLe`, `Ty.Le`, and `Cn.Serves` laws -/

theorem RowLe.keeps_mono {r₁ r₂ : Ty} (h : RowLe r₁ r₂) {n : Nat} (hk : r₁.Keeps n) :
    r₂.Keeps n := by
  induction h with
  | refl => exact hk
  | cnil => exact hk.elim
  | runL _ _ ih₁ ih₂ => exact hk.elim ih₁ ih₂
  | runRL _ ih => exact .inl (ih hk)
  | runRR _ ih => exact .inr (ih hk)
  | keeps hq => exact hq n (hk.elim id False.elim)

private theorem RowLe.runL_inv {r₁ r₂ r : Ty} (h : RowLe (.run r₁ r₂) r) :
    RowLe r₁ r ∧ RowLe r₂ r := by
  generalize hx : Ty.run r₁ r₂ = x at h
  induction h with
  | refl => subst hx; exact ⟨.runRL .refl, .runRR .refl⟩
  | cnil => cases hx
  | runL h₁ h₂ => cases hx; exact ⟨h₁, h₂⟩
  | runRL _ ih => obtain ⟨a, b⟩ := ih hx; exact ⟨.runRL a, .runRL b⟩
  | runRR _ ih => obtain ⟨a, b⟩ := ih hx; exact ⟨.runRR a, .runRR b⟩
  | keeps => cases hx

theorem RowLe.trans {r₁ r₂ r₃ : Ty} (h₁ : RowLe r₁ r₂) (h₂ : RowLe r₂ r₃) : RowLe r₁ r₃ := by
  induction h₁ generalizing r₃ with
  | refl => exact h₂
  | cnil => exact .cnil
  | runL _ _ ih₁ ih₂ => exact .runL (ih₁ h₂) (ih₂ h₂)
  | runRL _ ih => exact ih (RowLe.runL_inv h₂).1
  | runRR _ ih => exact ih (RowLe.runL_inv h₂).2
  | keeps hq => exact .keeps fun n hn => h₂.keeps_mono (hq n hn)

theorem RowLe.subst (σ : Nat → Ty) {r₁ r₂ : Ty} (h : RowLe r₁ r₂) :
    RowLe (r₁.subst σ) (r₂.subst σ) := by
  induction h with
  | refl => exact .refl
  | cnil => exact .cnil
  | runL _ _ ih₁ ih₂ => exact .runL ih₁ ih₂
  | runRL _ ih => exact .runRL ih
  | runRR _ ih => exact .runRR ih
  | keeps hq =>
    refine .keeps fun m hm => ?_
    obtain ⟨n, hn, hσ⟩ := (Ty.shown_subst_iff σ _ m).1 hm
    exact (hq n hn).subst hσ

theorem Stage.UseLe.refl (φ : Stage) (q : Ty) : Stage.UseLe φ q φ q := by
  cases φ with
  | P => trivial
  | R r e => exact ⟨.refl, rfl, fun _ h => .inl h⟩

theorem Stage.UseLe.trans {φ₁ φ₂ φ₃ : Stage} {q₁ q₂ q₃ : Ty}
    (h₁ : Stage.UseLe φ₁ q₁ φ₂ q₂) (h₂ : Stage.UseLe φ₂ q₂ φ₃ q₃) :
    Stage.UseLe φ₁ q₁ φ₃ q₃ := by
  cases φ₁ with
  | P => trivial
  | R r₁ e₁ =>
    cases φ₂ with
    | P => exact h₁.elim
    | R r₂ e₂ =>
      cases φ₃ with
      | P => exact h₂.elim
      | R r₃ e₃ =>
        obtain ⟨hr₁, he₁, hq₁⟩ := h₁
        obtain ⟨hr₂, he₂, hq₂⟩ := h₂
        refine ⟨hr₁.trans hr₂, he₁.trans he₂, fun n hn => ?_⟩
        rcases hq₁ n hn with h | h
        · exact hq₂ n h
        · exact .inr (hr₂.keeps_mono h)

theorem Stage.UseLe.subst (σ : Nat → Ty) {φ₁ φ₂ : Stage} {q₁ q₂ : Ty}
    (h : Stage.UseLe φ₁ q₁ φ₂ q₂) :
    Stage.UseLe (φ₁.subst σ) (q₁.subst σ) (φ₂.subst σ) (q₂.subst σ) := by
  cases φ₁ with
  | P => trivial
  | R r₁ e₁ =>
    cases φ₂ with
    | P => exact h.elim
    | R r₂ e₂ =>
      obtain ⟨hr, he, hq⟩ := h
      refine ⟨hr.subst σ, congrArg (Ty.subst σ) he, fun m hm => ?_⟩
      obtain ⟨n, hn, hσ⟩ := (Ty.shown_subst_iff σ q₁ m).1 hm
      rcases hq n hn with h' | h'
      · exact .inl ((Ty.shown_subst_iff σ q₂ m).2 ⟨n, h', hσ⟩)
      · exact .inr (h'.subst hσ)

theorem Ty.Le.shown {dvar : Nat → Nat → Variance} {a b : Ty} (h : Ty.Le dvar a b) :
    ∀ n, a.Shown n → b.Shown n := by
  cases h with
  | refl => intro _ hn; exact hn
  | arr _ _ _ hsh => exact hsh
  | data _ _ hl hsh _ _ _ =>
    intro n hn
    obtain ⟨j, u, hu, hnu⟩ := Ty.shown_tcon_iff.mp hn
    have hj : j < _ := hl ▸ (List.getElem?_eq_some_iff.mp hu).1
    exact Ty.shown_tcon_iff.mpr ⟨j, _, List.getElem?_eq_getElem hj,
      hsh j u _ hu (List.getElem?_eq_getElem hj) n hnu⟩

theorem Ty.Le.trans_lt {dvar : Nat → Nat → Variance} (n : Nat) :
    ∀ {b a c : Ty}, sizeOf b < n → Ty.Le dvar a b → Ty.Le dvar b c → Ty.Le dvar a c := by
  induction n with
  | zero => intro _ _ _ hn; exact absurd hn (Nat.not_lt_zero _)
  | succ n ihn =>
    intro b a c hn h₁ h₂
    have ih : ∀ {m x y : Ty}, sizeOf m < sizeOf b → Ty.Le dvar x m → Ty.Le dvar m y →
        Ty.Le dvar x y := fun hm => ihn (by omega)
    cases h₁ with
    | refl => exact h₂
    | @arr φ₁ φ₂ a₁ a₂ b₁ b₂ q₁ q₂ ha₁ hφ₁ hb₁ hsh₁ =>
      generalize hX : Ty.arr φ₂ a₂ b₂ q₂ = X at h₂
      cases h₂ with
      | refl => subst hX; exact .arr ha₁ hφ₁ hb₁ hsh₁
      | @arr _ φ₃ _ a₃ _ b₃ _ q₃ ha₂ hφ₂ hb₂ hsh₂ =>
        cases hX
        exact .arr (ih (by simp; omega) ha₂ ha₁) (hφ₁.trans hφ₂) (ih (by simp; omega) hb₁ hb₂)
          (fun m hm => hsh₂ m (hsh₁ m hm))
      | data => exact absurd hX Ty.arr_ne_tcon
    | @data T us vs h2 h4 hl₁ hsh₁ hp₁ hn₁ hi₁ =>
      generalize hX : Ty.tcon T vs = X at h₂
      cases h₂ with
      | refl => subst hX; exact .data h2 h4 hl₁ hsh₁ hp₁ hn₁ hi₁
      | arr => exact absurd hX.symm Ty.arr_ne_tcon
      | @data T' vs' ws _ _ hl₂ hsh₂ hp₂ hn₂ hi₂ =>
        obtain ⟨rfl, rfl⟩ := Ty.tcon_eq hX
        refine .data h2 h4 (hl₁.trans hl₂) (fun j u w hu hw m hm => ?_)
          (fun j u w hu hw hv => ?_) (fun j u w hu hw hv => ?_) (fun j u w hu hw hv => ?_)
        all_goals
          have hj : j < vs.length := hl₁ ▸ (List.getElem?_eq_some_iff.mp hu).1
          have hv' : vs[j]? = some vs[j] := List.getElem?_eq_getElem hj
        · exact hsh₂ j _ w hv' hw m (hsh₁ j u _ hu hv' m hm)
        · exact ih (Ty.sizeOf_lt_tcon hv') (hp₁ j u _ hu hv' hv) (hp₂ j _ w hv' hw hv)
        · exact ih (Ty.sizeOf_lt_tcon hv') (hn₂ j _ w hv' hw hv) (hn₁ j u _ hu hv' hv)
        · exact (hi₁ j u _ hu hv' hv).trans (hi₂ j _ w hv' hw hv)

theorem Ty.Le.trans {dvar : Nat → Nat → Variance} {a b c : Ty}
    (h₁ : Ty.Le dvar a b) (h₂ : Ty.Le dvar b c) : Ty.Le dvar a c :=
  Ty.Le.trans_lt (sizeOf b + 1) (by omega) h₁ h₂

theorem Ty.Le.subst {dvar : Nat → Nat → Variance} (σ : Nat → Ty) {a b : Ty}
    (h : Ty.Le dvar a b) : Ty.Le dvar (a.subst σ) (b.subst σ) := by
  have sh : ∀ {x y : Ty}, (∀ n, x.Shown n → y.Shown n) →
      ∀ m, (x.subst σ).Shown m → (y.subst σ).Shown m := fun {x y} hxy m hm => by
    obtain ⟨n, hn, hσ⟩ := (Ty.shown_subst_iff σ x m).1 hm
    exact (Ty.shown_subst_iff σ y m).2 ⟨n, hxy n hn, hσ⟩
  induction h with
  | refl => exact .refl
  | arr _ hφ _ hsh iha ihb =>
    exact .arr iha (hφ.subst σ) ihb (sh hsh)
  | data h2 h4 hl hsh _ _ hi ihp ihn =>
    rw [Ty.tcon_subst, Ty.tcon_subst]
    refine .data h2 h4 (by simpa using hl) (fun j x y hx hy => ?_)
      (fun j x y hx hy hv => ?_) (fun j x y hx hy hv => ?_) (fun j x y hx hy hv => ?_)
    all_goals
      simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hy
      obtain ⟨u, hu, rfl⟩ := hx
      obtain ⟨v, hv', rfl⟩ := hy
    · exact sh (hsh j u v hu hv')
    · exact ihp j u v hu hv' hv
    · exact ihn j u v hu hv' hv
    · rw [hi j u v hu hv' hv]

theorem Cn.Serves.refl {P : Prog} (Q : Cn) : Cn.Serves P Q Q :=
  ⟨rfl, rfl,
    fun _ q h hq hh _ => by rw [hq] at hh; cases hh; exact .refl,
    fun _ q h hq hh _ => by rw [hq] at hh; cases hh; exact ⟨.refl, fun _ hn => hn⟩,
    fun _ q h hq hh _ => by rw [hq] at hh; cases hh; rfl⟩

theorem Cn.Serves.of_eq {P : Prog} {Q H : Cn} (h : H = Q) : Cn.Serves P Q H := by
  cases h; exact Cn.Serves.refl _

theorem Cn.Serves.trans {P : Prog} {Q M H : Cn}
    (h₁ : Cn.Serves P Q M) (h₂ : Cn.Serves P M H) : Cn.Serves P Q H := by
  obtain ⟨c₁, l₁, p₁, n₁, i₁⟩ := h₁
  obtain ⟨c₂, l₂, p₂, n₂, i₂⟩ := h₂
  refine ⟨c₂.trans c₁, l₁.trans l₂, fun j q h hq hh hv => ?_,
    fun j q h hq hh hv => ?_, fun j q h hq hh hv => ?_⟩
  all_goals
    have hj : j < M.args.length := l₁ ▸ (List.getElem?_eq_some_iff.mp hq).1
    have hm : M.args[j]? = some M.args[j] := List.getElem?_eq_getElem hj
  · exact (p₂ j _ h hm hh (c₁.symm ▸ hv)).trans (p₁ j q _ hq hm hv)
  · obtain ⟨le₁, sh₁⟩ := n₁ j q _ hq hm hv
    obtain ⟨le₂, sh₂⟩ := n₂ j _ h hm hh (c₁.symm ▸ hv)
    exact ⟨le₁.trans le₂, fun k hk => sh₁ k (sh₂ k hk)⟩
  · exact (i₁ j q _ hq hm hv).trans (i₂ j _ h hm hh (c₁.symm ▸ hv))

theorem Cn.Serves.subst {P : Prog} (σ : Nat → Ty) {Q H : Cn} (h : Cn.Serves P Q H) :
    Cn.Serves P (Q.subst σ) (H.subst σ) := by
  obtain ⟨hc, hl, hp, hn, hi⟩ := h
  refine ⟨hc, by simpa [Cn.subst] using hl, fun j x y hx hy hv => ?_,
    fun j x y hx hy hv => ?_, fun j x y hx hy hv => ?_⟩
  all_goals
    simp only [Cn.subst, List.getElem?_map, Option.map_eq_some_iff] at hx hy
    obtain ⟨q, hq, rfl⟩ := hx
    obtain ⟨h', hh, rfl⟩ := hy
  · exact (hp j q h' hq hh hv).subst σ
  · obtain ⟨hle, hsh⟩ := hn j q h' hq hh hv
    refine ⟨hle.subst σ, fun m hm => ?_⟩
    obtain ⟨k, hk, hσ⟩ := (Ty.shown_subst_iff σ h' m).1 hm
    exact (Ty.shown_subst_iff σ q m).2 ⟨k, hsh k hk, hσ⟩
  · rw [hi j q h' hq hh hv]

theorem Cn.Serves.eq_of_inv {P : Prog} {Q H : Cn}
    (hinv : ∀ j, P.clsVar Q.cls j = .inv ∨ P.clsVar Q.cls j = .bi)
    (h : Cn.Serves P Q H) : Q = H := by
  obtain ⟨hc, hl, _, _, hi⟩ := h
  cases Q with
  | mk c₁ as₁ =>
    cases H with
    | mk c₂ as₂ =>
      simp only at hc hl hi hinv
      subst hc
      congr 1
      apply List.ext_getElem?
      intro j
      by_cases hj : j < as₁.length
      · have hj₂ : j < as₂.length := hl ▸ hj
        rw [List.getElem?_eq_getElem hj, List.getElem?_eq_getElem hj₂]
        exact congrArg some (hi j _ _ (List.getElem?_eq_getElem hj) (List.getElem?_eq_getElem hj₂) (hinv j))
      · have hj₂ : ¬ j < as₂.length := hl ▸ hj
        rw [List.getElem?_eq_none (by omega), List.getElem?_eq_none (by omega)]

/-- Backward-compatible exact-match (Inst) constructor. -/
theorem Ent.inst_eq {P : Prog} {Δ : List Cn} {Q : Cn} {I : Nat} {ins : Inst} {σ : Nat → Ty}
    {ds : List Dict} (hI : P.insts I = some ins) (hcls : ins.cls = Q.cls)
    (hargs : Q.args = ins.head.map (·.subst σ)) (hds : Ent P Δ (ins.ctx.map (·.subst σ)) ds) :
    Ent P Δ [Q] [apps (.inst I) ds] :=
  .inst hI (.of_eq (by cases Q; simp_all)) hds

/-- Explicit B1 variance-directed (Inst) transport: an instance serving `Q` in the class's
variance direction entails `Q` with the instance's dictionary. -/
theorem Ent.inst_le {P : Prog} {Δ : List Cn} {Q : Cn} {I : Nat} {ins : Inst} {σ : Nat → Ty}
    {ds : List Dict} (hI : P.insts I = some ins)
    (hserv : Cn.Serves P Q ⟨ins.cls, ins.head.map (·.subst σ)⟩)
    (hds : Ent P Δ (ins.ctx.map (·.subst σ)) ds) :
    Ent P Δ [Q] [apps (.inst I) ds] :=
  .inst hI hserv hds

end Flick.Elab
