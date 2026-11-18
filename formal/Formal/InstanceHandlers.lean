import Formal.OpEquiv

/-!
# A handler answers one instance

What the rules of a handler of an effect with parameters (`core.md` §3.7, `Row.hBody`) give a
body, on an effect `Store` of one parameter. A handler of `Store Int` lets its body perform
`Store Int` and no other instance: `Store Bool` is not congruent to it, and stays apart under any
substitution (`Store` at a closed argument is closed). A row variable of its body is taken
without `Store`, so a generic body cannot bring another instance in through the tail it does not
know; a substitution keeps both, as the arguments a handler answers move with it
(`Sig.answers_substT`). An effect without parameters is answered whole, at every argument.
-/

namespace Flick

variable {S : Sig}

/-- `Store Bool` and `Store Int` are different instances. -/
theorem Ty.store_bool_not_int : ¬ Ty.Equiv (Ty.tcon "Store" [.base .bool]) (Ty.tcon "Store" [.base .int]) :=
  fun h => by
    have := (Ty.Equiv.foldl_tapp (vs := [.base .bool]) (us := [.base .int]) rfl h).2 0
      (by decide) (by decide)
    cases Ty.Equiv.base_right this

/-- A handler of `Store Int` lets its body perform `Store Int`. -/
theorem Row.hBody_store_int (r : Row) :
    Row.hBody "Store" (S.answers "Store" [.base .int]) r
      (.nm "Store" (Ty.tcon "Store" [.base .int]).classOf) :=
  .inl ⟨rfl, .inr rfl⟩

/-- It does not let its body perform `Store Bool`, whatever the row outside holds. -/
theorem Row.hBody_store_bool (h : S.arity "Store" = 1) (r : Row) :
    ¬ Row.hBody "Store" (S.answers "Store" [.base .int]) r
      (.nm "Store" (Ty.tcon "Store" [.base .bool]).classOf) := by
  rintro (⟨-, hA⟩ | ⟨hN, -⟩)
  · exact Ty.store_bool_not_int (Sig.answers_equiv (by simp [h]) (by simp [h]) hA)
  · exact hN rfl

/-- A row variable of the body of a handler of `Store ū` is taken without `Store`: a generic
body cannot hide another instance of it in its tail. -/
theorem Row.hBody_var_without (h : S.arity "Store" = 1) {us : List Ty} {r : Row} {a : Atom}
    {G : Name → Prop} (hb : Row.hBody "Store" (S.answers "Store" us) r (.rv a G)) : G "Store" := by
  obtain ⟨_, _, _, _, hG⟩ := hb
  exact hG fun hu => by have := Sig.answers_all.mp hu; omega

/-- A substitution keeps what a handler of `Store ū` answers, at the substituted arguments. -/
theorem Row.hBody_store_substT (σ : Atom → Ty) {us : List Ty} {c : TyClass}
    (h : S.answers "Store" us c) : S.answers "Store" (us.map (Ty.substT σ)) (c.substT σ) :=
  Sig.answers_substT σ h

/-- A handler of an effect without parameters answers every argument, and a variable of its
body may stand without fewer names inside. -/
theorem Row.hBody_of_none (h : S.arity "Log" = 0) {r : Row} {x : Elem} (hx : r x) :
    Row.hBody "Log" (S.answers "Log" []) r x :=
  Row.hBody_of (fun _ => .inl h) hx

end Flick
