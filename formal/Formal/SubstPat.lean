import Formal.SubAdm

/-!
# Type substitution keeps subtyping, shapes, patterns and the arms of `try`
-/

namespace Flick

variable {S : Sig}

theorem Ty.hd_substT {t : Ty} {T : Name} (σ : Atom → Ty) (h : t.hd = .adt T) :
    (t.substT σ).hd = .adt T := by
  induction t <;> simp_all [Ty.hd, Ty.substT]

theorem Ty.hd_of_substT {t : Ty} {T : Name} (σ : Atom → Ty) (h : (t.substT σ).hd = .adt T) :
    t.hd = .adt T ∨ t.Opaque := by
  induction t with
  | tapp f a ih _ =>
    simp only [Ty.substT, Ty.hd] at h
    rcases ih h with h | h
    · exact .inl (by simp [Ty.hd, h])
    · exact .inr (by simpa [Ty.Opaque, Ty.hd] using h)
  | adt T' => simp [Ty.substT, Ty.hd] at h; exact .inl (by simp [Ty.hd, h])
  | base | rnil | rcons => simp [Ty.substT, Ty.hd] at h
  | _ => exact .inr (by simp [Ty.Opaque, Ty.hd])

theorem Ty.Opaque.of_substT {t : Ty} (σ : Atom → Ty) (h : (t.substT σ).Opaque) : t.Opaque := by
  induction t with
  | tapp f a ih _ => exact ih h
  | base | adt | rnil | rcons => simp [Ty.Opaque, Ty.hd, Ty.substT] at h
  | _ => simp [Ty.Opaque, Ty.hd]

theorem Le.substT {a b : Ty} (σ : Atom → Ty) (h : Le a b) : Le (a.substT σ) (b.substT σ) := by
  have sub : ∀ {x y : Ty}, Caps.Sub x.reach y.reach →
      Caps.Sub (x.substT σ).reach (y.substT σ).reach := fun hc a ha => by
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
    exact (Ty.reach_substT σ _ a).mpr ⟨b, hc b hb, h'⟩
  induction h with
  | refl => exact .refl
  | arrow _ hφ _ hc ih₁ ih₂ => exact .arrow ih₁ (Stage.le_substT σ hφ) ih₂ (sub hc)
  | eff hr he _ hc ih => exact .eff (hr.substT σ) (he.substT σ) ih (sub hc)

theorem Shp.of_substT {σ : Atom → Ty} :
    ∀ {t : Ty} {v : Tm}, Shp S true (t.substT σ) v → Shp S true t v
  | .base _, _, h | .adt _, _, h | .rnil, _, h => h
  | .rcons _ _ _, _, h => by
    simp only [Ty.substT] at h
    cases h with
    | hidden ho => simp [Ty.Opaque, Ty.hd] at ho
    | rcons h₁ h₂ => exact .rcons (Shp.of_substT h₁) (Shp.of_substT h₂)
    | adt _ _ hT => simp [Ty.hd] at hT
  | .tapp f a, _, h => by
    cases h with
    | hidden ho => exact .hidden (Ty.Opaque.of_substT σ ho)
    | adt hs hU hT =>
      rcases Ty.hd_of_substT σ hT with h | h
      · exact .adt hs hU h
      · exact .hidden h
  | .tvar _, _, _ | .rg _, _, _ | .caps _, _, _ | .row _, _, _ | .arrow .., _, _ | .ref _, _, _
  | .eff .., _, _ | .layer .., _, _ | .errors _, _, _ | .env _, _, _ | .hnd _, _, _
  | .owner _, _, _ | .dict .., _, _ => .hidden (by simp [Ty.Opaque, Ty.hd])

theorem Covers.substT {t ps e} (σ : Atom → Ty) (h : Covers S t ps e) :
    Covers S (t.substT σ) ps e :=
  h.imp (fun hx v hs => hx v (Shp.of_substT hs)) id

theorem PatTy.substT {fl p t Δ} (σ : Atom → Ty) (h : PatTy S fl p t Δ) :
    PatTy S fl p (t.substT σ) (Δ.map (Ty.substT σ)) := by
  induction h with
  | wild => exact .wild
  | var => exact .var
  | lit => exact .lit
  | head hK hk =>
    rw [S.ctorTy_substT σ _ hk hK]; exact .head hK (by simpa using hk)
  | app _ _ ih₁ ih₂ =>
    rw [List.map_append]
    exact .app (by simpa [Ty.substT, Stage.substT] using ih₁) ih₂
  | adt _ hU ih => exact .adt ih (Ty.hd_substT σ hU)
  | rany => exact .rany
  | rfield hR _ _ ih₁ ih₂ => rw [List.map_append]; exact .rfield (Ty.field_substT σ hR) ih₁ ih₂
  | or _ _ ih₁ ih₂ => exact .or ih₁ ih₂

theorem HPat.substT {ε₀ p θ Δ} (σ : Atom → Ty) (h : HPat S ε₀ p θ Δ) :
    HPat S (ε₀.substT σ) p (θ.substT σ) (Δ.map (Ty.substT σ)) := by
  cases h with
  | wild => exact .wild
  | var => exact .var
  | ctor hp => exact .ctor (hp.substT σ)

/-- A type that does not reach a region keeps not reaching it under a substitution that
keeps the region and brings in no other. -/
theorem Ty.no_rg_substT {t : Ty} {σ : Atom → Ty} {p : Nat}
    (hav : ∀ b q, (σ b).reach (.r q) → b = .r q) (ht : ¬ t.reach (.r p)) :
    ¬ (t.substT σ).reach (.r p) := fun h => by
  obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp h
  rw [hav b _ h'] at hb; exact ht hb

/-- What a `handle` lets through: all but the names of the raised row. -/
theorem Row.passes_substT {ε₀ ε' : Row} (σ : Atom → Ty) (h : ∀ x, ε₀ x → (∃ N, x = .nm N) ∨ ε' x) :
    ∀ x, ε₀.substT σ x → (∃ N, x = .nm N) ∨ ε'.substT σ x := by
  have hn : Row.substT σ (fun x => ∃ N, x = .nm N) = fun x => ∃ N, x = .nm N := by
    funext x; apply propext; cases x <;> simp [Row.substT]
  intro x hx
  have := Row.Sub.substT σ (r' := Row.union (fun x => ∃ N, x = .nm N) ε') h x hx
  rwa [Row.union_substT, hn] at this

theorem Catches.substT {ε₀ : Row} {p : Pat} {θ : Ty} {H H' : Row} (σ : Atom → Ty)
    (h : ∀ n, H n → Catches S ε₀ p θ n ∨ H' n) :
    ∀ n, H.substT σ n → Catches S (ε₀.substT σ) p (θ.substT σ) n ∨ H'.substT σ n := by
  intro n hn
  cases n with
  | nm N =>
    rcases hn with hn | ⟨b, hb, h'⟩
    · rcases h _ hn with (⟨rfl, h'⟩ | ⟨rfl, h'⟩) | h'
      · exact .inl (.inl ⟨rfl, .inl h'⟩)
      · exact .inl (.inr ⟨rfl, h'⟩)
      · exact .inr (.inl h')
    · rcases h _ hb with h | h
      · obtain ⟨rfl, hc⟩ := h; exact .inl (.inl ⟨rfl, .inr ⟨b, hc, h'⟩⟩)
      · exact .inr (.inr ⟨b, h, h'⟩)
  | atm a =>
    rcases hn with ⟨b, hb, h'⟩ | ⟨b, hb, h'⟩ <;> rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inl ⟨b, hc, h'⟩⟩
    · exact .inr (.inl ⟨b, h, h'⟩)
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inr ⟨b, hc, h'⟩⟩
    · exact .inr (.inr ⟨b, h, h'⟩)
  | ob N a =>
    rcases hn with ⟨b, hb, h'⟩ | ⟨v, b, hb, hv, h'⟩ <;> rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inl ⟨b, hc, h'⟩⟩
    · exact .inr (.inl ⟨b, h, h'⟩)
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inr ⟨v, b, hc, hv, h'⟩⟩
    · exact .inr (.inr ⟨v, b, h, hv, h'⟩)
  | obv w a =>
    obtain ⟨v, b, hb, hv, h'⟩ := hn
    rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, ⟨v, b, hc, hv, h'⟩⟩
    · exact .inr ⟨v, b, h, hv, h'⟩
  | need _ a | rv a =>
    obtain ⟨b, hb, h'⟩ := hn
    rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, ⟨b, hc, h'⟩⟩
    · exact .inr ⟨b, h, h'⟩

end Flick
