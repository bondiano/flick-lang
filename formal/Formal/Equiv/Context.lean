import Formal.Equiv.Cfg
import Formal.Soundness

/-!
# Contextual equivalence of terms of the core

Two terms are contextually equivalent (`CtxEquiv`) when every typed program built around them
by one context, a term of the source language with any number of holes (`Ctx`), may end with the
same literals (`Ends`) whichever of the two fills the holes. A filling stays under the binders
of the context, which bind its free variables.

A hole stands where a term runs or is bound, not at the top of a dictionary record, of the value
a handler matches or of a list of alternatives (`Pos.top`): a pattern reads a term there without
running it. Typing gives these places the pure stage or no term judgment at all, and a term
that makes a lazy cell has the stage `rt` at least (`Typ.lazy`); so no typed program has such a
term there, but in a dead arm of a GADT, which typing does not reach.

Two terms related by `Sim` both ways in every world are contextually equivalent
(`ctxEquiv_of_sim`): the context relates the two programs (`Ctx.sim`), typing makes their threads
closed wherever their runs go (`Prog.reach`), and the simulation of runs answers a left run
that ends with a literal by a right one that ends with it (`csteps_sim`, `lit_sim`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

/-- Two programs with holes: one context of the source language, with `e₁` and `e₂` in its
holes; the index is where the term stands, as in `Sim`. -/
inductive Ctx (e₁ e₂ : Tm) : Pos → Tm → Tm → Prop
  | hole : Ctx e₁ e₂ .any e₁ e₂
  | var {x i} : Ctx e₁ e₂ x (.var i) (.var i)
  | lit {x l} : Ctx e₁ e₂ x (.lit l) (.lit l)
  | const {x c} : Ctx e₁ e₂ x (.const c) (.const c)
  | ctor {x K} : Ctx e₁ e₂ x (.ctor K) (.ctor K)
  | rnil {x} : Ctx e₁ e₂ x .rnil .rnil
  | anil {x} : Ctx e₁ e₂ x .anil .anil
  | todo {x} : Ctx e₁ e₂ x .todo .todo
  | glob {x g} : Ctx e₁ e₂ x (.glob g) (.glob g)
  | lam {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.lam b) (.lam b')
  | fix {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.fix b) (.fix b')
  | task {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.task b) (.task b')
  | await {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.await b) (.await b')
  | lazy {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.lazy b) (.lazy b')
  | force {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.force b) (.force b')
  | raise {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.raise b) (.raise b')
  | scope {x b b'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.scope b) (.scope b')
  | proj {x b b' l} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.proj b l) (.proj b' l)
  | drec {x c r r'} : Ctx e₁ e₂ .top r r' → Ctx e₁ e₂ x (.drec c r) (.drec c r')
  | app {x f f' a a'} : Ctx e₁ e₂ x f f' → Ctx e₁ e₂ x a a' → Ctx e₁ e₂ x (.app f a) (.app f' a')
  | let_ {x a a' b b'} : Ctx e₁ e₂ .any a a' → Ctx e₁ e₂ .any b b' →
      Ctx e₁ e₂ x (.let_ a b) (.let_ a' b')
  | letrec {x a a' b b'} : Ctx e₁ e₂ .any a a' → Ctx e₁ e₂ .any b b' →
      Ctx e₁ e₂ x (.letrec a b) (.letrec a' b')
  | rcons {x l a a' r r'} : Ctx e₁ e₂ x a a' → Ctx e₁ e₂ x r r' →
      Ctx e₁ e₂ x (.rcons l a r) (.rcons l a' r')
  | update {x l a a' b b'} : Ctx e₁ e₂ .any a a' → Ctx e₁ e₂ .any b b' →
      Ctx e₁ e₂ x (.update a l b) (.update a' l b')
  | match_ {x a a' al al'} : Ctx e₁ e₂ .any a a' → Ctx e₁ e₂ .top al al' →
      Ctx e₁ e₂ x (.match_ a al) (.match_ a' al')
  | acons {x p b b' r r'} : Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x r r' →
      Ctx e₁ e₂ x (.acons p b r) (.acons p b' r')
  | aguard {x p g g' b b' r r'} : Ctx e₁ e₂ .any g g' → Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x r r' →
      Ctx e₁ e₂ x (.aguard p g b r) (.aguard p g' b' r')
  | try_ {x a a' al al'} : Ctx e₁ e₂ .any a a' → Ctx e₁ e₂ .top al al' →
      Ctx e₁ e₂ x (.try_ a al) (.try_ a' al')
  | handle {x v v' al al'} : Ctx e₁ e₂ .top v v' → Ctx e₁ e₂ .top al al' →
      Ctx e₁ e₂ x (.handle v al) (.handle v' al')
  | hdl {x E al al' r r' b b'} : Ctx e₁ e₂ .top al al' → Ctx e₁ e₂ .any r r' →
      Ctx e₁ e₂ .any b b' → Ctx e₁ e₂ x (.hdl E al r b) (.hdl E al' r' b')

variable {e₁ e₂ : Tm}

theorem Ctx.symm {x : Pos} {C₁ C₂ : Tm} (h : Ctx e₁ e₂ x C₁ C₂) : Ctx e₂ e₁ x C₂ C₁ := by
  induction h <;> constructor <;> assumption

theorem Ctx.src {x : Pos} {C₁ C₂ : Tm} (h : Ctx e₁ e₂ x C₁ C₂) (he : e₂.Src) : C₂.Src := by
  induction h <;> simp_all [Tm.Src]

theorem Pos.fr_any {x : Pos} (hx : x ≠ .park) : x.fr = .any := by
  cases x <;> simp_all

/-- **The context relates the two programs.** -/
theorem Ctx.sim {W : World} (h₀ : Sim W .any e₁ e₂) (he : e₂.Src) {x : Pos} {C₁ C₂ : Tm}
    (h : Ctx e₁ e₂ x C₁ C₂) (hx : x ≠ .park) : Sim W x C₁ C₂ := by
  have any : Pos.any ≠ .park := by decide
  have top : Pos.top ≠ .park := by decide
  have fr := Pos.fr_any hx
  induction h with
  | hole => exact h₀
  | var => exact .var hx
  | lit => exact .lit
  | const => exact .const
  | ctor => exact .ctor
  | rnil => exact .rnil
  | anil => exact .anil
  | todo => exact .todo
  | glob => exact .glob
  | lam h ih => exact .lam (ih any rfl) (h.src he).noIns
  | fix h ih => exact .fix (ih any rfl) (h.src he).noIns
  | task h ih => exact .task (ih any rfl) (h.src he).noIns
  | await _ ih => exact .await (fr ▸ ih any rfl)
  | lazy _ ih => exact .lazy (ih any rfl)
  | force _ ih => exact .force (fr ▸ ih any rfl)
  | raise _ ih => exact .raise (fr ▸ ih any rfl)
  | scope _ ih => exact .scope (ih any rfl)
  | proj _ ih => exact .proj (fr ▸ ih any rfl)
  | drec h ih => exact .drec (ih top rfl) (h.src he).noIns
  | app _ _ ih₁ ih₂ => exact .app hx (ih₁ hx fr) (ih₂ hx fr)
  | let_ _ _ ih₁ ih₂ => exact .let_ (fr ▸ ih₁ any rfl) (ih₂ any rfl)
  | letrec h₁ _ ih₁ ih₂ => exact .letrec (ih₁ any rfl) (h₁.src he).noIns (ih₂ any rfl)
  | rcons _ _ ih₁ ih₂ => exact .rcons hx (ih₁ hx fr) (ih₂ hx fr)
  | update _ _ ih₁ ih₂ => exact .update hx (ih₁ any rfl) (ih₂ any rfl)
  | match_ _ _ ih₁ ih₂ => exact .match_ (fr ▸ ih₁ any rfl) (ih₂ top rfl)
  | acons _ _ ih₁ ih₂ => exact .acons (ih₁ any rfl) (ih₂ hx fr)
  | aguard _ _ _ ih₁ ih₂ ih₃ => exact .aguard (ih₁ any rfl) (ih₂ any rfl) (ih₃ hx fr)
  | try_ _ _ ih₁ ih₂ => exact .try_ (fr ▸ ih₁ any rfl) (ih₂ top rfl)
  | handle _ _ ih₁ ih₂ => exact .handle (ih₁ top rfl) (ih₂ top rfl)
  | hdl _ _ _ ih₁ ih₂ ih₃ => exact .hdl (ih₁ top rfl) (ih₂ any rfl) (ih₃ any rfl)

/-- The world of two runs that have not started. -/
def World.empty : World := ⟨fun _ _ _ => False, fun _ => False, fun _ => False, fun _ _ => False⟩

theorem StR.empty : StR World.empty [] [] :=
  ⟨fun _ _ _ h => h.elim, fun _ _ _ _ _ _ h => h.elim, fun _ _ _ _ _ h => h.elim,
    fun _ h => h.elim, fun _ h => h.elim, fun _ _ h => h.elim, fun _ _ _ h => h.elim⟩

/-- A whole program: a closed typed term of the core under typed roots of the source language
(T1). -/
def Prog (S : Sig) (root : Roots) (e : Tm) : Prop :=
  RootTy S root ∧ root.Src ∧ e.Below 0 ∧
    ∃ t φ, Typ S [] Facts.none [] e (.ex t φ) ∧ t.YF 0 ∧ φ.substT (kill 0) = φ

/-- The literals a program may end with: its main thread ends with one. -/
def Ends (root : Roots) (e : Tm) (o : Lit) : Prop :=
  ∃ Λ μ, CSteps root ⟨[], [], e⟩ ⟨Λ, μ, .lit o⟩

/-- **Contextual equivalence**: every two typed programs one context builds around `e₁` and
`e₂` may end with the same literals. -/
def CtxEquiv (S : Sig) (e₁ e₂ : Tm) : Prop :=
  ∀ root C₁ C₂, Ctx e₁ e₂ .any C₁ C₂ → Prog S root C₁ → Prog S root C₂ →
    ∀ o, Ends root C₁ o ↔ Ends root C₂ o

theorem closed_of_typ {S : Sig} {Ψ : List Ty} {Θ : Facts} {e : Tm} {j : J}
    (h : Typ S Ψ Θ [] e j) : e.Closed := fun i hi => by
  have := h.frees i hi
  simp at this

/-- **Typing closes the threads** of every configuration a program reaches. -/
theorem Prog.reach {S : Sig} {root : Roots} {e : Tm} (h : Prog S root e) :
    Reach root ⟨[], [], e⟩ := by
  obtain ⟨hroot, -, hb, t, φ, h₀, ht, hφ⟩ := h
  intro c hs
  obtain ⟨X, _, hm, hΛ, -⟩ := steps_typed hroot hs ht hφ (CfgTy.init h₀ hb)
  refine ⟨closed_of_typ hm, fun p g hg k hk => ?_⟩
  obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
  exact closed_of_typ hk'

/-- A program simulated by another one from the start ends with a literal it ends with. -/
theorem Ends.sim {S : Sig} {root : Roots} {C₁ C₂ : Tm} {o : Lit}
    (hs : Sim World.empty .any C₁ C₂) (p₁ : Prog S root C₁) (p₂ : Prog S root C₂)
    (h : Ends root C₁ o) : Ends root C₂ o := by
  obtain ⟨Λ, μ, h⟩ := h
  have hr : CfgR World.empty ⟨[], [], C₁⟩ ⟨[], [], C₂⟩ :=
    ⟨.inl hs, StR.empty, ⟨rfl, fun _ _ _ h => by simp at h⟩⟩
  obtain ⟨W', c₂, -, hs₂, hr'⟩ := csteps_sim p₁.2.1 h hr p₁.reach p₂.reach
  obtain ⟨Λ₂, μ₂, hs₃⟩ := lit_sim (root := root) hr'
  exact ⟨Λ₂, μ₂, hs₂.trans hs₃⟩

/-- **Terms related both ways are contextually equivalent.** -/
theorem ctxEquiv_of_sim {S : Sig} (h₁₂ : Sim World.empty .any e₁ e₂)
    (h₂₁ : Sim World.empty .any e₂ e₁) (s₁ : e₁.Src) (s₂ : e₂.Src) : CtxEquiv S e₁ e₂ :=
  fun _ _ _ hc p₁ p₂ _ =>
    ⟨Ends.sim (hc.sim h₁₂ s₂ (by decide)) p₁ p₂, Ends.sim (hc.symm.sim h₂₁ s₁ (by decide)) p₂ p₁⟩

theorem _root_.Flick.Tm.Src.rename : ∀ {e : Tm}, e.Src → ∀ ξ, (e.rename ξ).Src := by
  intro e
  induction e <;> intro h ξ <;> simp_all [Tm.Src, Tm.rename]

/-- **O1a**: a cell nothing names is not made. -/
theorem o1a {S : Sig} {e c : Tm} (he : e.Src) (hc : c.Src) :
    CtxEquiv S (.let_ (.lazy e) (c.rename (· + 1))) c :=
  ctxEquiv_of_sim (.dropA rfl (Sim.refl hc _ (by decide)))
    (.dropC rfl hc.noIns (Sim.refl hc _ (by decide))) ⟨he, hc.rename _⟩ hc

/-- **O1b**: a cell of a value is made done. -/
theorem o1b {S : Sig} {v : Tm} (hv : Value v) (hs : v.Src) : CtxEquiv S (.lazy v) (born v) :=
  ctxEquiv_of_sim (.bornA hv hv (Sim.refl hs _ (by decide)))
    (.bornB hv hv (Sim.refl hs _ (by decide))) hs (by simp [born, Tm.Src, hs])

/-- **O1c**: a cell forced where it is made is not made. -/
theorem o1c {S : Sig} {e : Tm} (he : e.Src) : CtxEquiv S (.force (.lazy e)) e :=
  ctxEquiv_of_sim (.forceA (Sim.refl he _ (by decide)))
    (.forceC he.noIns (Sim.refl he _ (by decide))) he he

end Flick.Equiv
