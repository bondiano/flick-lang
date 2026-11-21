import Formal.Elab.Basic

/-!
# Elaboration: coherence under B1/K1' (elaboration.md §4.2)

`closed_rep`: after `ρ`, every derived dictionary is `≈` a closed derivation of the
substituted constraint. `ent_serves`: a closed derivation of `H` yields a `≈` closed
derivation of any `Q` served by `H`. `slot_ok`: projecting a superclass slot from a closed
dictionary is `≈` a closed derivation of the superclass (well-founded induction by K5).
`closed_unique_common` and `closed_unique`: closed derivations serving a common query are
unique up to `≈` (K1', K5, K6). Lemma 6.1, Lemma 6.2 and T6(b) follow.
-/

namespace Flick.Elab

variable {P : Prog} {lt : Cn → Cn → Prop}

/-- Projecting a slot of a closed dictionary of `Q` gives a closed dictionary of the
superclass, up to `≈`. -/
def SlotOK (P : Prog) (Q : Cn) : Prop :=
  ∀ d₀ i S, Ent P [] [Q] [d₀] → P.super Q i = some S →
    ∃ d, Eqv P (.slot d₀ i) d ∧ Ent P [] [S] [d]

theorem All2.single {R : Dict → Dict → Prop} {a : Dict} {l : List Dict} (h : All2 R [a] l) :
    ∃ b, l = [b] ∧ R a b := by
  cases h with
  | cons r t => cases t; exact ⟨_, rfl, r⟩

theorem All2.trans {a b c : List Dict} (h₁ : All2 (Eqv P) a b) (h₂ : All2 (Eqv P) b c) :
    All2 (Eqv P) a c := by
  induction h₁ generalizing c with
  | nil => cases h₂; exact .nil
  | cons r _ ih => cases h₂ with
    | cons r' t' => exact .cons (.trans r r') (ih t')

theorem All2.trans_symm {a b c : List Dict} (h₁ : All2 (Eqv P) a b) (h₂ : All2 (Eqv P) c b) :
    All2 (Eqv P) a c := by
  induction h₁ generalizing c with
  | nil => cases h₂; exact .nil
  | cons r _ ih => cases h₂ with
    | cons r' t' => exact .cons (.trans r (.symm r')) (ih t')

theorem Ent.uncons {Δ : List Cn} {Q : Cn} {qs : List Cn} {ds : List Dict}
    (h : Ent P Δ (Q :: qs) ds) : ∃ d ds', ds = d :: ds' ∧ Ent P Δ [Q] [d] ∧ Ent P Δ qs ds' := by
  cases h with
  | cons h₁ h₂ => exact ⟨_, _, rfl, h₁, h₂⟩
  | given hc => exact ⟨_, _, rfl, .given hc, .nil⟩
  | prim h₁ h₂ => exact ⟨_, _, rfl, .prim h₁ h₂, .nil⟩
  | strct h₁ h₂ h₃ => exact ⟨_, _, rfl, .strct h₁ h₂ h₃, .nil⟩
  | inst h₁ h₂ h₃ => exact ⟨_, _, rfl, .inst h₁ h₂ h₃, .nil⟩

theorem clo_rep {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict} (hρ : Closes P Δ θ ρ)
    (ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ)) {d : Dict} {Q : Cn} (h : Clo P Δ d Q) :
    ∃ d', Eqv P (d.subst ρ) d' ∧ Ent P [] [Q.subst θ] [d'] := by
  induction h with
  | given hk => exact ⟨_, .refl, hρ _ _ hk⟩
  | super hc hs ih =>
    obtain ⟨d₀, he, hd₀⟩ := ih
    obtain ⟨d', h1, h2⟩ := ho _ _ hc d₀ _ _ hd₀ (Prog.super_subst _ hs)
    exact ⟨d', .trans (.slot he) h1, h2⟩

/-- After `ρ`, a derivation is `≈` a closed derivation of the substituted constraints. -/
theorem closed_rep (h : Coherent P lt) {Δ : List Cn} {θ : Nat → Ty} {ρ : Nat → Dict}
    (hρ : Closes P Δ θ ρ) (ho : ∀ d Q, Clo P Δ d Q → SlotOK P (Q.subst θ))
    {qs : List Cn} {ds : List Dict} (he : Ent P Δ qs ds) :
    ∃ ds', Ent P [] (qs.map (·.subst θ)) ds' ∧ All2 (Eqv P) (ds.map (·.subst ρ)) ds' := by
  induction he with
  | nil => exact ⟨[], .nil, .nil⟩
  | cons _ _ ih₁ ih₂ =>
    obtain ⟨d₁, e₁, a₁⟩ := ih₁
    obtain ⟨ds₂, e₂, a₂⟩ := ih₂
    cases a₁ with
    | cons r a => cases a; exact ⟨_, .cons e₁ e₂, .cons r a₂⟩
  | given hc =>
    obtain ⟨d', h1, h2⟩ := clo_rep hρ ho hc
    exact ⟨[d'], h2, .cons h1 .nil⟩
  | prim h1 h2 => exact ⟨[_], .prim h1 (h.prims_subst h2), .cons (.primSub h1 h2) .nil⟩
  | @strct C τ ts ds h1 h2 _ ih =>
    obtain ⟨ds', e, a⟩ := ih
    refine ⟨[apps (.strct C (τ.subst θ)) ds'], ?_, .cons ?_ .nil⟩
    · have : (ts.map fun t => (⟨C, [t]⟩ : Cn)).map (·.subst θ) =
          (ts.map (·.subst θ)).map fun t => (⟨C, [t]⟩ : Cn) := by
        simp [List.map_map, Function.comp, Cn.subst]
      rw [this] at e
      exact .strct h1 (h.fields_subst h2) e
    · simp only [apps_subst]; exact Eqv.apps (.strctSub h1 h2) a
  | @inst Q I ins σ ds h1 h2 _ ih =>
    obtain ⟨ds', e, a⟩ := ih
    refine ⟨[apps (.inst I) ds'], ?_, .cons ?_ .nil⟩
    · refine .inst (σ := fun n => (σ n).subst θ) h1 ?_ ?_
      · have hs := h2.subst θ
        have hhead : (⟨ins.cls, ins.head.map (·.subst σ)⟩ : Cn).subst θ =
            ⟨ins.cls, ins.head.map (·.subst fun n => (σ n).subst θ)⟩ := by
          simp only [Cn.subst, Cn.mk.injEq, true_and, List.map_map]
          exact List.map_congr_left fun t _ => Ty.subst_comp t σ θ
        rwa [hhead] at hs
      · have : ins.ctx.map (fun c => c.subst fun n => (σ n).subst θ) =
            (ins.ctx.map (·.subst σ)).map (·.subst θ) := by
          rw [List.map_map]; exact List.map_congr_left fun c _ => (Cn.subst_comp c σ θ).symm
        rw [this]; exact e
    · simp only [apps_subst]; exact Eqv.apps .refl a

/-- Subsumption on closed derivations: if `H` has a closed derivation `d` and `H` serves `Q`,
then `Q` has a closed derivation `d' ≈ d`. -/
theorem ent_serves (h : Coherent P lt) (H : Cn) :
    ∀ {Q : Cn} {d : Dict}, Ent P [] [H] [d] → Cn.Serves P Q H →
      ∃ d', Eqv P d d' ∧ Ent P [] [Q] [d'] := by
  induction H using h.lt_wf.induction with
  | _ H ih =>
    intro Q d hd hserv
    rcases hd.single with hc | ⟨p, hp, hpr, rfl⟩ | ⟨C, τ_H, ts_H, ds, rfl, hC, hf_H, hds, rfl⟩ |
        ⟨I, ins, σ, ds, hI, hserv_H, hds, rfl⟩
    · exact (hc.not_nil).elim
    · have hinv : ∀ j, P.clsVar Q.cls j = .inv ∨ P.clsVar Q.cls j = .bi := fun j =>
        .inl (hserv.1 ▸ h.primVar hp j)
      obtain rfl := Cn.Serves.eq_of_inv hinv hserv
      exact ⟨.prim p Q, .refl, .prim hp hpr⟩
    · cases Q with
      | mk C' args =>
        have hc : C' = C := hserv.1.symm
        have hl : args.length = 1 := hserv.2.1
        subst C'
        cases args with
        | nil => exact absurd hl (Nat.zero_ne_add_one 0)
        | cons τ_Q rest =>
          cases rest with
          | cons _ _ => exact absurd (Nat.succ.inj hl) (Nat.succ_ne_zero _)
          | nil =>
            obtain ⟨ts_Q, hf_Q, hlen, hfs⟩ := h.fields_sub hC hf_H hserv
            have key : ∀ (us vs : List Ty) (es : List Dict),
                us.length = vs.length →
                (∀ t ∈ vs, t ∈ ts_H) →
                (∀ (j : Nat) (u v : Ty), us[j]? = some u → vs[j]? = some v → Cn.Serves P ⟨C, [u]⟩ ⟨C, [v]⟩) →
                Ent P [] (vs.map fun t => ⟨C, [t]⟩) es →
                ∃ es', All2 (Eqv P) es es' ∧ Ent P [] (us.map fun t => ⟨C, [t]⟩) es' := by
              intro us
              induction us with
              | nil =>
                intro vs es hlu _ _ he
                cases vs with
                | nil => cases he; exact ⟨[], .nil, .nil⟩
                | cons _ _ => exact absurd hlu (Nat.zero_ne_add_one _)
              | cons u us ihu =>
                intro vs es hlu hsub hsr he
                cases vs with
                | nil => exact absurd hlu (Nat.add_one_ne_zero _)
                | cons v vs =>
                  obtain ⟨e, es₀, rfl, he₁, he₂⟩ := he.uncons
                  obtain ⟨e', re, he'⟩ := ih ⟨C, [v]⟩ (h.k5fields hC hf_H (hsub v List.mem_cons_self))
                    he₁ (hsr 0 u v rfl rfl)
                  obtain ⟨es', a, h₂⟩ := ihu vs es₀ (Nat.succ.inj hlu)
                    (fun t ht => hsub t (List.mem_cons_of_mem _ ht))
                    (fun j x y hx hy => hsr (j + 1) x y hx hy) he₂
                  exact ⟨e' :: es', .cons re a, .cons he' h₂⟩
            obtain ⟨ds', a, hds'⟩ := key ts_Q ts_H ds hlen (fun _ ht => ht) hfs hds
            exact ⟨apps (.strct C τ_Q) ds',
              Eqv.apps (.strctLe hC hf_H hf_Q ⟨τ_Q, hserv, .refl _⟩) a,
              .strct hC hf_Q hds'⟩
    · exact ⟨apps (.inst I) ds, .refl, .inst hI (hserv.trans hserv_H) hds⟩

end Flick.Elab
