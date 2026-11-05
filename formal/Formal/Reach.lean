import Formal.Subst
import Formal.SchemeOps

/-!
# What a term holds (regions.md §2.1, §2.3, §5.2)

`e.Has` is what a term writes down: a run-time region (in `cap π`, `own π`,
`inside π e`), a location, a run-time form (`inside` or `with`), the `inside` form of a
region, the `provide` region of a `with` frame. `e.Free i` is a free variable.
`held Ψ Γ e` is what the premises of (Lam) and (Task) bound: `⌈Γ(y)⌉` of each
free `y`, the run-time regions written in `e` and `⌈Ψ(ℓ)⌉` of each location.
-/

namespace Flick

/-- A place in a term that reaches something. -/
inductive Site
  | rgn (p : Nat)
  | loc (l : Nat)
  | ins
  | frame (p : Nat)
  | prov (p : Nat)

def Tm.Has : Tm → Site → Prop
  | .var _, _ => False
  | .lam b, s => b.Has s
  | .app f a, s => f.Has s ∨ a.Has s
  | .let_ e b, s => e.Has s ∨ b.Has s
  | .letrec f b, s => f.Has s ∨ b.Has s
  | .fix f, s => f.Has s
  | .lit _, _ => False
  | .loc l, s => s = .loc l
  | .const _, _ => False
  | .ctor _, _ => False
  | .rnil, _ => False
  | .rcons _ e r, s => e.Has s ∨ r.Has s
  | .proj e _, s => e.Has s
  | .update e _ e', s => e.Has s ∨ e'.Has s
  | .match_ e al, s => e.Has s ∨ al.Has s
  | .anil, _ => False
  | .acons _ b r, s => b.Has s ∨ r.Has s
  | .aguard _ g b r, s => g.Has s ∨ b.Has s ∨ r.Has s
  | .todo, _ => False
  | .task e, s => e.Has s
  | .await e, s => e.Has s
  | .raise e, s => e.Has s
  | .try_ e al, s => e.Has s ∨ al.Has s
  | .handle v al, s => v.Has s ∨ al.Has s
  | .provide l e, s => l.Has s ∨ e.Has s
  | .with_ q θ e, s => s = .ins ∨ s = .prov q ∨ θ.Has s ∨ e.Has s
  | .enil, _ => False
  | .econs _ v θ, s => v.Has s ∨ θ.Has s
  | .bind _ e, s => e.Has s
  | .join e₁ e₂, s => e₁.Has s ∨ e₂.Has s
  | .scope e, s => e.Has s
  | .inside p e, s => s = .rgn p ∨ s = .ins ∨ s = .frame p ∨ e.Has s
  | .cap p, s => s = .rgn p
  | .own p, s => s = .rgn p
  | .drec _ r, s => r.Has s
  | .glob _, _ => False

/-- No `inside` form: values and unevaluated code. -/
def Tm.NoIns (e : Tm) : Prop := ¬ e.Has .ins

/-- `i` occurs free in `e`. -/
def Tm.Free : Tm → Nat → Prop
  | .var i, j => i = j
  | .lam b, j => b.Free (j + 1)
  | .app f a, j => f.Free j ∨ a.Free j
  | .let_ e b, j => e.Free j ∨ b.Free (j + 1)
  | .letrec f b, j => f.Free (j + 2) ∨ b.Free (j + 1)
  | .fix f, j => f.Free (j + 2)
  | .rcons _ e r, j => e.Free j ∨ r.Free j
  | .proj e _, j => e.Free j
  | .update e _ e', j => e.Free j ∨ e'.Free j
  | .match_ e al, j => e.Free j ∨ al.Free j
  | .acons p b r, j => b.Free (j + p.binds) ∨ r.Free j
  | .aguard p g b r, j => g.Free (j + p.binds) ∨ b.Free (j + p.binds) ∨ r.Free j
  | .task e, j => e.Free j
  | .await e, j => e.Free j
  | .raise e, j => e.Free j
  | .try_ e al, j => e.Free j ∨ al.Free j
  | .handle v al, j => v.Free j ∨ al.Free j
  | .provide l e, j => l.Free j ∨ e.Free j
  | .with_ _ θ e, j => θ.Free j ∨ e.Free j
  | .econs _ v θ, j => v.Free j ∨ θ.Free j
  | .bind _ e, j => e.Free j
  | .join e₁ e₂, j => e₁.Free j ∨ e₂.Free j
  | .scope e, j => e.Free (j + 1)
  | .inside _ e, j => e.Free j
  | .drec _ r, j => r.Free j
  | _, _ => False

theorem Tm.has_rename (ξ : Nat → Nat) (e : Tm) (s : Site) : (e.rename ξ).Has s ↔ e.Has s := by
  induction e generalizing ξ <;> simp_all [Tm.rename, Tm.Has]

theorem liftR_eq {n : Nat} {ξ : Nat → Nat} {i j : Nat} (h : liftR n ξ i = j + n) :
    n ≤ i ∧ ξ (i - n) = j := by
  unfold liftR at h
  split at h <;> omega

theorem free_rename_bind {n : Nat} {b : Tm} {ξ : Nat → Nat} {j : Nat}
    (ih : ∀ j, (b.rename (liftR n ξ)).Free j → ∃ i, b.Free i ∧ liftR n ξ i = j)
    (h : (b.rename (liftR n ξ)).Free (j + n)) : ∃ i, b.Free (i + n) ∧ ξ i = j := by
  obtain ⟨i, hi, he⟩ := ih _ h
  obtain ⟨hn, he'⟩ := liftR_eq he
  exact ⟨i - n, by rwa [Nat.sub_add_cancel hn], he'⟩

theorem Tm.free_rename (ξ : Nat → Nat) :
    ∀ (e : Tm) (j : Nat), (e.rename ξ).Free j → ∃ i, e.Free i ∧ ξ i = j := by
  intro e
  induction e generalizing ξ with
  | var i => intro j h; exact ⟨i, rfl, h⟩
  | lam b ih => intro j h; exact free_rename_bind (ih _) h
  | let_ e b ih₁ ih₂ =>
    intro j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₂ _) h; exact ⟨i, .inr hi, he⟩
  | letrec f b ih₁ ih₂ =>
    intro j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₁ _) h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₂ _) h; exact ⟨i, .inr hi, he⟩
  | fix f ih => intro j h; exact free_rename_bind (ih _) h
  | scope e ih => intro j h; exact free_rename_bind (ih _) h
  | acons p b r ih₁ ih₂ =>
    intro j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₁ _) h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := ih₂ ξ j h; exact ⟨i, .inr hi, he⟩
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro j h
    rcases h with h | h | h
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₁ _) h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := free_rename_bind (ih₂ _) h; exact ⟨i, .inr (.inl hi), he⟩
    · obtain ⟨i, hi, he⟩ := ih₃ ξ j h; exact ⟨i, .inr (.inr hi), he⟩
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := ih₁ ξ j h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := ih₂ ξ j h; exact ⟨i, .inr hi, he⟩
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih | drec _ _ ih =>
    intro j h; exact ih ξ j h
  | _ => intro j h; exact h.elim

theorem liftS_free {n : Nat} {σ : Nat → Tm} {i j : Nat} (h : (liftS n σ i).Free (j + n)) :
    n ≤ i ∧ (σ (i - n)).Free j := by
  unfold liftS at h
  split at h
  · simp [Tm.Free] at h; omega
  · obtain ⟨m, hm, he⟩ := Tm.free_rename _ _ _ h
    refine ⟨by omega, ?_⟩
    have : m = j := by omega
    rwa [← this]

theorem liftS_has {n : Nat} {σ : Nat → Tm} {i : Nat} {s : Site} (h : (liftS n σ i).Has s) :
    n ≤ i ∧ (σ (i - n)).Has s := by
  unfold liftS at h
  split at h
  · simp [Tm.Has] at h
  · exact ⟨by omega, (Tm.has_rename _ _ _).mp h⟩

theorem Tm.free_subst :
    ∀ (e : Tm) (σ : Nat → Tm) (j : Nat), (e.subst σ).Free j → ∃ i, e.Free i ∧ (σ i).Free j := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm} {j : Nat},
      (∀ σ' j, (b.subst σ').Free j → ∃ i, b.Free i ∧ (σ' i).Free j) →
      (b.subst (liftS n σ)).Free (j + n) → ∃ i, b.Free (i + n) ∧ (σ i).Free j := by
    intro n b σ j ih h
    obtain ⟨i, hi, he⟩ := ih _ _ h
    obtain ⟨hn, he'⟩ := liftS_free he
    exact ⟨i - n, by rwa [Nat.sub_add_cancel hn], he'⟩
  intro e
  induction e with
  | var i => intro σ j h; exact ⟨i, rfl, h⟩
  | lam b ih => intro σ j h; exact bind ih h
  | let_ e b ih₁ ih₂ =>
    intro σ j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := ih₁ σ j h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := bind ih₂ h; exact ⟨i, .inr hi, he⟩
  | letrec f b ih₁ ih₂ =>
    intro σ j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := bind ih₁ h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := bind ih₂ h; exact ⟨i, .inr hi, he⟩
  | fix f ih => intro σ j h; exact bind ih h
  | scope e ih => intro σ j h; exact bind ih h
  | acons p b r ih₁ ih₂ =>
    intro σ j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := bind ih₁ h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := ih₂ σ j h; exact ⟨i, .inr hi, he⟩
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ j h
    rcases h with h | h | h
    · obtain ⟨i, hi, he⟩ := bind ih₁ h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := bind ih₂ h; exact ⟨i, .inr (.inl hi), he⟩
    · obtain ⟨i, hi, he⟩ := ih₃ σ j h; exact ⟨i, .inr (.inr hi), he⟩
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂ | with_ _ _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ j h
    rcases h with h | h
    · obtain ⟨i, hi, he⟩ := ih₁ σ j h; exact ⟨i, .inl hi, he⟩
    · obtain ⟨i, hi, he⟩ := ih₂ σ j h; exact ⟨i, .inr hi, he⟩
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | inside _ _ ih | drec _ _ ih =>
    intro σ j h; exact ih σ j h
  | _ => intro σ j h; exact h.elim

theorem Tm.has_subst :
    ∀ (e : Tm) (σ : Nat → Tm) (s : Site), (e.subst σ).Has s →
      e.Has s ∨ ∃ i, e.Free i ∧ (σ i).Has s := by
  have bind : ∀ {n : Nat} {b : Tm} {σ : Nat → Tm} {s : Site},
      (∀ σ', (b.subst σ').Has s → b.Has s ∨ ∃ i, b.Free i ∧ (σ' i).Has s) →
      (b.subst (liftS n σ)).Has s → b.Has s ∨ ∃ i, b.Free (i + n) ∧ (σ i).Has s := by
    intro n b σ s ih h
    rcases ih _ h with h | ⟨i, hi, he⟩
    · exact .inl h
    · obtain ⟨hn, he'⟩ := liftS_has he
      exact .inr ⟨i - n, by rwa [Nat.sub_add_cancel hn], he'⟩
  intro e
  induction e with
  | var i => intro σ s h; exact .inr ⟨i, rfl, h⟩
  | lam b ih | fix b ih | scope b ih => intro σ s h; exact bind (ih · s) h
  | let_ e b ih₁ ih₂ =>
    intro σ s h
    rcases h with h | h
    · exact (ih₁ σ s h).imp .inl fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (bind (ih₂ · s) h).imp .inr fun ⟨i, hi, he⟩ => ⟨i, .inr hi, he⟩
  | letrec f b ih₁ ih₂ =>
    intro σ s h
    rcases h with h | h
    · exact (bind (ih₁ · s) h).imp .inl fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (bind (ih₂ · s) h).imp .inr fun ⟨i, hi, he⟩ => ⟨i, .inr hi, he⟩
  | acons p b r ih₁ ih₂ =>
    intro σ s h
    rcases h with h | h
    · exact (bind (ih₁ · s) h).imp .inl fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (ih₂ σ s h).imp .inr fun ⟨i, hi, he⟩ => ⟨i, .inr hi, he⟩
  | aguard p g b r ih₁ ih₂ ih₃ =>
    intro σ s h
    rcases h with h | h | h
    · exact (bind (ih₁ · s) h).imp .inl fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (bind (ih₂ · s) h).imp (.inr ∘ .inl) fun ⟨i, hi, he⟩ => ⟨i, .inr (.inl hi), he⟩
    · exact (ih₃ σ s h).imp (.inr ∘ .inr) fun ⟨i, hi, he⟩ => ⟨i, .inr (.inr hi), he⟩
  | app _ _ ih₁ ih₂ | rcons _ _ _ ih₁ ih₂ | update _ _ _ ih₁ ih₂ | match_ _ _ ih₁ ih₂
  | try_ _ _ ih₁ ih₂ | handle _ _ ih₁ ih₂ | provide _ _ ih₁ ih₂
  | econs _ _ _ ih₁ ih₂ | join _ _ ih₁ ih₂ =>
    intro σ s h
    rcases h with h | h
    · exact (ih₁ σ s h).imp .inl fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (ih₂ σ s h).imp .inr fun ⟨i, hi, he⟩ => ⟨i, .inr hi, he⟩
  | with_ _ _ _ ih₁ ih₂ =>
    intro σ s h
    rcases h with h | h | h | h
    · exact .inl (.inl h)
    · exact .inl (.inr (.inl h))
    · exact (ih₁ σ s h).imp (.inr ∘ .inr ∘ .inl) fun ⟨i, hi, he⟩ => ⟨i, .inl hi, he⟩
    · exact (ih₂ σ s h).imp (.inr ∘ .inr ∘ .inr) fun ⟨i, hi, he⟩ => ⟨i, .inr hi, he⟩
  | proj _ _ ih | task _ ih | await _ ih | raise _ ih | bind _ _ ih | drec _ _ ih =>
    intro σ s h; exact ih σ s h
  | inside p e ih =>
    intro σ s h
    rcases h with h | h | h | h
    · exact .inl (.inl h)
    · exact .inl (.inr (.inl h))
    · exact .inl (.inr (.inr (.inl h)))
    · exact (ih σ s h).imp (.inr ∘ .inr ∘ .inr) id
  | _ => intro σ s h; exact .inl h

/-- An atom of a scheme's type, outside its own binders, in the context's terms. -/
def Atom.down (k : Nat) : Atom → Option Atom
  | .v i => if i < k then none else some (.v (i - k))
  | .r p => some (.r p)

/-- The atoms shifted by `k`. -/
def Atom.up (k : Nat) : Atom → Atom
  | .v i => .v (i + k)
  | .r p => .r p

@[simp] theorem Atom.down_up (k : Nat) (a : Atom) : Atom.down k (a.up k) = some a := by
  cases a <;> simp [Atom.down, Atom.up]

theorem Ty.reach_up (t : Ty) (k : Nat) (b : Atom) :
    (t.up k).reach b ↔ ∃ a, t.reach a ∧ b = a.up k := by
  rw [Ty.up, Ty.reach_substT]
  constructor
  · rintro ⟨a, ha, h⟩
    cases a <;> simp [shiftV] at h <;> subst h <;> exact ⟨_, ha, rfl⟩
  · rintro ⟨a, ha, rfl⟩; exact ⟨a, ha, by cases a <;> simp [shiftV, Atom.up]⟩

/-- `⌈σ⌉` of a scheme: the atoms of its type other than its own variables. -/
def Scheme.reach (s : Scheme) : Caps := fun a => ∃ b, s.t.reach b ∧ Atom.down s.k b = some a

@[simp] theorem Scheme.reach_mono (t : Ty) (a : Atom) : (Scheme.mono t).reach a ↔ t.reach a := by
  simp only [Scheme.reach, Scheme.mono]
  constructor
  · rintro ⟨b, hb, h⟩; cases b <;> simp_all [Atom.down]
  · intro h; exact ⟨a, h, by cases a <;> simp [Atom.down]⟩

/-- `⌈·⌉` of a scheme commutes with substitution. -/
theorem Scheme.reach_substT (s : Scheme) (σ : Atom → Ty) (a : Atom) :
    (s.substT σ).reach a ↔ ∃ b, s.reach b ∧ (σ b).reach a := by
  simp only [Scheme.reach, Scheme.substT, Ty.reach_substT]
  constructor
  · rintro ⟨b, ⟨c, hc, hb⟩, hd⟩
    cases c with
    | v i =>
      by_cases hi : i < s.k
      · simp [liftT, hi] at hb; subst hb; simp [Atom.down, hi] at hd
      · simp only [liftT, hi, if_false, Ty.reach_up] at hb
        obtain ⟨x, hx, rfl⟩ := hb
        rw [Atom.down_up] at hd; cases hd
        exact ⟨.v (i - s.k), ⟨.v i, hc, by simp [Atom.down, hi]⟩, hx⟩
    | r p =>
      simp only [liftT, Ty.reach_up] at hb
      obtain ⟨x, hx, rfl⟩ := hb
      rw [Atom.down_up] at hd; cases hd
      exact ⟨.r p, ⟨.r p, hc, rfl⟩, hx⟩
  · rintro ⟨b, ⟨c, hc, hd⟩, hb⟩
    refine ⟨a.up s.k, ⟨c, hc, ?_⟩, Atom.down_up _ _⟩
    cases c with
    | v i =>
      by_cases hi : i < s.k
      · simp [Atom.down, hi] at hd
      · simp [Atom.down, hi] at hd; subst hd
        simp only [liftT, hi, if_false, Ty.reach_up]; exact ⟨a, hb, rfl⟩
    | r p =>
      simp [Atom.down] at hd; subst hd
      simp only [liftT, Ty.reach_up]; exact ⟨a, hb, rfl⟩

theorem Scheme.reach_up (s : Scheme) (k : Nat) (a : Atom) :
    (s.up k).reach (a.up k) ↔ s.reach a := by
  rw [Scheme.up, Scheme.reach_substT]
  constructor
  · rintro ⟨b, hb, h⟩
    cases b <;> simp [shiftV, Atom.up] at h <;> cases a <;> simp_all
  · intro h; exact ⟨a, h, by cases a <;> simp [shiftV, Atom.up]⟩

/-- The atoms a term writes down: run-time regions and the cell types of its
locations. -/
def Tm.Atoms (Ψ : List Ty) (e : Tm) : Caps := fun a =>
  (∃ p, e.Has (.rgn p) ∧ a = .r p) ∨ ∃ l t, e.Has (.loc l) ∧ Ψ[l]? = some t ∧ t.reach a

/-- What a term holds in a context: the premise of (Lam) and (Task). -/
def held (Ψ : List Ty) (Γ : List Scheme) (e : Tm) : Caps := fun a =>
  (∃ i s, e.Free i ∧ Γ[i]? = some s ∧ s.reach a) ∨ e.Atoms Ψ a

def Caps.Sub (C C' : Caps) : Prop := ∀ a, C a → C' a

theorem Caps.Sub.refl (C : Caps) : Caps.Sub C C := fun _ h => h

theorem Caps.Sub.trans {a b c : Caps} (h₁ : Caps.Sub a b) (h₂ : Caps.Sub b c) : Caps.Sub a c :=
  fun n h => h₂ n (h₁ n h)


end Flick
