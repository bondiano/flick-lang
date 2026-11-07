import Formal.WeakenBase

/-!
# Structural lemmas: store weakening and renaming
-/

namespace Flick

variable {S : Sig}

theorem held_weaken {Ψ Γ e} (X : List Ty) (hl : ∀ l, e.Has (.loc l) → l < Ψ.length) :
    Caps.Sub (held (Ψ ++ X) Γ e) (held Ψ Γ e) := by
  rintro a (h | ⟨p, hp, rfl⟩ | ⟨l, t, hle, ht, hr⟩)
  · exact .inl h
  · exact .inr (.inl ⟨p, hp, rfl⟩)
  · rw [List.getElem?_append_left (hl l hle)] at ht
    exact .inr (.inr ⟨l, t, hle, ht, hr⟩)

/-- Store weakening: `Ψ ⊆ Ψ'` keeps every judgment. -/
theorem Typ.weakenStore {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) (X : List Ty) :
    Typ S (Ψ ++ X) Θ Γ e J := by
  induction h generalizing X with
  | loc hl => exact .loc (by rw [List.getElem?_append_left (getElem?_lt hl)]; exact hl)
  | var hi hts hH hT => exact .var hi hts hH hT
  | allVar hi hk hc => exact .allVar hi hk hc
  | allI hv _ ih => refine .allI hv ?_; rw [List.map_append]; exact ih _
  | lam hb hn hc hT ih => exact .lam (ih X) hn (Caps.Sub.trans (held_weaken X hb.locs) hc) hT
  | app _ _ hψ hT ih₁ ih₂ => exact .app (ih₁ X) (ih₂ X) hψ hT
  | let_ _ _ ih₁ ih₂ => exact .let_ (ih₁ X) (ih₂ X)
  | letGen _ _ ih₁ ih₂ =>
    refine .letGen ?_ (ih₂ X)
    rw [List.map_append]; exact ih₁ _
  | letrec hf hn hc _ ih₁ ih₂ =>
    exact .letrec (ih₁ X) hn (Caps.Sub.trans (held_weaken X hf.locs) hc) (ih₂ X)
  | fix hf hn hc hT ih => exact .fix (ih X) hn (Caps.Sub.trans (held_weaken X hf.locs) hc) hT
  | lit => exact .lit
  | const hc hT => exact .const hc hT
  | ctor hK hk hH hT => exact .ctor hK hk hH hT
  | rnil => exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => exact .rcons hR hl (ih₁ X) (ih₂ X)
  | proj _ hf hT ih => exact .proj (ih X) hf hT
  | update _ hf _ ih₁ ih₂ => exact .update (ih₁ X) hf (ih₂ X)
  | match_ _ _ hc ih₁ ih₂ => exact .match_ (ih₁ X) (ih₂ X) hc
  | anil => exact .anil
  | acons hp _ _ ih₁ ih₂ => exact .acons hp (ih₁ X) (ih₂ X)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ => exact .aguard hp (ih₁ X) (ih₂ X) (ih₃ X)
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    refine .aref hK hU hn hfr (fun l h => by simp only [List.length_append]; have := hlc l h; omega) hpA
      (fun θ hθ hu => ?_) (ihr X)
    rw [List.map_append, List.map_append]; exact ihA θ hθ hu _
  | todo => exact .todo
  | task he hψ hn hc hT ih => exact .task (ih X) hψ hn (Caps.Sub.trans (held_weaken X he.locs) hc) hT
  | await _ hφ hT ih => exact .await (ih X) hφ hT
  | raise _ hφ ih => exact .raise (ih X) hφ
  | raiseAdt _ hU hφ ih => exact .raiseAdt (ih X) hU hφ
  | errVal hv _ hU hE ih => exact .errVal hv (ih X) hU hE
  | tryR _ _ hφ hs ih₁ ih₂ => exact .tryR (ih₁ X) (ih₂ X) hφ hs
  | tryP _ _ ih₁ ih₂ => exact .tryP (ih₁ X) (ih₂ X)
  | handle hv _ _ hc ha hφ ih₁ ih₂ => exact .handle hv (ih₁ X) (ih₂ X) hc ha hφ
  | hnil hH => exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ => exact .hcons hp (ih₁ X) (ih₂ X) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ => exact .hguard hp (ih₁ X) (ih₂ X) (ih₃ X) hH
  | scope hn hF _ hφ' hφ hT ih =>
    refine .scope hn hF ?_ hφ' hφ hT
    rw [List.map_append]; exact ih _
  | inside hF _ hφ' ht hε hφ hT ih => exact .inside hF (ih X) hφ' ht hε hφ hT
  | cap => exact .cap
  | own => exact .own
  | drec hc hk hr hn hh ih =>
    exact .drec hc hk (ih X) hn (Caps.Sub.trans (held_weaken (e := .drec _ _) X hr.locs) hh)
  | dnil => exact .dnil
  | dcons _ _ ih₁ ih₂ =>
    refine .dcons ?_ (ih₂ X)
    rw [List.map_append]; exact ih₁ _
  | dproj _ hc hk hl hts hH hT ih => exact .dproj (ih X) hc hk hl hts hH hT
  | glob hg hts hH hT => exact .glob hg hts hH hT
  | perform hop hT => exact .perform hop hT
  | hanil => exact .hanil
  | hatail hop _ _ ih₁ ih₂ => exact .hatail hop (ih₁ X) (ih₂ X)
  | haabort hop _ _ ih₁ ih₂ => exact .haabort hop (ih₁ X) (ih₂ X)
  | hactl hop _ _ ih₁ ih₂ => exact .hactl hop (ih₁ X) (ih₂ X)
  | hdl _ hn _ hψ _ hφ₀ hOb hc hφ hT ih₁ ih₂ ih₃ =>
    refine .hdl ?_ hn (ih₂ X) hψ (ih₃ X) hφ₀ hOb hc hφ hT
    rw [List.map_append]; exact ih₁ _
  | hwith hℓ _ _ hψ _ hφ₀ hc hφ hT ih₁ ih₂ ih₃ =>
    exact .hwith (getElem?_ext hℓ) (ih₁ X) (ih₂ X) hψ (ih₃ X) hφ₀ hc hφ hT
  | hctl hℓ _ _ hψ hop hc htl hE hk _ hφ₀ _ hl hcm hφ hT ih₁ ih₂ ih₃ ih₄ =>
    exact .hctl (getElem?_ext hℓ) (ih₁ X) (ih₂ X) hψ hop hc htl hE hk (ih₃ X) hφ₀ (ih₄ X)
      (fun h => let ⟨C', h₁, h₂⟩ := hl h
        ⟨C', by rw [List.getElem?_append_left (getElem?_lt h₁)]; exact h₁, h₂⟩) hcm hφ hT
  | hkeep hℓ _ _ hφ₀ hc hnt hle hφ hT ih₁ ih₂ =>
    exact .hkeep (getElem?_ext hℓ) (ih₁ X) (ih₂ X) hφ₀ hc hnt hle hφ hT
  | hkarm hℓ _ hop hE hk _ hφ₀ hc hnt _ hle hφ hT ih₁ ih₂ ih₃ =>
    exact .hkarm (getElem?_ext hℓ) (ih₁ X) hop hE hk (ih₂ X) hφ₀ hc hnt (ih₃ X) hle hφ hT
  | hres hℓ _ hA ha _ hl hφ hT ih₁ ih₂ =>
    exact .hres (getElem?_ext hℓ) (ih₁ X) hA ha (ih₂ X) (by rw [List.getElem?_append_left (getElem?_lt hl)]; exact hl) hφ hT
  | hdone _ ih => exact .hdone (ih X)
  | unw hℓ _ hφ ih => exact .unw (getElem?_ext hℓ) (ih X) hφ
  | kont hl hC hT => exact .kont (by rw [List.getElem?_append_left (getElem?_lt hl)]; exact hl) hC hT

/-- A renaming `ξ : Γ → Γ'`. -/
def RenOk (Γ Γ' : List Scheme) (ξ : Nat → Nat) : Prop :=
  ∀ (i : Nat) t, Γ[i]? = some t → Γ'[ξ i]? = some t

theorem RenOk.up {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (k : Nat) :
    RenOk (Γ.map (Scheme.up k)) (Γ'.map (Scheme.up k)) ξ := by
  intro i t hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi ⊢
  obtain ⟨s, hs, rfl⟩ := hi
  exact ⟨s, h _ _ hs, rfl⟩

theorem RenOk.map {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (f : Scheme → Scheme) :
    RenOk (Γ.map f) (Γ'.map f) ξ := by
  intro i t hi
  simp only [List.getElem?_map, Option.map_eq_some_iff] at hi ⊢
  obtain ⟨s, hs, rfl⟩ := hi
  exact ⟨s, h _ _ hs, rfl⟩

theorem RenOk.lift {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) (Δ : List Scheme) {n} (hn : Δ.length = n) :
    RenOk (Δ ++ Γ) (Δ ++ Γ') (liftR n ξ) := by
  intro i t hi
  subst hn
  unfold liftR
  split
  · rw [List.getElem?_append_left (by omega)] at hi ⊢; exact hi
  · rw [List.getElem?_append_right (by omega)] at hi ⊢
    simpa using h _ _ hi

theorem held_rename {Ψ Γ Γ' ξ} (hξ : RenOk Γ Γ' ξ) {e : Tm}
    (hfree : ∀ i, e.Free i → i < Γ.length) :
    Caps.Sub (held Ψ Γ' (e.rename ξ)) (held Ψ Γ e) := by
  rintro a (⟨j, s, hj, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hle, ht, hr⟩)
  · obtain ⟨i, hi, rfl⟩ := Tm.free_rename ξ e j hj
    have hs' := List.getElem?_eq_getElem (hfree i hi)
    rw [hξ _ _ hs'] at hs; cases hs
    exact .inl ⟨i, _, hi, hs', ha⟩
  · exact .inr (.inl ⟨p, (Tm.has_rename _ _ _).mp hp, rfl⟩)
  · exact .inr (.inr ⟨l, t, (Tm.has_rename _ _ _).mp hle, ht, hr⟩)

theorem RenOk.frees {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) {e : Tm} (hf : ∀ i, e.Free i → i < Γ.length) :
    ∀ i, (e.rename ξ).Free i → i < Γ'.length := by
  intro i hi
  obtain ⟨j, hj, rfl⟩ := Tm.free_rename ξ e i hi
  exact getElem?_lt (h _ _ (List.getElem?_eq_getElem (hf j hj)))

/-- Renaming under `n` binders keeps a term within its context. -/
theorem RenOk.frees_lift {Γ Γ' ξ} (h : RenOk Γ Γ' ξ) {e : Tm} {n : Nat}
    (hf : ∀ i, e.Free i → i < n + Γ.length) :
    ∀ i, (e.rename (liftR n ξ)).Free i → i < n + Γ'.length := by
  have hl := h.lift (List.replicate n (Scheme.mono .rnil)) (n := n) (by simp)
  intro i hi
  simpa using hl.frees (fun j hj => by simpa using hf j hj) i hi

theorem Tm.NoIns.rename {e : Tm} (ξ : Nat → Nat) (h : e.NoIns) : (e.rename ξ).NoIns :=
  fun h' => h ((Tm.has_rename _ _ _).mp h')

/-- A body parked at a hole whose filling is closed and typed is closed. -/
theorem plug_hole_closed {Ψ Θ Ei x J} {p : Nat} (h : Typ S Ψ Θ [] (plug Ei x) J) :
    (plug Ei (.hole p)).Closed :=
  fun i hi => absurd (h.frees i (plug_free_of_hole hi)) (Nat.not_lt_zero _)

theorem Tm.armOf_rename {ξ : Nat → Nat} {o : Name} :
    ∀ {al : Tm}, ((al.rename ξ).armOf o).isSome = (al.armOf o).isSome := by
  intro al
  induction al generalizing ξ with
  | acons p b r _ ih =>
    simp only [Tm.rename, Tm.armOf]
    split <;> simp_all
  | _ => simp [Tm.rename, Tm.armOf]

theorem Typ.rename {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ {Γ' ξ}, RenOk Γ Γ' ξ → Typ S Ψ Θ Γ' (e.rename ξ) J := by
  induction h with
  | var hi hts hH hT => intro _ _ hξ; exact .var (hξ _ _ hi) hts hH hT
  | allVar hi hk hc => intro _ _ hξ; exact .allVar (hξ _ _ hi) hk hc
  | allI hv _ ih => intro _ _ hξ; exact .allI hv.rename (ih (hξ.up _))
  | lam hb hn hc hT ih =>
    intro _ _ hξ
    exact .lam (ih (hξ.lift [_] rfl)) (hn.rename _)
      (Caps.Sub.trans (held_rename (e := .lam _) hξ fun i h => by
        have := hb.frees _ h; simp at this; omega) hc) hT
  | app _ _ hψ hT ih₁ ih₂ => intro _ _ hξ; exact .app (ih₁ hξ) (ih₂ hξ) hψ hT
  | let_ _ _ ih₁ ih₂ => intro _ _ hξ; exact .let_ (ih₁ hξ) (ih₂ (hξ.lift [_] rfl))
  | letGen _ _ ih₁ ih₂ =>
    intro _ _ hξ; exact .letGen (ih₁ (hξ.up _)) (ih₂ (hξ.lift [_] rfl))
  | letrec hf hn hc _ ih₁ ih₂ =>
    intro _ _ hξ
    exact .letrec (ih₁ (hξ.lift [_, _] rfl)) (hn.rename _)
      (Caps.Sub.trans (held_rename (e := .fix _) hξ fun i h => by
        have := hf.frees _ h; simp at this; omega) hc) (ih₂ (hξ.lift [_] rfl))
  | fix hf hn hc hT ih =>
    intro _ _ hξ
    exact .fix (ih (hξ.lift [_, _] rfl)) (hn.rename _)
      (Caps.Sub.trans (held_rename (e := .fix _) hξ fun i h => by
        have := hf.frees _ h; simp at this; omega) hc) hT
  | lit => intro _ _ _; exact .lit
  | loc hl => intro _ _ _; exact .loc hl
  | const hc hT => intro _ _ _; exact .const hc hT
  | ctor hK hk hH hT => intro _ _ _; exact .ctor hK hk hH hT
  | rnil => intro _ _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ => intro _ _ hξ; exact .rcons hR hl (ih₁ hξ) (ih₂ hξ)
  | proj _ hf hT ih => intro _ _ hξ; exact .proj (ih hξ) hf hT
  | update _ hf _ ih₁ ih₂ => intro _ _ hξ; exact .update (ih₁ hξ) hf (ih₂ hξ)
  | match_ _ _ hc ih₁ ih₂ =>
    intro _ ξ hξ
    exact .match_ (ih₁ hξ) (ih₂ hξ)
      (by rw [Tm.unguarded_rename]; exact hc.map (Homo.rename ξ))
  | anil => intro _ _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro _ _ hξ; exact .acons hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ hξ)
  | aref hK hU hn hfr hlc hpA _ _ ihA ihr =>
    intro _ _ hξ
    exact .aref hK hU (hn.rename _) (hξ.frees_lift hfr) (fun l h => hlc l ((Tm.has_rename _ _ _).mp h))
      (fun θ hθ hu => hpA θ ((Fits.rename_iff _ _ _).mp hθ) hu)
      (fun θ hθ hu => ihA θ ((Fits.rename_iff _ _ _).mp hθ) hu
        (((hξ.up _).map _).lift _ (hpA θ ((Fits.rename_iff _ _ _).mp hθ) hu).spine_binds))
      (ihr hξ)
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro _ _ hξ
    exact .aguard hp (ih₁ (hξ.lift _ (by simpa using hp.binds)))
      (ih₂ (hξ.lift _ (by simpa using hp.binds))) (ih₃ hξ)
  | todo => intro _ _ _; exact .todo
  | task he hψ hn hc hT ih =>
    intro _ _ hξ
    exact .task (ih hξ) hψ (hn.rename _)
      (Caps.Sub.trans (held_rename (e := .task _) hξ he.frees) hc) hT
  | await _ hφ hT ih => intro _ _ hξ; exact .await (ih hξ) hφ hT
  | raise _ hφ ih => intro _ _ hξ; exact .raise (ih hξ) hφ
  | raiseAdt _ hU hφ ih => intro _ _ hξ; exact .raiseAdt (ih hξ) hU hφ
  | errVal hv _ hU hE ih => intro _ _ hξ; exact .errVal hv.rename (ih hξ) hU hE
  | tryR _ _ hφ hs ih₁ ih₂ => intro _ _ hξ; exact .tryR (ih₁ hξ) (ih₂ hξ) hφ hs
  | tryP _ _ ih₁ ih₂ => intro _ _ hξ; exact .tryP (ih₁ hξ) (ih₂ hξ)
  | handle hv _ _ hc ha hφ ih₁ ih₂ =>
    intro _ ξ hξ
    exact .handle hv.rename (ih₁ hξ) (ih₂ hξ) (hc.map (Homo.rename ξ) hv) ha hφ
  | hnil hH => intro _ _ _; exact .hnil hH
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro _ _ hξ; exact .hcons hp (ih₁ (hξ.lift _ (by simpa using hp.binds))) (ih₂ hξ) hH
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro _ _ hξ
    exact .hguard hp (ih₁ (hξ.lift _ (by simpa using hp.binds)))
      (ih₂ (hξ.lift _ (by simpa using hp.binds))) (ih₃ hξ) hH
  | scope hn hF _ hφ' hφ hT ih =>
    intro _ _ hξ; exact .scope (hn.rename _) hF (ih ((hξ.up 1).lift [_] rfl)) hφ' hφ hT
  | inside hF _ hφ' ht hε hφ hT ih => intro _ _ hξ; exact .inside hF (ih hξ) hφ' ht hε hφ hT
  | cap => intro _ _ _; exact .cap
  | own => intro _ _ _; exact .own
  | drec hc hk hr hn hh ih =>
    intro _ _ hξ
    exact .drec hc hk (ih hξ) (hn.rename _)
      (Caps.Sub.trans (held_rename (e := .drec _ _) hξ hr.frees) hh)
  | dnil => intro _ _ _; exact .dnil
  | dcons _ _ ih₁ ih₂ => intro _ _ hξ; exact .dcons (ih₁ (hξ.up _)) (ih₂ hξ)
  | dproj _ hc hk hl hts hH hT ih => intro _ _ hξ; exact .dproj (ih hξ) hc hk hl hts hH hT
  | perform hop hT => intro _ _ _; exact .perform hop hT
  | hanil => intro _ _ _; exact .hanil
  | hatail hop _ _ ih₁ ih₂ => intro _ _ hξ; exact .hatail hop (ih₁ (hξ.lift [_] rfl)) (ih₂ hξ)
  | haabort hop _ _ ih₁ ih₂ => intro _ _ hξ; exact .haabort hop (ih₁ (hξ.lift [_] rfl)) (ih₂ hξ)
  | hactl hop _ _ ih₁ ih₂ => intro _ _ hξ; exact .hactl hop (ih₁ (hξ.lift [_, _] rfl)) (ih₂ hξ)
  | hdl _ hn _ hψ _ hφ₀ hOb hc hφ hT ih₁ ih₂ ih₃ =>
    intro _ ξ hξ
    exact .hdl (ih₁ (hξ.up 1)) (hn.rename ξ) (ih₂ hξ) hψ (ih₃ hξ) hφ₀ hOb ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [Tm.armOf_rename]; exact hc' o c a b ho⟩ hφ hT
  | hwith hℓ _ _ hψ _ hφ₀ hc hφ hT ih₁ ih₂ ih₃ =>
    intro _ ξ hξ
    exact .hwith hℓ (ih₁ hξ) (ih₂ hξ) hψ (ih₃ hξ) hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [Tm.armOf_rename]; exact hc' o c a b ho⟩ hφ hT
  | hctl hℓ _ _ hψ hop hc htl hE hk hp hφ₀ _ hl hcm hφ hT ih₁ ih₂ _ ih₄ =>
    intro _ ξ hξ
    subst hk
    rw [Tm.rename, (plug_hole_closed hp).rename]
    exact .hctl hℓ (ih₁ hξ) (ih₂ hξ) hψ hop hc htl hE rfl hp hφ₀ (ih₄ hξ) hl ⟨hcm.1, fun h => hcm.2 fun hc' => h fun o c a b ho => by
      rw [Tm.armOf_rename]; exact hc' o c a b ho⟩ hφ hT
  | hkeep hℓ _ _ hφ₀ hc hnt hle hφ hT ih₁ ih₂ =>
    intro _ ξ hξ
    exact .hkeep hℓ (ih₁ hξ) (ih₂ hξ) hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [Tm.armOf_rename]; exact hc' o c a b ho⟩ hnt hle hφ hT
  | hkarm hℓ _ hop hE hk hp hφ₀ hc hnt _ hle hφ hT ih₁ _ ih₃ =>
    intro _ ξ hξ
    subst hk
    rw [Tm.rename, (plug_hole_closed hp).rename]
    exact .hkarm hℓ (ih₁ hξ) hop hE rfl hp hφ₀ ⟨hc.1, fun h => hc.2 fun hc' => h fun o c a b ho => by
      rw [Tm.armOf_rename]; exact hc' o c a b ho⟩ hnt (ih₃ hξ) hle hφ hT
  | hres hℓ _ hA ha hp hl hφ hT ih₁ _ =>
    intro _ ξ hξ
    subst ha
    rw [Tm.rename, (plug_hole_closed hp).rename]
    exact .hres hℓ (ih₁ hξ) hA rfl hp hl hφ hT
  | hdone _ ih => intro _ _ hξ; exact .hdone (ih hξ)
  | unw hℓ _ hφ ih => intro _ _ hξ; exact .unw hℓ (ih hξ) hφ
  | kont hl hC hT => intro _ _ _; exact .kont hl hC hT
  | glob hg hts hH hT => intro _ _ _; exact .glob hg hts hH hT

end Flick
