import Formal.Elab.CoreDict
import Formal.Elab.CoreCaseK
import Formal.Elab.Run
import Formal.Elab.DoNotation
import Formal.Elab.Comprehension

/-!
# λFlick⁺ as a `Target`: T6(a) for the core

`typingTarget` is the typing of the core of `Formal/` with dictionary records as the laws of
`Target`, each law one rule of the core (`CoreLaws.lean`, `CoreDict.lean`), under the
hypotheses `Real` on the core's signature. Its run is empty; the run of the core with the
fields of `Rel` is in `CoreRun.lean`. T6(a), its sugar forms and the closed case are its instances.
-/

namespace Flick.Elab

variable {P : Prog} {Sg : Sig} {N : Names}

/-- The typing of λFlick⁺, with an empty run. -/
def typingTarget (hR : Real P Sg N) : Target P where
  Has := Has Sg N
  var := has_var
  allLam := has_allLam
  allVar := has_allVar
  varAll := has_varAll
  const := has_const hR
  app := has_app
  ite := has_ite
  caseOpt := has_caseOpt hR
  con := has_con hR
  caseK he hK hT hlen hn hU hb hρ _ hcl hr := has_caseK hR he hK hT hlen hn hU hb hρ hcl hr
  caseKDead := has_caseKDead hR
  lam := has_lam
  let_ := has_let
  lamD := has_lamD
  dapp := has_dapp
  meth := has_meth hR
  dvar := has_dvar
  slot := has_slot hR
  dap := has_dap
  inst := has_inst hR
  strct := has_strct hR
  prim := has_prim hR
  St := Unit
  load _ := ()
  Step _ _ := False
  Obs := Empty
  obs _ := none
  Rel _ _ := True
  load_rel _ _ _ _ := trivial
  step_rel _ h := h.elim
  final_rel _ h := nomatch h

theorem Corr.nil (γ δ : Nat → Nat) : Corr N [] [] [] γ δ := ⟨fun _ => rfl, fun _ => rfl⟩

/-- **T6(a) in the core**: an elaboration is a core term typed at the translated type, in
every core context that corresponds to `⟦Γ⟧` and `⟦Δ⟧`. -/
theorem T6a_core (hR : Real P Sg N) {Γ Δ e τ φ e'} (h : Elab P Γ Δ e τ φ e') {Ψ Θ Γc γ δ}
    (hc : Corr N Γc (Γ.map Scheme.tr) Δ γ δ) :
    ∃ e'', Tr N γ δ e' e'' ∧ TypM Sg .live Ψ Θ Γc e'' (.ex (trT N τ) (trStage N φ)) :=
  T6a (typingTarget hR) h Ψ Θ Γc γ δ hc

/-- A closed program: its elaboration is a closed core term of the translated type. -/
theorem T6a_closed (hR : Real P Sg N) {e τ φ e'} (h : Elab P [] [] e τ φ e') (Ψ : List Flick.Ty)
    (Θ : Facts) : ∃ e'', Tr N id id e' e'' ∧ TypM Sg .live Ψ Θ [] e'' (.ex (trT N τ) (trStage N φ)) :=
  T6a_core hR h (Corr.nil id id)

/-- `f (λx. x)` for a constant `f : (∀a. a → a) → Int` elaborates, and its elaboration is typed
in the core: a parameter of rank 2. -/
example (hR : Real P Sg N) {c : Nat}
    (hc : P.consts c = some ⟨0, [], .arr .P (.all 1 (.arr .P (.var 0) (.var 0) .cnil)) intTy .cnil, .cnil⟩)
    (Ψ : List Flick.Ty) (Θ : Facts) :
    ∃ e'', Tr N id id (.app (.const c) (.lam (.var 0))) e'' ∧
      TypM Sg .live Ψ Θ [] e'' (.ex (trT N intTy) .pure) := by
  have hl : Elab P [] [] (.app (.const c) (.lam (.var 0))) intTy .P (.app (.const c) (.lam (.var 0))) := by
    refine .app (φ := .P) (ψ := .P) (.const (ts := []) (ds := []) hc rfl .nil) (.allLam (k := 1) ?_) (.inl rfl)
    exact .lam (.var (σ := ⟨0, [], .var 0, .cnil⟩) (ts := []) (ds := []) rfl rfl .nil)
  exact T6a_closed hR hl Ψ Θ

/-- **T6(a) for `do`** in the core. -/
theorem T6a_do_core (hR : Real P Sg N) (S : Sugar P) {Γ Δ ss r m b φ}
    (h : DoOk S Γ Δ ss r m b φ) :
    ∃ q e', Elab P Γ Δ (S.desugarDo ss r) (appQ m q b) φ e' ∧
      Has Sg N (Γ.map Scheme.tr) Δ e' (.mono (appQ m q b)) φ :=
  T6a_do S (typingTarget hR) h

/-- **T6(a) for `for`, `?` and blocks** in the core. -/
theorem T6a_sugar_core (hR : Real P Sg N) {Γ Δ e τ φ} (h : ∃ e', Elab P Γ Δ e τ φ e') :
    ∃ e', Elab P Γ Δ e τ φ e' ∧ Has Sg N (Γ.map Scheme.tr) Δ e' (.mono τ) φ :=
  T6a_sugar (typingTarget hR) h

end Flick.Elab
