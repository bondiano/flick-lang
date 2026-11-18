import Formal.Lemma3
import Formal.OpEquiv
import Formal.Decomp
import Formal.FunctionTyping

/-!
# Refilling a parked body

A handler parks a body where it performs an operation, and an arm where it calls
`resume`: `f x` at a hole of a context. The operation's result, a signal to the
handler or `todo` fills it.
-/

namespace Flick

variable {S : Sig}

theorem Stage.none_le {φ : Stage} (h : φ ≠ .pure) : (Stage.rt Row.none Row.none).le φ := by
  cases φ with
  | pure => exact (h rfl).elim
  | rt => exact ⟨fun _ h => h.elim, fun _ h => h.elim⟩

theorem Stage.rt_ne_pure {r ε : Row} {φ : Stage} (h : (Stage.rt r ε).le φ) : φ ≠ .pure :=
  fun h' => by subst h'; exact h

theorem Ext.entails {Θ Θ' : Facts} (h : Ext Θ Θ') : Entails Θ Θ' := fun _ _ h' => h'.mono h.1

/-- Typing by use only makes a stage larger, so a use stage is pure only of a pure one. -/
theorem Stage.use_eq_pure (φ : Stage) (C : Caps) :
    Stage.use φ C = .pure ↔ φ = .pure := by
  cases φ with
  | pure => simp [Stage.use_pure]
  | rt r ε => simp [Stage.use]

theorem Stage.use_ne_pure {φ : Stage} {C : Caps}
    (h : Stage.use φ C ≠ .pure) : φ ≠ .pure :=
  fun h' => h (by rw [h']; exact Stage.use_pure C)

/-- A run-time stage stays run-time under the use stage, which only adds to its services. -/
theorem Stage.use_rt (r ε : Row) (C : Caps) :
    ∃ r' ε', Stage.use (.rt r ε) C = .rt r' ε' := by
  exact ⟨_, ε, rfl⟩

/-- Above a non-pure stage only a non-pure stage lies, whatever the adds to it. -/
theorem Stage.use_rt_le_ne_pure {r ε : Row} {C : Caps} {φ : Stage}
    (h : (Stage.use (.rt r ε) C).le φ) : φ ≠ .pure := by
  obtain ⟨r', ε', he⟩ := Stage.use_rt r ε C
  exact Stage.rt_ne_pure (he ▸ h)

/-- The function at the hole, used at a stage of which `P` holds: of run-time stage, above `ψ₀`
where it is used, its argument below `a₀`, and its result `B`, which holds of every type above
it. -/
def HoleFn (S : Sig) (Ψ : List Ty) (f : Tm) (a₀ : Ty) (ψ₀ : Stage)
    (B : Ty → Prop) (P : Stage → Prop) : Prop :=
  ∀ {K Θ' a ψ t C φ'}, Typ S (Ψ.map (Ty.up K)) Θ' [] f (.ex (.arrow a ψ t C) φ') →
    P (Stage.use ψ.asStage C) →
      ψ.asStage ≠ .pure ∧ (K = 0 → LeLive S a a₀ ∧ ψ₀.le (Stage.use ψ.asStage C) ∧ B t)

/-- The hole `f x` of a context: what has every type of `B` at a run-time stage above `ψ₀`
fills it, given `x` there at `a₀`; the hole is used at a stage of which `P` holds when every
stage below one the context passes to its hole does (`EInst`). -/
theorem refill_app {Ψ Θ E f x t₀ φ₀ a₀ w ψ₀ B P} (hE : OkE E) (hn : ¬ Value (.app f x))
    (h : Typ S Ψ Θ [] (plug E (.app f x)) (.ex t₀ φ₀)) (hf : HoleFn S Ψ f a₀ ψ₀ B P)
    (hB : ∀ t T, B t → LeLive S t T → B T)
    (hP : ∀ φ', EInst S E φ' φ₀ → ∀ ψ', ψ'.le φ' → P ψ')
    (hw : ∀ {Θ'} T φ', Ext Θ Θ' → ψ₀.le φ' → B T → Typ S Ψ Θ' [] x (.ex a₀ φ') →
      Typ S Ψ Θ' [] w (.ex T φ')) :
    Typ S Ψ Θ [] (plug E w) (.ex t₀ φ₀) := by
  obtain ⟨K, Θ', t', φ', hr, -, hp, back, hI⟩ := ctx_typing E hE hn h
  obtain ⟨_, _, _, _, hf', hx, hψ, hT⟩ := hr.app_nonvalue f x t' φ' rfl rfl hn
  obtain ⟨hψp, hb⟩ := hf hf' (hP φ' hI _ hψ)
  have hφ : φ' ≠ .pure := fun h' =>
    hψp ((Stage.use_eq_pure _ _).mp (Stage.eq_pure (h' ▸ hψ)))
  rcases hp with hp | ⟨rfl, hΘ⟩
  · exact (hφ hp).elim
  · obtain ⟨ha, hψ₀, hbt⟩ := hb rfl
    simp only [List.map_up_zero] at hx
    simpa using Refill.id back [] w (by
      simpa using hw _ _ hΘ (Stage.le_trans hψ₀ hψ) (hB _ _ hbt hT)
        (hx.sub ⟨ha, Stage.le_refl _⟩))

/-- The arguments of an effect answered by a handler at a stage below one that answers them. -/
theorem Stage.Answered.mono {E : Name} {us : List Ty} {ψ φ : Stage} (h : ψ.le φ)
    (ha : Stage.Answered S E us φ) : Stage.Answered S E us ψ :=
  fun c hc hk => ha c (Stage.le_svc h _ hc) hk

/-- The body of a handler at `us` performs its effect only at arguments the handler answers. -/
theorem Stage.answered_hP {φ₀ : Stage} {Θ : Facts} {q : Nat} {E : Name} {us : List Ty} {r ε : Row}
    (h : φ₀.le (.rt (Row.hP Θ q E (S.answers E us) r) ε)) : Stage.Answered S E us φ₀ :=
  fun c hc _ => by
    rcases Stage.le_svc h _ hc with ⟨-, h⟩ | ⟨hn, -⟩
    · exact h
    · exact (hn rfl).elim

/-- Frames that do not answer the operation `o` of `E` pass to their hole what the handler at
`us` around them answers, and so to every stage below the hole's. -/
theorem answered_hole {Ei : List Frame} {φ₀ : Stage} {E o : Name} {cl : OpClass} {a b : Ty}
    {us : List Ty} (hop : S.op E o = some (cl, a, b)) (hno : ∀ G ∈ Ei, G.handles E o = none)
    (h : Stage.Answered S E us φ₀) :
    ∀ φ', EInst S Ei φ' φ₀ → ∀ ψ', ψ'.le φ' → Stage.Answered S E us ψ' :=
  fun _ hI _ hle => (hI E o cl a b us hop hno h).mono hle

/-- An operation of a declared effect at the hole, used where its effect is performed only at
arguments the handler at `us` answers: its types are those of the handler's arms. -/
theorem perform_fn {Ψ E o c a₀ b₀} {us : List Ty} (hop : S.op E o = some (c, a₀, b₀))
    (hus : us.length = S.arity E) :
    HoleFn S Ψ (.const (.perform E o c)) (a₀.substT (instT us))
      (.rt (Ty.rnm E (Ty.tcon E us)).asRow Row.none)
      (fun t => c ≠ .abort → LeLive S (b₀.substT (instT us)) t) (Stage.Answered S E us) := by
  intro K Θ' a ψ t C φ' h hP
  obtain ⟨vs, _, _, _, hvs, hop', hT⟩ := h.perform_arrow
  rw [hop] at hop'; cases hop'
  obtain ⟨hdom, hψ, ht⟩ := LeLive.arrow_inv hT
  have hle := Stage.le_trans (Stage.le_use _ _) hψ
  have he := Sig.answers_equiv hus hvs (hP _ (Stage.le_svc hle _ (.inl rfl)) ⟨vs, hvs, rfl⟩)
  have hl : vs.length = us.length := hvs.trans hus.symm
  refine ⟨Stage.use_ne_pure (Stage.use_rt_le_ne_pure hψ), fun _ => ⟨?_, ?_, fun hc => ?_⟩⟩
  · exact hdom.trans (.equiv (Ty.Equiv.at_tcon a₀ hl he))
  · rw [show (Ty.rnm E (Ty.tcon E us)).asRow = (Ty.rnm E (Ty.tcon E vs)).asRow from
      (Ty.Equiv.rnm he).asRow.symm]
    exact hle
  · have := ht
    simp only [hc, if_false] at this
    exact (LeLive.equiv (Ty.Equiv.at_tcon b₀ hl he).symm).trans this

theorem perform_up {c : OpClass} {b₀ : Ty} :
    ∀ t T, (c ≠ .abort → LeLive S b₀ t) → LeLive S t T →
    c ≠ .abort → LeLive S b₀ T := fun _ _ h l hc => (h hc).trans l

/-- `resume` at the hole: its cell gives its type. -/
theorem kont_fn {Ψ : List Ty} {p ℓ b₀ r ε t' C'}
    (hl : Ψ[ℓ]? = some (.arrow b₀ (.rt r ε) t' C')) :
    HoleFn S Ψ (.kont p ℓ) b₀ (.rt r.asRow ε.asRow) (fun t => LeLive S t' t) (fun _ => True) := by
  intro K Θ' a ψ t C φ' h _
  obtain ⟨_, _, _, _, hl', _, hT⟩ := h.kont_arrow
  obtain ⟨hdom, hψ, ht⟩ := LeLive.arrow_inv hT
  rw [List.getElem?_map, hl] at hl'
  simp only [Option.map_some, Option.some.injEq, Ty.up_def, Ty.substT, Ty.isStage, if_true,
    Ty.arrow.injEq] at hl'
  obtain ⟨rfl, rfl, rfl, rfl⟩ := hl'
  refine ⟨fun h => ?_, fun hK => ?_⟩
  · exact Stage.use_ne_pure (Stage.use_rt_le_ne_pure hψ) h
  · subst hK
    have h0 : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
    simp only [h0, Ty.substT_ident] at hdom hψ ht
    exact ⟨hdom, Stage.le_trans (Stage.le_use _ _) hψ, ht⟩


/-- `resume` of a region `p` that its cell names: the hole uses that region too. -/
theorem kont_fn_rg {Ψ : List Ty} {p ℓ b₀ r ε t' C'}
    (hl : Ψ[ℓ]? = some (.arrow b₀ (.rt r ε) t' C')) (hcp : C' (.r p)) :
    HoleFn S Ψ (.kont p ℓ) b₀ (Stage.use (.rt r.asRow ε.asRow) (Ty.rg p).reach)
      (fun t => LeLive S t' t) (fun _ => True) := by
  intro K Θ' a ψ t C φ' h _
  have h₁ := kont_fn (S := S) (p := p) hl h trivial
  refine ⟨h₁.1, fun hK => ?_⟩
  obtain ⟨hdom, -, ht⟩ := h₁.2 hK
  subst hK
  obtain ⟨_, _, _, _, hl', _, hT⟩ := h.kont_arrow
  obtain ⟨-, hψ, -⟩ := LeLive.arrow_inv hT
  rw [List.getElem?_map, hl] at hl'
  simp only [Option.map_some, Option.some.injEq, Ty.up_def, Ty.substT, Ty.isStage, if_true,
    Ty.arrow.injEq] at hl'
  obtain ⟨rfl, rfl, rfl, rfl⟩ := hl'
  have h0 : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
  simp only [h0, Caps.substT_ident, Ty.substT_ident] at hψ
  exact ⟨hdom, Stage.le_trans (Stage.use_mono (Stage.le_refl _)
    fun x hx => by cases hx; exact hcp) hψ, ht⟩


/-- `lam todo` has every function type: the type of a cell before its use. -/
theorem lam_todo {Ψ Θ a ψ t C φ} :
    Typ S Ψ Θ [] (.lam .todo) (.ex (.arrow a ψ t C) φ) := by
  exact .lam .todo (by simp [Tm.NoIns, Tm.Has]) .refl

/-- The arm of the operation `o` in the arms of a handler. A control arm calls `resume`,
so it is typed at the use stage of the handler's region. -/
theorem Typ.arm_inv {Ψ Θ E us t φ s o c a₀ b₀} (hop : S.op E o = some (c, a₀, b₀)) :
    ∀ {al : Tm} {Γ b}, Typ S Ψ Θ Γ al (.harms E us t φ s) → al.armOf o = some b →
      (c = .tail → Typ S Ψ Θ (.mono (a₀.substT (instT us)) :: Γ) b
        (.ex (b₀.substT (instT us)) φ.asStage)) ∧
      (c = .abort → Typ S Ψ Θ (.mono (a₀.substT (instT us)) :: Γ) b (.ex t φ.asStage)) ∧
      (c = .control →
        Typ S Ψ Θ (.mono (a₀.substT (instT us)) :: .mono (.arrow (b₀.substT (instT us)) φ t s.reach) ::
          Γ) b (.ex t (Stage.use φ.asStage s.reach)))
  | .acons _ _ _, _, _, h, ha => by
    simp only [Tm.armOf] at ha
    cases h with
    | hatail _ hop' he hr | haabort _ hop' he hr | hactl _ hop' he hr =>
      split at ha
      · rename_i hn
        cases ha
        simp only [Pat.spine, List.foldl, Pat.headName, Option.some.injEq] at hn
        subst hn
        rw [hop] at hop'; cases hop'
        refine ⟨fun h => ?_, fun h => ?_, fun h => ?_⟩ <;> first | exact he | cases h
      · exact Typ.arm_inv hop hr ha
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

end Flick
