import Formal.Soundness
import Formal.Effects
import Formal.Regions

/-!
# `let mut` variables (elaboration.md §3.4)

A block of a function with variables, `Blk`, over the terms of the core: `let`,
`let mut`, an assignment `x = e`, a `scope` and a two-armed `match` whose values the
rest of the block takes, and the value of the block. A variable is a name of the
context, read as any other. `MutOk` is the checker's typing of such a block
(`types/mutation.rs`): a variable has one monotype, fixed where it is declared, an
assignment gives it a value of exactly that type, at any stage — it is not a step of
the stage — and inside a `scope` the variables from outside have their types
shifted past the scope's region, as the rest of the context does.

`tr` is the reading of `docs/formal/elaboration.md#local-mutable-variables`: an
assignment is a `let` of a new name, which the rest of the block reads for the
variable; a `scope` and the arms of a `match` give a record of their value and of
what the variables hold where they end, `{a = v, b = {a = x₁, b = {…}}}`, which the
rest of the block takes apart. **T7**: a block `MutOk` types translates to a core term
of its type at its stage, so T1, T2a and T4 hold of it (`T7_pure`, `T7_regions`); in
particular a block typed `.pure` makes only pure steps, and a value assigned inside a
`scope` to a variable from outside it never reaches the scope's region.

A function, task or arm of a handler inside the block does not use a variable
(`FLICK-G005`) and the body of `try` and loops do not assign one from outside
(`FLICK-T061`): these keep the reading here equal to the compiled code's, which assigns
in place, and are not premises of typing, so `MutOk` leaves them out; lambdas and
handlers stay terms of the core inside the block's terms.
-/

namespace Flick

/-- Binders: `let_ e b` and `var e b` bind one in `b`; `set x e b` binds none; `scope a b`
binds the owner in `a` and the scope's value in `b`; `br e p a a' b` binds what `p` binds
in `a`, none in `a'` and the value of the `match` in `b`. -/
inductive Blk
  | ret (e : Tm)
  | let_ (e : Tm) (b : Blk)
  | var (e : Tm) (b : Blk)
  | set (x : Nat) (e : Tm) (b : Blk)
  | scope (a b : Blk)
  | br (e : Tm) (p : Pat) (a a' b : Blk)

/-- A site one of the terms of a block writes down. -/
def Blk.Has : Blk → Site → Prop
  | .ret e, s => e.Has s
  | .let_ e b, s | .var e b, s | .set _ e b, s => e.Has s ∨ b.Has s
  | .scope a b, s => a.Has s ∨ b.Has s
  | .br e _ a a' b, s => e.Has s ∨ a.Has s ∨ a'.Has s ∨ b.Has s

/-- `{a = v, b = s}`: a value and the state of the variables after it. -/
def pack (v s : Tm) : Tm := .rcons "a" v (.rcons "b" s .rnil)

def packTy (t s : Ty) : Ty := .rcons "a" t (.rcons "b" s .rnil)

/-- The type a context gives the variable `x`. -/
def tyOf (Γ : List Scheme) (x : Nat) : Ty := ((Γ[x]?).map Scheme.t).getD .rnil

/-- The variables `xs` of the context as a record: `{a = x₁, b = {a = x₂, …}}`. -/
def state : List Nat → Tm
  | [] => .rnil
  | x :: xs => pack (.var x) (state xs)

def stateTy (Γ : List Scheme) : List Nat → Ty
  | [] => .rnil
  | x :: xs => packTy (tyOf Γ x) (stateTy Γ xs)

/-- The variable `x` of a state `s`, at place `j`. -/
def path : Nat → Tm → Tm
  | 0, s => .proj s "a"
  | j + 1, s => path j (.proj s "b")

/-- The renaming after `let x' = …`: `x` is the new name, the others one further. -/
def redirect (x i : Nat) : Nat := if i = x then 0 else i + 1

/-- Under `{a = v, b = s}` at `1` and `v` at `0`: each variable of `xs`, from place `j` of
`s` on, bound again to what `s` holds for it, around `k`. -/
def rebind : List Nat → Nat → Tm → Tm
  | [], _, k => k
  | x :: xs, j, k => rebind xs (j + 1) (.let_ (path j (.proj (.var 1) "b")) (k.rename (redirect (x + 2))))

/-- `k`, the rest of a block, after a record `{a = v, b = s}` at `0`: `v` and each
variable of `ms` from `s`. -/
def unpack (ms : List Nat) (k : Tm) : Tm :=
  .let_ (.proj (.var 0) "a") (rebind ms 0 (k.rename (liftR 1 (· + 1))))

/-- The value `e` of a block, with the state of `fin`. -/
def finish : Option (List Nat) → Tm → Tm
  | none, e => e
  | some fs, e => pack e (state fs)

/-- A block as a core term. `ms` are its variables, `fin` the variables whose state its
value comes with: those of the `scope` or arm it ends. -/
def tr : Option (List Nat) → List Nat → Blk → Tm
  | fin, _, .ret e => finish fin e
  | fin, ms, .let_ e b => .let_ e (tr (fin.map (·.map (· + 1))) (ms.map (· + 1)) b)
  | fin, ms, .var e b => .let_ e (tr (fin.map (·.map (· + 1))) (0 :: ms.map (· + 1)) b)
  | fin, ms, .set x e b => .let_ e ((tr fin ms b).rename (redirect x))
  | fin, ms, .scope a b =>
    .let_ (.scope (tr (some (ms.map (· + 1))) (ms.map (· + 1)) a))
      (unpack ms (tr (fin.map (·.map (· + 1))) (ms.map (· + 1)) b))
  | fin, ms, .br e p a a' b =>
    .let_ (.match_ e (.acons p (tr (some (ms.map (· + p.binds))) (ms.map (· + p.binds)) a)
        (.acons .wild (tr (some ms) ms a') .anil)))
      (unpack ms (tr (fin.map (·.map (· + 1))) (ms.map (· + 1)) b))

/-- The variables `ms` are monomorphic names of `Γ`. -/
def VarsOk (Γ : List Scheme) (ms : List Nat) : Prop := ∀ x ∈ ms, Γ[x]? = some (.mono (tyOf Γ x))

/-- The type of a block's term: its value's, with the state of `fin`. -/
def finTy (Γ : List Scheme) : Option (List Nat) → Ty → Ty
  | none, t => t
  | some fs, t => packTy t (stateTy Γ fs)

variable (S : Sig)

/-- `Γ ⊢ b : t ! φ` of a block whose variables are `ms` (`types/mutation.rs`). -/
inductive MutOk : List Ty → Facts → List Scheme → List Nat → Blk → Ty → Stage → Prop
  | ret {Ψ Θ Γ ms e t φ} : Typ S Ψ Θ Γ e (.ex t φ) → MutOk Ψ Θ Γ ms (.ret e) t φ
  | let_ {Ψ Θ Γ ms e b a t φ} : Typ S Ψ Θ Γ e (.ex a φ) →
      MutOk Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) b t φ → MutOk Ψ Θ Γ ms (.let_ e b) t φ
  | var {Ψ Θ Γ ms e b a t φ} : Typ S Ψ Θ Γ e (.ex a φ) →
      MutOk Ψ Θ (.mono a :: Γ) (0 :: ms.map (· + 1)) b t φ → MutOk Ψ Θ Γ ms (.var e b) t φ
  /-- an assignment is of a variable, at its type, at any stage -/
  | set {Ψ Θ Γ ms x e b a t φ} : x ∈ ms → Γ[x]? = some (.mono a) → Typ S Ψ Θ Γ e (.ex a φ) →
      MutOk Ψ Θ Γ ms b t φ → MutOk Ψ Θ Γ ms (.set x e b) t φ
  /-- (Scope): the variables from outside are of the shifted context, so what is assigned
  to them inside does not name the region -/
  | scope {Ψ Θ Γ ms a b t t₁ φ φ' r ε F} : (∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0)) →
      MutOk (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
        (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) (ms.map (· + 1)) a (t₁.up 1) φ' →
      φ'.le (.rt (Row.union (Row.scope (.tvar 0))
        (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))) →
      (Stage.rt r ε).le φ →
      MutOk Ψ Θ (.mono t₁ :: Γ) (ms.map (· + 1)) b t φ → MutOk Ψ Θ Γ ms (.scope a b) t φ
  | br {Ψ Θ Γ ms e p a a' b u t t₁ φ Δ} : Typ S Ψ Θ Γ e (.ex u φ) → PatTy S true p u Δ →
      MutOk Ψ Θ (Δ.map .mono ++ Γ) (ms.map (· + Δ.length)) a t₁ φ → MutOk Ψ Θ Γ ms a' t₁ φ →
      MutOk Ψ Θ (.mono t₁ :: Γ) (ms.map (· + 1)) b t φ → MutOk Ψ Θ Γ ms (.br e p a a' b) t φ

/-! ## The state of the variables -/

theorem tyOf_append (Δ Γ : List Scheme) (x : Nat) : tyOf (Δ ++ Γ) (x + Δ.length) = tyOf Γ x := by
  simp [tyOf, List.getElem?_append_right]

theorem stateTy_append (Δ Γ : List Scheme) (fs : List Nat) :
    stateTy (Δ ++ Γ) (fs.map (· + Δ.length)) = stateTy Γ fs := by
  induction fs with
  | nil => rfl
  | cons x xs ih => simp only [List.map_cons, stateTy, tyOf_append, ih]

theorem stateTy_cons (s : Scheme) (Γ : List Scheme) (fs : List Nat) :
    stateTy (s :: Γ) (fs.map (· + 1)) = stateTy Γ fs := by
  simpa using stateTy_append [s] Γ fs

theorem VarsOk.append {Γ ms} (h : VarsOk Γ ms) (Δ : List Scheme) :
    VarsOk (Δ ++ Γ) (ms.map (· + Δ.length)) := by
  intro y hy
  obtain ⟨x, hx, rfl⟩ := List.mem_map.mp hy
  rw [List.getElem?_append_right (by omega), Nat.add_sub_cancel, tyOf_append]
  exact h x hx

theorem VarsOk.cons {Γ ms} (h : VarsOk Γ ms) (s : Scheme) : VarsOk (s :: Γ) (ms.map (· + 1)) := by
  simpa using h.append [s]

theorem VarsOk.var {Γ ms} (h : VarsOk Γ ms) (a : Ty) :
    VarsOk (.mono a :: Γ) (0 :: ms.map (· + 1)) := by
  intro y hy
  rcases List.mem_cons.mp hy with rfl | hy
  · simp [tyOf, Scheme.mono]
  · exact h.cons _ y hy

theorem tyOf_up {Γ : List Scheme} {x : Nat} (hx : Γ[x]? = some (.mono (tyOf Γ x))) (s : Scheme) :
    tyOf (s :: Γ.map (Scheme.up 1)) (x + 1) = (tyOf Γ x).up 1 := by
  generalize tyOf Γ x = a at hx ⊢
  rw [tyOf, List.getElem?_cons_succ, List.getElem?_map, hx]
  simp [Scheme.up, Scheme.mono_substT, Ty.up_def]

theorem stateTy_up {Γ ms} (h : VarsOk Γ ms) (s : Scheme) :
    stateTy (s :: Γ.map (Scheme.up 1)) (ms.map (· + 1)) = (stateTy Γ ms).up 1 := by
  induction ms with
  | nil => rfl
  | cons x xs ih =>
    simp only [List.map_cons, stateTy, packTy, tyOf_up (h x (List.mem_cons_self ..)),
      ih (fun y hy => h y (List.mem_cons_of_mem _ hy))]
    rfl

theorem VarsOk.scope {Γ ms} (h : VarsOk Γ ms) (s : Scheme) :
    VarsOk (s :: Γ.map (Scheme.up 1)) (ms.map (· + 1)) := by
  intro y hy
  obtain ⟨x, hx, rfl⟩ := List.mem_map.mp hy
  have hx' := h x hx
  rw [tyOf_up hx']
  generalize tyOf Γ x = a at hx' ⊢
  simp [List.getElem?_cons_succ, List.getElem?_map, hx', Scheme.up, Scheme.mono_substT, Ty.up_def]

theorem packTy_up (t s : Ty) : (packTy t s).up 1 = packTy (t.up 1) (s.up 1) := rfl

variable {S}

theorem Typ.var_mono {Ψ Θ Γ x a φ} (h : Γ[x]? = some (.mono a)) :
    Typ S Ψ Θ Γ (.var x) (.ex a φ) :=
  .var (ts := []) h rfl (fun _ _ h => h.elim) (by rw [Scheme.inst_mono]; exact .refl)

theorem Typ.pack {Ψ Θ Γ v s t u φ} (hv : Typ S Ψ Θ Γ v (.ex t φ)) (hs : Typ S Ψ Θ Γ s (.ex u φ)) :
    Typ S Ψ Θ Γ (pack v s) (.ex (packTy t u) φ) :=
  .rcons trivial (by simp [Ty.labels]) hv (.rcons trivial (by simp [Ty.labels]) hs .rnil)

theorem Typ.state {Ψ Θ Γ fs φ} (h : VarsOk Γ fs) : Typ S Ψ Θ Γ (state fs) (.ex (stateTy Γ fs) φ) := by
  induction fs with
  | nil => exact .rnil
  | cons x xs ih =>
    exact .pack (.var_mono (h x (List.mem_cons_self ..)))
      (ih fun y hy => h y (List.mem_cons_of_mem _ hy))

theorem Typ.path {Ψ Θ G φ} {Γ : List Scheme} :
    ∀ {j : Nat} {xs : List Nat} {s : Tm} {x : Nat}, xs[j]? = some x →
      Typ S Ψ Θ G s (.ex (stateTy Γ xs) φ) → Typ S Ψ Θ G (path j s) (.ex (tyOf Γ x) φ)
  | 0, y :: _, _, _, hx, hs => by
    cases hx; exact .proj hs (by simp [stateTy, packTy, Ty.field]) .refl
  | j + 1, _ :: ys, _, _, hx, hs =>
    Typ.path (j := j) (xs := ys) hx (.proj hs (by simp [stateTy, packTy, Ty.field]) .refl)

/-! ## Renamings -/

theorem RenOk.redirect {Γ : List Scheme} {x a} (hx : Γ[x]? = some (.mono a)) :
    RenOk Γ (.mono a :: Γ) (redirect x) := by
  intro i t hi
  unfold Flick.redirect
  split
  · subst i; rw [hx] at hi; cases hi; rfl
  · simpa using hi

theorem RenOk.skip (s r : Scheme) (Γ : List Scheme) :
    RenOk (s :: Γ) (s :: r :: Γ) (liftR 1 (· + 1)) := by
  intro i t hi
  cases i with
  | zero => simpa [liftR] using hi
  | succ i => simpa [liftR] using hi

theorem Typ.rebind {Ψ Θ Γ T t R φ} {ms : List Nat} (hR : R = packTy t (stateTy Γ ms))
    (hv : VarsOk Γ ms) :
    ∀ {xs : List Nat} {j : Nat} {k : Tm}, (∀ i x, xs[i]? = some x → ms[j + i]? = some x) →
      Typ S Ψ Θ (.mono t :: .mono R :: Γ) k (.ex T φ) →
      Typ S Ψ Θ (.mono t :: .mono R :: Γ) (rebind xs j k) (.ex T φ)
  | [], _, _, _, hk => hk
  | x :: xs, j, k, hxs, hk => by
    have hx : ms[j]? = some x := by simpa using hxs 0 x rfl
    have hm : x ∈ ms := List.mem_of_getElem? hx
    have hG : (Scheme.mono t :: .mono R :: Γ)[x + 2]? = some (.mono (tyOf Γ x)) := by
      simpa using hv x hm
    have hs : Typ S Ψ Θ (.mono t :: .mono R :: Γ) (.proj (.var 1) "b") (.ex (stateTy Γ ms) φ) :=
      .proj (.var_mono rfl) (by simp [hR, packTy, Ty.field]) .refl
    refine Typ.rebind hR hv (xs := xs) (j := j + 1) (fun i y hy => ?_)
      (.let_ (Typ.path hx hs) (hk.rename (RenOk.redirect hG)))
    have := hxs (i + 1) y (by simpa using hy)
    rwa [show j + (i + 1) = j + 1 + i by omega] at this

theorem Typ.unpack {Ψ Θ Γ T t φ} {ms : List Nat} {k : Tm} (hv : VarsOk Γ ms)
    (hk : Typ S Ψ Θ (.mono t :: Γ) k (.ex T φ)) :
    Typ S Ψ Θ (.mono (packTy t (stateTy Γ ms)) :: Γ) (unpack ms k) (.ex T φ) :=
  .let_ (.proj (.var_mono rfl) (by simp [packTy, Ty.field]) .refl)
    (Typ.rebind rfl hv (xs := ms) (j := 0) (fun i x h => by simpa using h)
      (hk.rename (RenOk.skip _ _ _)))

/-! ## What the translation writes down -/

theorem Tm.has_path {j : Nat} {s : Tm} {x : Site} (h : (path j s).Has x) : s.Has x := by
  induction j generalizing s with
  | zero => exact h
  | succ j ih => exact ih (s := .proj s "b") h

theorem Tm.has_rebind {xs : List Nat} {j : Nat} {k : Tm} {x : Site}
    (h : (rebind xs j k).Has x) : k.Has x := by
  induction xs generalizing j k with
  | nil => exact h
  | cons y ys ih =>
    rcases ih h with h | h
    · exact absurd (Tm.has_path h) (by simp [Tm.Has])
    · exact (Tm.has_rename _ _ _).mp h

theorem Tm.has_unpack {ms : List Nat} {k : Tm} {x : Site} (h : (unpack ms k).Has x) : k.Has x := by
  rcases h with h | h
  · simp [Tm.Has] at h
  · exact (Tm.has_rename _ _ _).mp (Tm.has_rebind h)

theorem Tm.has_state {fs : List Nat} {x : Site} : ¬ (state fs).Has x := by
  induction fs with
  | nil => simp [state, Tm.Has]
  | cons y ys ih => simp_all [state, pack, Tm.Has]

/-- The translation writes down only what the terms of the block do. -/
theorem tr_has {x : Site} : ∀ {b : Blk} {fin : Option (List Nat)} {ms : List Nat},
    (tr fin ms b).Has x → b.Has x
  | .ret e, none, _, h => h
  | .ret e, some fs, _, h => by
    simp only [tr, finish, pack, Tm.Has] at h
    rcases h with h | h | h
    · exact h
    · exact absurd h Tm.has_state
    · exact h.elim
  | .let_ e b, _, _, h | .var e b, _, _, h => by
    simp only [tr, Tm.Has] at h; exact h.imp id tr_has
  | .set _ e b, _, _, h => by
    simp only [tr, Tm.Has] at h; exact h.imp id fun h => tr_has ((Tm.has_rename _ _ _).mp h)
  | .scope a b, _, _, h => by
    simp only [tr, Tm.Has] at h; exact h.imp tr_has fun h => tr_has (Tm.has_unpack h)
  | .br e p a a' b, _, _, h => by
    simp only [tr, Tm.Has] at h
    rcases h with (h | h | h | h) | h
    · exact .inl h
    · exact .inr (.inl (tr_has h))
    · exact .inr (.inr (.inl (tr_has h)))
    · exact h.elim
    · exact .inr (.inr (.inr (tr_has (Tm.has_unpack h))))

/-! ## T7 -/

theorem Exh.wild {t : Ty} {p : Pat} : Exh S t [p, .wild] :=
  fun _ _ => ⟨.wild, by simp, by simp [matchP]⟩

theorem finTy_cons (s : Scheme) (Γ : List Scheme) (fin : Option (List Nat)) (t : Ty) :
    finTy (s :: Γ) (fin.map (·.map (· + 1))) t = finTy Γ fin t := by
  cases fin <;> simp [finTy, stateTy_cons]

/-- The typing of the translation, for any state a block's value comes with. -/
theorem MutOk.typ {Ψ Θ Γ ms b t φ} (h : MutOk S Ψ Θ Γ ms b t φ) (hb : ¬ b.Has .ins) :
    VarsOk Γ ms → ∀ fin : Option (List Nat), (∀ fs, fin = some fs → VarsOk Γ fs) →
      Typ S Ψ Θ Γ (tr fin ms b) (.ex (finTy Γ fin t) φ) := by
  have shifted : ∀ {Γ : List Scheme} (s : Scheme) (fin : Option (List Nat)),
      (∀ fs, fin = some fs → VarsOk Γ fs) →
        ∀ fs, fin.map (·.map (· + 1)) = some fs → VarsOk (s :: Γ) fs := by
    intro Γ s fin hfin fs h
    cases fin with
    | none => cases h
    | some fs' => cases h; exact (hfin _ rfl).cons s
  induction h with
  | ret he =>
    intro _ fin hfin
    cases fin with
    | none => exact he
    | some fs => exact .pack he (Typ.state (hfin fs rfl))
  | let_ he _ ih =>
    intro hv fin hfin
    have := ih (fun h => hb (.inr h)) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at this
    exact .let_ he this
  | @var Ψ Θ Γ ms e b a t φ he _ ih =>
    intro hv fin hfin
    have := ih (fun h => hb (.inr h)) (hv.var a) _ (shifted _ fin hfin)
    rw [finTy_cons] at this
    exact .let_ he this
  | set _ hx he _ ih =>
    intro hv fin hfin
    exact .let_ he ((ih (fun h => hb (.inr h)) hv fin hfin).rename (RenOk.redirect hx))
  | @scope Ψ Θ Γ ms a b t t₁ φ φ' r ε F hF _ hφ' hφ _ iha ihb =>
    intro hv fin hfin
    have ha := iha (fun h => hb (.inl h)) (hv.scope _) (some (ms.map (· + 1))) (by
      intro fs h; cases h; exact hv.scope _)
    simp only [finTy, stateTy_up hv, ← packTy_up] at ha
    have hn : (tr (some (ms.map (· + 1))) (ms.map (· + 1)) a).NoIns :=
      fun h => hb (.inl (tr_has h))
    have hk := ihb (fun h => hb (.inr h)) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at hk
    exact .let_ (.scope hn hF ha hφ' hφ .refl) (.unpack hv hk)
  | @br Ψ Θ Γ ms e p a a' b u t t₁ φ Δ he hp _ _ _ iha iha' ihb =>
    intro hv fin hfin
    have hl := hp.binds
    have hvΔ : VarsOk (Δ.map Scheme.mono ++ Γ) (ms.map (· + Δ.length)) := by
      simpa using hv.append (Δ.map Scheme.mono)
    have ha := iha (fun h => hb (.inr (.inl h))) hvΔ (some (ms.map (· + Δ.length))) (by
      intro fs h; cases h; exact hvΔ)
    have ha' := iha' (fun h => hb (.inr (.inr (.inl h)))) hv (some ms) (by
      intro fs h; cases h; exact hv)
    have hst : stateTy (Δ.map Scheme.mono ++ Γ) (ms.map (· + Δ.length)) = stateTy Γ ms := by
      simpa using stateTy_append (Δ.map Scheme.mono) Γ ms
    simp only [finTy, hst] at ha ha'
    rw [hl] at ha
    have hk := ihb (fun h => hb (.inr (.inr (.inr h)))) (hv.cons _) _ (shifted _ fin hfin)
    rw [finTy_cons] at hk
    exact .let_ (.match_ he (.acons hp ha (.acons PatTy.wild (by simpa using ha') .anil))
      (.inl Exh.wild)) (.unpack hv hk)

/-- **T7**: a block of a function with `let mut` variables translates to a term of the
core with the block's type at the block's stage. -/
theorem T7 {Ψ Θ Γ b t φ} (h : MutOk S Ψ Θ Γ [] b t φ) (hb : ¬ b.Has .ins) :
    Typ S Ψ Θ Γ (tr none [] b) (.ex t φ) :=
  h.typ hb (fun _ h => nomatch h) none (fun _ h => nomatch h)

theorem tr_below {n : Nat} {b : Blk} (hb : ∀ p, b.Has (.rgn p) → p < n) :
    (tr none [] b).Below n :=
  fun p h => hb p (tr_has h)

/-- T2a for a pure block: its translation makes only pure steps, which change no cell
and no region, and raises nothing. -/
theorem T7_pure {root Ψ Λ μ b t Λ' μ' e'} (hroot : RootTy S root) (hμ : StoreTy S Ψ μ)
    (hb : ∀ p, b.Has (.rgn p) → p < Λ.length) (hins : ¬ b.Has .ins)
    (hmem : ∀ (l : Nat) (v : Tm), μ[l]? = some v → v.Below Λ.length)
    (hrgs : ∀ (p : Nat) (g : Rgn), Λ[p]? = some g →
      (∀ k ∈ g.kids, k.Below Λ.length) ∧ ∀ f ∈ g.fins, f.Below Λ.length)
    (hΨ : ∀ u ∈ Ψ, u.YF Λ.length) (h : MutOk S Ψ Facts.age [] [] b t .pure)
    (hs : Steps root (fun _ => True) Λ μ (tr none [] b) Λ' μ' e') :
    Steps root (· = .pure) Λ μ (tr none [] b) Λ' μ' e' ∧ μ' = μ ∧ Λ' = Λ ∧ ∀ v, e' ≠ .raise v :=
  T2a hroot hμ ⟨tr_below hb, hmem, hrgs⟩ hΨ (T7 h hins) hs

/-- T4 for a closed block: live work reaches only live regions; in particular no value
assigned in a `scope` is reached after the scope ends. -/
theorem T7_regions {root b t φ₀ c} (hroot : RootTy S root) (h : MutOk S [] Facts.none [] [] b t φ₀)
    (hins : ¬ b.Has .ins) (hp : ∀ p, ¬ b.Has (.rgn p)) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], tr none [] b⟩ c) :
    (∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids, ∀ E r, OkE E → k = plug E r →
      ¬ DeadUse c.rgs r) ∧
    (∀ p, Reaches c.mem c.main p → Live c.rgs p) ∧
    ∀ (r : Nat) (g : Rgn), c.rgs[r]? = some g → g.live = true → ∀ k ∈ g.kids ++ g.fins, ∀ p,
      Reaches c.mem k p → Live c.rgs p :=
  T4 hroot (T7 h hins) (tr_below fun p h => absurd h (hp p)) ht hφ hs

end Flick
