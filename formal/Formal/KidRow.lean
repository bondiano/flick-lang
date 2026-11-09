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
  intro C s h k hk
  rcases h k hk with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · exact hΘ C' s (fun y hy => .inr ⟨C', h₁, hy⟩) k h₂
  · obtain ⟨rfl, hC⟩ := hF C' s h₁
    exact .inr ⟨C', fun q p' hq hp' => by cases hp'; exact hC q hq, h₂⟩

theorem Outl.age_lt {q p : Nat} (h : q < p) : Outl Facts.age (· = .r q) (Atom.r p).ty :=
  fun k hk => .inr ⟨(· = .r q), fun q' p' hq hp => by subst hk; cases hq; cases hp; exact h, hk⟩

theorem Outl.age_le {q o : Nat} (h : Outl Facts.age (· = .r q) (Atom.r o).ty) : q ≤ o := by
  rcases h (.r q) rfl with h | ⟨C', h₁, h₂⟩
  · simp [Atom.ty] at h; omega
  · exact Nat.le_of_lt (h₁ q o h₂ rfl)

/-- What a younger region outlives, an older one does too. -/
theorem Outl.age_mono {a : Atom} {q q₂ : Nat} (h : Outl Facts.age (· = .r q₂) a.ty) (hlt : q < q₂) :
    Outl Facts.age (· = .r q) a.ty := fun k hk => .inr ⟨(· = .r q), fun q' p hq' hp => by
  subst hk; cases hq'
  rcases h (.r q₂) rfl with h' | ⟨C', h₁, h₂⟩
  · cases a <;> simp [Atom.ty] at h' hp
    omega
  · exact Nat.lt_trans hlt (h₁ q₂ p h₂ hp), hk⟩

/-- An effect with an operation that is not `tail`. -/
def Sig.HasNonTail (S : Sig) (N : Name) : Prop := ∃ o c a b, S.op N o = some (c, a, b) ∧ c ≠ .tail

/-- What the frames of the place say of the work wrapped in them: its obligations are those of
the place, of a region of the place that discharged them, or are older than a handler of the
place left out; its effects reach the place, or are effects with an operation only a parent
runs, and they reach it with the obligation for the owner `o`, or under the region of `o` or a
kept handler older than `o`, which every handler around is older than too. -/
def KidRow (S : Sig) (o : Nat) (φ : Stage) (E : List Frame) (R : Row) : Prop :=
  (∀ N a, R (.ob N a) → φ.svc (.ob N a) ∨ (∃ p, a = .r p ∧ .inside p ∈ E) ∨
    ∃ F ∈ E, ∃ q, F.isHandler ∧ F.bump = some q ∧ Outl Facts.age (· = .r q) a.ty) ∧
  (∀ N, R (.nm N) → (φ.svc (.nm N) ∨ S.HasNonTail N ∧ ∃ F ∈ E, F.installs N) ∧
    (φ.svc (.ob N (.r o)) ∨ .inside o ∈ E ∨ ∃ F ∈ E, ∃ q, F.isHandler ∧ F.bump = some q ∧ q ≤ o))

/-- A handler handles an effect. -/
theorem installs_op {mode : TypingMode} {Ψ' Θ x t φ} {F : Frame} {N : Name}
    (h : TypM S mode Ψ' Θ [] (F.plug x) (.ex t φ))
    (hi : F.installs N) : ∃ o c a b, S.op N o = some (c, a, b) := by
  cases F <;> simp only [Frame.installs] at hi
  · subst hi
    cases h with
    | hwith _ _ _ _ _ _ hc _ _ => exact hc.1
    | errVal hv | allI hv => cases hv
  · subst hi
    cases h with
    | hkeep _ _ _ _ hc _ _ _ _ => exact hc.1
    | errVal hv | allI hv => cases hv

/-- The row of the work under a kept handler of `E`: what the work needs besides `E` and what
the handler's arms and the operations it does not answer need. -/
def Row.kid (S : Sig) (E : Name) (R' rout : Row) : Row
  | .nm N => (N ≠ E ∧ R' (.nm N)) ∨ rout (.nm N) ∨ (N = E ∧ S.HasNonTail E)
  | .ob N a => (N ≠ E ∧ R' (.ob N a)) ∨ rout (.ob N a)
  | x => R' x ∨ rout x

theorem Row.kid_out {E : Name} {R' rout : Row} : ∀ y, rout y → Row.kid S E R' rout y
  | .nm _, h => .inr (.inl h)
  | .ob _ _, h => .inr h
  | .atm _, h | .rv _, h | .obv _ _, h => .inr h

/-- An obligation about the effect of a handler that the work holds is discharged by it. -/
theorem kid_outl {Θ : Facts} {φ₀ : Stage} {E : Name} {q b : Nat} {G : Frame} {E₀ : List Frame} {R' rout ε₀ : Row}
    {a : Atom} (hΘ : Entails Θ Facts.age) (hG : G.bump = some q) (hA : AscE b (E₀ ++ [G]))
    (hrow : φ₀.le (.rt (Row.hP Θ q E rout) ε₀))
    (h : R' (.ob E a) → φ₀.svc (.ob E a) ∨ (∃ p, a = .r p ∧ .inside p ∈ E₀) ∨
      ∃ F ∈ E₀, ∃ q₂, F.isHandler ∧ F.bump = some q₂ ∧ Outl Facts.age (· = .r q₂) a.ty)
    (hy : R' (.ob E a)) : Outl Facts.age (· = .r q) a.ty := by
  rcases h hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, q₂, hi, hb, hO⟩
  · exact hΘ _ _ ((Stage.le_svc hrow _ h').1 rfl)
  · exact Outl.age_lt (AscE.under (p := p) hG hA hp (.inl rfl))
  · exact Outl.age_mono hO (AscE.under (p := q₂) hG hA hF (.inr ⟨hb, hi⟩))

/-- The row of the work under a kept handler. -/
theorem kid_kept {o b q : Nat} {Θ : Facts} {φ₀ φ : Stage} {E : Name} {G : Frame} {E₀ : List Frame}
    {R' rout rk ε₀ : Row} (hΘ : Entails Θ Facts.age) (hG : G.bump = some q) (hi : G.isHandler)
    (hGi : G.installs E) (hA : AscE b (E₀ ++ [G])) (hq : q ≤ o)
    (hrow : φ₀.le (.rt (Row.hP Θ q E rout) ε₀))
    (hsub : ∀ y, (∀ a, y ≠ .atm a) → rout y → rk y) (up : ∀ y, rk y → φ.svc y)
    (hk : KidRow S o φ₀ E₀ R') :
    (∀ y, R' y → Row.hP Facts.age q E (Row.kid S E R' rk) y) ∧ KidRow S o φ (E₀ ++ [G]) (Row.kid S E R' rk) := by
  obtain ⟨h1, h2⟩ := hk
  have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
  have self : G ∈ E₀ ++ [G] := by simp
  have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => mem' h
  refine ⟨fun y hy => ?_, fun N a hy => ?_, fun N hy => ?_⟩
  · cases y with
    | nm N =>
      by_cases hN : N = E
      · exact .inl hN
      · exact .inr (.inl ⟨hN, hy⟩)
    | ob N a =>
      refine ⟨fun hN => ?_, fun hN => .inl ⟨hN, hy⟩⟩
      subst hN
      exact kid_outl hΘ hG hA hrow (h1 N a) hy
    | atm a | rv a | obv _ a => exact .inl hy
  · rcases hy with ⟨hN, hy⟩ | hy
    · rcases h1 N a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, x⟩
      · exact .inl (up _ (hsub _ (fun _ h => by cases h) ((Stage.le_svc hrow _ h').2 hN)))
      · exact .inr (.inl ⟨p, rfl, mem hp⟩)
      · exact .inr (.inr ⟨F, mem' hF, x⟩)
    · exact .inl (up _ hy)
  · have own : φ.svc (.ob N (.r o)) ∨ .inside o ∈ E₀ ++ [G] ∨
        ∃ F ∈ E₀ ++ [G], ∃ q, F.isHandler ∧ F.bump = some q ∧ q ≤ o :=
      .inr (.inr ⟨G, self, q, hi, hG, hq⟩)
    rcases hy with ⟨hN, hy⟩ | hy | ⟨rfl, hnt⟩
    · obtain ⟨a₁, -⟩ := h2 N hy
      refine ⟨?_, own⟩
      rcases a₁ with a₁ | ⟨hn, F, hF, hI⟩
      · rcases Stage.le_svc hrow _ a₁ with h' | h'
        · exact (hN h').elim
        · exact .inl (up _ (hsub _ (fun _ h => by cases h) h'))
      · exact .inr ⟨hn, F, mem' hF, hI⟩
    · exact ⟨.inl (up _ hy), own⟩
    · exact ⟨.inr ⟨hnt, G, self, hGi⟩, own⟩

/-- The row of the work when a handler younger than its owner is left out. -/
theorem kid_dropped {o b q : Nat} {Θ : Facts} {φ₀ φ : Stage} {E : Name} {G : Frame} {E₀ : List Frame}
    {R' rout ε₀ : Row} (hΘ : Entails Θ Facts.age) (hG : G.bump = some q) (hi : G.isHandler)
    (hA : AscE b (E₀ ++ [G])) (hq : ¬ q ≤ o) (hrow : φ₀.le (.rt (Row.hP Θ q E rout) ε₀))
    (up : ∀ y, rout y → φ.svc y) (hk : KidRow S o φ₀ E₀ R') : KidRow S o φ (E₀ ++ [G]) R' := by
  obtain ⟨h1, h2⟩ := hk
  have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
  have self : G ∈ E₀ ++ [G] := by simp
  have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => mem' h
  refine ⟨fun N a hy => ?_, fun N hy => ?_⟩
  · by_cases hN : N = E
    · subst hN
      exact .inr (.inr ⟨G, self, q, hi, hG, kid_outl hΘ hG hA hrow (h1 N a) hy⟩)
    · rcases h1 N a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF, x⟩
      · exact .inl (up _ ((Stage.le_svc hrow _ h').2 hN))
      · exact .inr (.inl ⟨p, rfl, mem hp⟩)
      · exact .inr (.inr ⟨F, mem' hF, x⟩)
  · obtain ⟨a₁, a₂⟩ := h2 N hy
    have hNE : N ≠ E := by
      rintro rfl
      apply hq
      rcases a₂ with h' | hp | ⟨F, hF, q₂, hi₂, hb₂, hle⟩
      · exact Outl.age_le (hΘ _ _ ((Stage.le_svc hrow _ h').1 rfl))
      · exact Nat.le_of_lt (AscE.under (p := o) hG hA hp (.inl rfl))
      · exact Nat.le_trans (Nat.le_of_lt (AscE.under (p := q₂) hG hA hF (.inr ⟨hb₂, hi₂⟩))) hle
    refine ⟨?_, ?_⟩
    · rcases a₁ with a₁ | ⟨hn, F, hF, hI⟩
      · rcases Stage.le_svc hrow _ a₁ with h' | h'
        · exact (hNE h').elim
        · exact .inl (up _ h')
      · exact .inr ⟨hn, F, mem' hF, hI⟩
    · rcases a₂ with h' | hp | ⟨F, hF, x⟩
      · exact .inl (up _ ((Stage.le_svc hrow _ h').2 hNE))
      · exact .inr (.inl (mem hp))
      · exact .inr (.inr ⟨F, mem' hF, x⟩)

/-- The handler cell of a frame a child keeps. -/
def Frame.cell : Frame → Option (Nat × Nat)
  | .hwith q ℓ _ _ _ => some (q, ℓ)
  | .hkeep q ℓ _ _ => some (q, ℓ)
  | _ => none

/-- An atom of the row of the arms of a handler of `E` no younger than `o`: what a child
of `o` keeps of it. -/
def KeptAtm (Ψ : List Ty) (o : Nat) (E : List Frame) (a : Atom) : Prop :=
  ∃ F ∈ E, ∃ q ℓ r ε t, F.cell = some (q, ℓ) ∧ q ≤ o ∧
    Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧ r (.atm a)

theorem KeptAtm.mono {o : Nat} {E E' : List Frame} {a : Atom} (h : ∀ F, F ∈ E → F ∈ E')
    (hk : KeptAtm Ψ o E a) : KeptAtm Ψ o E' a :=
  let ⟨F, hF, rest⟩ := hk; ⟨F, h F hF, rest⟩

/-- What work an owner keeps uses is the owner or older than it. -/
theorem keep_old {Θ : Facts} {r : Row} {p : Nat} (hΘ : Entails Θ Facts.age)
    (hk : TypingMode.live.keep Θ r (.rg p)) {a : Atom} (ha : r (.atm a)) :
    a = .r p ∨ ∀ x, a = .r x → x < p := by
  rcases hΘ _ _ hk a (.inl ha) with h | ⟨C', h₁, h₂⟩
  · exact .inl h
  · exact .inr fun x hx => h₁ x p (hx ▸ h₂) rfl

end Flick
