import Formal.Preservation

/-!
# A handler of a record of functions, and a known layer

`provide E w { e }` is a handler of `E` whose arm for an operation `o` is `x → w.o x`. A `tail`
operation under it runs the field of the record (`T6c_tail`), under a frame kept by a child too
(`T6c_tail_kept`); its value goes back to the operation.

The handler chain below models selection and typing for the bindings of
`Layer.succeed` and `Layer.merge`: the first binding of an effect is selected
(`chain`, `layer_typ`, `layer_answers`). It does not model the evidence used by
the selected record's functions. Runtime layer services run with the caller's
evidence; explicit `provide E w` handlers run with their handler's evidence.
-/

namespace Flick

variable {S : Sig}

/-- The arms of a handler of the record `w`, for the operations `os`. -/
def recArms (w : Tm) : List Name → Tm
  | [] => .anil
  | o :: os => .acons (Pat.spine o [.var]) (.app (.proj w o) (.var 0)) (recArms w os)

theorem recArms_armOf {w : Tm} {o : Name} :
    ∀ {os : List Name}, o ∈ os → (recArms w os).armOf o = some (.app (.proj w o) (.var 0))
  | o' :: os, h => by
    by_cases he : o' = o
    · subst he; simp [recArms, Tm.armOf, Pat.spine, Pat.headName]
    · simp only [recArms, Tm.armOf, Pat.spine, List.foldl, Pat.headName, Option.some.injEq, he, if_false]
      exact recArms_armOf ((List.mem_cons.mp h).resolve_left (Ne.symm he))

/-- The arm of a record at the argument of its operation: the record is closed. -/
theorem rec_arm_subst {w v : Tm} {o : Name} (hc : w.Closed) :
    (Tm.app (.proj w o) (.var 0)).subst (inst [v]) = .app (.proj w o) v := by
  simp [Tm.subst, hc.subst, inst]

/-- A `tail` operation under a handler of a record of functions runs the field
of the record, with the rest of the body waiting; the value of that goes back to the operation. -/
theorem T6c_tail {root Λ μ E Ei p ℓ Ef r o os w f v u} (hE : OkE E) (hEi : OkE Ei)
    (hh : ∀ G ∈ Ei, G.handles Ef o .tail = none) (ho : o ∈ os) (hw : Value w) (hc : w.Closed)
    (hf : w.field o = some f) (hv : Value v) (hu : Value u) :
    let al := recArms w os
    Steps root (fun _ => True) Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei (.app (.const (.perform Ef o .tail)) v))))
      Λ μ (plug E (.hctl p ℓ 0 true Ef al r (.app f v) (plug Ei (.hole p)))) ∧
    Step root .pure Λ μ (plug E (.hctl p ℓ 0 true Ef al r u (plug Ei (.hole p))))
      (.cfg Λ μ (plug E (.hwith p ℓ Ef al r (plug Ei u)))) := by
  intro al
  refine ⟨.step (.performTail hE hEi hh (recArms_armOf ho) hv) trivial ?_,
    .pure hE (.hctlTail hEi rfl hu)⟩
  rw [rec_arm_subst hc]
  have hF : OkE (Frame.appL v :: Frame.hctlA p ℓ 0 true Ef al r (plug Ei (.hole p)) :: E) := by
    intro G hG
    simp only [List.mem_cons] at hG
    rcases hG with rfl | rfl | hG
    · trivial
    · trivial
    · exact hE G hG
  have hs := Step.pure (root := root) (Λ := Λ) (μ := μ) hF (PHead.proj hw hf)
  exact .step hs trivial .refl

/-- The same under a handler a child keeps, while its region is live. -/
theorem T6c_tail_kept {root Λ μ E Ei q ℓ Ef o os w f v u} (hE : OkE E) (hEi : OkE Ei)
    (hh : ∀ G ∈ Ei, G.handles Ef o .tail = none) (ho : o ∈ os) (hw : Value w) (hc : w.Closed)
    (hf : w.field o = some f) (hv : Value v) (hu : Value u) (hl : Live Λ q) :
    let al := recArms w os
    Steps root (fun _ => True) Λ μ (plug E (.hkeep q ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))))
      Λ μ (plug E (.hkarm q ℓ Ef al (.app f v) (plug Ei (.hole q)))) ∧
    Step root .pure Λ μ (plug E (.hkarm q ℓ Ef al u (plug Ei (.hole q))))
      (.cfg Λ μ (plug E (.hkeep q ℓ Ef al (plug Ei u)))) := by
  intro al
  refine ⟨.step (.performKept hE hEi hh (recArms_armOf ho) hv hl) trivial ?_,
    .pure hE (.hkarmVal hEi rfl hu)⟩
  rw [rec_arm_subst hc]
  have hF : OkE (Frame.appL v :: Frame.hkarmA q ℓ Ef al (plug Ei (.hole q)) :: E) := by
    intro G hG
    simp only [List.mem_cons] at hG
    rcases hG with rfl | rfl | hG
    · trivial
    · trivial
    · exact hE G hG
  have hs := Step.pure (root := root) (Λ := Λ) (μ := μ) hF (PHead.proj hw hf)
  exact .step hs trivial .refl

/-- A binding of a known layer: an effect, its operations and the record of functions. -/
abbrev Binding := Name × List Name × Tm

/-- A layer whose expansion is known: `Layer.succeed` and `Layer.merge`. -/
inductive KnownLayer
  | succeed (E : Name) (os : List Name) (w : Tm)
  | merge (l r : KnownLayer)

/-- The bindings of a layer: those of `merge l r` are those of `l`, then those of `r`. -/
def KnownLayer.binds : KnownLayer → List Binding
  | .succeed E os w => [(E, os, w)]
  | .merge l r => l.binds ++ r.binds

/-- The handler of a binding around a body: it gives the value of the body back. -/
def Binding.hdl (b : Binding) (e : Tm) : Tm := .hdl b.1 (recArms b.2.2 b.2.1) (.lam (.var 0)) e

/-- The chain of handlers of a list of bindings, the first the innermost. -/
def chain : List Binding → Tm → Tm
  | [], e => e
  | b :: bs, e => chain bs (b.hdl e)

/-- The outputs of a list of bindings. -/
def outputs : List Binding → Row
  | [] => Row.none
  | b :: bs => Row.union (Row.single b.1) (outputs bs)

/-- A binding of a root's worth of record: closed, typed, with an arm for each operation of an
effect of `tail` operations. -/
structure Binding.Ok (S : Sig) (b : Binding) : Prop where
  val : Value b.2.2
  closed : b.2.2.Closed
  typ : Typ S [] Facts.none [] b.2.2 (.ex (S.impl b.1) .pure)
  ne : S.impl b.1 ≠ .errors Row.none
  ops : ∀ o c x y, S.op b.1 o = some (c, x, y) → o ∈ b.2.1
  decl : ∃ o c x y, S.op b.1 o = some (c, x, y)
  has : ∀ o ∈ b.2.1, ∃ x y, S.op b.1 o = some (.tail, x, y)

/-- A closed record is typed anywhere. -/
theorem Binding.Ok.typAt {b : Binding} (h : b.Ok S) (Ψ : List Ty) (Θ : Facts) (Γ : List Scheme) (φ : Stage) :
    Typ S Ψ Θ Γ b.2.2 (.ex (S.impl b.1) φ) := by
  have h₁ := (h.typ.cut (Entails.none Θ)).weakenStore Ψ
  have h₂ := h₁.rename (Γ' := Γ) (ξ := id) (fun i _ hi => by simp at hi)
  rw [h.closed.rename] at h₂
  simpa using h₂.sub (J' := .ex _ φ) ⟨.refl, Stage.pure_le _⟩

/-- The arms of a record are typed at any stage. -/
theorem recArms_typ {E : Name} {w : Tm} {Ψ Θ Γ t s} {r ε : Row}
    (hw : ∀ a, Typ S Ψ Θ (.mono a :: Γ) w (.ex (S.impl E) (.rt r ε))) (hne : S.impl E ≠ .errors Row.none) :
    ∀ {os : List Name}, (∀ o ∈ os, ∃ x y, S.op E o = some (.tail, x, y)) →
      Typ S Ψ Θ Γ (recArms w os) (.harms E t (.rt r ε) s)
  | [], _ => .hanil
  | o :: os, h => by
    obtain ⟨x, y, hop⟩ := h o (by simp)
    refine .hatail hop ?_ (recArms_typ hw hne fun o' ho' => h o' (by simp [ho']))
    exact .app (C := Caps.none) (ψ := .rt Row.none Row.none)
      (.proj (hw x) (S.op_field _ _ _ _ hop hne) .refl)
      (.var (ts := []) rfl rfl (fun _ _ h => h.elim) (by rw [Scheme.inst_mono]; exact .refl))
      (by rw [Stage.use_none]; exact ⟨fun _ h => h.elim, fun _ h => h.elim⟩) .refl

theorem recArms_noIns {w : Tm} (hw : w.NoIns) : ∀ {os : List Name}, (recArms w os).NoIns
  | [] => fun h => h
  | _ :: os => by
    rintro (h | h)
    · rcases h with h | h
      · exact hw h
      · exact h
    · exact recArms_noIns hw h

/-- The function `λx. x` of a handler that gives the value of its body back. -/
theorem id_typ {Ψ Θ Γ t φ} : Typ S Ψ Θ Γ (.lam (.var 0)) (.ex (.arrow t .pure t (t.reach)) φ) := by
  exact .lam (.var (ts := []) rfl rfl (fun _ _ h => h.elim) (by rw [Scheme.inst_mono]; exact .refl))
    (fun h => by simp [Tm.Has] at h) .refl

/-- A layer of known bindings is (Provide): its outputs leave the row of the body, unless an
obligation is about them. -/
theorem layer_typ {Ψ Θ Γ t} {r ε : Row} : ∀ (bs : List Binding) {e : Tm} {φ₀ : Stage}, (∀ b ∈ bs, b.Ok S) →
    Typ S Ψ Θ Γ e (.ex t φ₀) → φ₀.le (.rt (Row.union (outputs bs) r) ε) → φ₀.ObOut (outputs bs) →
    Typ S Ψ Θ Γ (chain bs e) (.ex t (.rt r ε))
  | [], _, φ₀, _, he, hle, _ => by
    refine he.sub ⟨.refl, ?_⟩
    cases φ₀ with
    | pure => trivial
    | rt r₀ e₀ =>
      exact ⟨fun x hx => (hle.1 x hx).elim (fun h => h.elim) id, hle.2⟩
  | b :: bs, e, φ₀, hall, he, hle, hO => by
    have hb := hall b (by simp)
    let r₁ : Row := fun x => φ₀.svc x ∧ x ≠ .nm b.1
    have hh : Typ S Ψ Θ Γ (b.hdl e) (.ex t (.rt r₁ ε)) := by
      refine .hdl (ψ := .pure) (C := t.reach) (t' := t) ?_ (recArms_noIns (hb.typ.noIns hb.val)) id_typ trivial he ?_ ?_
        ⟨hb.decl, fun hn => (hn fun o c x y hop => ?_).elim⟩ (Stage.le_refl _) .refl
      · simp only [Stage.substT]
        exact recArms_typ (fun a => hb.typAt _ _ _ _) hb.ne hb.has
      · cases φ₀ with
        | pure => trivial
        | rt r₀ e₀ =>
          refine ⟨fun x hx => ?_, hle.2⟩
          by_cases hx' : x = .nm b.1
          · exact .inl hx'
          · exact .inr ⟨hx, hx'⟩
      · refine ⟨fun N a h => ⟨fun v h' => (by cases h'), fun h' => (hO.1 N a h).2 (.inl h')⟩,
          fun v a h => ⟨fun w h' => (by cases h'), fun N h' => (hO.2 v a h).2 N (.inl h')⟩⟩
      · rw [recArms_armOf (hb.ops o c x y hop)]; rfl
    refine layer_typ bs (fun b' h => hall b' (List.mem_cons_of_mem _ h)) hh ⟨fun x hx => ?_, fun _ h => h⟩ ?_
    · rcases Stage.le_svc hle x hx.1 with (h | h) | h
      · exact (hx.2 h).elim
      · exact .inl h
      · exact .inr h
    · refine ⟨fun N a h => ?_, fun v a h => ?_⟩
      · obtain ⟨h₁, h₂⟩ := hO.1 N a h.1
        exact ⟨fun v h' => h₁ v (.inr h'), fun h' => h₂ (.inr h')⟩
      · obtain ⟨h₁, h₂⟩ := hO.2 v a h.1
        exact ⟨fun v h' => h₁ v (.inr h'), fun N h' => h₂ N (.inr h')⟩

/-- The record that answers an operation: the first binding of its effect with it. -/
def answer : List Binding → Name → Name → Option Tm
  | [], _, _ => none
  | b :: bs, E, o => if b.1 = E ∧ o ∈ b.2.1 then some b.2.2 else answer bs E o

/-- The frames of a running chain, with their regions and cells, the first binding the innermost. -/
def hframes : List Binding → List (Nat × Nat) → List Frame
  | b :: bs, (p, ℓ) :: ps =>
    .hwith p ℓ b.1 (recArms b.2.2 b.2.1) (.lam (.var 0)) :: .inside p :: hframes bs ps
  | _, _ => []

theorem hframes_ok : ∀ {bs : List Binding} {ps : List (Nat × Nat)}, OkE (hframes bs ps)
  | [], _ | _ :: _, [] => OkE.nil
  | _ :: _, (_, _) :: _ => fun G hG => by
    simp only [hframes, List.mem_cons] at hG
    rcases hG with rfl | rfl | hG
    · trivial
    · trivial
    · exact hframes_ok G hG

theorem Steps.snoc {root : Roots} {Q : Lbl → Prop} {Λ Λ₁ Λ₂ : List Rgn} {μ μ₁ μ₂ : List Tm} {e e₁ e₂ : Tm} {l : Lbl}
    (h : Steps root Q Λ μ e Λ₁ μ₁ e₁) (hs : Step root l Λ₁ μ₁ e₁ (.cfg Λ₂ μ₂ e₂)) (hq : Q l) :
    Steps root Q Λ μ e Λ₂ μ₂ e₂ := by
  induction h with
  | refl => exact .step hs hq .refl
  | step s q _ ih => exact .step s q (ih hs)

/-- A chain installs its handlers, the last binding the outermost. -/
theorem chain_installs {root : Roots} {Λ : List Rgn} {μ : List Tm} : ∀ (bs : List Binding) (e : Tm),
    ∃ ps Λ' μ', ps.length = bs.length ∧
      Steps root (fun _ => True) Λ μ (chain bs e) Λ' μ' (plug (hframes bs ps) e)
  | [], _ => ⟨[], Λ, μ, rfl, .refl⟩
  | b :: bs, e => by
    obtain ⟨ps, Λ', μ', hl, h⟩ := chain_installs (root := root) (Λ := Λ) (μ := μ) bs (b.hdl e)
    have hs := Step.rt (root := root) (hframes_ok (bs := bs) (ps := ps))
      (RHead.hdl (Λ := Λ') (μ := μ') (E := b.1) (al := recArms b.2.2 b.2.1) (r := .lam (.var 0)) (e := e))
    exact ⟨(Λ'.length, μ'.length) :: ps, Λ' ++ [Rgn.new], μ' ++ [.lam .todo], by simp [hl],
      Steps.snoc h hs trivial⟩

theorem recArms_mem {w : Tm} {o : Name} : ∀ {os : List Name}, ((recArms w os).armOf o).isSome → o ∈ os
  | [], h => by simp [recArms, Tm.armOf] at h
  | o' :: os, h => by
    by_cases he : o' = o
    · simp [he]
    · simp only [recArms, Tm.armOf, Pat.spine, List.foldl, Pat.headName, Option.some.injEq, he, if_false] at h
      exact List.mem_cons_of_mem _ (recArms_mem h)

/-- The frame that answers an operation in a running chain is that of the first binding of
its effect. -/
theorem hframes_split {E o w} : ∀ {bs : List Binding} {ps : List (Nat × Nat)}, ps.length = bs.length →
    answer bs E o = some w → ∃ Ei Eo p ℓ os, (E, os, w) ∈ bs ∧ o ∈ os ∧
      hframes bs ps = Ei ++ .hwith p ℓ E (recArms w os) (.lam (.var 0)) :: Eo ∧ OkE Ei ∧ OkE Eo ∧
      ∀ G ∈ Ei, G.handles E o .tail = none
  | [], _, _, ha => by simp [answer] at ha
  | _ :: _, [], hl, _ => by simp at hl
  | ⟨E', os', w'⟩ :: bs, (p, ℓ) :: ps, hl, ha => by
    simp only [answer] at ha
    split at ha
    · rename_i hc
      obtain ⟨rfl, ho⟩ := hc
      cases ha
      refine ⟨[], .inside p :: hframes bs ps, p, ℓ, os', by simp, ho, rfl, OkE.nil, ?_, fun _ h => by cases h⟩
      exact fun G hG => (List.mem_cons.mp hG).elim (fun h => h ▸ trivial) (hframes_ok G)
    · rename_i hc
      obtain ⟨Ei, Eo, q, ℓ', os, hm, ho, hs, hE, hE', hn⟩ := hframes_split (by simpa using hl) ha
      refine ⟨.hwith p ℓ E' (recArms w' os') (.lam (.var 0)) :: .inside p :: Ei, Eo, q, ℓ', os,
        List.mem_cons_of_mem _ hm, ho, by simp only [hframes, hs, List.cons_append], ?_, hE', ?_⟩
      · intro G hG
        simp only [List.mem_cons] at hG
        rcases hG with rfl | rfl | hG
        · trivial
        · trivial
        · exact hE G hG
      · intro G hG
        simp only [List.mem_cons] at hG
        rcases hG with rfl | rfl | hG
        · simp only [Frame.handles]
          rw [if_neg]
          rintro ⟨h₁, h₂⟩
          exact hc ⟨h₁, recArms_mem h₂⟩
        · rfl
        · exact hn G hG

/-- Selection for known layer bindings: an operation in the abstract handler
chain runs the field of the first binding of its effect. This does not establish
which evidence is visible inside that field. -/
theorem layer_answers {root Λ μ E o v w f} {bs : List Binding} {ps : List (Nat × Nat)}
    (hl : ps.length = bs.length) (hall : ∀ b ∈ bs, b.Ok S) (ha : answer bs E o = some w)
    (hf : w.field o = some f) (hv : Value v) :
    ∃ Ei Eo p ℓ os, Steps root (fun _ => True) Λ μ
        (plug (hframes bs ps) (.app (.const (.perform E o .tail)) v)) Λ μ
        (plug Eo (.hctl p ℓ 0 true E (recArms w os) (.lam (.var 0)) (.app f v) (plug Ei (.hole p)))) := by
  obtain ⟨Ei, Eo, p, ℓ, os, hm, ho, hs, hE, hE', hn⟩ := hframes_split hl ha
  have hb := hall _ hm
  refine ⟨Ei, Eo, p, ℓ, os, ?_⟩
  rw [hs, plug_append]
  exact (T6c_tail (u := .lit .unit) hE' hE hn ho hb.val hb.closed hf hv .lit).1

end Flick
