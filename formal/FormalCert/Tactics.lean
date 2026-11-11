import Formal

/-!
# Tactics of a certificate

A certificate (`docs/formal/certificates.md`) types a closed program of the model's generator
with every type written out: rows and capture rows are finite sets, written as predicates
`fun x => x = e₁ ∨ …`. Its side conditions are inclusions and equalities of such sets, under
substitution too (`Row.substT`, `Caps.substT` are existentials). These tactics close them the
way `elide_eq` does (`Live/Elide.lean`): split an element by its constructor, an atom into
`.v n` or `.r p`, and leave the rest to `simp_all` and `omega`. Each is kernel-checked: a
tactic that fails is a certificate that fails, never an assumption.
-/

namespace Flick

/-- The one fact `Outlives C₀ s₀`. -/
def Facts.single (C₀ : Caps) (s₀ : Ty) : Facts := fun C s => C = C₀ ∧ s = s₀

@[simp] theorem Facts.single_substT (C₀ : Caps) (s₀ : Ty) (σ : Atom → Ty) :
    (Facts.single C₀ s₀).substT σ = Facts.single (C₀.substT σ) (s₀.substT σ) := by
  funext C s
  simp only [Facts.substT, Facts.single, eq_iff_iff]
  constructor
  · rintro ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩; exact ⟨rfl, rfl⟩
  · rintro ⟨rfl, rfl⟩; exact ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩

@[simp] theorem Facts.bnd_substT (i : Nat) (D : Caps) (σ : Atom → Ty) :
    (Facts.bnd i D).substT σ = Facts.single (Caps.substT σ (· = .v i)) (.caps (D.substT σ)) := by
  funext C s
  simp only [Facts.substT, Facts.bnd, Facts.single, eq_iff_iff]
  constructor
  · rintro ⟨_, _, ⟨rfl, rfl⟩, rfl, rfl⟩; exact ⟨rfl, by simp [Ty.substT]⟩
  · rintro ⟨rfl, rfl⟩; exact ⟨_, _, ⟨rfl, rfl⟩, rfl, by simp [Ty.substT]⟩

theorem Facts.single_congr {C C' : Caps} {t t' : Ty} (hC : C = C') (ht : t = t') :
    Facts.single C t = Facts.single C' t' := by
  subst hC; subst ht; rfl

theorem Facts.union_congr {A A' B B' : Facts} (hA : A = A') (hB : B = B') :
    Facts.union A B = Facts.union A' B' := by
  subst hA; subst hB; rfl

theorem Caps.substT_congr {C : Caps} {σ σ' : Atom → Ty} (h : ∀ b, C b → σ b = σ' b) :
    C.substT σ = C.substT σ' := by
  funext a
  simp only [Caps.substT, eq_iff_iff]
  exact ⟨fun ⟨b, hb, hr⟩ => ⟨b, hb, h b hb ▸ hr⟩, fun ⟨b, hb, hr⟩ => ⟨b, hb, (h b hb).symm ▸ hr⟩⟩

end Flick

namespace Flick.Cert

/-- The simp set of the finite sets of a certificate: elements, atoms, reach, substitution. -/
macro "cert_simp" : tactic => `(tactic| (simp_all [Row.union, Row.ofCaps, Row.plain, Row.single,
  Row.none, Row.substT, Caps.substT, Caps.none, Ty.reach, instT, Atom.up, Atom.ty, Ty.asRow,
  Row.err, Row.scope, Row.disch, shiftV] <;> omega))

/-- `h : x ∈ A` of a finite set `A`, split into one goal for each element `A` names. -/
macro "split_mem " h:ident : tactic => `(tactic| (
  (try simp only [Row.union, Row.ofCaps, Row.single, Row.none, Caps.none, Ty.reach, Row.scope,
    Row.disch] at $h:ident)
  repeat' (obtain $h:ident | $h:ident := $h:ident)))

/-- `C = C'` or `C ⊆ C'` of finite sets of atoms. -/
macro "caps_set" : tactic => `(tactic| first
  | (funext x; rcases x with n | p <;> cert_simp)
  | (simp only [Caps.Sub]; intro x hx; rcases x with n | p <;> split_mem hx <;> cert_simp))

/-- `r = r'` or `r ⊆ r'` of finite rows. -/
macro "row_set" : tactic => `(tactic| first
  | (funext x
     rcases x with _ | a | ⟨_, a⟩ | ⟨a, _⟩ | ⟨v, _, a⟩ <;> (try cases a) <;> (try cases v) <;> cert_simp)
  | (simp only [Row.Sub]
     intro x hx
     rcases x with _ | a | ⟨_, a⟩ | ⟨a, _⟩ | ⟨v, _, a⟩ <;> (try cases a) <;> (try cases v) <;> split_mem hx <;>
       cert_simp))

/-- `φ ≤ φ'` of concrete stages, `Stage.use` unfolded. -/
macro "stage_le" : tactic => `(tactic| first
  | exact Stage.le_refl _
  | (simp only [Stage.use, Stage.le, Stage.use_none] <;>
     first
     | trivial
     | exact Stage.le_refl _
     | (refine ⟨?_, ?_⟩ <;> row_set)))

/-- `σ.inst ts = T` of a concrete scheme at a concrete instance. -/
macro "inst_eq" : tactic => `(tactic| (
  simp only [Scheme.inst, Scheme.mono, Scheme.up, Scheme.substT, liftT_zero, Ty.up_def, Ty.substT,
    Stage.substT, Ty.spine, Lit.base]
  try simp [instT, Ty.substT, Stage.substT, Ty.spine, shiftV, Lit.base]
  all_goals first
    | caps_set
    | row_set
    | ((repeat' apply And.intro) <;> first | caps_set | row_set)))

/-- `s.Closed s.k` of a concrete scheme: two substitutions that agree on its variables agree
on it, rows and capture rows compared point by point, its facts by their parts. -/
macro "closed_tac" : tactic => `(tactic| (
  intro σ σ' hσ
  refine ⟨?_, ?_⟩
  · simp [Ty.substT, Ty.spine, Stage.substT, hσ]
    all_goals first
      | (funext x
         rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> simp [Row.substT, Caps.substT, hσ, and_assoc])
      | ((repeat' apply And.intro) <;> funext x <;>
         rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> simp [Row.substT, Caps.substT, hσ, and_assoc])
  · simp only [Facts.single_substT, Facts.bnd_substT, Facts.union_substT, Facts.none_substT]
    repeat' (first | apply Facts.union_congr | apply Facts.single_congr)
    all_goals first
      | rfl
      | (apply Caps.substT_congr; intro b hb; subst hb; simp [hσ])
      | (congr 1; apply Caps.substT_congr; intro b hb; subst hb; simp [hσ])
      | simp [Ty.substT, hσ]))

/-- `LeLive t t'` of concrete types: the same type, or an arrow or a computation whose stage
and reach widen. -/
syntax "le_live" : tactic
macro_rules
  | `(tactic| le_live) => `(tactic| first
    | exact LeLive.refl
    | (refine LeLive.arrow ?_ ?_ ?_ ?_ <;> first | le_live | stage_le | caps_set)
    | (refine LeLive.eff ?_ ?_ ?_ <;> first | le_live | stage_le | caps_set))

/-- `φ₀.ObOut o` of a concrete stage with no obligation about `o`. -/
macro "ob_out" : tactic => `(tactic| (
  refine ⟨fun N a h => ?_, fun v a h => ?_⟩ <;>
    simp_all [Stage.svc, Row.Named, Row.single, Row.none, Row.union]))

/-- `Outl Θ C s` of concrete sets: each atom of `C` is reached by `s` or is in a fact about
`s` of `Θ`. -/
macro "outl" : tactic => `(tactic| (
  intro k hk
  rcases k with n | p <;>
    simp_all [Facts.union, Flick.Facts.single, Facts.none, Facts.bnd, Facts.substT, Ty.substT,
      shiftV, Ty.reach, Caps.substT, instT, liftT, Ty.up, Ty.ren, Atom.up, Atom.ty] <;> omega))

/-- `σ.Holds Θ ts`: each fact of the scheme at the instance, shifted under binders or not. -/
macro "holds" : tactic => `(tactic| first
  | exact Cert.holds_none _
  | (intro C r h
     simp only [Scheme.up, Scheme.substT, Scheme.mono, liftT_zero, Flick.Facts.single_substT,
       Flick.Facts.bnd_substT, Facts.union_substT, Facts.none_substT, Facts.union,
       Flick.Facts.single, Facts.none] at h
     repeat' (first | (obtain ⟨rfl, rfl⟩ := h) | (rcases h with h | h))
     all_goals outl))

/-- `Const.ok` of a cell's constant: what the content reaches is in the capacity. -/
macro "cell_ok" : tactic => `(tactic| (
  intro k hk
  left
  rcases k with n | p <;> cert_simp))

end Flick.Cert
