import Formal.Patterns
import Formal.Semantics

/-!
# Progress (core.md §4, stage P1a)
-/

namespace Flick

variable {S : Sig}

theorem plug_append (E E' : List Frame) (e : Tm) : plug (E ++ E') e = plug E' (plug E e) := by
  induction E generalizing e with
  | nil => rfl
  | cons F E ih => exact ih _

theorem Step.ofHead {μ e μ' e'} (h : Head μ e μ' e') : Step μ e (.cfg μ' e') :=
  Step.head (E := []) (by simp) h

theorem ok_append {E : List Frame} {F : Frame} (hE : ∀ G ∈ E, G.ok) (hF : F.ok) :
    ∀ G ∈ E ++ [F], G.ok := by
  intro G hG
  rcases List.mem_append.mp hG with h | h
  · exact hE G h
  · simp at h; subst h; exact hF

/-- A step under one more frame. -/
theorem Step.frame {μ e o} (h : Step μ e o) {F : Frame} (hF : F.ok) :
    ∃ o', Step μ (F.plug e) o' := by
  cases h with
  | head hE hh =>
    have := Step.head (ok_append hE hF) hh
    rw [plug_append, plug_append] at this
    exact ⟨_, this⟩
  | todo hE =>
    have := Step.todo (μ := μ) (ok_append hE hF)
    rw [plug_append] at this
    exact ⟨_, this⟩

/-- The redex of a match on a value: some arm applies (by `Exh` or Р29). -/
theorem match_progress {Ψ μ v A t t'} (hv : Value v) (he : Typ S Ψ [] v (.ex t))
    (hA : Typ S Ψ [] A (.alts t t')) (hc : Covers S t A.unguarded v) :
    ∃ e', Head μ (.match_ v A) μ e' := by
  cases hA with
  | anil =>
    exfalso
    rcases hc with hx | ⟨_, _, hp, _⟩
    · obtain ⟨_, hp, _⟩ := hx v (hv.shape he).1; simp [Tm.unguarded] at hp
    · simp [Tm.unguarded] at hp
  | @acons p _ _ _ _ _ _ =>
    cases hm : matchP p v with
    | none => exact ⟨_, .matchNo hv hm⟩
    | some δ => exact ⟨_, .matchYes hv hm⟩
  | @aguard p _ _ _ _ _ _ _ =>
    cases hm : matchP p v with
    | none => exact ⟨_, .guardNo hv hm⟩
    | some δ => exact ⟨_, .guardYes hv hm⟩

theorem progress {Ψ μ} (hμ : StoreTy S Ψ μ) :
    ∀ {Γ e J}, Typ S Ψ Γ e J → Γ = [] → ∀ t, J = .ex t → Value e ∨ ∃ o, Step μ e o := by
  intro Γ e J h
  induction h with
  | var hi => intro hΓ; subst hΓ; simp at hi
  | lam => intros; exact .inl .lam
  | fix => intros; exact .inl .fix
  | lit => intros; exact .inl .lit
  | loc => intros; exact .inl .loc
  | const => intros; exact .inl .const
  | ctor => intros; exact .inl .ctor
  | rnil => intros; exact .inl .rnil
  | anil => intro _ _ hJ; cases hJ
  | acons => intro _ _ hJ; cases hJ
  | aguard => intro _ _ hJ; cases hJ
  | todo => intros; exact .inr ⟨_, Step.todo (E := []) (by simp)⟩
  | letrec => intros; exact .inr ⟨_, .ofHead .letrec⟩
  | let_ _ _ ih₁ _ =>
    intro hΓ _ _
    rcases ih₁ hΓ _ rfl with hv | ⟨_, hs⟩
    · exact .inr ⟨_, .ofHead (.let_ hv)⟩
    · exact .inr (hs.frame (F := .let_ _) trivial)
  | rcons _ _ _ ih₁ ih₂ =>
    intro hΓ _ _
    rcases ih₁ hΓ _ rfl with hv | ⟨_, hs⟩
    · rcases ih₂ hΓ _ rfl with hr | ⟨_, hs⟩
      · exact .inl (.rcons hv hr)
      · exact .inr (hs.frame (F := .rconsR _ _) hv)
    · exact .inr (hs.frame (F := .rconsL _ _) trivial)
  | proj he hR ih =>
    intro hΓ _ _
    rcases ih hΓ _ rfl with hv | ⟨_, hs⟩
    · subst hΓ
      obtain ⟨⟨w, hw⟩, _⟩ := (hv.shape he).1.field rfl hR
      exact .inr ⟨_, .ofHead (.proj hv hw)⟩
    · exact .inr (hs.frame (F := .proj _) trivial)
  | update he hR _ ih₁ ih₂ =>
    intro hΓ _ _
    rcases ih₁ hΓ _ rfl with hv | ⟨_, hs⟩
    · rcases ih₂ hΓ _ rfl with hw | ⟨_, hs⟩
      · subst hΓ
        obtain ⟨_, hset⟩ := (hv.shape he).1.field rfl hR
        obtain ⟨_, hv'⟩ := hset _
        exact .inr ⟨_, .ofHead (.update hv hw hv')⟩
      · exact .inr (hs.frame (F := .updR _ _) hv)
    · exact .inr (hs.frame (F := .updL _ _) trivial)
  | match_ he hA hc ih _ =>
    intro hΓ _ _
    rcases ih hΓ _ rfl with hv | ⟨_, hs⟩
    · subst hΓ
      obtain ⟨_, hh⟩ := match_progress (μ := μ) hv he hA hc
      exact .inr ⟨_, .ofHead hh⟩
    · exact .inr (hs.frame (F := .match_ _) trivial)
  | app hf he ih₁ ih₂ =>
    intro hΓ _ hJ
    cases hJ
    rcases ih₁ hΓ _ rfl with hvf | ⟨_, hs⟩
    · rcases ih₂ hΓ _ rfl with hve | ⟨_, hs⟩
      · subst hΓ
        rcases canonical_arrow hvf hf with ⟨b, rfl⟩ | ⟨f, rfl⟩ | ⟨K, hK⟩ | ⟨c, rfl⟩ |
            ⟨c, w, rfl, hc, hw⟩
        · exact .inr ⟨_, .ofHead (.beta hve)⟩
        · exact .inr ⟨_, .ofHead (.fix hve)⟩
        · exact .inl (.ctorApp hvf hK hve)
        · cases c with
          | refMake => exact .inr ⟨_, .ofHead (.refMake hve)⟩
          | refGet =>
            have hty := hf.const_inv
            simp only [Const.ty, Ty.arrow.injEq] at hty
            obtain ⟨rfl, rfl⟩ := hty
            obtain ⟨l, rfl, hl⟩ := canonical_ref hve he
            have hlt : l < μ.length := hμ.1 ▸ getElem?_lt hl
            exact .inr ⟨_, .ofHead (.refGet (List.getElem?_eq_getElem hlt))⟩
          | refSet => exact .inl (.constApp rfl hve)
        · cases c with
          | refMake => simp [Const.arity] at hc
          | refGet => simp [Const.arity] at hc
          | refSet =>
            cases hf with
            | app hc' hw' =>
              have hty := hc'.const_inv
              simp only [Const.ty, Ty.arrow.injEq] at hty
              obtain ⟨rfl, rfl, rfl⟩ := hty
              obtain ⟨l, rfl, hl⟩ := canonical_ref hw hw'
              have hlt : l < μ.length := hμ.1 ▸ getElem?_lt hl
              exact .inr ⟨_, .ofHead (.refSet hve hlt)⟩
      · exact .inr (hs.frame (F := .appR _) hvf)
    · exact .inr (hs.frame (F := .appL _) trivial)

end Flick
