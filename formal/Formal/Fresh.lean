import Formal.Lemma3

/-!
# Regions not yet created

A typing derivation may name a run-time region before it exists: the type of a
hole is chosen by the derivation. `kill n` sends every region `≥ n` to `Unit`, so a
derivation renamed by it names only regions that exist (`Young`-free). A term
writes only regions that exist (`Tm.Below`), so `kill n` fits it, and L3 carries
the renaming through the whole derivation (`Refill`). A new scope then gets a region
no type of the derivation mentions, a new cell a type without regions created
later (L6), and the facts of a scope hold of the order of creation (`Facts.age`).
-/

namespace Flick

variable {S : Sig}

/-- Send every run-time region `≥ n` to `Unit`. -/
def kill (n : Nat) : Atom → Ty
  | .v i => .tvar i
  | .r p => if p < n then .rg p else .base .unit

/-- A type that names no run-time region `≥ n`. -/
def Ty.YF (n : Nat) (t : Ty) : Prop := t.substT (kill n) = t

/-- A term writes only regions `< n`. -/
def Tm.Below (n : Nat) (e : Tm) : Prop := ∀ p, (e.Has (.rgn p) ∨ e.Has (.prov p)) → p < n

theorem kill_reach {n : Nat} {b a : Atom} (h : (kill n b).reach a) : a = b ∧ ∀ p, a = .r p → p < n := by
  cases b with
  | v i => simp [kill] at h; subst h; simp
  | r q =>
    simp only [kill] at h
    split at h
    · simp at h; subst h; exact ⟨rfl, fun p hp => by cases hp; assumption⟩
    · simp [Ty.reach, Caps.none] at h

theorem kill_atom (n : Nat) (b a : Atom) (h : (kill n b).reach a) : kill n b = a.ty := by
  obtain ⟨rfl, -⟩ := kill_reach h
  cases a with
  | v i => rfl
  | r q =>
    by_cases hq : q < n
    · simp [kill, hq, Atom.ty]
    · simp [kill, hq, Ty.reach, Caps.none] at h

theorem kill_noNames (n : Nat) : NoNames (kill n) := by
  intro b N hb
  cases b with
  | v i => simp [kill, Ty.asRow_tvar] at hb
  | r q =>
    simp only [kill] at hb
    split at hb <;> simp [Ty.asRow, Row.ofCaps] at hb

/-- `kill n` sends a row variable to itself or to nothing. -/
theorem kill_asRow {n : Nat} {b : Atom} {x : Elem} (h : (kill n b).asRow x) : x = .rv b := by
  cases b with
  | v i => simpa [kill, Ty.asRow_tvar] using h
  | r q =>
    simp only [kill] at h
    split at h
    · simpa [Ty.asRow_rg] using h
    · cases x <;> simp [Ty.asRow, Row.ofCaps, Ty.reach, Caps.none] at h

theorem Fits.kill {n : Nat} {e : Tm} (h : e.Below n) : Fits (kill n) e :=
  ⟨fun p hp => by simp [Flick.kill, h p hp], fun _ b _ hb => (kill_reach hb).1.symm,
    fun _ b a hb => kill_atom n b a hb, fun _ => kill_noNames n⟩

theorem Ty.reach_kill {n : Nat} {t : Ty} {p : Nat} (h : (t.substT (kill n)).reach (.r p)) : p < n := by
  obtain ⟨b, _, hb⟩ := (Ty.reach_substT _ _ _).mp h
  exact (kill_reach hb).2 p rfl

theorem kill_kill (n : Nat) (a : Atom) : (kill n a).substT (kill n) = kill n a := by
  cases a with
  | v i => rfl
  | r p => by_cases h : p < n <;> simp [kill, h, Ty.substT]

theorem Ty.yf_kill (n : Nat) (t : Ty) : (t.substT (kill n)).YF n := by
  simp only [Ty.YF, Ty.substT_comp]; exact Ty.substT_congr (kill_kill n)

theorem Ty.YF.reach {n : Nat} {t : Ty} {p : Nat} (h : t.YF n) (ht : t.reach (.r p)) : p < n := by
  rw [← h] at ht; exact Ty.reach_kill ht

theorem Facts.age_kill (n : Nat) : Entails (Facts.age.substT (kill n)) Facts.age := by
  intro C s hC x hx
  rcases hC x hx with h | ⟨C', ⟨C₀, s₀, h₀, rfl, rfl⟩, h₂⟩
  · exact .inl h
  · refine .inr ⟨_, fun q p hq hp => ?_, h₂⟩
    obtain ⟨b, hb, hb'⟩ := hq
    obtain ⟨b', hb'', hp'⟩ := (Ty.reach_substT _ _ _).mp hp
    obtain ⟨rfl, _⟩ := kill_reach hb'
    obtain ⟨rfl, _⟩ := kill_reach hp'
    exact h₀ q p hb hb''

theorem Facts.age_up (k : Nat) : Entails (Facts.age.substT (shiftV k)) Facts.age := by
  intro C s hC x hx
  rcases hC x hx with h | ⟨C', ⟨C₀, s₀, h₀, rfl, rfl⟩, h₂⟩
  · exact .inl h
  · refine .inr ⟨_, fun q p hq hp => ?_, h₂⟩
    obtain ⟨b, hb, hb'⟩ := hq
    obtain ⟨b', hb'', hp'⟩ := (Ty.reach_substT _ _ _).mp hp
    cases b <;> simp [shiftV] at hb'
    cases b' <;> simp [shiftV] at hp'
    subst hb' hp'; exact h₀ _ _ hb hb''

end Flick
