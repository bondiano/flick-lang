import Formal.BelowStep

/-!
# A region is younger than the `with` frames around its `inside` form

Run-time forms stand only where the term evaluates, and under a `with` frame of the
`provide` region `q` a `scope` creates a region after `q`. A child keeps the `with`
frames of the place that registered it, so an obligation `Provided {N} ϱ` that a
region `ϱ` of that place discharged meets, in the child, frames older than `ϱ`
(`KidTy.lean`). `e.Asc n`: the `inside` forms of `e` name regions `≥ n`, and those
under a `with` frame of `q` regions `> q`.
-/

namespace Flick

def Tm.Asc : Tm → Nat → Prop
  | .var _, _ => True
  | .lam b, _ => b.NoIns
  | .app f a, n => f.Asc n ∧ a.Asc n ∧ (a.NoIns ∨ Value f)
  | .let_ e b, n => e.Asc n ∧ b.NoIns
  | .letrec f b, _ => f.NoIns ∧ b.NoIns
  | .fix f, _ => f.NoIns
  | .lit _, _ => True
  | .loc _, _ => True
  | .const _, _ => True
  | .ctor _, _ => True
  | .rnil, _ => True
  | .rcons _ e r, n => e.Asc n ∧ r.Asc n ∧ (r.NoIns ∨ Value e)
  | .proj e _, n => e.Asc n
  | .update e _ e', n => e.Asc n ∧ e'.Asc n ∧ (e'.NoIns ∨ Value e)
  | .match_ e al, n => e.Asc n ∧ al.NoIns
  | .anil, _ => True
  | .acons _ b r, _ => b.NoIns ∧ r.NoIns
  | .aguard _ g b r, _ => g.NoIns ∧ b.NoIns ∧ r.NoIns
  | .todo, _ => True
  | .task e, _ => e.NoIns
  | .await e, n => e.Asc n
  | .raise e, n => e.Asc n
  | .try_ e al, n => e.Asc n ∧ al.NoIns
  | .handle v al, _ => v.NoIns ∧ al.NoIns
  | .provide l e, n => l.Asc n ∧ e.NoIns
  | .with_ q θ e, n => θ.NoIns ∧ e.Asc (max n (q + 1))
  | .enil, _ => True
  | .econs _ v θ, _ => v.NoIns ∧ θ.NoIns
  | .bind _ e, n => e.Asc n
  | .join e₁ e₂, n => e₁.Asc n ∧ e₂.Asc n ∧ (e₂.NoIns ∨ Value e₁)
  | .scope e, _ => e.NoIns
  | .inside p e, n => n ≤ p ∧ e.Asc n
  | .cap _, _ => True
  | .own _, _ => True

theorem Tm.Asc.of_noIns : ∀ {e : Tm} {n : Nat}, e.NoIns → e.Asc n := by
  intro e
  induction e <;> intro n hn <;> simp only [Tm.NoIns, Tm.Has, not_or] at hn <;>
    simp_all [Tm.Asc, Tm.NoIns]

theorem Value.ascNoIns {e : Tm} {n : Nat} (hv : Value e) (h : e.Asc n) : e.NoIns := by
  induction hv generalizing n with
  | lam => exact h
  | fix => exact h
  | task => exact h
  | rcons _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | ctorApp _ _ _ ih₁ ih₂ => rintro (h' | h'); exact ih₁ h.1 h'; exact ih₂ h.2.1 h'
  | constApp _ _ ih => rintro (h' | h'); cases h'; exact ih h.2.1 h'
  | constApp2 _ _ _ _ ih₁ ih₂ =>
    rintro ((h' | h') | h'); cases h'; exact ih₁ h.1.2.1 h'; exact ih₂ h.2.1 h'
  | econs => rintro (h' | h'); exact h.1 h'; exact h.2 h'
  | _ => intro h'; cases h'

/-- The bound under a frame. -/
def Frame.ascB (n : Nat) : Frame → Nat
  | .with_ q _ => max n (q + 1)
  | _ => n

def Frame.Asc (n : Nat) : Frame → Prop
  | .appL a | .let_ a | .match_ a | .rconsL _ a | .updL _ a | .try_ a | .provide a | .joinL a =>
    a.NoIns
  | .appR v | .rconsR _ v | .updR v _ | .joinR v => v.Asc n ∧ Value v
  | .with_ _ θ => θ.NoIns
  | .inside p => n ≤ p
  | _ => True

theorem Frame.Asc.plug {F : Frame} {n : Nat} {r : Tm} (hF : F.Asc n) (hr : r.Asc (F.ascB n)) :
    (F.plug r).Asc n := by
  cases F <;> simp only [Frame.Asc, Frame.ascB, Frame.plug, Tm.Asc] at hF hr ⊢ <;>
    first
    | exact ⟨hr, Tm.Asc.of_noIns hF, .inl hF⟩
    | exact ⟨hF.1, hr, .inr hF.2⟩
    | exact ⟨hr, hF⟩
    | exact ⟨hF, hr⟩
    | exact hr

theorem Frame.asc_of_plug {F : Frame} {n : Nat} {r : Tm} (hok : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).Asc n) : F.Asc n ∧ r.Asc (F.ascB n) := by
  cases F <;> simp only [Frame.Asc, Frame.ascB, Frame.plug, Frame.ok, Tm.Asc] at hok h ⊢ <;> grind

def ascBE (n : Nat) : List Frame → Nat
  | [] => n
  | F :: E => F.ascB (ascBE n E)

def AscE (n : Nat) : List Frame → Prop
  | [] => True
  | F :: E => AscE n E ∧ F.Asc (ascBE n E)

theorem AscE.fill {n : Nat} {r : Tm} :
    ∀ {E : List Frame}, AscE n E → r.Asc (ascBE n E) → (plug E r).Asc n
  | [], _, hr => hr
  | _ :: _, ⟨hE, hF⟩, hr => AscE.fill (r := Frame.plug _ r) hE (hF.plug hr)

theorem asc_of_plug {n : Nat} {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).Asc n → AscE n E ∧ r.Asc (ascBE n E)
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := asc_of_plug (r := F.plug r) (not_value_plug hv F) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.asc_of_plug hF hv h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

theorem ascBE_le {n m : Nat} {E : List Frame} (hn : n ≤ m)
    (hq : ∀ q θ, .with_ q θ ∈ E → q < m) : ascBE n E ≤ m := by
  induction E with
  | nil => exact hn
  | cons F E ih =>
    have := ih fun q θ h => hq q θ (List.mem_cons_of_mem _ h)
    cases F <;> simp only [ascBE, Frame.ascB] <;> try exact this
    have := hq _ _ List.mem_cons_self
    omega

/-- An `inside` form under a `with` frame of `q` names a region younger than `q`. -/
theorem AscE.with_inside {n q p : Nat} {θ : Tm} :
    ∀ {E₀ E₁ : List Frame}, AscE n (E₀ ++ .with_ q θ :: E₁) → .inside p ∈ E₀ → q < p
  | F :: E₀, E₁, h, hp => by
    simp only [List.cons_append, AscE] at h
    rcases List.mem_cons.mp hp with rfl | hp
    · have key : ∀ E : List Frame, q + 1 ≤ ascBE n (E ++ .with_ q θ :: E₁) := by
        intro E
        induction E with
        | nil =>
          simp only [List.nil_append, ascBE, Frame.ascB]
          exact Nat.le_max_right _ _
        | cons G E ih => cases G <;> simp only [List.cons_append, ascBE, Frame.ascB] at ih ⊢ <;> omega
      have := key E₀
      have := h.2; simp only [Frame.Asc] at this; omega
    · exact AscE.with_inside h.1 hp

/-- A child or a finalizer in the `with` frames of its context. -/
theorem withs_asc {x : Tm} :
    ∀ {E : List Frame}, (∀ q θ, .with_ q θ ∈ E → θ.NoIns) → (∀ m, x.Asc m) → ∀ m, (withs E x).Asc m
  | [], _, hx => hx
  | F :: E, hE, hx => by
    simp only [withs]
    refine withs_asc (fun q θ h => hE q θ (List.mem_cons_of_mem _ h)) ?_
    cases F with
    | with_ q θ => exact fun m => ⟨hE q θ List.mem_cons_self, hx _⟩
    | _ => exact hx

end Flick
