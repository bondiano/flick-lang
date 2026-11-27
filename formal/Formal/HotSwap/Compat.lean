import Formal.Soundness
import Formal.Effects

/-!
# Hot-swap scheme compatibility and compatible signature updates
-/

namespace Flick.HotSwap

/-! ## 1. Scheme subsumption (hot-swap compatibility) -/

/-- `σ_new` is a hot-swap-compatible replacement for `σ_old` under `S`:
every valid instantiation of `σ_old` under any fact context `Θ` can be met by
a valid instantiation of `σ_new` whose body is a live subtype (`LeLive`) of
`σ_old`'s instantiated body.

In particular:
- generalizing a scheme is accepted, narrowing is rejected;
- effect/service/error rows on covariant arrows and `Eff` are no wider;
- capture and region qualifiers on contravariant parameters are no narrower,
  and on covariant callables/results require no extra live regions. -/
def SchemeSubsumes (S : Sig) (σ_new σ_old : Scheme) : Prop :=
  ∀ (Θ : Facts) (ts : List Ty), ts.length = σ_old.k → σ_old.Holds Θ ts →
    ∃ us : List Ty, us.length = σ_new.k ∧ σ_new.Holds Θ us ∧
      LeLive S (σ_new.inst us) (σ_old.inst ts)

theorem SchemeSubsumes.refl (S : Sig) (σ : Scheme) : SchemeSubsumes S σ σ :=
  fun _ ts hlen hH => ⟨ts, hlen, hH, .refl⟩

theorem SchemeSubsumes.trans {S : Sig} {σ₁ σ₂ σ₃ : Scheme}
    (h₁₂ : SchemeSubsumes S σ₁ σ₂) (h₂₃ : SchemeSubsumes S σ₂ σ₃) :
    SchemeSubsumes S σ₁ σ₃ := by
  intro Θ ts hlen hH
  obtain ⟨ms, hml, hmH, hle₂₃⟩ := h₂₃ Θ ts hlen hH
  obtain ⟨us, hul, huH, hle₁₂⟩ := h₁₂ Θ ms hml hmH
  exact ⟨us, hul, huH, hle₁₂.trans hle₂₃⟩

/-- Concrete witness: generalizing `Int -> Int` to `∀ α. α -> α` is compatible. -/
theorem subsumes_generalize_mono (S : Sig) :
    SchemeSubsumes S
      ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩
      (Scheme.mono (.arrow (.base .int) .pure (.base .int) Caps.none)) := by
  intro Θ ts _ _
  refine ⟨[.base .int], rfl, fun _ _ h => h.elim, ?_⟩
  simp [Scheme.inst, Scheme.mono, Ty.substT, instT, Caps.none_substT]
  exact .refl

/-- Concrete witness: narrowing `∀ α. α -> α` to `Int -> Int` is rejected. -/
theorem not_subsumes_narrow_poly (S : Sig) :
    ¬ SchemeSubsumes S
      (Scheme.mono (.arrow (.base .int) .pure (.base .int) Caps.none))
      ⟨1, Facts.none, .arrow (.tvar 0) .pure (.tvar 0) Caps.none⟩ := by
  intro h
  obtain ⟨us, hus, _, hle⟩ := h Facts.none [.base .bool] rfl (fun _ _ hx => hx.elim)
  cases us with
  | nil =>
    simp [Scheme.inst, Scheme.mono, Ty.substT, instT, Caps.none_substT] at hle
    obtain ⟨_, _, _, _, hEq, hArg, _, _, _⟩ := hle.arrow_left
    cases hEq
    have hBase := hArg.eq_plain rfl
    cases hBase.base_right
  | cons => simp [Scheme.mono] at hus

/-- Concrete witness: widening a pure arrow `Int -> Int` to an effectful arrow
`Int ->{E; ∅} Int` is rejected. -/
theorem not_subsumes_widen_effect (S : Sig) (E : Name) :
    ¬ SchemeSubsumes S
      (Scheme.mono (.arrow (.base .int) (.rt (.rnm E (.adt E)) .cnil) (.base .int) Caps.none))
      (Scheme.mono (.arrow (.base .int) .pure (.base .int) Caps.none)) := by
  intro h
  obtain ⟨us, hus, _, hle⟩ := h Facts.none [] rfl (fun _ _ hx => hx.elim)
  cases us with
  | nil =>
    simp [Scheme.inst, Scheme.mono, Ty.substT, Caps.none_substT] at hle
    have huse := LeLive.arrow_use hle
    simp [Stage.use, Ty.asStage, Stage.le] at huse
  | cons => simp [Scheme.mono] at hus

/-- Concrete witness: narrowing a parameter's capture qualifier from `{r}` to `∅`
(demanding a pure-captured callback where the old scheme accepted one capturing `r`)
is rejected. -/
theorem not_subsumes_narrow_param_cap (S : Sig) (p : Nat) :
    ¬ SchemeSubsumes S
      (Scheme.mono (.arrow (.arrow (.base .unit) (.rt .cnil .cnil) (.base .unit) Caps.none)
        .pure (.base .unit) Caps.none))
      (Scheme.mono (.arrow (.arrow (.base .unit) (.rt .cnil .cnil) (.base .unit) (· = .r p))
        .pure (.base .unit) Caps.none)) := by
  intro h
  obtain ⟨us, hus, _, hle⟩ := h Facts.none [] rfl (fun _ _ hx => hx.elim)
  cases us with
  | nil =>
    have hcap : Caps.substT (instT []) (· = .r p) = (· = .r p) := by
      funext a; apply propext
      constructor
      · rintro ⟨b, rfl, ha⟩; simpa [instT, Ty.reach] using ha
      · intro ha; exact ⟨.r p, rfl, by simpa [instT, Ty.reach] using ha⟩
    have hle' : LeLive S
        (.arrow (.arrow (.base .unit) (.rt .cnil .cnil) (.base .unit) Caps.none)
          .pure (.base .unit) Caps.none)
        (.arrow (.arrow (.base .unit) (.rt .cnil .cnil) (.base .unit) (· = .r p))
          .pure (.base .unit) Caps.none) := by
      simpa [Scheme.inst, Scheme.mono, Ty.substT, Caps.none_substT, hcap] using hle
    obtain ⟨_, _, _, _, hEq, hParam, _, _, _⟩ := hle'.arrow_left
    cases hEq
    have huse := LeLive.arrow_use hParam
    have hsvc := Stage.le_svc huse (.atm (.r p)) (Or.inr rfl)
    rcases hsvc with hnil | hnone
    · exact hnil
    · exact hnone
  | cons => simp [Scheme.mono] at hus

/-! ## 2. Compatible signature updates and root updates -/

/-- `S'` is a hot-swap-compatible successor of `S`: every non-global declaration
is unchanged, and every global declared in `S` remains declared in `S'` at a
scheme that subsumes the old one (new globals may also be added). -/
structure SigSwapCompat (S S' : Sig) : Prop where
  ctor : S'.ctor = S.ctor
  res : S'.res = S.res
  impl : S'.impl = S.impl
  arity : S'.arity = S.arity
  dict : S'.dict = S.dict
  op : S'.op = S.op
  hidden : S'.hidden = S.hidden
  var : S'.var = S.var
  dictVar : S'.dictVar = S.dictVar
  glob : ∀ g σ_old, S.glob g = some σ_old →
    ∃ σ_new, S'.glob g = some σ_new ∧ SchemeSubsumes S σ_new σ_old

/-- Update a single global cell's scheme in `S`. -/
def sigWithGlob (S : Sig) (g : Nat) (σ : Scheme) (hclosed : σ.Closed σ.k) : Sig where
  ctor := S.ctor
  impl := S.impl
  ctor_closed := S.ctor_closed
  ctor_reach := S.ctor_reach
  res := S.res
  res_closed := S.res_closed
  arity := S.arity
  impl_closed := S.impl_closed
  dict := S.dict
  glob := fun g' => if g' = g then some σ else S.glob g'
  op := S.op
  op_closed := S.op_closed
  op_impl := S.op_impl
  op_field := S.op_field
  dict_closed := S.dict_closed
  glob_closed := by
    intro g' s hs
    by_cases h : g' = g
    · simp only [h, ↓reduceIte, Option.some.injEq] at hs
      subst hs; exact hclosed
    · simp only [h, ↓reduceIte] at hs
      exact S.glob_closed g' s hs
  hidden := S.hidden
  hidden_ok := S.hidden_ok
  var := S.var
  var_ok := S.var_ok
  dictVar := S.dictVar
  dictVar_ok := S.dictVar_ok

theorem sigWithGlob_compat (S : Sig) (g : Nat) (σ_new : Scheme) (hclosed : σ_new.Closed σ_new.k)
    (hsub : ∀ σ_old, S.glob g = some σ_old → SchemeSubsumes S σ_new σ_old) :
    SigSwapCompat S (sigWithGlob S g σ_new hclosed) where
  ctor := rfl
  res := rfl
  impl := rfl
  arity := rfl
  dict := rfl
  op := rfl
  hidden := rfl
  var := rfl
  dictVar := rfl
  glob := by
    intro g' σ_old hg'
    by_cases h : g' = g
    · subst h
      exact ⟨σ_new, by simp [sigWithGlob], hsub σ_old hg'⟩
    · exact ⟨σ_old, by simp [sigWithGlob, h, hg'], SchemeSubsumes.refl S σ_old⟩

/-- Update a single global cell's code body in `Roots`. -/
def rootsWithGlob (root : Roots) (g : Nat) (v : Tm) : Roots where
  svc := root.svc
  glob := fun g' => if g' = g then some v else root.glob g'


end Flick.HotSwap
