import Formal.SubAdm

/-!
# A typed value holds no more than its type shows (L4, syntactic part)

`held Ψ Γ v ⊆ ⌈τ⌉` for a value `v : τ`: its free variables, the regions it writes
down and the cell types of its locations are all within `⌈τ⌉` (regions.md §5.2).
The cases are those of L4: (Lam), (Task) and the fixpoint by their premise;
(Cap), (Own) and a location by their type; a constructor spine because its fields
mention only its parameters (`Sig.ctor_reach`); a partial application of a
constant because each constant shows its first argument in the rest of its type;
(≤-Cap) by `Le.reach`. The substitution lemma uses it to keep the premise of (Lam).
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

theorem Sig.impl_reach (S : Sig) (N : Name) (a : Atom) : ¬ (S.impl N).reach a := by
  intro h
  rw [← S.impl_closed N (fun _ => .base .unit), Ty.reach_substT] at h
  obtain ⟨_, _, h⟩ := h
  exact h


/-- The result after all stages of an arrow. -/
def Ty.res : Ty → Ty
  | .arrow _ _ t _ => t.res
  | t => t

/-- Each argument of a chain of arrows is shown by its final result. -/
def Ty.Shows : Ty → Prop
  | .arrow a _ t _ => Caps.Sub a.reach t.res.reach ∧ t.Shows
  | _ => True

theorem Le.res {a b : Ty} (h : Le a b) : Le a.res b.res := by
  induction h with
  | refl => exact .refl
  | arrow _ _ _ _ _ ih => exact ih
  | eff hr he ht hc => exact .eff hr he ht hc

theorem Ty.res_reach (t : Ty) : Caps.Sub t.res.reach t.reach := by
  induction t with
  | arrow _ _ _ _ _ ih => exact fun a h => .inr (.inl (ih a h))
  | _ => exact Caps.Sub.refl _

theorem Le.shows {a b : Ty} (h : Le a b) (hs : a.Shows) : b.Shows := by
  induction h with
  | refl => exact hs
  | arrow ha _ ht _ _ ih => exact ⟨ha.reach.trans (hs.1.trans ht.res.reach), ih hs.2⟩
  | eff => trivial

theorem Ty.arrows_res (fs : List Ty) (r : Ty) (hr : ∀ a φ t C, r ≠ .arrow a φ t C) :
    (Ty.arrows fs r).res = r := by
  induction fs with
  | nil =>
    cases r with
    | arrow => exact absurd rfl (hr _ _ _ _)
    | _ => rfl
  | cons _ _ ih => exact ih

theorem Ty.tcon_not_arrow (T : Name) (us : List Ty) : ∀ a φ t C, Ty.tcon T us ≠ .arrow a φ t C := by
  have : ∀ (us : List Ty) (f : Ty), (∀ a φ t C, f ≠ .arrow a φ t C) →
      ∀ a φ t C, us.foldl .tapp f ≠ .arrow a φ t C := by
    intro us
    induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ (by intros; simp)
  exact this us _ (by intros; simp)

theorem Ty.shows_of {t : Ty} (h : ∀ a φ b C, t ≠ .arrow a φ b C) : t.Shows := by
  cases t with
  | arrow => exact absurd rfl (h _ _ _ _)
  | _ => trivial

theorem Sig.ctorTyR_res (ts : List Ty) (T : Name) (rs us : List Ty) :
    (Sig.ctorTyR ts T rs us).res = Ty.tcon T (rs.map (Ty.substT (instT us))) :=
  Ty.arrows_res _ _ (Ty.tcon_not_arrow T _)

/-- A constructor's type shows its fields when they hold no more than its value. -/
theorem Sig.ctorTyR_shows {ts : List Ty} {T : Name} {rs us : List Ty}
    (hH : Sig.Holds ts T rs us) : (Sig.ctorTyR ts T rs us).Shows := by
  have key : ∀ fs : List Ty, fs ⊆ ts →
      (Ty.arrows (fs.map (Ty.substT (instT us))) (Ty.tcon T (rs.map (Ty.substT (instT us))))).Shows := by
    intro fs
    induction fs with
    | nil => intro _; exact Ty.shows_of (Ty.tcon_not_arrow T _)
    | cons f fs ih =>
      intro hfs
      refine ⟨?_, ih fun x hx => hfs (List.mem_cons_of_mem _ hx)⟩
      rw [Ty.arrows_res _ _ (Ty.tcon_not_arrow T _)]
      exact hH f (hfs (List.mem_cons_self ..))
  exact key ts (List.Subset.refl _)

theorem Sig.ctorTy_shows (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hk : us.length = k) : (Sig.ctorTy ts T us).Shows ∧
      (Sig.ctorTy ts T us).res = Ty.tcon T us := by
  have hres := Ty.arrows_res (ts.map (Ty.substT (instT us))) _ (Ty.tcon_not_arrow T us)
  refine ⟨?_, hres⟩
  have key : ∀ fs : List Ty, fs ⊆ ts → (Ty.arrows (fs.map (Ty.substT (instT us))) (Ty.tcon T us)).Shows := by
    intro fs
    induction fs with
    | nil => intro _; exact Ty.shows_of (Ty.tcon_not_arrow T us)
    | cons f fs ih =>
      intro hfs
      refine ⟨?_, ih fun x hx => hfs (List.mem_cons_of_mem _ hx)⟩
      rw [Ty.arrows_res _ _ (Ty.tcon_not_arrow T us)]
      exact S.field_reach hK hk f (hfs (List.mem_cons_self ..))
  exact key ts (List.Subset.refl _)

/-- What a constant with one argument shows of it. -/
theorem Const.shows_first (c : Const) (ι : CArg) (S : Sig) (hc : c.arity = 2) :
    ∃ a t, c.ty S ι = .arrow a .pure t Caps.none ∧ Caps.Sub a.reach t.reach := by
  cases c <;> simp [Const.arity] at hc <;>
    refine ⟨_, _, rfl, ?_⟩ <;> intro x h <;>
    simp_all [Ty.reach, Caps.none]

theorem Const.ty_isArrow (c : Const) (ι : CArg) (S : Sig) :
    ∃ a φ t C, c.ty S ι = .arrow a φ t C := by
  cases c
  case perform E o _ =>
    simp only [Const.ty]
    split <;> exact ⟨_, _, _, _, rfl⟩
  all_goals exact ⟨_, _, _, _, rfl⟩

/-- The claim of `Typ.held` with the shape of a spine. -/
def HeldC (S : Sig) (Ψ : List Ty) (Γ : List Scheme) (e : Tm) (T : Ty) : Prop :=
  Caps.Sub (held Ψ Γ e) T.reach ∧
    (∀ K, e.head = .ctor K → T.Shows ∧ Caps.Sub (held Ψ Γ e) T.res.reach) ∧
    (∀ c, e = .const c → ¬ T.isAll → ∃ ι, Le (c.ty S ι) T)

theorem Ty.tcon_hd (T : Name) (us : List Ty) : (Ty.tcon T us).hd = .adt T := by
  have : ∀ (us : List Ty) (f : Ty), f.hd = .adt T → (us.foldl .tapp f).hd = .adt T := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f h; exact ih _ h
  exact this us _ rfl

theorem Ty.not_all_of_hd {U : Ty} {E : Name} (h : U.hd = .adt E) : ¬ U.isAll := by
  cases U <;> simp_all [Ty.hd, Ty.isAll]

theorem Ty.res_tcon_not_all {k : Nat} {t : Ty} {T : Name} {us : List Ty}
    (h : (Ty.all k t).res = Ty.tcon T us) : False :=
  Ty.not_all_of_hd (Ty.tcon_hd T us) (h ▸ trivial)

/-- Going under a binder keeps what a term holds. -/
theorem held_shift {Ψ Γ e} (k : Nat) {a : Atom} (h : held Ψ Γ e a) :
    held (Ψ.map (Ty.up k)) (Γ.map (Scheme.up k)) e (a.up k) := by
  rcases h with ⟨i, s', hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩
  · exact .inl ⟨i, _, hi, by simp [hs], (Scheme.reach_up _ _ _).mpr ha⟩
  · exact .inr (.inl ⟨p, hp, rfl⟩)
  · exact .inr (.inr ⟨l, t.up k, hl, by simp [ht], (Ty.reach_up _ _ _).mpr ⟨a, ha, rfl⟩⟩)

theorem Le.not_all {x y : Ty} (h : Le x y) (hx : ¬ x.isAll) : ¬ y.isAll := by
  cases h <;> first | exact hx | simp [Ty.isAll]

theorem LeLive.res {a b : Ty} (h : LeLive a b) : LeLive a.res b.res := by
  induction h with
  | refl => exact .refl
  | arrow _ _ _ _ _ ih => exact ih
  | eff hφ ht hc => exact .eff hφ ht hc

theorem LeLive.not_all {x y : Ty} (h : LeLive x y) (hx : ¬ x.isAll) : ¬ y.isAll := by
  cases h <;> first | exact hx | simp [Ty.isAll]

theorem LeLiveI.snoc {a k t} {us : List Ty} (h : LeLiveI a (.all k t)) (hus : us.length = k) :
    LeLiveI a (t.substT (instT us)) := by
  generalize ht : Ty.all k t = T at h
  induction h with
  | le h => subst ht; cases h; exact .inst hus .refl
  | inst hus' _ ih => exact .inst hus' (ih ht)

theorem TypingMode.subtype.res {mode : TypingMode} {a b : Ty} (h : mode.subtype a b) :
    mode.subtype a.res b.res := by
  cases mode with
  | captures => exact Le.res h
  | live => exact LeLive.res h

theorem TypingMode.subtype.not_all {mode : TypingMode} {x y : Ty} (h : mode.subtype x y)
    (hx : ¬ x.isAll) : ¬ y.isAll := by
  cases mode with
  | captures => exact Le.not_all h hx
  | live => exact LeLive.not_all h hx

/-- Only a binder is below a binder. -/
theorem TypingMode.subtype.eq_all {mode : TypingMode} {x : Ty} {k t}
    (h : mode.subtype x (.all k t)) : x = .all k t := by
  cases mode <;> cases h <;> rfl

/-- Subtyping keeps the shape of an arrow and relates its results. -/
theorem TypingMode.subtype.arrow_res {mode : TypingMode} {a φ t C T : _}
    (h : mode.subtype (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ mode.subtype t t' := by
  cases mode with
  | captures =>
    obtain ⟨_, _, _, _, he, _, _, ht, -⟩ := Le.arrow_left h; exact ⟨_, _, _, _, he, ht⟩
  | live =>
    obtain ⟨_, _, _, _, he, _, _, ht, -⟩ := LeLive.arrow_left h; exact ⟨_, _, _, _, he, ht⟩

theorem TypingMode.instSubtype.refl {mode : TypingMode} {t : Ty} : mode.instSubtype t t := by
  cases mode with
  | captures => exact LeI.refl
  | live => exact LeLiveI.refl

theorem TypingMode.instSubtype.snoc {mode : TypingMode} {a k t} {us : List Ty}
    (h : mode.instSubtype a (.all k t)) (hus : us.length = k) :
    mode.instSubtype a (t.substT (instT us)) := by
  cases mode with
  | captures => exact LeI.snoc h hus
  | live => exact LeLiveI.snoc h hus

theorem Const.ty_isArrow_mode (c : Const) (ι : CArg) (S : Sig) (mode : TypingMode) :
    ∃ a φ t C, c.ty S ι mode = .arrow a φ t C := by
  cases c
  case perform E o _ =>
    simp only [Const.ty]
    split <;> exact ⟨_, _, _, _, rfl⟩
  case op => cases mode <;> exact ⟨_, _, _, _, rfl⟩
  all_goals exact ⟨_, _, _, _, rfl⟩

/-- The result of a constructor spine is not a binder, unless the value itself is one. -/
theorem TypM.spine_notAll {Ψ Θ Γ e J} (h : TypM S mode Ψ Θ Γ e J) :
    Value e → ∀ {K T φ}, e.head = .ctor K → J = .ex T φ → T.isAll ∨ ¬ T.res.isAll := by
  induction h with
  | allI => intro _ _ _ _ _ hJ; cases hJ; exact .inl trivial
  | errVal => intro _ _ _ _ _ hJ; cases hJ; exact .inr (by simp [Ty.res, Ty.isAll])
  | ctor hK hk hH hT =>
    intro _ _ _ _ _ hJ; cases hJ
    refine .inr (TypingMode.subtype.not_all hT.res ?_)
    rw [Sig.ctorTyR, Ty.arrows_res _ _ (Ty.tcon_not_arrow _ _)]
    exact Ty.not_all_of_hd (Ty.tcon_hd _ _)
  | app hf _ _ hT ih₁ _ =>
    intro hv _ _ _ hK hJ; cases hJ
    cases hv with
    | ctorApp hvf hK' _ =>
      rcases ih₁ hvf hK' rfl with h' | h'
      · exact h'.elim
      · exact .inr (TypingMode.subtype.not_all hT.res h')
    | constApp => simp [Tm.head] at hK
  | _ => intro hv _ _ _ hK hJ; cases hJ <;> cases hv <;> simp [Tm.head] at hK

theorem Typ.spine_notAll {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ {K T φ}, e.head = .ctor K → J = .ex T φ → T.isAll ∨ ¬ T.res.isAll :=
  TypM.spine_notAll h

theorem held_app {Ψ Γ f e} :
    Caps.Sub (held Ψ Γ (.app f e)) fun a => held Ψ Γ f a ∨ held Ψ Γ e a := by
  rintro a (⟨i, s, hi | hi, hs, ha⟩ | ⟨p, hp | hp, rfl⟩ | ⟨l, t, hl | hl, ht, ha⟩)
  · exact .inl (.inl ⟨i, s, hi, hs, ha⟩)
  · exact .inr (.inl ⟨i, s, hi, hs, ha⟩)
  · exact .inl (.inr (.inl ⟨p, hp, rfl⟩))
  · exact .inr (.inr (.inl ⟨p, hp, rfl⟩))
  · exact .inl (.inr (.inr ⟨l, t, hl, ht, ha⟩))
  · exact .inr (.inr (.inr ⟨l, t, hl, ht, ha⟩))

theorem held_none {Ψ Γ e} (hf : ∀ i, ¬ e.Free i) (hs : ∀ s, ¬ e.Has s) :
    Caps.Sub (held Ψ Γ e) Caps.none := by
  rintro a (⟨i, _, hi, _⟩ | ⟨p, hp, _⟩ | ⟨l, _, hl, _⟩)
  · exact hf _ hi
  · exact hs _ hp
  · exact hs _ hl

theorem held_const {Ψ Γ c x} : ¬ held Ψ Γ (.const c) x := by
  rintro (⟨_, _, h, _⟩ | ⟨_, h, _⟩ | ⟨_, _, h, _⟩) <;> cases h

theorem Caps.Sub.of_none {C : Caps} (D : Caps) (h : Caps.Sub C Caps.none) : Caps.Sub C D :=
  fun a ha => (h a ha).elim

theorem HeldC.closed {Ψ Γ e T} (hf : ∀ i, ¬ e.Free i) (hs : ∀ s, ¬ e.Has s)
    (hh : ∀ K, e.head ≠ .ctor K) (hc : ∀ c, e ≠ .const c) :
    HeldC S Ψ Γ e T :=
  ⟨(held_none hf hs).of_none _, fun K h => (hh K h).elim, fun c h => (hc c h).elim⟩

/-- **L4, syntactic.** A value holds no more than its type shows. -/
theorem Typ.holds {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ {T φ}, J = .ex T φ → HeldC S Ψ Γ e T := by
  induction h with
  | lam _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp⟩
  | fix _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp⟩
  | task _ _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp⟩
  | lit | rnil =>
    intro _ _ _ hJ; cases hJ
    exact HeldC.closed (by simp [Tm.Free]) (by simp [Tm.Has]) (by simp [Tm.head]) (by simp)
  | loc hl =>
    intro _ _ _ hJ; cases hJ
    refine ⟨?_, by simp [Tm.head], by simp⟩
    rintro a (⟨_, _, h, _⟩ | ⟨p, h, _⟩ | ⟨l, t, h, ht, ha⟩)
    · exact h.elim
    · cases h
    · cases h; rw [hl] at ht; cases ht; exact ha
  | cap | own =>
    intro _ _ _ hJ; cases hJ
    refine ⟨?_, by simp [Tm.head], by simp⟩
    rintro a (⟨_, _, h, _⟩ | ⟨p, h, rfl⟩ | ⟨l, t, h, _⟩)
    · exact h.elim
    · cases h; rfl
    · cases h
  | const _ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨(held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _, by simp [Tm.head],
      fun _ h _ => by cases h; exact ⟨_, hT⟩⟩
  | ctor hK hk hH hT =>
    intro _ _ _ hJ; cases hJ
    have hs := Sig.ctorTyR_shows hH
    exact ⟨(held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _,
      fun _ _ => ⟨hT.shows hs, (held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _⟩,
      by simp⟩
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | rcons hv₁ hv₂ =>
      refine ⟨?_, by simp [Tm.head], by simp⟩
      rintro a (⟨i, s, hi | hi, hs, ha⟩ | ⟨p, hp | hp, rfl⟩ | ⟨l, t, hl | hl, ht, ha⟩)
      · exact .inl ((ih₁ hv₁ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩))
      · exact .inr ((ih₂ hv₂ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩))
      · exact .inl ((ih₁ hv₁ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))
      · exact .inr ((ih₂ hv₂ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))
      · exact .inl ((ih₁ hv₁ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩)))
      · exact .inr ((ih₂ hv₂ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩)))
  | allI hv' _ ih =>
    intro _ _ _ hJ; cases hJ
    have key : Caps.Sub (held _ _ _) (Ty.all _ _).reach := fun a ha =>
      (ih hv' rfl).1 _ (held_shift _ ha)
    exact ⟨key, fun _ _ => ⟨trivial, key⟩, fun _ _ h => (h trivial).elim⟩
  | errVal hv' _ hU hE ih =>
    intro _ _ _ hJ; cases hJ
    obtain ⟨h₁, _, hc⟩ := ih hv' rfl
    have key : Caps.Sub (held _ _ _) (Ty.errors _).reach := fun a ha => .inl (hE _ (.inr (h₁ a ha)))
    refine ⟨key, fun _ _ => ⟨trivial, key⟩, fun c hc' _ => ?_⟩
    obtain ⟨ι, hι⟩ := hc c hc' (Ty.not_all_of_hd hU)
    obtain ⟨_, _, _, _, hx⟩ := Const.ty_isArrow c ι S
    rw [hx] at hι
    obtain ⟨_, _, _, _, rfl, -⟩ := hι.arrow_left
    simp [Ty.hd] at hU
  | app hf _ _ hT ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | ctorApp hvf hK hve =>
      obtain ⟨_, hsp, _⟩ := ih₁ hvf rfl
      obtain ⟨⟨hsa, hst⟩, hhf⟩ := hsp _ hK
      have h₂ := (ih₂ hve rfl).1
      have key : Caps.Sub (held _ _ (.app _ _)) _ := fun x hx => by
        rcases held_app x hx with h | h
        · exact hhf x h
        · exact hsa x (h₂ x h)
      refine ⟨key.trans (hT.res.reach.trans (Ty.res_reach _)),
        fun _ _ => ⟨hT.shows hst, key.trans hT.res.reach⟩, by simp⟩
    | constApp hc hve =>
      rename_i c
      obtain ⟨_, _, hcT⟩ := ih₁ .const rfl
      obtain ⟨ι, hι⟩ := hcT _ rfl (by simp [Ty.isAll])
      obtain ⟨a₀, t₀, hty, hsh⟩ := Const.shows_first _ ι S hc
      rw [hty] at hι
      obtain ⟨_, _, _, _, he', ha, _, ht, _⟩ := hι.arrow_left
      cases he'
      have h₂ := (ih₂ hve rfl).1
      have key : Caps.Sub (held _ _ (.app (.const c) _)) _ := fun x hx => by
        rcases held_app x hx with h | h
        · exact (held_const h).elim
        · exact ht.reach x (hsh x (ha.reach x (h₂ x h)))
      exact ⟨key.trans hT.reach, by simp [Tm.head], by simp⟩
  | drec _ _ _ _ hh =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hh, by simp [Tm.head], by simp⟩
  | @perform E o c a b t T _ _ _ _ hop hT =>
    intro _ _ _ hJ; cases hJ
    refine ⟨(held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _, by simp [Tm.head],
      fun _ h _ => ?_⟩
    cases h
    refine ⟨{ (default : CArg) with t := t }, ?_⟩
    cases c <;> simpa [Const.ty, hop] using hT
  | kont _ hC hT =>
    intro _ _ _ hJ; cases hJ
    refine ⟨?_, by simp [Tm.head], by simp⟩
    rintro x (⟨_, _, h, _⟩ | ⟨q, h, rfl⟩ | ⟨l, t, h, _⟩)
    · exact h.elim
    · cases h; exact hT.reach _ (.inr (.inr hC))
    · cases h
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

/-- A value has no `inside` form. -/
theorem TypM.noIns {Ψ Θ Γ e J} (h : TypM S mode Ψ Θ Γ e J) : Value e → e.NoIns := by
  induction h with
  | lam _ hn => intro _; exact hn
  | fix _ hn => intro _; exact hn
  | task _ _ hn => intro _; exact hn
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv; cases hv with
    | rcons hv₁ hv₂ =>
      rintro (h | h)
      · exact ih₁ hv₁ h
      · exact ih₂ hv₂ h
  | errVal hv _ _ _ ih | allI hv _ ih => intro _; exact ih hv
  | app _ _ _ _ ih₁ ih₂ =>
    intro hv
    cases hv with
    | ctorApp hvf _ hve =>
      rintro (h | h)
      · exact ih₁ hvf h
      · exact ih₂ hve h
    | constApp _ hve =>
      rintro (h | h)
      · exact h
      · exact ih₂ hve h
  | lit | loc | const | ctor | rnil | cap | own | dnil | perform => intro _ h; cases h
  | kont => intro _ h; simp [Tm.Has] at h
  | drec _ _ _ hn => intro _; exact hn
  | dcons _ _ ih₁ ih₂ =>
    intro hv; cases hv with
    | rcons hv₁ hv₂ =>
      rintro (h | h)
      · exact ih₁ hv₁ h
      · exact ih₂ hv₂ h
  | _ => intro hv; cases hv

theorem Typ.noIns {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : Value e → e.NoIns := TypM.noIns h

end Flick
