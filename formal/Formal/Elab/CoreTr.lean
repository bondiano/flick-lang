import Formal.Elab.CoreTy

/-!
# λFlick⁺ as a `Target`: the terms

`Tr N γ δ e e'`: the elaborated term `e` is the core term `e'`. The variables of `Γ` and the
premises of `Δ` go into one context of the core, through the maps `γ` and `δ`: `lam` pushes
one variable, `lamD l` pushes `l` premises (the `i`-th argument is the premise `i`). Methods
and slots are projections, instances, constants and the compiler's dictionaries are globals,
`if` and `match` on `Option` are `match_` with a last arm `_`. The translation is a relation
only in the globals of structural and primitive dictionaries (`docs/formal/elaboration.md#structural-and-primitive-dictionaries-are-families-indexed-by-type`).

`Corr Γc Γ Δ γ δ`: the context of the core gives `⟦Γ[x]⟧` at `γ x` and `D_{Δ[k]}` at `δ k`,
and nothing where `Γ` and `Δ` give nothing. A translation has no run-time sites
(`Tr.has`), and its free variables are those of `e` through `γ`, or premises (`Tr.free`);
so what it holds in the core is what `e` holds in `Γ` (`corr_held`).
-/

namespace Flick.Elab

/-- `λ … λ. b` with `n` binders. -/
def lamsC : Nat → Flick.Tm → Flick.Tm
  | 0, b => b
  | n + 1, b => .lam (lamsC n b)

/-- `γ` under one binder. -/
def ext (γ : Nat → Nat) : Nat → Nat
  | 0 => 0
  | x + 1 => γ x + 1

/-- `δ` under the `l` premises of `lamD l`: the premise `k < l` is the variable `l - 1 - k`. -/
def dext (l : Nat) (δ : Nat → Nat) (k : Nat) : Nat := if k < l then l - 1 - k else δ (k - l) + l

variable (N : Names)

/-- The dictionary `d` is the core term `d'`. -/
inductive TrD (δ : Nat → Nat) : Dict → Flick.Tm → Prop
  | var {k} : TrD δ (.var k) (.var (δ k))
  | slot {d d' i} : TrD δ d d' → TrD δ (.slot d i) (.proj d' (N.slot i))
  | inst {I} : TrD δ (.inst I) (.glob (N.inst I))
  | strct {C τ} : TrD δ (.strct C τ) (.glob (N.strct C τ))
  | prim {p q} : TrD δ (.prim p q) (.glob (N.prim p q))
  | ap {d d' e e'} : TrD δ d d' → TrD δ e e' → TrD δ (.ap d e) (.app d' e')

/-- The elaborated term `e` is the core term `e'`. -/
inductive Tr : (Nat → Nat) → (Nat → Nat) → Tm → Flick.Tm → Prop
  | var {γ δ x} : Tr γ δ (.var x) (.var (γ x))
  | dict {γ δ d d'} : TrD N δ d d' → Tr γ δ (.dict d) d'
  | meth {γ δ e e' m} : Tr γ δ e e' → Tr γ δ (.meth e m) (.proj e' (N.meth m))
  | const {γ δ c} : Tr γ δ (.const c) (.glob (N.const c))
  | app {γ δ f f' a a'} : Tr γ δ f f' → Tr γ δ a a' → Tr γ δ (.app f a) (.app f' a')
  | lam {γ δ b b'} : Tr (ext γ) (fun k => δ k + 1) b b' → Tr γ δ (.lam b) (.lam b')
  | let_ {γ δ e e' b b'} : Tr γ δ e e' → Tr (ext γ) (fun k => δ k + 1) b b' →
      Tr γ δ (.let_ e b) (.let_ e' b')
  | lamD {γ δ l b b'} : Tr (fun x => γ x + l) (dext l δ) b b' → Tr γ δ (.lamD l b) (lamsC l b')
  | ite {γ δ c c' a a' b b'} : Tr γ δ c c' → Tr γ δ a a' → Tr γ δ b b' →
      Tr γ δ (.ite c a b) (.match_ c' (.acons (.lit (.bool true)) a' (.acons .wild b' .anil)))
  | caseOpt {γ δ e e' n n' s s'} : Tr γ δ e e' → Tr γ δ n n' →
      Tr (ext γ) (fun k => δ k + 1) s s' →
      Tr γ δ (.caseOpt e n s)
        (.match_ e' (.acons (.app (.ctor N.some) .var) s' (.acons .wild n' .anil)))

theorem lamsC_free {n : Nat} {b : Flick.Tm} {i : Nat} : (lamsC n b).Free i ↔ b.Free (i + n) := by
  induction n generalizing i with
  | zero => rfl
  | succ n ih =>
    show (lamsC n b).Free (i + 1) ↔ _
    rw [ih, Nat.add_right_comm, Nat.add_assoc]

theorem lamsC_has {n : Nat} {b : Flick.Tm} {s : Site} : (lamsC n b).Has s ↔ b.Has s := by
  induction n with
  | zero => rfl
  | succ n ih => exact ih

theorem TrD.has {δ d d'} (h : TrD N δ d d') (s : Site) : ¬ d'.Has s := by
  induction h <;> simp_all [Flick.Tm.Has]

theorem TrD.free {δ d d'} (h : TrD N δ d d') {i : Nat} (hi : d'.Free i) : ∃ k, δ k = i := by
  induction h with
  | var => exact ⟨_, hi⟩
  | slot _ ih => exact ih hi
  | inst => cases hi
  | strct => cases hi
  | prim => cases hi
  | ap _ _ ih₁ ih₂ => exact hi.elim ih₁ ih₂

/-- A translation has no run-time sites. -/
theorem Tr.has {γ δ e e'} (h : Tr N γ δ e e') (s : Site) : ¬ e'.Has s := by
  induction h with
  | dict hd => exact hd.has N s
  | lamD _ ih => exact fun h => ih (lamsC_has.1 h)
  | _ => simp_all [Flick.Tm.Has]

theorem Tr.noIns {γ δ e e'} (h : Tr N γ δ e e') : e'.NoIns := h.has N .ins

/-- **The free variables of a translation.** -/
theorem Tr.free {γ δ e e'} (h : Tr N γ δ e e') {i : Nat} (hi : e'.Free i) :
    (∃ x, e.Free x ∧ γ x = i) ∨ ∃ k, δ k = i := by
  induction h generalizing i with
  | var => exact .inl ⟨_, rfl, hi⟩
  | dict hd => exact .inr (hd.free N hi)
  | meth _ ih => exact ih hi
  | const => cases hi
  | app _ _ ih₁ ih₂ =>
    rcases hi with hi | hi
    · exact (ih₁ hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inl hx, he⟩) id
    · exact (ih₂ hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inr hx, he⟩) id
  | @lam γ δ b b' _ ih =>
    rcases ih hi with ⟨x, hx, he⟩ | ⟨k, he⟩
    · cases x with
      | zero => cases he
      | succ x => exact .inl ⟨x, hx, by simp [ext] at he; exact he⟩
    · exact .inr ⟨k, by simp at he; exact he⟩
  | @let_ γ δ e e' b b' _ _ ih₁ ih₂ =>
    rcases hi with hi | hi
    · exact (ih₁ hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inl hx, he⟩) id
    · rcases ih₂ hi with ⟨x, hx, he⟩ | ⟨k, he⟩
      · cases x with
        | zero => cases he
        | succ x => exact .inl ⟨x, .inr hx, by simp [ext] at he; exact he⟩
      · exact .inr ⟨k, by simp only at he; omega⟩
  | @lamD γ δ l b b' _ ih =>
    rcases ih (lamsC_free.1 hi) with ⟨x, hx, he⟩ | ⟨k, he⟩
    · exact .inl ⟨x, hx, by simp only at he; omega⟩
    · unfold dext at he
      split at he
      · omega
      · exact .inr ⟨k - l, by omega⟩
  | ite _ _ _ ihc iha ihb =>
    simp only [Flick.Tm.Free, Pat.binds, Nat.add_zero, or_false] at hi
    rcases hi with hi | hi | hi
    · exact (ihc hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inl hx, he⟩) id
    · exact (iha hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inr (.inl hx), he⟩) id
    · exact (ihb hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inr (.inr hx), he⟩) id
  | caseOpt _ _ _ ihe ihn ihs =>
    simp only [Flick.Tm.Free, Pat.binds, Nat.add_zero, Nat.zero_add, or_false] at hi
    rcases hi with hi | hi | hi
    · exact (ihe hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inl hx, he⟩) id
    · rcases ihs hi with ⟨x, hx, he⟩ | ⟨k, he⟩
      · cases x with
        | zero => cases he
        | succ x => exact .inl ⟨x, .inr (.inr hx), by simp [ext] at he; exact he⟩
      · exact .inr ⟨k, by simp only at he; omega⟩
    · exact (ihn hi).imp (fun ⟨x, hx, he⟩ => ⟨x, .inr (.inl hx), he⟩) id

/-- The context of the core gives `⟦Γ[x]⟧` at `γ x` and `D_{Δ[k]}` at `δ k`. -/
structure Corr (Γc : List Flick.Scheme) (Γ : List TSch) (Δ : List Cn) (γ δ : Nat → Nat) : Prop where
  var : ∀ x, Γc[γ x]? = Γ[x]?.map (trS N)
  dict : ∀ k, Γc[δ k]? = Δ[k]?.map fun q => Flick.Scheme.mono (trD N q)

theorem Corr.cons {Γc Γ Δ γ δ} (h : Corr N Γc Γ Δ γ δ) (s : TSch) :
    Corr N (trS N s :: Γc) (s :: Γ) Δ (ext γ) (fun k => δ k + 1) where
  var x := by cases x <;> simp [ext, h.var]
  dict k := by simp [h.dict]

theorem Corr.up {Γc Γ Δ γ δ} (h : Corr N Γc Γ Δ γ δ) (k : Nat) :
    Corr N (Γc.map (Flick.Scheme.up k)) (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) γ δ where
  var x := by
    rw [List.getElem?_map, h.var, List.getElem?_map, Option.map_map, Option.map_map]
    congr 1; funext s; exact (trS_up N s k).symm
  dict j := by
    rw [List.getElem?_map, h.dict, List.getElem?_map, Option.map_map, Option.map_map]
    congr 1; funext q; exact (trD_up N q k).symm

/-- The premises of `lamD qs.length`, innermost last. -/
def dctx (qs : List Cn) : List Flick.Scheme := qs.reverse.map fun q => Flick.Scheme.mono (trD N q)

theorem Corr.lamD {Γc Γ Δ γ δ} (h : Corr N Γc Γ Δ γ δ) (qs : List Cn) :
    Corr N (dctx N qs ++ Γc) Γ (qs ++ Δ) (fun x => γ x + qs.length) (dext qs.length δ) where
  var x := by
    rw [List.getElem?_append_right (by simp [dctx])]
    simp [dctx, h.var]
  dict k := by
    unfold dext
    split
    · rename_i hk
      rw [List.getElem?_append_left (by simp [dctx]; omega), List.getElem?_append_left hk]
      simp only [dctx, List.getElem?_map, List.getElem?_reverse (by omega : qs.length - 1 - k < qs.length)]
      congr 2; omega
    · rename_i hk
      rw [List.getElem?_append_right (by simp [dctx]), List.getElem?_append_right (by omega)]
      simp [dctx, h.dict]

/-- What the core context holds through a variable of `Γ` free in `e` is what `e` holds in
`Γ`; a premise holds nothing. -/
theorem corr_held {Γc Γ Δ γ δ} (h : Corr N Γc Γ Δ γ δ) {e : Tm} {i : Nat}
    (hi : (∃ x, e.Free x ∧ γ x = i) ∨ ∃ k, δ k = i) {s : Flick.Scheme} (hs : Γc[i]? = some s)
    {a : Atom} (ha : s.reach a) : ∃ n, a = .v n ∧ heldT Γ e n := by
  rcases hi with ⟨x, hx, rfl⟩ | ⟨k, rfl⟩
  · rw [h.var] at hs
    cases hx' : Γ[x]? with
    | none => rw [hx'] at hs; cases hs
    | some s₀ =>
      rw [hx'] at hs; cases hs
      obtain ⟨n, rfl, hn⟩ := (trS_reach N s₀ a).1 ha
      exact ⟨n, rfl, x, s₀, hx, hx', hn⟩
  · rw [h.dict] at hs
    cases hk : Δ[k]? with
    | none => rw [hk] at hs; cases hs
    | some q =>
      rw [hk] at hs; cases hs
      exact absurd ((Flick.Scheme.reach_mono _ _).1 ha) (trD_reach N q a)

end Flick.Elab
