import Formal.TySubstBase

/-!
# Substitution in what a handler answers

A substitution keeps the arguments a handler answers (`Sig.answers`), what it leaves outside
(`Sig.Handles`, `Sig.NonTail`) and what it gives its body (`Row.hP`), at the substituted
arguments of its arms.
-/

namespace Flick

variable {S : Sig}

theorem Sig.Handles.substR {E : Name} {us : List Ty} {al : Tm} {r : Row} (σ : Atom → Ty)
    (h : S.Handles E us al r) : S.Handles E (us.map (Ty.substT σ)) al (r.substT σ) :=
  ⟨h.1, fun hc => .inl ⟨_, h.2 hc, by rw [TyClass.substT_classOf, Ty.tcon_substT]⟩⟩

theorem Sig.Handles.substT {E : Name} {us : List Ty} {al : Tm} {r : Ty} (σ : Atom → Ty)
    (h : S.Handles E us al r.asRow) : S.Handles E (us.map (Ty.substT σ)) al (r.substT σ).asRow := by
  rw [Ty.asRow_substT]; exact h.substR σ

theorem Sig.NonTail.substR {E : Name} {us : List Ty} {r : Row} (σ : Atom → Ty)
    (h : S.NonTail E us r) : S.NonTail E (us.map (Ty.substT σ)) (r.substT σ) :=
  fun hc => .inl ⟨_, h hc, by rw [TyClass.substT_classOf, Ty.tcon_substT]⟩

theorem Ty.foldl_tapp_ne_base : ∀ (us : List Ty) (f : Ty), (∀ b, f ≠ .base b) →
    ∀ b, us.foldl .tapp f ≠ .base b
  | [], _, hf, b => hf b
  | u :: us, f, _, b => Ty.foldl_tapp_ne_base us (.tapp f u) (fun _ h => by cases h) b

theorem Ty.tcon_not_base {E : Name} {us : List Ty} {b : Base} :
    ¬ Ty.Equiv (.base b) (Ty.tcon E us) := fun h =>
  Ty.foldl_tapp_ne_base us _ (fun _ h => by cases h) b h.symm.base_right

/-- The arguments a handler answers after a substitution of its own. -/
theorem Sig.answers_substT {E : Name} {us : List Ty} (σ : Atom → Ty) {c : TyClass}
    (h : S.answers E us c) : S.answers E (us.map (Ty.substT σ)) (c.substT σ) :=
  h.imp id fun h => by rw [h, TyClass.substT_classOf, Ty.tcon_substT]

/-- A handler answers every argument exactly when its effect has no parameters. -/
theorem Sig.answers_all {E : Name} {us : List Ty} :
    (∀ c, S.answers E us c) ↔ S.arity E = 0 := by
  refine ⟨fun h => ?_, fun h c => .inl h⟩
  rcases h (Ty.base .unit).classOf with h | h
  · exact h
  · exact absurd (Ty.classOf_eq.mp h) fun he => Ty.tcon_not_base he

theorem Sig.answers_all_substT {E : Name} {us : List Ty} (σ : Atom → Ty) :
    (∀ c, S.answers E us c) ↔ ∀ c, S.answers E (us.map (Ty.substT σ)) c := by
  rw [Sig.answers_all, Sig.answers_all]

theorem Row.hP_substT_asRow {Θ : Facts} {q : Nat} {E : Name} {us : List Ty} {r : Ty}
    {σ : Atom → Ty} (hq : σ (.r q) = .rg q) (hat : ∀ b a, (σ b).reach a → σ b = a.ty)
    (hn : NoNames σ) :
    Row.Sub ((Row.hP Θ q E (S.answers E us) r.asRow).substT σ)
      (Row.hP (Θ.substT σ) q E (S.answers E (us.map (Ty.substT σ))) (r.substT σ).asRow) := by
  rw [Ty.asRow_substT]
  exact Row.hP_substT hq hat hn (fun _ => Sig.answers_substT σ) (Sig.answers_all_substT σ)

end Flick
