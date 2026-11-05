import Formal.Elab.T6a

/-!
# Elaboration: what an instance takes from a default (elaboration.md §2.5)

The default of the method `m` of class `C` is the top-level value `default_C_m` of the scheme
`∀ᾱ β̄. (C ᾱ, Q̄) ⇒ τ`: the class constraint first, then those of its default signature (none
without one). An instance `I` of `C τ̄` with the context `Q̄ᵢ` that does not define `m` has the
field `m = default_C_m (I δ̄) d̄`, where `d̄` are the dictionaries of `Q̄[ᾱ := τ̄, β̄ := σ]`
derived under the context (`instances.rs` `default_dictionaries`).

`this_ent`: the premises `δ̄` of an instance's context derive its own dictionary `I δ̄`.
`T6a_default`: a default typed at its scheme gives a field of the method's type, for the
dictionary of the class and derivations of the other constraints; `T6a_default_instance` is
it at the instance's own dictionary. What T6(b) says of any derivation holds of `d̄` as of the
dictionaries of a use, so coherence needs nothing more.
-/

namespace Flick.Elab

variable {P : Prog}

/-- The premises `δ_off, …, δ_{off+n-1}`. -/
def premises : Nat → Nat → List Dict
  | _, 0 => []
  | off, n + 1 => .var off :: premises (off + 1) n

/-- The premises from `off` on derive the constraints they are. -/
theorem ent_premises {Δ : List Cn} : ∀ (qs : List Cn) (off : Nat),
    (∀ k, k < qs.length → Δ[off + k]? = qs[k]?) → Ent P Δ qs (premises off qs.length)
  | [], _, _ => .nil
  | q :: qs, off, h => by
    have h₀ : Δ[off]? = some q := by simpa using h 0 (by simp)
    have h₁ : ∀ k, k < qs.length → Δ[off + 1 + k]? = qs[k]? := fun k hk => by
      have := h (k + 1) (by simp; omega)
      rwa [show off + (k + 1) = off + 1 + k by omega] at this
    exact .cons (.given (.given h₀)) (ent_premises qs (off + 1) h₁)

/-- **The instance's own dictionary**: under its context, `δ̄ ⊩ C τ̄σ ⇝ I δ̄`. -/
theorem this_ent {I : Nat} {ins : Inst} (σ : Nat → Ty) (hI : P.insts I = some ins) :
    Ent P (ins.ctx.map (·.subst σ)) [⟨ins.cls, ins.head.map (·.subst σ)⟩]
      [apps (.inst I) (premises 0 ins.ctx.length)] := by
  have h := ent_premises (P := P) (Δ := ins.ctx.map (·.subst σ)) (ins.ctx.map (·.subst σ)) 0
    (fun k _ => by simp)
  rw [List.length_map] at h
  exact .inst hI rfl rfl h

variable (X : Target P)

/-- **T6(a) of a default**: the field `default_C_m d d̄` has the method's type when `d` derives
the class constraint and `d̄` the default signature's. -/
theorem T6a_default {Γ Δ φ f this ds q C qs t} (hf : X.Has Γ Δ f (dicts q (C :: qs) t) φ)
    (hthis : Ent P Δ [C] [this]) (hds : Ent P Δ qs ds) :
    X.Has Γ Δ (f.dapps (this :: ds)) t φ :=
  tm_dapps X hf (ent_typed X (.cons hthis hds))

/-- T6(a) of a default at an instance: `default_C_m (I δ̄) d̄`, under the instance's context. -/
theorem T6a_default_instance {Γ φ I ins σ f ds q qs t} (hI : P.insts I = some ins)
    (hf : X.Has Γ (ins.ctx.map (·.subst σ)) f
      (dicts q (⟨ins.cls, ins.head.map (·.subst σ)⟩ :: qs) t) φ)
    (hds : Ent P (ins.ctx.map (·.subst σ)) qs ds) :
    X.Has Γ (ins.ctx.map (·.subst σ)) (f.dapps (apps (.inst I) (premises 0 ins.ctx.length) :: ds))
      t φ :=
  T6a_default X hf (this_ent σ hI) hds

end Flick.Elab
