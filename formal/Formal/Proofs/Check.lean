import Formal.Proofs.Reduce

/-!
# The certificate checker of λPT and its soundness

A statement is an equation of two terms of one type over typed
variables, with premises; it holds when every instance of its type
variables and every values of its variables that satisfy the premises
make the two sides evaluate to equal values. `check` decides a
certificate of a statement, with lemmas checked before it, and
`check_sound` proves that what it accepts holds.

The rules are those of the compiler's kernel (`docs/formal/proofs.md` §4):
`refl` compares normal forms without their annotations, `symm`, `trans`, `cong` of a context,
`funext`, a split of a variable of a unit, product or sum type, a
premise, and a lemma at an instance of its type variables. A lemma
here has no premises of its own.
-/

namespace Flick.PT

/-- `∀ ctx. facts → lhs = rhs : ty`. Variable `0` is the head of `ctx`. -/
structure Stmt where
  ctx : List Ty
  facts : List (Tm × Tm × Ty)
  lhs : Tm
  rhs : Tm
  ty : Ty
  deriving DecidableEq, Repr

variable {G : List Tm}

/-- Values for the variables of `Γ` at `θ`, each a value of its type. -/
def Vals (G : List Tm) (θ : Nat → Ty) (Γ : List Ty) (γ : Nat → Tm) : Prop :=
  ∀ n t, Γ[n]? = some t → R G (t.subst θ) (γ n) (γ n)

theorem Vals.rs {θ : Nat → Ty} {Γ : List Ty} {γ : Nat → Tm} (h : Vals G θ Γ γ) :
    RS G θ Γ γ γ :=
  fun n t hn => (h n t hn).E

theorem Vals.cons {θ : Nat → Ty} {Γ : List Ty} {γ : Nat → Tm} {a : Ty} {v : Tm}
    (hv : R G (a.subst θ) v v) (h : Vals G θ Γ γ) : Vals G θ (a :: Γ) (scons v γ) := by
  intro n t hn
  cases n with
  | zero => simp at hn; subst hn; exact hv
  | succ n => exact h n t (by simpa using hn)

/-- The premises of `s` hold at `θ` and `γ`. -/
def Stmt.FactsHold (G : List Tm) (s : Stmt) (θ : Nat → Ty) (γ : Nat → Tm) : Prop :=
  ∀ f ∈ s.facts, E G (f.2.2.subst θ) (f.1.subst γ) (f.2.1.subst γ)

/-- **What a statement means.** -/
def Stmt.Holds (G : List Tm) (s : Stmt) : Prop :=
  ∀ θ γ, Vals G θ s.ctx γ → s.FactsHold G θ γ →
    E G (s.ty.subst θ) (s.lhs.subst γ) (s.rhs.subst γ)

/-- Both sides and every premise are typed. -/
def Stmt.Typed (GT : List Ty) (s : Stmt) : Prop :=
  Flick.PT.Typed GT s.ctx s.lhs s.ty ∧ Flick.PT.Typed GT s.ctx s.rhs s.ty ∧
    ∀ f ∈ s.facts, Flick.PT.Typed GT s.ctx f.1 f.2.2 ∧ Flick.PT.Typed GT s.ctx f.2.1 f.2.2

def Stmt.typed? (GT : List Ty) (s : Stmt) : Bool :=
  infer GT s.ctx s.lhs == some s.ty && infer GT s.ctx s.rhs == some s.ty &&
    s.facts.all fun f => infer GT s.ctx f.1 == some f.2.2 && infer GT s.ctx f.2.1 == some f.2.2

theorem Stmt.typed?_sound {GT : List Ty} {s : Stmt} (h : s.typed? GT = true) : s.Typed GT := by
  simp only [Stmt.typed?, Bool.and_eq_true, beq_iff_eq, List.all_eq_true] at h
  obtain ⟨⟨hl, hr⟩, hf⟩ := h
  exact ⟨infer_sound hl, infer_sound hr, fun f hf' =>
    ⟨infer_sound (hf f hf').1, infer_sound (hf f hf').2⟩⟩

/-! ## Substituting a statement -/

/-- `x` replaced by `t`, and every other variable `i` by `i + k`: the
substitution of a split, which binds `k` new variables. -/
def at? (x : Nat) (t : Tm) (k : Nat) (i : Nat) : Tm :=
  if i = x then t else .var (i + k)

def Stmt.substBy (s : Stmt) (σ : Nat → Tm) (ctx' : List Ty) : Stmt where
  ctx := ctx'
  facts := s.facts.map fun f => (f.1.subst σ, f.2.1.subst σ, f.2.2)
  lhs := s.lhs.subst σ
  rhs := s.rhs.subst σ
  ty := s.ty

/-- A statement holds when each way its variables can be is a case it
holds in: a substitution of the cases is related to every values. -/
theorem Stmt.holds_of_cases {P : Prog} (hP : P.WF) {s : Stmt} (hs : s.Typed P.schemes)
    {σ₁ σ₂ : Nat → Tm} {c₁ c₂ : List Ty}
    (hcover : ∀ θ γ, Vals P.bodies θ s.ctx γ →
      (∃ γ', Vals P.bodies θ c₁ γ' ∧ RS P.bodies θ s.ctx γ (fun i => (σ₁ i).subst γ')) ∨
      (∃ γ', Vals P.bodies θ c₂ γ' ∧ RS P.bodies θ s.ctx γ (fun i => (σ₂ i).subst γ')))
    (h₁ : (s.substBy σ₁ c₁).Holds P.bodies) (h₂ : (s.substBy σ₂ c₂).Holds P.bodies) :
    s.Holds P.bodies := by
  have hG := hP.globOK
  obtain ⟨hl, hr, hf⟩ := hs
  -- one case: its substitution relates each side and premise to itself
  have one : ∀ {σ : Nat → Tm} {c : List Ty}, (s.substBy σ c).Holds P.bodies →
      ∀ θ γ γ', Vals P.bodies θ s.ctx γ → s.FactsHold P.bodies θ γ →
      Vals P.bodies θ c γ' → RS P.bodies θ s.ctx γ (fun i => (σ i).subst γ') →
      E P.bodies (s.ty.subst θ) (s.lhs.subst γ) (s.rhs.subst γ) := by
    intro σ c h θ γ γ' _ hF hV' hrel
    have facts : (s.substBy σ c).FactsHold P.bodies θ γ' := by
      intro f hfm
      simp only [Stmt.substBy, List.mem_map] at hfm
      obtain ⟨f₀, hf₀, rfl⟩ := hfm
      simp only [Tm.subst_subst]
      obtain ⟨tl, tr⟩ := hf f₀ hf₀
      exact ((fundamental hG tl θ _ _ hrel).symm.trans (hF f₀ hf₀)).trans
        (fundamental hG tr θ _ _ hrel)
    have hc := h θ γ' hV' facts
    simp only [Stmt.substBy, Tm.subst_subst] at hc
    exact ((fundamental hG hl θ _ _ hrel).trans hc).trans (fundamental hG hr θ _ _ hrel).symm
  intro θ γ hV hF
  rcases hcover θ γ hV with ⟨γ', hV', hrel⟩ | ⟨γ', hV', hrel⟩
  · exact one h₁ θ γ γ' hV hF hV' hrel
  · exact one h₂ θ γ γ' hV hF hV' hrel

/-- A value of `unit` is `unit`. -/
theorem R.unit_eq {v w : Tm} (h : R G .unit v w) : v = .unit := by
  cases v <;> cases w <;> simp only [R] at h ⊢

theorem R.prod_eq {a b : Ty} {v w : Tm} (h : R G (.prod a b) v w) :
    ∃ v₁ v₂ w₁ w₂, v = .pair v₁ v₂ ∧ w = .pair w₁ w₂ ∧ R G a v₁ w₁ ∧ R G b v₂ w₂ := by
  cases v <;> cases w <;> simp only [R] at h
  exact ⟨_, _, _, _, rfl, rfl, h.1, h.2⟩

theorem R.sum_eq {a b : Ty} {v w : Tm} (h : R G (.sum a b) v w) :
    (∃ v₁ w₁ d₁ d₂, v = .inl v₁ d₁ ∧ w = .inl w₁ d₂ ∧ R G a v₁ w₁) ∨
    (∃ v₁ w₁ d₁ d₂, v = .inr d₁ v₁ ∧ w = .inr d₂ w₁ ∧ R G b v₁ w₁) := by
  cases v <;> cases w <;> simp only [R] at h
  · exact .inl ⟨_, _, _, _, rfl, rfl, h⟩
  · exact .inr ⟨_, _, _, _, rfl, rfl, h⟩

/-! ## Certificates -/

inductive Cert where
  | refl
  | symm (p : Cert)
  | trans (m : Tm) (p q : Cert)
  /-- `cong c p`: `c` has a hole, variable `0`, of type `t`; `p` proves `a = b`. -/
  | cong (c : Tm) (t : Ty) (a b : Tm) (p : Cert)
  /-- `funext p`: `p` proves `f x = g x` for a new variable `x`. -/
  | funext (p : Cert)
  /-- A split of variable `x`: `p` for a unit or a pair, `p` and `q` for the
  two injections of a sum. -/
  | split (x : Nat) (p q : Cert)
  | fact (h : Nat)
  /-- Lemma `i` at the types `ts` and the values `args` of its variables. -/
  | lemma (i : Nat) (ts : List Ty) (args : List Tm)
  deriving Repr

/-- The values of a lemma's variables, by position. -/
def argSubst (args : List Tm) (n : Nat) : Tm := args.getD n (.var n)

/-- The premises of `s` in a context one variable longer. -/
def shiftFacts (fs : List (Tm × Tm × Ty)) : List (Tm × Tm × Ty) :=
  fs.map fun f => (f.1.ren .succ, f.2.1.ren .succ, f.2.2)

def check (P : Prog) (k : Nat) (L : List Stmt) : Stmt → Cert → Bool
  | s, .refl =>
    s.typed? P.schemes && (norm P.bodies k s.lhs).erase == (norm P.bodies k s.rhs).erase
  | s, .symm p => check P k L { s with lhs := s.rhs, rhs := s.lhs } p
  | s, .trans m p q => check P k L { s with rhs := m } p && check P k L { s with lhs := m } q
  | s, .cong c t a b p =>
    s.lhs == c.inst a && s.rhs == c.inst b && infer P.schemes (t :: s.ctx) c == some s.ty &&
      check P k L { s with lhs := a, rhs := b, ty := t } p
  | s, .funext p =>
    match s.ty with
    | .arrow a b =>
      s.typed? P.schemes &&
        check P k L
          { ctx := a :: s.ctx, facts := shiftFacts s.facts,
            lhs := .app (s.lhs.ren .succ) (.var 0), rhs := .app (s.rhs.ren .succ) (.var 0),
            ty := b } p
    | _ => false
  | s, .split x p q =>
    s.typed? P.schemes &&
      match s.ctx[x]? with
      | some .unit => check P k L (s.substBy (at? x .unit 0) s.ctx) p
      | some (.prod a b) =>
        check P k L (s.substBy (at? x (.pair (.var 1) (.var 0)) 2) (b :: a :: s.ctx)) p
      | some (.sum a b) =>
        check P k L (s.substBy (at? x (.inl (.var 0) b) 1) (a :: s.ctx)) p &&
          check P k L (s.substBy (at? x (.inr a (.var 0)) 1) (b :: s.ctx)) q
      | _ => false
  | s, .fact h => s.facts[h]? == some (s.lhs, s.rhs, s.ty)
  | s, .lemma i ts args =>
    match L[i]? with
    | some l =>
      l.facts.isEmpty && args.length == l.ctx.length &&
        s.lhs == l.lhs.subst (argSubst args) && s.rhs == l.rhs.subst (argSubst args) &&
        s.ty == l.ty.subst (tyInst ts) &&
        (List.range args.length).all fun j =>
          infer P.schemes s.ctx (args.getD j .unit) == some ((l.ctx.getD j .unit).subst (tyInst ts))
    | none => false

end Flick.PT
