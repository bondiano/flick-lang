import Formal.Asc

/-!
# A child keeps the handlers of the place that registered it

`Fiber.spawn` and `Scope.add_finalizer` wrap the work in the handlers of their context older
than the owner (`withs`, runtime.md §3.5). The work is typed with the effects and obligations
of the task; a kept handler answers its effect and discharges obligations about it. An
obligation the context discharged at a handler, it discharges in the child too; one a region
of the context discharged at its end meets, in the child, handlers older than that region
(`AscE.under`), which outlive it. A handler left out is younger than the owner, and the
obligation of the work keeps its effect out of the row of the work.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

theorem list_rev_ind {α : Type} {P : List α → Prop} (h₀ : P []) (h₁ : ∀ E x, P E → P (E ++ [x])) :
    ∀ l, P l := by
  have : ∀ l : List α, P l.reverse := by
    intro l
    induction l with
    | nil => exact h₀
    | cons x l ih => simpa using h₁ _ x ih
  intro l; simpa using this l.reverse

theorem OkE.of_append {E : List Frame} {F : Frame} (h : OkE (E ++ [F])) : OkE E ∧ F.ok :=
  ⟨fun G hG => h G (List.mem_append_left _ hG), h F (by simp)⟩

theorem withs_append {p : Nat} {x : Tm} {F : Frame} :
    ∀ {E : List Frame}, withs p (E ++ [F]) x = F.wrap p (withs p E x)
  | [] => rfl
  | G :: E => by simp only [List.cons_append, withs]; exact withs_append (E := E) (x := G.wrap p x)

theorem ascBE_append {b : Nat} {F : Frame} : ∀ {E : List Frame}, ascBE b (E ++ [F]) = ascBE (F.ascB b) E
  | [] => rfl
  | G :: E => by simp only [List.cons_append, ascBE]; rw [ascBE_append]

theorem AscE.of_append {b : Nat} {F : Frame} : ∀ {E : List Frame}, AscE b (E ++ [F]) → AscE (F.ascB b) E
  | [], _ => trivial
  | G :: E, h => by
    simp only [List.cons_append, AscE] at h ⊢
    rw [ascBE_append] at h
    exact ⟨AscE.of_append h.1, h.2⟩

/-- Facts of an `inside` form follow the order of creation. -/
theorem Entails.inside_age {Θ F : Facts} {p : Nat} (hΘ : Entails Θ Facts.age)
    (hF : ∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) : Entails (Facts.union Θ F) Facts.age := by
  apply Entails.of_facts
  intro C s h
  rcases h with h | h
  · exact hΘ C s (Outl.fact h)
  · obtain ⟨rfl, hC⟩ := hF C s h
    exact Outl.fact fun q p' hq hp' => by cases hp'; exact hC q hq

theorem Outl.age_lt {q p : Nat} (h : q < p) : Outl Facts.age (· = .r q) (Atom.r p).ty := by
  apply Outl.fact
  intro q' p' hq hp
  cases hq; cases hp
  exact h

theorem Outl.age_le {q o : Nat} (h : Outl Facts.age (· = .r q) (Atom.r o).ty) : q ≤ o := by
  simp only [Outl, Facts.age_close] at h
  rcases h (.r q) rfl with h | ⟨C', h₁, h₂⟩
  · simp [Atom.ty] at h; omega
  · exact Nat.le_of_lt (h₁ q o h₂ rfl)

/-- What a younger region outlives, an older one does too. -/
theorem Outl.age_mono {a : Atom} {q q₂ : Nat} (h : Outl Facts.age (· = .r q₂) a.ty) (hlt : q < q₂) :
    Outl Facts.age (· = .r q) a.ty := by
  simp only [Outl, Facts.age_close] at h
  apply Outl.fact
  intro q' p hq' hp
  cases hq'
  rcases h (.r q₂) rfl with h' | ⟨C', h₁, h₂⟩
  · cases a <;> simp [Atom.ty] at h' hp
    omega
  · exact Nat.lt_trans hlt (h₁ q₂ p h₂ hp)

/-- What the frames of the place say of the work wrapped in them: its obligations are those of
the place, of a region of the place that discharged them, or are older than a handler of the
place left out; its effects are those of the place, and they reach it with the obligation for
the owner `o`, or under the region of `o` or a kept handler older than `o`, which every handler
around is older than too. A row variable of the work is one of the place, taken without every
name with parameters that the place's is taken without: a handler of such an effect answers
only its own arguments, so the work cannot bring others in. -/
def KidRow (S : Sig) (o : Nat) (φ : Stage) (E : List Frame) (R : Row) : Prop :=
  (∀ N c a, R (.ob N c a) → φ.svc (.ob N c a) ∨ (∃ p, a = .r p ∧ .inside p ∈ E) ∨
    ∃ F ∈ E, ∃ q, F.isHandler ∧ F.bump = some q ∧ Outl Facts.age (· = .r q) a.ty) ∧
  (∀ N c, R (.nm N c) → φ.svc (.nm N c) ∧
    (φ.svc (.ob N c (.r o)) ∨ .inside o ∈ E ∨
      ∃ F ∈ E, ∃ q, F.isHandler ∧ F.bump = some q ∧ q ≤ o)) ∧
  ∀ a G, R (.rv a G) → ∃ G₀, φ.svc (.rv a G₀) ∧ ∀ n, G₀ n → G n ∨ S.arity n = 0

/-- A handler handles an effect. -/
theorem installs_op {Ψ' Θ x t φ} {F : Frame} {N : Name}
    (h : Typ S Ψ' Θ [] (F.plug x) (.ex t φ))
    (hi : F.installs N) : ∃ o c a b, S.op N o = some (c, a, b) := by
  cases F <;> simp only [Frame.installs] at hi
  · subst hi
    obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, _, _, hc, _, _, _⟩ := h.hwith_type _ _ _ _ _ _ _ _ rfl rfl
    exact hc.1
  · subst hi
    obtain ⟨_, _, _, _, _, _, _, _, _, _, _, _, _, hc, _, _, _⟩ := h.hkeep_type _ _ _ _ _ _ _ rfl rfl
    exact hc.1

/-- The row of the work under a kept handler of `E`: what the work needs besides `E` and what
the handler's arms and the operations it does not answer need. -/
def Row.kid (E : Name) (R' rout : Row) : Row
  | .nm N c => (N ≠ E ∧ R' (.nm N c)) ∨ rout (.nm N c)
  | .ob N c a => (N ≠ E ∧ R' (.ob N c a)) ∨ rout (.ob N c a)
  | x => R' x ∨ rout x

theorem Row.kid_out {E : Name} {R' rout : Row} : ∀ y, rout y → Row.kid E R' rout y
  | .nm _ _, h | .ob _ _ _, h | .atm _, h | .rv _ _, h | .obv _ _ _, h => .inr h

/-- An obligation about the effect of a handler that the work holds is discharged by it. -/
theorem kid_outl {Θ : Facts} {φ₀ : Stage} {E : Name} {A : TyClass → Prop} {q b : Nat} {G : Frame}
    {E₀ : List Frame} {R' rout ε₀ : Row} {a : Atom} {c : TyClass} (hΘ : Entails Θ Facts.age)
    (hG : G.bump = some q) (hA : AscE b (E₀ ++ [G])) (hrow : φ₀.le (.rt (Row.hP Θ q E A rout) ε₀))
    (h : R' (.ob E c a) → φ₀.svc (.ob E c a) ∨ (∃ p, a = .r p ∧ .inside p ∈ E₀) ∨
      ∃ F ∈ E₀, ∃ q₂, F.isHandler ∧ F.bump = some q₂ ∧ Outl Facts.age (· = .r q₂) a.ty)
    (hy : R' (.ob E c a)) : Outl Facts.age (· = .r q) a.ty := by
  rcases h hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, q₂, hi, hb, hO⟩
  · exact hΘ _ _ ((Stage.le_svc hrow _ h').1 rfl)
  · exact Outl.age_lt (AscE.under (p := p) hG hA hp (.inl rfl))
  · exact Outl.age_mono hO (AscE.under (p := q₂) hG hA hF (.inr ⟨hb, hi⟩))

/-- A row variable of the work under a handler of `E` at `us`, as the handler gives it to the
row around: taken without the same names, but `E` when it answers every argument. -/
theorem kid_var {Θ : Facts} {φ₀ : Stage} {E : Name} {us : List Ty} {q : Nat} {rout ε₀ : Row}
    {a : Atom} {G G₀ : Name → Prop} (hrow : φ₀.le (.rt (Row.hP Θ q E (S.answers E us) rout) ε₀))
    (h₀ : φ₀.svc (.rv a G₀)) (hG : ∀ n, G₀ n → G n ∨ S.arity n = 0) :
    (S.arity E ≠ 0 → G E) ∧ ∃ G₁, rout (.rv a G₁) ∧ ∀ n, G₁ n → G n ∨ S.arity n = 0 := by
  obtain ⟨G₁, h₁, -, h₂, h₃⟩ := Stage.le_svc hrow _ h₀
  refine ⟨fun hn => (hG E (h₃ fun hu => hn (Sig.answers_all.mp hu))).resolve_right hn,
    G₁, h₁, fun n hn => ?_⟩
  rcases h₂ n hn with h | ⟨rfl, hu⟩
  · exact hG n h
  · exact .inr (Sig.answers_all.mp hu)

/-- The row of the work under a kept handler. -/
theorem kid_kept {o b q : Nat} {Θ : Facts} {φ₀ φ : Stage} {E : Name} {us : List Ty} {G : Frame}
    {E₀ : List Frame} {R' rout rk ε₀ : Row} (hΘ : Entails Θ Facts.age) (hG : G.bump = some q)
    (hi : G.isHandler) (hA : AscE b (E₀ ++ [G])) (hq : q ≤ o)
    (hrow : φ₀.le (.rt (Row.hP Θ q E (S.answers E us) rout) ε₀))
    (hsub : ∀ y, (∀ a, y ≠ .atm a) → rout y → rk y) (up : ∀ y, rk y → φ.svc y)
    (hk : KidRow S o φ₀ E₀ R') :
    (∀ y, R' y → Row.hP Facts.age q E (S.answers E us) (Row.kid E R' rk) y) ∧
      KidRow S o φ (E₀ ++ [G]) (Row.kid E R' rk) := by
  obtain ⟨h1, h2, h3⟩ := hk
  have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
  have self : G ∈ E₀ ++ [G] := by simp
  have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => mem' h
  refine ⟨fun y hy => ?_, fun N c a hy => ?_, fun N c hy => ?_, fun a G' hy => ?_⟩
  · cases y with
    | nm N c =>
      rcases Stage.le_svc hrow _ (h2 N c hy).1 with h | ⟨hN, -⟩
      · exact .inl h
      · exact .inr ⟨hN, .inl ⟨hN, hy⟩⟩
    | ob N c a =>
      refine ⟨fun hN => ?_, fun hN => .inl ⟨hN, hy⟩⟩
      subst hN
      exact kid_outl hΘ hG hA hrow (h1 N c a) hy
    | atm a | obv _ a => exact .inl hy
    | rv a G' =>
      obtain ⟨G₀, h₀, hG₀⟩ := h3 a G' hy
      exact ⟨G', .inl hy, fun _ h => .inl h, fun _ h => .inl h, fun hu =>
        (kid_var hrow h₀ hG₀).1 fun hn => hu (Sig.answers_all.mpr hn)⟩
  · rcases hy with ⟨hN, hy⟩ | hy
    · rcases h1 N c a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, x⟩
      · exact .inl (up _ (hsub _ (fun _ h => by cases h) ((Stage.le_svc hrow _ h').2 hN)))
      · exact .inr (.inl ⟨p, rfl, mem hp⟩)
      · exact .inr (.inr ⟨F, mem' hF, x⟩)
    · exact .inl (up _ hy)
  · have own : φ.svc (.ob N c (.r o)) ∨ .inside o ∈ E₀ ++ [G] ∨
        ∃ F ∈ E₀ ++ [G], ∃ q, F.isHandler ∧ F.bump = some q ∧ q ≤ o :=
      .inr (.inr ⟨G, self, q, hi, hG, hq⟩)
    rcases hy with ⟨hN, hy⟩ | hy
    · rcases Stage.le_svc hrow _ (h2 N c hy).1 with ⟨h', -⟩ | ⟨-, h'⟩
      · exact (hN h').elim
      · exact ⟨up _ (hsub _ (fun _ h => by cases h) h'), own⟩
    · exact ⟨up _ hy, own⟩
  · rcases hy with hy | hy
    · obtain ⟨G₀, h₀, hG₀⟩ := h3 a G' hy
      obtain ⟨-, G₁, h₁, hG₁⟩ := kid_var hrow h₀ hG₀
      exact ⟨G₁, up _ (hsub _ (fun _ h => by cases h) h₁), hG₁⟩
    · exact ⟨G', up _ hy, fun _ h => .inl h⟩

/-- The row of the work when a handler younger than its owner is left out. -/
theorem kid_dropped {o b q : Nat} {Θ : Facts} {φ₀ φ : Stage} {E : Name} {us : List Ty} {G : Frame}
    {E₀ : List Frame} {R' rout ε₀ : Row} (hΘ : Entails Θ Facts.age) (hG : G.bump = some q)
    (hi : G.isHandler) (hA : AscE b (E₀ ++ [G])) (hq : ¬ q ≤ o)
    (hrow : φ₀.le (.rt (Row.hP Θ q E (S.answers E us) rout) ε₀))
    (up : ∀ y, rout y → φ.svc y) (hk : KidRow S o φ₀ E₀ R') : KidRow S o φ (E₀ ++ [G]) R' := by
  obtain ⟨h1, h2, h3⟩ := hk
  have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
  have self : G ∈ E₀ ++ [G] := by simp
  have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => mem' h
  refine ⟨fun N c a hy => ?_, fun N c hy => ?_, fun a G' hy => ?_⟩
  · by_cases hN : N = E
    · subst hN
      exact .inr (.inr ⟨G, self, q, hi, hG, kid_outl hΘ hG hA hrow (h1 N c a) hy⟩)
    · rcases h1 N c a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, x⟩
      · exact .inl (up _ ((Stage.le_svc hrow _ h').2 hN))
      · exact .inr (.inl ⟨p, rfl, mem hp⟩)
      · exact .inr (.inr ⟨F, mem' hF, x⟩)
  · obtain ⟨a₁, a₂⟩ := h2 N c hy
    have hNE : N ≠ E := by
      rintro rfl
      apply hq
      rcases a₂ with h' | hp | ⟨F, hF, q₂, hi₂, hb₂, hle⟩
      · exact Outl.age_le (hΘ _ _ ((Stage.le_svc hrow _ h').1 rfl))
      · exact Nat.le_of_lt (AscE.under (p := o) hG hA hp (.inl rfl))
      · exact Nat.le_trans (Nat.le_of_lt (AscE.under (p := q₂) hG hA hF (.inr ⟨hb₂, hi₂⟩))) hle
    refine ⟨?_, ?_⟩
    · rcases Stage.le_svc hrow _ a₁ with ⟨h', -⟩ | ⟨-, h'⟩
      · exact (hNE h').elim
      · exact up _ h'
    · rcases a₂ with h' | hp | ⟨F, hF, x⟩
      · exact .inl (up _ ((Stage.le_svc hrow _ h').2 hNE))
      · exact .inr (.inl (mem hp))
      · exact .inr (.inr ⟨F, mem' hF, x⟩)
  · obtain ⟨G₀, h₀, hG₀⟩ := h3 a G' hy
    obtain ⟨-, G₁, h₁, hG₁⟩ := kid_var hrow h₀ hG₀
    exact ⟨G₁, up _ h₁, hG₁⟩

/-- The handler cell of a frame a child keeps. -/
def Frame.cell : Frame → Option (Nat × Nat)
  | .hwith q ℓ _ _ _ => some (q, ℓ)
  | .hkeep q ℓ _ _ => some (q, ℓ)
  | _ => none

/-- An atom of the row of the arms of a handler of `E` no younger than `o`: what a child
of `o` keeps of it. -/
def KeptAtm (Ψ : List Ty) (o : Nat) (E : List Frame) (a : Atom) : Prop :=
  ∃ F ∈ E, ∃ q ℓ u r ε t, F.cell = some (q, ℓ) ∧ q ≤ o ∧
    Ψ[ℓ]? = some (.arrow u (.rt r ε) t Caps.none) ∧ r.asRow (.atm a)

theorem KeptAtm.mono {o : Nat} {E E' : List Frame} {a : Atom} (h : ∀ F, F ∈ E → F ∈ E')
    (hk : KeptAtm Ψ o E a) : KeptAtm Ψ o E' a :=
  let ⟨F, hF, rest⟩ := hk; ⟨F, h F hF, rest⟩

/-- What work an owner keeps uses is the owner or older than it. -/
theorem keep_old {Θ : Facts} {r : Ty} {p : Nat} (hΘ : Entails Θ Facts.age)
    (hk : Row.Outlives Θ r (.rg p)) {a : Atom} (ha : r.asRow (.atm a)) :
    a = .r p ∨ ∀ x, a = .r x → x < p := by
  have ho := hΘ _ _ hk
  simp only [Outl, Facts.age_close] at ho
  rcases ho a ((Ty.reach_asRow r a).mpr (.inl ha)) with h | ⟨C', h₁, h₂⟩
  · exact .inl h
  · exact .inr fun x hx => h₁ x p (hx ▸ h₂) rfl

end Flick
