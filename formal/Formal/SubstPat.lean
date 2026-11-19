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
  | base | rnil | rcons | errors => simp [Ty.substT, Ty.hd] at h
  | _ => exact .inr (by simp [Ty.Opaque, Ty.hd])

theorem Ty.Opaque.of_substT {t : Ty} (σ : Atom → Ty) (h : (t.substT σ).Opaque) : t.Opaque := by
  induction t with
  | tapp f a ih _ => exact ih h
  | base | adt | rnil | rcons | errors => simp [Ty.Opaque, Ty.hd, Ty.substT] at h
  | _ => simp [Ty.Opaque, Ty.hd]

/-- A constructor that can build an instance can build the type. -/
theorem Sig.Builds.of_substT {K t} {σ : Atom → Ty} (h : S.Builds K (t.substT σ)) :
    S.Builds K t := by
  intro k ts T us hK ht hl
  obtain ⟨ρ, hρ⟩ := h k ts T (us.map (Ty.substT σ)) hK (by rw [ht, Ty.tcon_substT]) (by simpa using hl)
  refine ⟨fun a => (liftT k σ a).substT ρ, ?_⟩
  rw [← Ty.substT_comp, ← Ty.substT_comp, Ty.up_comm, Ty.tcon_substT T (liftT k σ),
    S.res_closed K k ts T _ hK fun i hi => by simp [liftT, hi]]
  exact hρ

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
    | adt hs hU hT hB =>
      rcases Ty.hd_of_substT σ hT with h | h
      · exact .adt hs hU h fun K hh => (hB K hh).of_substT
      · exact .hidden h
  -- a name of the substituted row is one of the row's, or comes from one of its variables
  | .errors ε, _, h => by
    simp only [Ty.substT] at h
    cases h with
    | hidden ho => simp [Ty.Opaque, Ty.hd] at ho
    | err hs hU hm =>
      obtain ⟨c, hc⟩ := hm
      rw [← Row.substT_asRow] at hc
      simp only [Row.substT] at hc
      rcases hc with ⟨c₀, h₀, _⟩ | ⟨b, G, hb, _⟩
      · exact .err hs hU ⟨c₀, h₀⟩
      · exact .errOpen ⟨b, G, hb⟩
    | errOpen hm =>
      obtain ⟨a, G, ha⟩ := hm
      rw [← Row.substT_asRow] at ha
      simp only [Row.substT] at ha
      obtain ⟨b, G', hb, _⟩ := ha
      exact .errOpen ⟨b, G', hb⟩
    | adt _ _ hT => simp [Ty.hd] at hT
  | .tvar _, _, _ | .rg _, _, _ | .caps _, _, _ | .arrow .., _, _ | .ref _, _, _
  | .eff .., _, _ | .hnd _, _, _
  | .owner _, _, _ | .dict .., _, _ | .all .., _, _ | .bare, _, _ | .lazy .., _, _
  | .lazyCell .., _, _ =>
    .hidden (by simp [Ty.Opaque, Ty.hd])
  | .cnil, _, _ | .rnm .., _, _ | .run .., _, _ | .rless .., _, _
  | .pure, _, _ | .rt .., _, _ | .rtob .., _, _ =>
    .hidden (by simp [Ty.Opaque, Ty.hd])

theorem Covers.substT {t ps e} (σ : Atom → Ty) (h : Covers S t ps e) :
    Covers S (t.substT σ) ps e :=
  h.imp (fun hx v hs => hx v (Shp.of_substT hs)) id

theorem PatTy.substT {fl p t Δ} (σ : Atom → Ty) (h : PatTy S fl p t Δ) :
    PatTy S fl p (t.substT σ) (Δ.map (Ty.substT σ)) := by
  induction h with
  | conv _ he ih => exact .conv ih (he.substT σ)
  | wild => exact .wild
  | var => exact .var
  | lit => exact .lit
  | head hK hj hk =>
    rw [S.ctorTyR_substT σ _ hk hK]; exact .head hK hj (by simpa using hk)
  | app _ _ ih₁ ih₂ =>
    rw [List.map_append]
    exact .app (by simpa [Ty.substT, Stage.substT] using ih₁) ih₂
  | adt _ hU ih => exact .adt ih (Ty.hd_substT σ hU)
  | rany => exact .rany
  | rfield hR _ _ ih₁ ih₂ => rw [List.map_append]; exact .rfield (Ty.field_substT σ hR) ih₁ ih₂
  | or _ _ ih₁ ih₂ => exact .or ih₁ ih₂
  | err _ hm ih =>
    refine .err ih ?_
    rw [← Row.substT_asRow]; simp only [Row.substT]
    exact .inl ⟨_, hm, rfl⟩

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
theorem Row.passes_substT {ε₀ ε' : Row} (σ : Atom → Ty)
    (h : ∀ x, ε₀ x → (∃ N c, x = .nm N c) ∨ ε' x) :
    ∀ x, ε₀.substT σ x → (∃ N c, x = .nm N c) ∨ ε'.substT σ x := by
  have hn : ∀ x, Row.substT σ (fun x => ∃ N c, x = .nm N c) x → ∃ N c, x = .nm N c := by
    intro x hx; cases x <;> simp [Row.substT] at hx ⊢
  intro x hx
  have := Row.Sub.substT σ (r' := Row.union (fun x => ∃ N c, x = .nm N c) ε') h x hx
  rw [Row.union_substT] at this
  exact this.imp_left (hn x)

theorem Catches.substT {ε₀ : Ty} {p : Pat} {θ : Ty} {H H' : Row} (σ : Atom → Ty)
    (h : ∀ n, H n → Catches S ε₀ p θ n ∨ H' n) :
    ∀ n, H.substT σ n → Catches S (ε₀.substT σ) p (θ.substT σ) n ∨ H'.substT σ n := by
  intro n hn
  cases n with
  | nm N c =>
    simp only [Catches, Ty.asRow_substT]
    rcases hn with ⟨c₀, hn, rfl⟩ | ⟨b, G, hb, hG, h'⟩
    · rcases h _ hn with (⟨rfl, h'⟩ | ⟨rfl, h'⟩) | h'
      · exact .inl (.inl ⟨rfl, .inl ⟨c₀, h', rfl⟩⟩)
      · exact .inl (.inr ⟨rfl, h'⟩)
      · exact .inr (.inl ⟨c₀, h', rfl⟩)
    · rcases h _ hb with h | h
      · obtain ⟨rfl, hc⟩ := h; exact .inl (.inl ⟨rfl, .inr ⟨b, G, hc, hG, h'⟩⟩)
      · exact .inr (.inr ⟨b, G, h, hG, h'⟩)
  | atm a =>
    simp only [Catches, Ty.asRow_substT]
    rcases hn with ⟨b, hb, h'⟩ | ⟨b, G, hb, h'⟩ <;> rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inl ⟨b, hc, h'⟩⟩
    · exact .inr (.inl ⟨b, h, h'⟩)
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inr ⟨b, G, hc, h'⟩⟩
    · exact .inr (.inr ⟨b, G, h, h'⟩)
  | ob N c a =>
    simp only [Catches, Ty.asRow_substT]
    rcases hn with ⟨c₀, b, hb, hc₀, h'⟩ | ⟨v, G, b, hb, hG, hv, h'⟩ <;> rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inl ⟨c₀, b, hc, hc₀, h'⟩⟩
    · exact .inr (.inl ⟨c₀, b, h, hc₀, h'⟩)
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, .inr ⟨v, G, b, hc, hG, hv, h'⟩⟩
    · exact .inr (.inr ⟨v, G, b, h, hG, hv, h'⟩)
  | obv w H a =>
    simp only [Catches, Ty.asRow_substT]
    obtain ⟨v, G, b, hb, hv, h'⟩ := hn
    rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, ⟨v, G, b, hc, hv, h'⟩⟩
    · exact .inr ⟨v, G, b, h, hv, h'⟩
  | rv a H =>
    simp only [Catches, Ty.asRow_substT]
    obtain ⟨b, G, hb, h'⟩ := hn
    rcases h _ hb with h | h
    · obtain ⟨rfl, hc⟩ := h; exact .inl ⟨rfl, ⟨b, G, hc, h'⟩⟩
    · exact .inr ⟨b, G, h, h'⟩

end Flick
