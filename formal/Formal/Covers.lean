import Formal.Typing

/-!
# Renaming and substitution keep values, matches and the coverage premise
-/

namespace Flick

variable {S : Sig}

theorem Tm.unguarded_rename (ξ : Nat → Nat) (A : Tm) : (A.rename ξ).unguarded = A.unguarded := by
  induction A <;> simp_all [Tm.rename, Tm.unguarded]

theorem Typ.unguarded_subst {Ψ σ} :
    ∀ {Γ A t t'}, Typ S Ψ Γ A (.alts t t') → (A.subst σ).unguarded = A.unguarded
  | _, .anil, _, _, _ => rfl
  | _, .acons _ _ _, _, _, h => by
    cases h with
    | acons _ _ hr => simp [Tm.subst, Tm.unguarded, Typ.unguarded_subst hr]
  | _, .aguard _ _ _ _, _, _, h => by
    cases h with
    | aguard _ _ _ hr => simp [Tm.subst, Tm.unguarded, Typ.unguarded_subst hr]
  | _, .var _, _, _, h | _, .lam _, _, _, h | _, .app _ _, _, _, h | _, .let_ _ _, _, _, h
  | _, .letrec _ _, _, _, h | _, .fix _, _, _, h | _, .lit _, _, _, h | _, .loc _, _, _, h
  | _, .const _, _, _, h | _, .ctor _, _, _, h | _, .rnil, _, _, h | _, .rcons _ _ _, _, _, h
  | _, .proj _ _, _, _, h | _, .update _ _ _, _, _, h | _, .match_ _ _, _, _, h
  | _, .todo, _, _, h => by cases h

theorem Tm.head_rename {ξ : Nat → Nat} {K} :
    ∀ {f : Tm}, f.head = .ctor K → (f.rename ξ).head = .ctor K
  | .app f _, h => by simp only [Tm.head, Tm.rename] at h ⊢; exact Tm.head_rename h
  | .ctor _, h => by simp_all [Tm.head, Tm.rename]
  | .var _, h | .lam _, h | .let_ _ _, h | .letrec _ _, h | .fix _, h | .lit _, h
  | .loc _, h | .const _, h | .rnil, h | .rcons _ _ _, h | .proj _ _, h | .update _ _ _, h
  | .match_ _ _, h | .anil, h | .acons _ _ _, h | .aguard _ _ _ _, h | .todo, h => by
    simp [Tm.head] at h

theorem Tm.head_subst {σ : Nat → Tm} {K} :
    ∀ {f : Tm}, f.head = .ctor K → (f.subst σ).head = .ctor K
  | .app f _, h => by simp only [Tm.head, Tm.subst] at h ⊢; exact Tm.head_subst h
  | .ctor _, h => by simp_all [Tm.head, Tm.subst]
  | .var _, h | .lam _, h | .let_ _ _, h | .letrec _ _, h | .fix _, h | .lit _, h
  | .loc _, h | .const _, h | .rnil, h | .rcons _ _ _, h | .proj _ _, h | .update _ _ _, h
  | .match_ _ _, h | .anil, h | .acons _ _ _, h | .aguard _ _ _ _, h | .todo, h => by
    simp [Tm.head] at h

theorem Value.rename {ξ : Nat → Nat} {e} (h : Value e) : Value (e.rename ξ) := by
  induction h with
  | ctorApp _ hf _ ih₁ ih₂ => exact .ctorApp ih₁ (Tm.head_rename hf) ih₂
  | constApp hc _ ih => exact .constApp hc ih
  | rcons _ _ ih₁ ih₂ => exact .rcons ih₁ ih₂
  | lam => exact .lam
  | fix => exact .fix
  | lit => exact .lit
  | loc => exact .loc
  | const => exact .const
  | ctor => exact .ctor
  | rnil => exact .rnil

theorem Value.subst {σ : Nat → Tm} {e} (h : Value e) : Value (e.subst σ) := by
  induction h with
  | ctorApp _ hf _ ih₁ ih₂ => exact .ctorApp ih₁ (Tm.head_subst hf) ih₂
  | constApp hc _ ih => exact .constApp hc ih
  | rcons _ _ ih₁ ih₂ => exact .rcons ih₁ ih₂
  | lam => exact .lam
  | fix => exact .fix
  | lit => exact .lit
  | loc => exact .loc
  | const => exact .const
  | ctor => exact .ctor
  | rnil => exact .rnil

theorem Tm.field_map (m : Tm → Tm)
    (hm : ∀ l e r, m (.rcons l e r) = .rcons l (m e) (m r)) {l} :
    ∀ {e x : Tm}, e.field l = some x → (m e).field l = some (m x)
  | .rcons l' e r, x, h => by
    rw [hm]
    simp only [Tm.field] at h ⊢
    split at h
    · simp_all
    · simp only [*, if_false]; exact Tm.field_map m hm h
  | .var _, _, h | .lam _, _, h | .app _ _, _, h | .let_ _ _, _, h | .letrec _ _, _, h
  | .fix _, _, h | .lit _, _, h | .loc _, _, h | .const _, _, h | .ctor _, _, h | .rnil, _, h
  | .proj _ _, _, h | .update _ _ _, _, h | .match_ _ _, _, h | .anil, _, h
  | .acons _ _ _, _, h | .aguard _ _ _ _, _, h | .todo, _, h => by simp [Tm.field] at h

/-- A map that keeps the constructors patterns inspect keeps every match. -/
theorem matchP_map (m : Tm → Tm)
    (hrc : ∀ l e r, m (.rcons l e r) = .rcons l (m e) (m r))
    (hap : ∀ f e, m (.app f e) = .app (m f) (m e))
    (hlit : ∀ l, m (.lit l) = .lit l) (hct : ∀ K, m (.ctor K) = .ctor K)
    (hnil : m .rnil = .rnil) :
    ∀ (p : Pat) {e : Tm}, (matchP p e).isSome → (matchP p (m e)).isSome
  | .wild, _, _ => by simp [matchP]
  | .var, _, _ => by simp [matchP]
  | .lit l, e, h => by
    cases e <;> simp [matchP] at h
    subst h; rw [hlit]; simp [matchP]
  | .ctor K, e, h => by
    cases e <;> simp [matchP] at h
    subst h; rw [hct]; simp [matchP]
  | .app p q, e, h => by
    cases e with
    | app f a =>
      rw [hap]
      simp only [matchP] at h ⊢
      cases h1 : matchP p f with
      | none => simp [h1] at h
      | some δ₁ =>
        cases h2 : matchP q a with
        | none => simp [h1, h2] at h
        | some δ₂ =>
          obtain ⟨d₁, e₁⟩ := Option.isSome_iff_exists.mp
            (matchP_map m hrc hap hlit hct hnil p (e := f) (by simp [h1]))
          obtain ⟨d₂, e₂⟩ := Option.isSome_iff_exists.mp
            (matchP_map m hrc hap hlit hct hnil q (e := a) (by simp [h2]))
          simp [e₁, e₂]
    | _ => simp [matchP] at h
  | .rany, e, h => by
    cases e <;> simp [matchP] at h
    · rw [hnil]; simp [matchP]
    · rw [hrc]; simp [matchP]
  | .rfield l p r, e, h => by
    simp only [matchP] at h ⊢
    cases hx : e.field l with
    | none => simp [hx] at h
    | some x =>
      cases ha : matchP p x with
      | none => simp [hx, ha] at h
      | some a =>
        cases hb : matchP r e with
        | none => simp [hx, hb] at h
        | some b =>
          obtain ⟨a', ha'⟩ := Option.isSome_iff_exists.mp
            (matchP_map m hrc hap hlit hct hnil p (e := x) (by simp [ha]))
          obtain ⟨b', hb'⟩ := Option.isSome_iff_exists.mp
            (matchP_map m hrc hap hlit hct hnil r (e := e) (by simp [hb]))
          simp [Tm.field_map m hrc hx, ha', hb']
  | .or p q, e, h => by
    simp only [matchP, Option.isSome_or] at h ⊢
    rcases Bool.or_eq_true_iff.mp h with h | h
    · simp [matchP_map m hrc hap hlit hct hnil p h]
    · simp [matchP_map m hrc hap hlit hct hnil q h]

theorem Covers.rename {ξ : Nat → Nat} {t ps e} (h : Covers S t ps e) :
    Covers S t ps (e.rename ξ) :=
  h.imp id fun ⟨hv, p, hp, hm⟩ => ⟨hv.rename, p, hp,
    matchP_map (Tm.rename ξ) (fun _ _ _ => rfl) (fun _ _ => rfl) (fun _ => rfl) (fun _ => rfl)
      rfl p hm⟩

theorem Covers.subst {σ : Nat → Tm} {t ps e} (h : Covers S t ps e) :
    Covers S t ps (e.subst σ) :=
  h.imp id fun ⟨hv, p, hp, hm⟩ => ⟨hv.subst, p, hp,
    matchP_map (Tm.subst σ) (fun _ _ _ => rfl) (fun _ _ => rfl) (fun _ => rfl) (fun _ => rfl)
      rfl p hm⟩

end Flick
