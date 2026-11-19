import Formal.Equiv.RevInv

/-!
# Equivalence of every run: inversion at `top`, from the right

At `top` no special pair stands at the top, so a term related to a former of the right side is
the same former on the left, with related parts (`Formal/Equiv/Inv.lean` the other way round).
-/

namespace Flick.Equiv

variable {W : World}

theorem Sim.rinv_lam {r : Tm} {b' : Tm} (h : Sim W .top r (.lam b')) :
    ∃ b, r = .lam b := by
  generalize hx : Pos.top = x at h
  cases h with
  | lam => subst hx; exact ⟨_, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_fix {r : Tm} {f' : Tm} (h : Sim W .top r (.fix f')) :
    ∃ f, r = .fix f := by
  generalize hx : Pos.top = x at h
  cases h with
  | fix => subst hx; exact ⟨_, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_app {r : Tm} {f' a' : Tm} (h : Sim W .top r (.app f' a')) :
    ∃ f a, r = .app f a ∧ Sim W .top f f' ∧ Sim W .top a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | app => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_let {r : Tm} {e' b' : Tm} (h : Sim W .top r (.let_ e' b')) :
    ∃ e b, r = .let_ e b ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | let_ => subst hx; exact ⟨_, _, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_letrec {r : Tm} {f' b' : Tm} (h : Sim W .top r (.letrec f' b')) :
    ∃ f b, r = .letrec f b := by
  generalize hx : Pos.top = x at h
  cases h with
  | letrec => subst hx; exact ⟨_, _, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_lit {r : Tm} {l : Lit} (h : Sim W .top r (.lit l)) :
    r = .lit l := by
  generalize hx : Pos.top = x at h
  cases h with
  | lit => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_loc {r : Tm} {l' : Nat} (h : Sim W .top r (.loc l')) :
    ∃ l k, r = .loc l ∧ W.L l l' k := by
  generalize hx : Pos.top = x at h
  cases h with
  | loc => subst hx; exact ⟨_, _, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_rcons {r : Tm} {l : Label} {e' r' : Tm} (h : Sim W .top r (.rcons l e' r')) :
    ∃ e rr, r = .rcons l e rr ∧ Sim W .top e e' ∧ Sim W .top rr r' := by
  generalize hx : Pos.top = x at h
  cases h with
  | rcons => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_proj {r : Tm} {e' : Tm} {l : Label} (h : Sim W .top r (.proj e' l)) :
    ∃ e, r = .proj e l ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | proj => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_update {r : Tm} {e' : Tm} {l : Label} {a' : Tm} (h : Sim W .top r (.update e' l a')) :
    ∃ e a, r = .update e l a ∧ Sim W .any e e' ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | update => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_match {r : Tm} {e' al' : Tm} (h : Sim W .top r (.match_ e' al')) :
    ∃ e al, r = .match_ e al ∧ Sim W .any e e' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | match_ => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_anil {r : Tm}  (h : Sim W .top r (.anil)) :
    r = .anil := by
  generalize hx : Pos.top = x at h
  cases h with
  | anil => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_acons {r : Tm} {p : Pat} {b' r' : Tm} (h : Sim W .top r (.acons p b' r')) :
    ∃ b rr, r = .acons p b rr := by
  generalize hx : Pos.top = x at h
  cases h with
  | acons => subst hx; exact ⟨_, _, rfl⟩
  | aconsD => subst hx; exact ⟨_, _, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_aguard {r : Tm} {p : Pat} {g' b' r' : Tm} (h : Sim W .top r (.aguard p g' b' r')) :
    ∃ g b rr, r = .aguard p g b rr := by
  generalize hx : Pos.top = x at h
  cases h with
  | aguard => subst hx; exact ⟨_, _, _, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_todo {r : Tm}  (h : Sim W .top r (.todo)) :
    r = .todo := by
  generalize hx : Pos.top = x at h
  cases h with
  | todo => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_task {r : Tm} {e' : Tm} (h : Sim W .top r (.task e')) :
    ∃ e, r = .task e := by
  generalize hx : Pos.top = x at h
  cases h with
  | task => subst hx; exact ⟨_, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_await {r : Tm} {e' : Tm} (h : Sim W .top r (.await e')) :
    ∃ e, r = .await e ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | await => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_lazy {r : Tm} {e' : Tm} (h : Sim W .top r (.lazy e')) :
    ∃ e, r = .lazy e := by
  generalize hx : Pos.top = x at h
  cases h with
  | lazy => subst hx; exact ⟨_, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_force {r : Tm} {e' : Tm} (h : Sim W .top r (.force e')) :
    ∃ e, r = .force e ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | force => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_raise {r : Tm} {e' : Tm} (h : Sim W .top r (.raise e')) :
    ∃ e, r = .raise e ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | raise => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_try {r : Tm} {e' al' : Tm} (h : Sim W .top r (.try_ e' al')) :
    ∃ e al, r = .try_ e al ∧ Sim W .any e e' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | try_ => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_handle {r : Tm} {v' al' : Tm} (h : Sim W .top r (.handle v' al')) :
    ∃ v al, r = .handle v al ∧ Sim W .top v v' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | handle => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_scope {r : Tm} {e' : Tm} (h : Sim W .top r (.scope e')) :
    ∃ e, r = .scope e := by
  generalize hx : Pos.top = x at h
  cases h with
  | scope => subst hx; exact ⟨_, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_inside {r : Tm} {p : Nat} {e' : Tm} (h : Sim W .top r (.inside p e')) :
    ∃ e, r = .inside p e ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | inside => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_own {r : Tm} {p : Nat} (h : Sim W .top r (.own p)) :
    r = .own p := by
  generalize hx : Pos.top = x at h
  cases h with
  | own => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_cap {r : Tm} {p : Nat} (h : Sim W .top r (.cap p)) :
    r = .cap p := by
  generalize hx : Pos.top = x at h
  cases h with
  | cap => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_glob {r : Tm} {g : Nat} (h : Sim W .top r (.glob g)) :
    r = .glob g := by
  generalize hx : Pos.top = x at h
  cases h with
  | glob => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hdl {r : Tm} {E : Name} {al' r' e' : Tm} (h : Sim W .top r (.hdl E al' r' e')) :
    ∃ al rr e, r = .hdl E al rr e := by
  generalize hx : Pos.top = x at h
  cases h with
  | hdl => subst hx; exact ⟨_, _, _, rfl⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hwith {r : Tm} {p ℓ' : Nat} {E : Name} {al' r' e' : Tm} (h : Sim W .top r (.hwith p ℓ' E al' r' e')) :
    ∃ ℓ k al rr e, r = .hwith p ℓ E al rr e ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any rr r' ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hwith => subst hx; exact ⟨_, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hkeep {r : Tm} {q ℓ' : Nat} {E : Name} {al' e' : Tm} (h : Sim W .top r (.hkeep q ℓ' E al' e')) :
    ∃ ℓ k al e, r = .hkeep q ℓ E al e ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hkeep => subst hx; exact ⟨_, _, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hkarm {r : Tm} {q ℓ' : Nat} {E : Name} {al' a' c' : Tm} (h : Sim W .top r (.hkarm q ℓ' E al' a' c')) :
    ∃ ℓ k al a c, r = .hkarm q ℓ E al a c ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any a a' ∧ Sim W .park c c' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hkarm => subst hx; exact ⟨_, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hctl {r : Tm} {p ℓ' κ' : Nat} {tl : Bool} {E : Name} {al' r' a' c' : Tm} (h : Sim W .top r (.hctl p ℓ' κ' tl E al' r' a' c')) :
    ∃ ℓ k κ al rr a c, r = .hctl p ℓ κ tl E al rr a c ∧ W.L ℓ ℓ' k ∧ KapR W tl κ κ' ∧ Sim W .top al al' ∧ Sim W .any rr r' ∧ Sim W .any a a' ∧ Sim W .park c c' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hctl => subst hx; exact ⟨_, _, _, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hres {r : Tm} {p ℓ' : Nat} {a' b' : Tm} (h : Sim W .top r (.hres p ℓ' a' b')) :
    ∃ ℓ k a b, r = .hres p ℓ a b ∧ W.L ℓ ℓ' k ∧ Sim W .park a a' ∧ Sim W .any b b' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hres => subst hx; exact ⟨_, _, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hdone {r : Tm} {p : Nat} {a' : Tm} (h : Sim W .top r (.hdone p a')) :
    ∃ a, r = .hdone p a ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hdone => subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_unw {r : Tm} {p ℓ' : Nat} {a' : Tm} (h : Sim W .top r (.unw p ℓ' a')) :
    ∃ ℓ k a, r = .unw p ℓ a ∧ W.L ℓ ℓ' k ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | unw => subst hx; exact ⟨_, _, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_kont {r : Tm} {p ℓ' : Nat} (h : Sim W .top r (.kont p ℓ')) :
    ∃ ℓ k, r = .kont p ℓ ∧ W.L ℓ ℓ' k := by
  generalize hx : Pos.top = x at h
  cases h with
  | kont => subst hx; exact ⟨_, _, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_hole {r : Tm} {p : Nat} (h : Sim W .top r (.hole p)) :
    r = .hole p := by
  generalize hx : Pos.top = x at h
  cases h with
  | hole => subst hx; exact rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.rinv_settle {r : Tm} {ℓ' : Nat} {e' : Tm} (h : Sim W .top r (.settle ℓ' e')) :
    ∃ ℓ a, r = .settle ℓ a ∧ W.L ℓ ℓ' .sym ∧ Sim W .any a e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | settle => subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

end Flick.Equiv
