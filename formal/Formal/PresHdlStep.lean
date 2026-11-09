import Formal.Below

/-!
# Preservation of the steps of handlers in a context

An operation runs its arm at the handler, which the body waits for; `resume` runs the
body again under the handler while the arm waits; unwinding passes the frames between
a signal and its handler.
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

theorem List.map_kill_YF {Ψ : List Ty} {n : Nat} (h : ∀ u ∈ Ψ, u.YF n) :
    Ψ.map (Ty.substT (kill n)) = Ψ := by
  conv => rhs; rw [← List.map_id Ψ]
  exact List.map_congr_left fun u hu => h u hu

theorem hwith_not_pure {Ψ Θ p ℓ E al r e t} (h : TypM S mode Ψ Θ [] (.hwith p ℓ E al r e) (.ex t .pure)) :
    False := by
  cases h with
  | hwith _ _ _ _ _ _ _ hφ _ => exact hφ
  | errVal hv | allI hv => cases hv

theorem hkeep_not_pure {Ψ Θ q ℓ E al e t} (h : TypM S mode Ψ Θ [] (.hkeep q ℓ E al e) (.ex t .pure)) :
    False := by
  cases h with
  | hkeep _ _ _ _ _ _ _ hφ _ => exact hφ
  | errVal hv | allI hv => cases hv

theorem hctl_not_pure {Ψ Θ p ℓ κ tl E al r a k t}
    (h : TypM S mode Ψ Θ [] (.hctl p ℓ κ tl E al r a k) (.ex t .pure)) : False := by
  cases h with
  | hctl _ _ _ _ _ _ _ _ _ _ _ _ _ _ hφ _ => exact hφ
  | errVal hv | allI hv => cases hv

theorem unw_not_value {p ℓ a} : ¬ Value (.unw p ℓ a) := fun h => by cases h

theorem unw_not_pure {Ψ Θ p ℓ a t} (h : TypM S mode Ψ Θ [] (.unw p ℓ a) (.ex t .pure)) : False := by
  cases h with
  | unw _ _ h => exact h
  | errVal hv | allI hv => cases hv

/-- Removing a frame around unwinding keeps it typed. -/
theorem unw_frame {Ψ Θ p ℓ a t φ} {F : Frame} (hF : F.ok) (hni : ¬ F.isInside)
    (h : TypM S mode Ψ Θ [] (F.plug (.unw p ℓ a)) (.ex t φ)) : TypM S mode Ψ Θ [] (.unw p ℓ a) (.ex t φ) := by
  obtain ⟨k, Θ', _, φ', hr, rel, hk, _⟩ := frame_typing_mode unw_not_value F hF h
  cases hr with
  | unw hℓ ha hst =>
    have hne : φ' ≠ .pure := Stage.rt_ne_pure hst
    obtain ⟨rfl, -, hΘ⟩ := hk.resolve_left hne
    obtain rfl := hΘ hni
    simp only [List.map_up_zero] at hℓ ha
    exact .unw hℓ ha (Stage.none_le fun hp => hne (rel.pure hp))
  | errVal hv | allI hv => cases hv

/-- Facts of a hole, renamed with the regions not yet created, follow from those around. -/
theorem kill_entails {Θ Θ' : Facts} {n : Nat} (hidem : Θ.substT (kill n) = Θ)
    (hin : Entails (Facts.age.substT (kill n)) Θ) (hΘ : Ext Θ Θ') : Entails (Θ'.substT (kill n)) Θ := by
  intro C s hC x hx
  rcases hC x hx with h | ⟨C', ⟨C₀, s₀, h₀, rfl, rfl⟩, h₂⟩
  · exact .inl h
  · rcases hΘ.2 _ _ h₀ with h | h
    · exact .inr ⟨_, by rw [← hidem]; exact ⟨C₀, s₀, h, rfl, rfl⟩, h₂⟩
    · exact hin _ _ (fun y hy => .inr ⟨_, ⟨C₀, s₀, h, rfl, rfl⟩, hy⟩) x h₂

/-- The argument of the hole, at the facts around, when its type has no region. -/
theorem hole_arg {Ψ Θ E f x t₀ φ₀ a₀ ψ₀ B n} (hE : OkE E) (hn : ¬ Value (.app f x)) (hx : Value x)
    (h : TypM S mode Ψ Θ [] (plug E (.app f x)) (.ex t₀ φ₀)) (hf : HoleFnM S mode Ψ f a₀ ψ₀ B)
    (hc : RtCtx Ψ Θ n) (hb : x.Below n) (ha : a₀.substT (kill n) = a₀) :
    TypM S mode Ψ Θ [] x (.ex a₀ .pure) := by
  obtain ⟨K, Θ', t', φ', hr, -, hp, -⟩ := ctx_typing_mode E hE hn h
  cases hr with
  | app hf' hx' hψ _ =>
    obtain ⟨hψp, hb'⟩ := hf hf'
    have hφ : φ' ≠ .pure := fun h =>
      hψp ((TypingMode.use_eq_pure mode _ _).mp (Stage.eq_pure (h ▸ hψ)))
    rcases hp with hp | ⟨rfl, hΘ⟩
    · exact (hφ hp).elim
    · simp only [List.map_up_zero] at hx'
      have h₁ := (hx'.sub (J' := .ex a₀ φ') ⟨(hb' rfl).1, Stage.le_refl _⟩).substT (kill n) (Fits.kill hb)
      rw [List.map_kill_YF hc.yf] at h₁
      simp only [Flick.J.substT, ha] at h₁
      exact (h₁.cut (kill_entails hc.idem hc.inΘ hΘ)).valueAny hx _
  | errVal hv | allI hv => exact (hn hv).elim

theorem perform_op {Ψ Θ E o c x t φ} {Ei : List Frame} (hEi : OkE Ei)
    (h : TypM S mode Ψ Θ [] (plug Ei (.app (.const (.perform E o c)) x)) (.ex t φ)) :
    ∃ a b, S.op E o = some (c, a, b) := by
  obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing_mode Ei hEi perform_not_value h
  cases hr with
  | app hf _ _ _ =>
    cases hf with
    | perform hop _ => exact ⟨_, _, hop⟩
    | const hok _ => exact hok.elim
  | errVal hv | allI hv => exact (perform_not_value hv).elim

/-- `todo` for the argument of the hole. -/
theorem refill_todo {Ψ Θ E f x t₀ φ₀} (hE : OkE E) (hn : ¬ Value (.app f x))
    (h : TypM S mode Ψ Θ [] (plug E (.app f x)) (.ex t₀ φ₀)) : TypM S mode Ψ Θ [] (plug E (.app f .todo)) (.ex t₀ φ₀) := by
  obtain ⟨K, Θ', t', φ', hr, -, -, back⟩ := ctx_typing_mode E hE hn h
  cases hr with
  | app hf' _ hψ hT => simpa using RefillM.id back [] (.app f .todo) (by simpa using TypM.app hf' .todo hψ hT)
  | errVal hv | allI hv => exact (hn hv).elim

/-- The cell of `resume` has no region not yet created. -/
theorem ctl_cell_yf {Ψ : List Ty} {Θ n} {ℓ p : Nat} {r ε : Row} {t' b₀ : Ty} (hc : RtCtx Ψ Θ n)
    (hℓ : Ψ[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t' Caps.none)) (hb₀ : b₀.substT (kill n) = b₀)
    (hp : p < n) : (Ty.arrow b₀ (.rt r ε) t' (Ty.rg p).reach).YF n := by
  have hcy := hc.yf _ (List.mem_of_getElem? hℓ)
  simp only [Ty.YF, Ty.substT, Stage.substT, Ty.arrow.injEq, Stage.rt.injEq] at hcy
  obtain ⟨-, ⟨hr, hε⟩, ht', -⟩ := hcy
  simp only [Ty.YF, Ty.substT, Stage.substT, hr, hε, ht', hb₀]
  rw [Caps.kill_fixed fun q hq => by cases hq; exact hp]

/-- `abort`: the arm goes with the signal that unwinds the body. -/
theorem abort_pres {Ψ Θ p ℓ Ef al rf o v b t φ} {Ei : List Frame} (hEi : OkE Ei)
    (ha : al.armOf o = some b) (hv : Value v)
    (h : TypM S mode Ψ Θ [] (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .abort)) v))) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.hwith p ℓ Ef al rf (plug Ei (.unw p ℓ (b.subst (inst [v]))))) (.ex t φ) := by
  cases h with
  | hwith hℓ hal hrf hψ he hφ₀ hc hφ hT hat =>
    obtain ⟨a₀, b₀, hop⟩ := perform_op hEi he
    have hb := (hal.arm_inv hop ha).2.1 rfl
    refine .hwith hℓ hal hrf hψ (refill_app_mode hEi perform_not_value he (perform_fn hop) perform_up
      fun T φ' hΘ hψ₀ _ hx => ?_) hφ₀ hc hφ hT hat
    exact .unw hℓ (((hb.cut hΘ.entails).inst (Δ := [_]) (.cons hv (hx.valueAny hv _) .nil)).sub
      (J' := .ex _ (mode.use (.rt _ _) (Ty.rg _).reach))
      ⟨TypingMode.subtype.refl, TypingMode.le_use mode _ _⟩)
      (Stage.none_le (Stage.rt_ne_pure hψ₀))
  | errVal hv | allI hv => cases hv

/-- `tail`: the arm runs at the handler while the body waits for its value. -/
theorem tail_pres {Ψ Θ n p ℓ Ef al rf o v b t φ} {Ei : List Frame} (hc : RtCtx Ψ Θ n)
    (hbv : v.Below n) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (h : TypM S mode Ψ Θ [] (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .tail)) v))) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.hctl p ℓ 0 true Ef al rf (b.subst (inst [v])) (plug Ei (.hole p))) (.ex t φ) := by
  cases h with
  | hwith hℓ hal hrf hψ he hφ₀ hc' hφ hT hat =>
    obtain ⟨a₀, b₀, hop⟩ := perform_op hEi he
    have hb := (hal.arm_inv hop ha).1 rfl
    have hx := hole_arg hEi perform_not_value hv he (perform_fn hop) hc hbv
      (S.op_closed _ _ _ _ _ (kill n) hop).1
    refine .hctl hℓ hal hrf hψ hop (by decide) (by simp) hEi rfl
      (refill_todo hEi perform_not_value he) hφ₀ ?_ (fun h => by cases h) hc' hφ hT hat
    rw [if_pos rfl]
    exact (hb.inst (Δ := [_]) (.cons hv hx .nil)).sub
      ⟨TypingMode.subtype.refl, TypingMode.le_use mode _ _⟩
  | errVal hv | allI hv => cases hv

/-- `tail` under a kept handler: its arm runs there while the body waits for its value. -/
theorem kept_pres {Ψ Θ n q ℓ Ef al o v b t φ} {Ei : List Frame} (hc : RtCtx Ψ Θ n)
    (hbv : v.Below n) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (h : TypM S mode Ψ Θ [] (.hkeep q ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.hkarm q ℓ Ef al (b.subst (inst [v])) (plug Ei (.hole q))) (.ex t φ) := by
  cases h with
  | hkeep hℓ hal he hφ₀ hc' hnt hle hφ hT =>
    obtain ⟨a₀, b₀, hop⟩ := perform_op hEi he
    have hb := (hal.arm_inv hop ha).1 rfl
    have hx := hole_arg hEi perform_not_value hv he (perform_fn hop) hc hbv
      (S.op_closed _ _ _ _ _ (kill n) hop).1
    exact .hkarm hℓ hal hop hEi rfl (refill_todo hEi perform_not_value he) hφ₀ hc' hnt
      (hb.inst (Δ := [_]) (.cons hv hx .nil)) hle hφ hT
  | errVal hv | allI hv => cases hv

/-- `control`: the arm runs at the handler with `resume`, a new cell of the type of the rest
of the body. -/
theorem ctl_pres {Ψ Θ n p ℓ Ef al rf o v b t φ} {Ei : List Frame} (hc : RtCtx Ψ Θ n)
    (hbv : v.Below n) (hp : p < n) (hEi : OkE Ei) (ha : al.armOf o = some b) (hv : Value v)
    (h : TypM S mode Ψ Θ [] (.hwith p ℓ Ef al rf (plug Ei (.app (.const (.perform Ef o .control)) v)))
      (.ex t φ)) :
    ∃ c, c.YF n ∧ TypM S mode Ψ Facts.age [] (.lam .todo) (.ex c .pure) ∧
      TypM S mode (Ψ ++ [c]) Θ [] (.hctl p ℓ Ψ.length false Ef al rf (b.subst (inst [v, .kont p Ψ.length]))
        (plug Ei (.hole p))) (.ex t φ) := by
  cases h with
  | hwith hℓ hal hrf hψ he hφ₀ hc' hφ hT hat =>
    obtain ⟨a₀, b₀, hop⟩ := perform_op hEi he
    have hb := (hal.arm_inv hop ha).2.2 rfl
    have hx := hole_arg hEi perform_not_value hv he (perform_fn hop) hc hbv
      (S.op_closed _ _ _ _ _ (kill n) hop).1
    refine ⟨_, ctl_cell_yf hc hℓ (S.op_closed _ _ _ _ _ (kill n) hop).2 hp, lam_todo, ?_⟩
    refine .hctl (getElem?_ext hℓ) (hal.weakenStore _) (hrf.weakenStore _) hψ hop (by decide) (by simp)
      hEi rfl ((refill_todo hEi perform_not_value he).weakenStore _) hφ₀ ?_
      (fun _ => ⟨_, List.getElem?_concat_length, rfl⟩) hc' hφ hT hat
    rw [if_neg (by decide)]
    exact (hb.weakenStore _).inst (Δ := [_, _])
      (.cons hv (hx.weakenStore _) (.cons .kont (.kont List.getElem?_concat_length rfl .refl) .nil))
  | errVal hv | allI hv => cases hv

theorem TypingMode.use_rg_svc (mode : TypingMode) {r ε : Row} {p : Nat} :
    mode = .live → (mode.use (.rt r ε) (Ty.rg p).reach).svc (.atm (.r p)) := by
  rintro rfl; exact Or.inr rfl

/-- `resume`: the body runs again under its handler, while the arm waits for its value. -/
theorem resume_pres {Ψ Θ n p ℓh ℓ Ef al rf u k t φ} {A Ei : List Frame} (hc : RtCtx Ψ Θ n)
    (hbu : u.Below n) (hA : OkE A) (hEi : OkE Ei) (hk : k = plug Ei (.hole p)) (hu : Value u)
    (h : TypM S mode Ψ Θ [] (.hctl p ℓh ℓ false Ef al rf (plug A (.app (.kont p ℓ) u)) k) (.ex t φ)) :
    TypM S mode Ψ Θ [] (.hres p ℓh (plug A (.hole p)) (.hwith p ℓh Ef al rf (plug Ei u))) (.ex t φ) := by
  cases h with
  | hctl hℓh hal hrf hψ hop hc' _ hE hk' hp hφ₀ ha hl hcm hφ hT _ =>
    obtain ⟨rfl, -⟩ := plug_hole_inj hEi hE (hk.symm.trans hk')
    obtain ⟨C', hl₁, hcp⟩ := hl rfl
    rw [if_neg (by decide)] at ha
    have hx := hole_arg hA kont_not_value hu ha (kont_fn hl₁) hc hbu (S.op_closed _ _ _ _ _ (kill n) hop).2
    have hw := TypM.hwith hℓh hal hrf hψ (refill_app_mode hE perform_not_value hp (perform_fn hop) perform_up
        fun T φ' hΘ _ hB _ => ((hx.valueAny hu φ').sub (J' := .ex T φ') ⟨hB hc', Stage.le_refl _⟩).cut
          hΘ.entails) hφ₀ hcm (TypingMode.le_use mode _ _) TypingMode.subtype.refl
        (TypingMode.use_rg_svc mode)
    exact .hres hℓh hw hA rfl (refill_todo hA kont_not_value ha) hl₁ hcp hφ hT
  | errVal hv | allI hv => cases hv

end Flick
