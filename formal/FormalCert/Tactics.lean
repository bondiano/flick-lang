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
  Row.none, Row.substT, Caps.substT, Caps.none, Ty.reach, instT, liftT, Ty.up, Ty.ren, Atom.up,
  Atom.ty, Ty.asRow, Row.err, Row.scope, Row.disch, Row.back, shiftV, and_assoc] <;> omega))

/-- `h : x ∈ A` of a finite set `A`, split into one goal for each element `A` names. Only a
disjunction is split: `rcases` of an `∃` over atoms would split the witness, its `Nat`, and so
on without end. -/
macro "split_mem " h:ident : tactic => `(tactic| (
  (try simp only [Row.union, Row.ofCaps, Row.single, Row.none, Caps.none, Ty.reach, Row.scope,
    Row.disch] at $h:ident)
  repeat' (obtain $h:ident | $h:ident := ($h:ident : _ ∨ _))))

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
  try simp [instT, liftT, Ty.up, Ty.ren, Ty.substT, Stage.substT, Ty.spine, shiftV, Lit.base]
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

theorem Ty.unspine_eq : ∀ {a : Ty} {T : Name} {us : List Ty}, a.unspine = some (T, us) →
    a = Ty.tcon T us
  | .adt _, _, _, h => by simp [Ty.unspine] at h; obtain ⟨rfl, rfl⟩ := h; rfl
  | .tapp f x, T, us, h => by
    simp only [Ty.unspine, Option.map_eq_some_iff, Prod.exists] at h
    obtain ⟨T', vs, hf, he⟩ := h
    simp only [Prod.mk.injEq] at he
    obtain ⟨rfl, rfl⟩ := he
    rw [Ty.unspine_eq hf]; simp [Ty.tcon]

/-- The premises of `LeLive.data` for the arguments from the `j`-th on, one by one. -/
def DataArgs (S : Sig) (T : Name) : Nat → List Ty → List Ty → Prop
  | j, u :: us, u' :: us' => Caps.Sub u.reach u'.reach ∧
      (S.hidden T ≠ some j → (S.var T j = .pos → LeLive S u u') ∧
        (S.var T j = .neg → LeLive S u' u) ∧ (S.var T j = .inv → u = u')) ∧
      DataArgs S T (j + 1) us us'
  | _, [], [] => True
  | _, _, _ => False

theorem DataArgs.at {T : Name} : ∀ {n : Nat} {us us' : List Ty}, DataArgs S T n us us' →
    us.length = us'.length ∧ ∀ j u u', us[j]? = some u → us'[j]? = some u' →
      Caps.Sub u.reach u'.reach ∧ (S.hidden T ≠ some (n + j) →
        (S.var T (n + j) = .pos → LeLive S u u') ∧ (S.var T (n + j) = .neg → LeLive S u' u) ∧
        (S.var T (n + j) = .inv → u = u'))
  | _, [], [], _ => ⟨rfl, fun _ _ _ h => by simp at h⟩
  | n, u :: us, u' :: us', ⟨hr, hv, h⟩ => by
    obtain ⟨hl, hs⟩ := DataArgs.at h
    refine ⟨by simp [hl], fun j x x' hx hx' => ?_⟩
    rcases j with _ | j
    · simp at hx hx'; subst hx; subst hx'; exact ⟨hr, hv⟩
    · simp at hx hx'
      have := hs j x x' hx hx'
      rwa [show n + 1 + j = n + (j + 1) by omega] at this
  | _, [], _ :: _, h | _, _ :: _, [], h => h.elim

/-- `LeLive.data` of two concrete spines of the same type. -/
theorem LeLive.data_of {a b : Ty} {T : Name} {us us' : List Ty} (ha : a.unspine = some (T, us))
    (hb : b.unspine = some (T, us')) (h : DataArgs S T 0 us us') : LeLive S a b := by
  obtain ⟨hl, hs⟩ := h.at
  refine .data hl (fun j u u' x x' => (hs j u u' x x').1)
    (fun j u u' x x' hh hv => ?_) (fun j u u' x x' hh hv => ?_) (fun j u u' x x' hh hv => ?_)
    (Ty.unspine_eq ha) (Ty.unspine_eq hb)
  all_goals have := (hs j u u' x x').2 (by simpa using hh); simp only [Nat.zero_add] at this
  · exact this.1 hv
  · exact this.2.1 hv
  · exact (this.2.2 hv) ▸ Ty.Equiv.refl

/-- Closes a goal under `h : S.var T j = s` where `T` of the signature has no such variance;
`GenSig.lean`, which knows the variances, gives its rule. -/
syntax "var_absurd" : tactic

/-- `LeLive t t'` of concrete types: the same type, an arrow or a computation whose stage
and reach widen, a record whose fields do, or data whose arguments do by their variances.
A constructor's type `Sig.ctorTy` is written out first. -/
syntax "le_live" : tactic
macro_rules
  | `(tactic| le_live) => `(tactic| first
    | exact LeLive.refl
    | (unfold Sig.ctorTy
       simp only [Ty.arrows, Ty.tcon, List.map, List.foldl, Ty.substT, Stage.substT]
       le_live)
    | (refine LeLive.arrow ?_ ?_ ?_ ?_ <;> first | le_live | stage_le | caps_set)
    | (refine LeLive.eff ?_ ?_ ?_ <;> first | le_live | stage_le | caps_set)
    | (refine LeLive.rcons' ?_ ?_ <;> le_live)
    | (refine LeLive.data_of rfl rfl ?_
       simp only [DataArgs]
       repeat' (first | apply And.intro | intro _)
       all_goals first | trivial | exact Caps.Sub.refl _ | rfl | le_live | caps_set | var_absurd))

/-- `φ₀.ObOut o` of a concrete stage with no obligation about `o`. -/
macro "ob_out" : tactic => `(tactic| (
  refine ⟨fun N a h => ?_, fun v G a h => ?_⟩ <;>
    simp_all [Stage.svc, Row.Named, Row.single, Row.none, Row.union]))

/-- `Outl Θ C s` of concrete sets: each atom of `C` is reached by `s` or is in a fact about
`s` of `Θ`. -/
theorem outl_declared {Θ : Facts} {C : Caps} {s : Ty}
    (h : ∀ k, C k → s.reach k ∨ ∃ D, Θ D s ∧ D k) : Outl Θ C s := by
  intro k hk
  rcases h k hk with hs | ⟨D, hD, hd⟩
  · exact .inl hs
  · exact Outl.fact hD k hd

macro "outl" : tactic => `(tactic| (
  apply Cert.outl_declared
  intro k hk
  rcases k with n | p <;>
    simp_all [Facts.union, Flick.Facts.single, Facts.none, Facts.bnd, Facts.substT, Ty.substT,
      shiftV, Ty.reach, Caps.substT, Caps.none, instT, liftT, Ty.up, Ty.ren, Atom.up, Atom.ty,
      or_and_right, exists_or] <;>
      omega))

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
