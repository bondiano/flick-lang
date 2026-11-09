import Formal.Lemma3
import Formal.Decomp

/-!
# Refilling a parked body

A handler parks a body where it performs an operation, and an arm where it calls
`resume`: `f x` at a hole of a context. The operation's result, a signal to the
handler or `todo` fills it.
-/

namespace Flick

variable {S : Sig} {mode : TypingMode}

theorem Stage.none_le {φ : Stage} (h : φ ≠ .pure) : (Stage.rt Row.none Row.none).le φ := by
  cases φ with
  | pure => exact (h rfl).elim
  | rt => exact ⟨fun _ h => h.elim, fun _ h => h.elim⟩

theorem Stage.rt_ne_pure {r ε : Row} {φ : Stage} (h : (Stage.rt r ε).le φ) : φ ≠ .pure :=
  fun h' => by subst h'; exact h

theorem Ext.entails {Θ Θ' : Facts} (h : Ext Θ Θ') : Entails Θ Θ' := fun _ _ h' => h'.mono h.1

/-- Typing by use only makes a stage larger, so a use stage is pure only of a pure one. -/
theorem TypingMode.use_eq_pure (mode : TypingMode) (φ : Stage) (C : Caps) :
    mode.use φ C = .pure ↔ φ = .pure := by
  cases φ with
  | pure => simp [TypingMode.use_pure]
  | rt r ε => cases mode <;> simp [TypingMode.use]

theorem TypingMode.use_ne_pure {mode : TypingMode} {φ : Stage} {C : Caps}
    (h : mode.use φ C ≠ .pure) : φ ≠ .pure :=
  fun h' => h (by rw [h']; exact TypingMode.use_pure mode C)

/-- A run-time stage stays run-time under the use stage, which only adds to its services. -/
theorem TypingMode.use_rt {mode : TypingMode} (r ε : Row) (C : Caps) :
    ∃ r' ε', mode.use (.rt r ε) C = .rt r' ε' := by
  cases mode with
  | captures => exact ⟨r, ε, rfl⟩
  | live => exact ⟨_, ε, rfl⟩

/-- Above a non-pure stage only a non-pure stage lies, whatever the mode adds to it. -/
theorem TypingMode.use_rt_le_ne_pure {mode : TypingMode} {r ε : Row} {C : Caps} {φ : Stage}
    (h : (mode.use (.rt r ε) C).le φ) : φ ≠ .pure := by
  obtain ⟨r', ε', he⟩ := TypingMode.use_rt (mode := mode) r ε C
  exact Stage.rt_ne_pure (he ▸ h)

/-- The function at the hole: of run-time stage, above `ψ₀` where it is used, its
argument below `a₀`, and its result `B`, which holds of every type above it. -/
def HoleFnM (S : Sig) (mode : TypingMode) (Ψ : List Ty) (f : Tm) (a₀ : Ty) (ψ₀ : Stage)
    (B : Ty → Prop) : Prop :=
  ∀ {K Θ' a ψ t C φ'}, TypM S mode (Ψ.map (Ty.up K)) Θ' [] f (.ex (.arrow a ψ t C) φ') →
    ψ ≠ .pure ∧ (K = 0 → mode.subtype a a₀ ∧ ψ₀.le (mode.use ψ C) ∧ B t)

abbrev HoleFn (S : Sig) := HoleFnM S .captures

/-- The hole `f x` of a context: what has every type of `B` at a run-time stage above `ψ₀`
fills it, given `x` there at `a₀`. -/
theorem refill_app_mode {Ψ Θ E f x t₀ φ₀ a₀ w ψ₀ B} (hE : OkE E) (hn : ¬ Value (.app f x))
    (h : TypM S mode Ψ Θ [] (plug E (.app f x)) (.ex t₀ φ₀)) (hf : HoleFnM S mode Ψ f a₀ ψ₀ B)
    (hB : ∀ t T, B t → mode.subtype t T → B T)
    (hw : ∀ {Θ'} T φ', Ext Θ Θ' → ψ₀.le φ' → B T → TypM S mode Ψ Θ' [] x (.ex a₀ φ') →
      TypM S mode Ψ Θ' [] w (.ex T φ')) :
    TypM S mode Ψ Θ [] (plug E w) (.ex t₀ φ₀) := by
  obtain ⟨K, Θ', t', φ', hr, -, hp, back⟩ := ctx_typing_mode E hE hn h
  cases hr with
  | app hf' hx hψ hT =>
    obtain ⟨hψp, hb⟩ := hf hf'
    have hφ : φ' ≠ .pure := fun h' =>
      hψp ((TypingMode.use_eq_pure mode _ _).mp (Stage.eq_pure (h' ▸ hψ)))
    rcases hp with hp | ⟨rfl, hΘ⟩
    · exact (hφ hp).elim
    · obtain ⟨ha, hψ₀, hbt⟩ := hb rfl
      simp only [List.map_up_zero] at hx
      simpa using RefillM.id back [] w (by
        simpa using hw _ _ hΘ (Stage.le_trans hψ₀ hψ) (hB _ _ hbt hT)
          (hx.sub ⟨ha, Stage.le_refl _⟩))
  | errVal hv | allI hv => exact (hn hv).elim

/-- An operation of a declared effect at the hole. -/
theorem perform_fn {mode : TypingMode} {Ψ E o c a₀ b₀} (hop : S.op E o = some (c, a₀, b₀)) :
    HoleFnM S mode Ψ (.const (.perform E o c)) a₀ (.rt (Row.single E) Row.none)
      (fun t => c ≠ .abort → mode.subtype b₀ t) := by
  intro K Θ' a ψ t C φ' h
  cases h with
  | perform hop' hT =>
    rw [hop] at hop'; cases hop'
    obtain ⟨hdom, hψ, ht⟩ := TypingMode.arrow_inv hT
    refine ⟨TypingMode.use_ne_pure (TypingMode.use_rt_le_ne_pure hψ),
      fun _ => ⟨hdom, Stage.le_trans (TypingMode.le_use mode _ _) hψ,
        fun hc => by simpa [hc] using ht⟩⟩
  | const hok _ => exact hok.elim

theorem perform_up {mode : TypingMode} {c : OpClass} {b₀ : Ty} :
    ∀ t T, (c ≠ .abort → mode.subtype b₀ t) → mode.subtype t T →
    c ≠ .abort → mode.subtype b₀ T := fun _ _ h l hc => (h hc).trans l

/-- `resume` at the hole: its cell gives its type. -/
theorem kont_fn {mode : TypingMode} {Ψ : List Ty} {p ℓ b₀ r ε t' C'}
    (hl : Ψ[ℓ]? = some (.arrow b₀ (.rt r ε) t' C')) :
    HoleFnM S mode Ψ (.kont p ℓ) b₀ (.rt r ε) (fun t => mode.subtype t' t) := by
  intro K Θ' a ψ t C φ' h
  cases h with
  | kont hl' _ hT =>
    obtain ⟨hdom, hψ, ht⟩ := TypingMode.arrow_inv hT
    rw [List.getElem?_map, hl] at hl'
    simp only [Option.map_some, Option.some.injEq, Ty.up_def, Ty.substT, Ty.arrow.injEq] at hl'
    obtain ⟨rfl, rfl, rfl, rfl⟩ := hl'
    refine ⟨fun h => ?_, fun hK => ?_⟩
    · simp only [Stage.substT] at hψ
      exact TypingMode.use_ne_pure (TypingMode.use_rt_le_ne_pure hψ) h
    · subst hK
      have h0 : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
      simp only [h0, Ty.substT_ident, Stage.substT_ident] at hdom hψ ht
      exact ⟨hdom, Stage.le_trans (TypingMode.le_use mode _ _) hψ, ht⟩

/-- `lam todo` has every function type: the type of a cell before its use. -/
theorem lam_todo {mode : TypingMode} {Ψ Θ a ψ t C φ} :
    TypM S mode Ψ Θ [] (.lam .todo) (.ex (.arrow a ψ t C) φ) := by
  cases mode with
  | captures =>
    exact .lam .todo (by simp [Tm.NoIns, Tm.Has])
      (fun _ h => by simp [held, Tm.Free, Tm.Atoms, Tm.Has] at h) .refl
  | live => exact .lam .todo (by simp [Tm.NoIns, Tm.Has]) trivial .refl

/-- The arm of the operation `o` in the arms of a handler. A control arm calls `resume`,
so it is typed at the use stage of the handler's region. -/
theorem TypM.arm_inv {Ψ Θ E t φ s o c a₀ b₀} (hop : S.op E o = some (c, a₀, b₀)) :
    ∀ {al : Tm} {Γ b}, TypM S mode Ψ Θ Γ al (.harms E t φ s) → al.armOf o = some b →
      (c = .tail → TypM S mode Ψ Θ (.mono a₀ :: Γ) b (.ex b₀ φ)) ∧
      (c = .abort → TypM S mode Ψ Θ (.mono a₀ :: Γ) b (.ex t φ)) ∧
      (c = .control →
        TypM S mode Ψ Θ (.mono a₀ :: .mono (.arrow b₀ φ t s.reach) :: Γ) b
          (.ex t (mode.use φ s.reach)))
  | .acons _ _ _, _, _, h, ha => by
    simp only [Tm.armOf] at ha
    cases h with
    | hatail hop' he hr | haabort hop' he hr | hactl hop' he hr =>
      split at ha
      · rename_i hn
        cases ha
        simp only [Pat.spine, List.foldl, Pat.headName, Option.some.injEq] at hn
        subst hn
        rw [hop] at hop'; cases hop'
        refine ⟨fun h => ?_, fun h => ?_, fun h => ?_⟩ <;> first | exact he | cases h
      · exact TypM.arm_inv hop hr ha
  | .anil, _, _, _, ha => by simp [Tm.armOf] at ha
  | .var _, _, _, h, _ | .lam _, _, _, h, _ | .app _ _, _, _, h, _ | .let_ _ _, _, _, h, _
  | .letrec _ _, _, _, h, _ | .fix _, _, _, h, _ | .lit _, _, _, h, _ | .loc _, _, _, h, _
  | .const _, _, _, h, _ | .ctor _, _, _, h, _ | .rnil, _, _, h, _ | .rcons _ _ _, _, _, h, _
  | .proj _ _, _, _, h, _ | .update _ _ _, _, _, h, _ | .match_ _ _, _, _, h, _
  | .aguard _ _ _ _, _, _, h, _ | .todo, _, _, h, _ | .task _, _, _, h, _ | .await _, _, _, h, _
  | .raise _, _, _, h, _ | .try_ _ _, _, _, h, _ | .handle _ _, _, _, h, _
  | .scope _, _, _, h, _ | .inside _ _, _, _, h, _ | .cap _, _, _, h, _
  | .own _, _, _, h, _ | .drec _ _, _, _, h, _ | .glob _, _, _, h, _ | .hdl _ _ _ _, _, _, h, _
  | .hwith _ _ _ _ _ _, _, _, h, _ | .hctl _ _ _ _ _ _ _ _ _, _, _, h, _ | .hres _ _ _ _, _, _, h, _
  | .hdone _ _, _, _, h, _ | .unw _ _ _, _, _, h, _ | .kont _ _, _, _, h, _ | .hole _, _, _, h, _ => by
    cases h

theorem refill_app {Ψ Θ E f x t₀ φ₀ a₀ w ψ₀ B} (hE : OkE E) (hn : ¬ Value (.app f x))
    (h : Typ S Ψ Θ [] (plug E (.app f x)) (.ex t₀ φ₀)) (hf : HoleFn S Ψ f a₀ ψ₀ B)
    (hB : ∀ t T, B t → Le t T → B T)
    (hw : ∀ {Θ'} T φ', Ext Θ Θ' → ψ₀.le φ' → B T → Typ S Ψ Θ' [] x (.ex a₀ φ') →
      Typ S Ψ Θ' [] w (.ex T φ')) :
    Typ S Ψ Θ [] (plug E w) (.ex t₀ φ₀) :=
  refill_app_mode hE hn h hf hB hw

theorem lam_todo_captures {Ψ Θ a ψ t C φ} :
    Typ S Ψ Θ [] (.lam .todo) (.ex (.arrow a ψ t C) φ) := lam_todo (mode := .captures)

theorem Typ.arm_inv {Ψ Θ E t φ s o c a₀ b₀} (hop : S.op E o = some (c, a₀, b₀)) :
    ∀ {al : Tm} {Γ b}, Typ S Ψ Θ Γ al (.harms E t φ s) → al.armOf o = some b →
      (c = .tail → Typ S Ψ Θ (.mono a₀ :: Γ) b (.ex b₀ φ)) ∧
      (c = .abort → Typ S Ψ Θ (.mono a₀ :: Γ) b (.ex t φ)) ∧
      (c = .control → Typ S Ψ Θ (.mono a₀ :: .mono (.arrow b₀ φ t s.reach) :: Γ) b (.ex t φ)) := by
  intro al Γ b h ha
  have := TypM.arm_inv (mode := .captures) hop h ha
  simpa only [TypingMode.use] using this

end Flick
