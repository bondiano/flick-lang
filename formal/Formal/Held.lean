import Formal.SubAdm

/-!
# A typed value holds no more than its type shows (Л4, syntactic part)

`held Ψ Γ v ⊆ ⌈τ⌉` for a value `v : τ`: its free variables, the regions it writes
down and the cell types of its locations are all within `⌈τ⌉` (regions.md §5.2).
The cases are those of Л4: (Lam), (Task) and the fixpoint by their premise;
(Cap), (Own) and a location by their type; a constructor spine because its fields
mention only its parameters (`Sig.ctor_reach`); a partial application of a
constant because each constant shows its first argument in the rest of its type;
(≤-Cap) by `Le.reach`. The substitution lemma uses it to keep the premise of (Lam).
-/

namespace Flick

variable {S : Sig}

theorem Sig.impl_reach (S : Sig) (N : Name) (a : Atom) : ¬ (S.impl N).reach a := by
  intro h
  rw [← S.impl_closed N (fun _ => .base .unit), Ty.reach_substT] at h
  obtain ⟨_, _, h⟩ := h
  exact h

theorem Ty.reach_tcon_arg {T : Name} {us : List Ty} {u : Ty} (hu : u ∈ us) :
    Caps.Sub u.reach (Ty.tcon T us).reach := by
  have key : ∀ (us : List Ty) (f : Ty), Caps.Sub f.reach (us.foldl .tapp f).reach ∧
      ∀ u ∈ us, Caps.Sub u.reach (us.foldl .tapp f).reach := by
    intro us
    induction us with
    | nil => intro f; exact ⟨Caps.Sub.refl _, by simp⟩
    | cons x xs ih =>
      intro f
      obtain ⟨h₁, h₂⟩ := ih (.tapp f x)
      refine ⟨fun a h => h₁ a (.inl h), fun u hu => ?_⟩
      rcases List.mem_cons.mp hu with rfl | hu
      · exact fun a h => h₁ a (.inr h)
      · exact h₂ u hu
  exact (key us _).2 u hu

/-- A field of a constructor at an instance reaches no more than the instance. -/
theorem Sig.field_reach (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hk : us.length = k) : ∀ f ∈ ts, Caps.Sub (f.substT (instT us)).reach (Ty.tcon T us).reach := by
  intro f hf a ha
  obtain ⟨b, hb, h⟩ := (Ty.reach_substT _ _ _).mp ha
  obtain ⟨i, hi, rfl⟩ := S.ctor_reach K k ts T hK f hf b hb
  have hi' : i < us.length := by omega
  simp only [instT, hi', dif_pos] at h
  exact Ty.reach_tcon_arg (List.getElem_mem hi') a h

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
theorem Const.shows_first (c : Const) (ι : CArg) (S : Sig) (hc : c.arity = 2 ∨ c.layer) :
    ∃ a t, c.ty S ι = .arrow a .pure t Caps.none ∧ Caps.Sub a.reach t.reach := by
  cases c <;> simp [Const.arity, Const.layer] at hc <;>
    refine ⟨_, _, rfl, ?_⟩ <;> intro x h <;>
    simp_all [Ty.reach, Caps.none, Sig.impl_reach]

theorem Const.ty_isArrow (c : Const) (ι : CArg) (S : Sig) :
    ∃ a φ t C, c.ty S ι = .arrow a φ t C := by
  cases c <;> exact ⟨_, _, _, _, rfl⟩

theorem Le.layer_right {x : Ty} {i e o : Row} (h : Le x (.layer i e o)) : x = .layer i e o := by
  cases h; rfl

/-- The claim of `Typ.held` with the shape of a spine. -/
def HeldC (S : Sig) (Ψ : List Ty) (Γ : List Scheme) (e : Tm) (T : Ty) : Prop :=
  Caps.Sub (held Ψ Γ e) T.reach ∧
    (∀ K, e.head = .ctor K → T.Shows ∧ Caps.Sub (held Ψ Γ e) T.res.reach) ∧
    (∀ c, e = .const c → ∃ ι, Le (c.ty S ι) T) ∧
    (∀ c e₀, e = .app (.const c) e₀ → c.arity = 2 → c.layer →
      Caps.Sub (held Ψ Γ e) Caps.none ∧ ∀ a φ t C, T = .arrow a φ t C → Caps.Sub a.reach Caps.none)

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
    (hh : ∀ K, e.head ≠ .ctor K) (hc : ∀ c, e ≠ .const c) (ha : ∀ c e₀, e ≠ .app (.const c) e₀) :
    HeldC S Ψ Γ e T :=
  ⟨(held_none hf hs).of_none _, fun K h => (hh K h).elim, fun c h => (hc c h).elim,
    fun c e₀ h => (ha c e₀ h).elim⟩

/-- **Л4, syntactic.** A value holds no more than its type shows. -/
theorem Typ.holds {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    Value e → ∀ {T φ}, J = .ex T φ → HeldC S Ψ Γ e T := by
  induction h with
  | lam _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp, by simp⟩
  | fix _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp, by simp⟩
  | task _ _ _ hc hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨hc.trans hT.reach, by simp [Tm.head], by simp, by simp⟩
  | lit | rnil | enil =>
    intro _ _ _ hJ; cases hJ
    exact HeldC.closed (by simp [Tm.Free]) (by simp [Tm.Has]) (by simp [Tm.head]) (by simp)
      (by simp)
  | loc hl =>
    intro _ _ _ hJ; cases hJ
    refine ⟨?_, by simp [Tm.head], by simp, by simp⟩
    rintro a (⟨_, _, h, _⟩ | ⟨p, h, _⟩ | ⟨l, t, h, ht, ha⟩)
    · exact h.elim
    · cases h
    · cases h; rw [hl] at ht; cases ht; exact ha
  | cap | own =>
    intro _ _ _ hJ; cases hJ
    refine ⟨?_, by simp [Tm.head], by simp, by simp⟩
    rintro a (⟨_, _, h, _⟩ | ⟨p, h, rfl⟩ | ⟨l, t, h, _⟩)
    · exact h.elim
    · cases h; rfl
    · cases h
  | const _ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨(held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _, by simp [Tm.head],
      fun _ h => by cases h; exact ⟨_, hT⟩, by simp⟩
  | ctor hK hk hT =>
    intro _ _ _ hJ; cases hJ
    obtain ⟨hs, _⟩ := S.ctorTy_shows hK hk
    exact ⟨(held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _,
      fun _ _ => ⟨hT.shows hs, (held_none (by simp [Tm.Free]) (by simp [Tm.Has])).of_none _⟩,
      by simp, by simp⟩
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | rcons hv₁ hv₂ =>
      refine ⟨?_, by simp [Tm.head], by simp, by simp⟩
      rintro a (⟨i, s, hi | hi, hs, ha⟩ | ⟨p, hp | hp, rfl⟩ | ⟨l, t, hl | hl, ht, ha⟩)
      · exact .inl ((ih₁ hv₁ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩))
      · exact .inr ((ih₂ hv₂ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩))
      · exact .inl ((ih₁ hv₁ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))
      · exact .inr ((ih₂ hv₂ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))
      · exact .inl ((ih₁ hv₁ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩)))
      · exact .inr ((ih₂ hv₂ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩)))
  | econs _ _ _ _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | econs hv₁ hv₂ =>
      refine ⟨?_, by simp [Tm.head], by simp, by simp⟩
      rintro a (⟨i, s, hi | hi, hs, ha⟩ | ⟨p, hp | hp, rfl⟩ | ⟨l, t, hl | hl, ht, ha⟩)
      · exact (S.impl_reach _ _ ((ih₁ hv₁ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩))).elim
      · exact (ih₂ hv₂ rfl).1 a (.inl ⟨i, s, hi, hs, ha⟩)
      · exact (S.impl_reach _ _ ((ih₁ hv₁ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩)))).elim
      · exact (ih₂ hv₂ rfl).1 _ (.inr (.inl ⟨p, hp, rfl⟩))
      · exact (S.impl_reach _ _ ((ih₁ hv₁ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩)))).elim
      · exact (ih₂ hv₂ rfl).1 a (.inr (.inr ⟨l, t, hl, ht, ha⟩))
  | errVal hv' _ hU hE ih =>
    intro _ _ _ hJ; cases hJ
    obtain ⟨h₁, _, hc, hm⟩ := ih hv' rfl
    have key : Caps.Sub (held _ _ _) (Ty.errors _).reach := fun a ha => hE _ (.inr (h₁ a ha))
    refine ⟨key, fun _ _ => ⟨trivial, key⟩, fun c hc' => ?_, fun c e₀ he h₁ h₂ =>
      ⟨(hm c e₀ he h₁ h₂).1, fun _ _ _ _ h => by cases h⟩⟩
    obtain ⟨ι, hι⟩ := hc c hc'
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
        fun _ _ => ⟨hT.shows hst, key.trans hT.res.reach⟩, by simp, ?_⟩
      intro c e₀ he _ _
      cases he; simp [Tm.head] at hK
    | constApp hc hve =>
      rename_i c
      obtain ⟨_, _, hcT, _⟩ := ih₁ .const rfl
      obtain ⟨ι, hι⟩ := hcT _ rfl
      obtain ⟨a₀, t₀, hty, hsh⟩ := Const.shows_first _ ι S hc
      rw [hty] at hι
      obtain ⟨_, _, _, _, he', ha, _, ht, _⟩ := hι.arrow_left
      cases he'
      have h₂ := (ih₂ hve rfl).1
      have key : Caps.Sub (held _ _ (.app (.const c) _)) _ := fun x hx => by
        rcases held_app x hx with h | h
        · exact (held_const h).elim
        · exact ht.reach x (hsh x (ha.reach x (h₂ x h)))
      refine ⟨key.trans hT.reach, by simp [Tm.head], by simp, ?_⟩
      intro c' e₀ heq h₁ h₂'
      cases heq
      cases c <;> simp [Const.arity, Const.layer] at h₁ h₂'
      simp only [Const.ty] at hty
      cases hty
      have hl := ha.layer_right; subst hl
      refine ⟨fun x hx => ?_, fun a' φ' t' C' hT' => ?_⟩
      · rcases held_app x hx with h | h
        · exact (held_const h).elim
        · exact h₂ x h
      · subst hT'
        obtain ⟨_, _, _, _, rfl, hl₁, -⟩ := ht.arrow_left
        obtain ⟨_, _, _, _, he₂, hl₂, -⟩ := hT.arrow_left
        cases he₂
        have := (hl₂.trans hl₁).layer_right; subst this
        exact fun _ h => h
    | constApp2 h₁ h₂ hv₁ hve =>
      obtain ⟨_, _, _, hm⟩ := ih₁ (.constApp (.inl h₁) hv₁) rfl
      obtain ⟨hf₀, ha₀⟩ := hm _ _ rfl h₁ h₂
      have h₃ := (ih₂ hve rfl).1
      have key : Caps.Sub (held _ _ (.app _ _)) Caps.none := fun x hx => by
        rcases held_app x hx with h | h
        · exact hf₀ x h
        · exact ha₀ _ _ _ _ rfl x (h₃ x h)
      refine ⟨key.of_none _, by simp [Tm.head], by simp, ?_⟩
      intro c e₀ he _ _; cases he
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

/-- A value has no `inside` form. -/
theorem Typ.noIns {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : Value e → e.NoIns := by
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
  | econs _ _ _ _ _ ih₁ ih₂ =>
    intro hv; cases hv with
    | econs hv₁ hv₂ =>
      rintro (h | h)
      · exact ih₁ hv₁ h
      · exact ih₂ hv₂ h
  | errVal hv _ _ _ ih => intro _; exact ih hv
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
    | constApp2 _ _ hv₁ hve =>
      rintro (h | h)
      · exact ih₁ (.constApp (.inl ‹_›) hv₁) h
      · exact ih₂ hve h
  | lit | loc | const | ctor | rnil | enil | cap | own => intro _ h; cases h
  | _ => intro hv; cases hv

end Flick
