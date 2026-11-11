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
     rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> (try cases a) <;> (try cases v) <;> cert_simp)
  | (simp only [Row.Sub]
     intro x hx
     rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> (try cases a) <;> (try cases v) <;> split_mem hx <;>
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
on it, rows and capture rows compared point by point. -/
macro "closed_tac" : tactic => `(tactic| (
  intro σ σ' hσ
  refine ⟨?_, by simp⟩
  simp [Ty.substT, Ty.spine, Stage.substT, hσ]
  all_goals first
    | (funext x
       rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> simp [Row.substT, Caps.substT, hσ])
    | ((repeat' apply And.intro) <;> funext x <;>
       rcases x with _ | a | ⟨_, a⟩ | a | ⟨v, a⟩ <;> simp [Row.substT, Caps.substT, hσ])))

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

/-- `σ.Holds Θ ts` of a scheme without facts, shifted under binders or not. -/
macro "holds" : tactic => `(tactic| first
  | exact Cert.holds_none _
  | (intro C r h
     simp only [Scheme.up, Scheme.substT, Scheme.mono, Facts.none_substT] at h
     exact h.elim))

/-- `Const.ok` of a cell's constant: what the content reaches is in the capacity. -/
macro "cell_ok" : tactic => `(tactic| (
  intro k hk
  left
  rcases k with n | p <;> cert_simp))

end Flick.Cert
