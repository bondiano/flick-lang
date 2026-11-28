import Formal.MemorySoundness.HeapInv.DropReuse

/-!
# HeapBalanced preservation for recursive typed-cell Reuse (#463)
-/

namespace Flick.Memory.Soundness

open Flick.Memory.Aggregate

/-- Recursive typed-cell `Reuse`: handles Stage 2 `reuseToken` AND Stage 1 live
`tagged` cells under full `CellLayout` compatibility, and falls back cleanly on
incompatible `CellLayout`, shared cells, non-reusable cells, or `.imm`. -/
def reuseTypedCellRec (fuel nextId : Nat) (h : AggHeap) (cand : Aggregate.Value)
    (tag : Nat) (targetLayout : CellLayout) (fields : List Aggregate.Value) :
    Option (Nat × AggHeap × Aggregate.Value × Bool) :=
  match cand with
  | .agg _ _ => none
  | .imm =>
      some (nextId + 1, setAggObj h nextId (some ⟨1, .tagged tag targetLayout fields⟩),
        .boxed nextId, false)
  | .boxed id =>
      match h id with
      | none => none
      | some ⟨1, .reuseToken candLayout⟩ =>
          if candLayout = targetLayout then
            some (nextId, setAggObj h id (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed id, true)
          else
            let h1 := dropIdFuel fuel h id
            some (nextId + 1, setAggObj h1 nextId (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed nextId, false)
      | some ⟨1, .tagged _ candLayout oldFields⟩ =>
          if candLayout = targetLayout then
            let h1 := dropIdsFuel fuel (setAggObj h id (some ⟨1, .reuseToken candLayout⟩))
              (boxedLeavesList oldFields)
            some (nextId, setAggObj h1 id (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed id, true)
          else
            let h1 := dropIdFuel fuel h id
            some (nextId + 1, setAggObj h1 nextId (some ⟨1, .tagged tag targetLayout fields⟩),
              .boxed nextId, false)
      | some _ =>
          let h1 := dropIdFuel fuel h id
          some (nextId + 1, setAggObj h1 nextId (some ⟨1, .tagged tag targetLayout fields⟩),
            .boxed nextId, false)

theorem heapBalanced_reuseTypedCellRec {nextId bound fuel : Nat} {h : AggHeap}
    {rank roots : ObjId → Nat} (cand : Aggregate.Value)
    (tag : Nat) (targetLayout : CellLayout) (fields : List Aggregate.Value)
    (hb : HeapBalanced nextId h rank bound roots)
    (hcnt : ∀ k, boxedTokens cand k + valsTokens fields k ≤ roots k)
    (hnotAgg : ∀ t fs, cand ≠ .agg t fs)
    (hfuel : bound ≤ fuel) :
    ∃ nextId' h' rank' res reused excl,
      reuseTypedCellRec fuel nextId h cand tag targetLayout fields =
        some (nextId', h', res, reused) ∧
      HeapBalanced nextId' h' rank' (bound + 1)
        (fun k => roots k - (boxedTokens cand k + valsTokens fields k) + boxedTokens res k) ∧
      (∀ c, excl = some c → heapInternalRefs nextId h c = 0 ∧
        roots c = boxedTokens cand c + valsTokens fields c ∧
        boxedTokens cand c = 1 ∧ valsTokens fields c = 0) ∧
      (∀ k o1 o2, some k ≠ excl → h k = some o1 → h' k = some o2 → o1.slot = o2.slot) ∧
      (∃ id', res = .boxed id' ∧
        h' id' = some ⟨1, .tagged tag targetLayout fields⟩) := by
  have hfallback : ∀ (id : ObjId),
      cand = .boxed id →
      (∃ obj, h id = some obj) →
      ∃ nextId' h' rank' res,
        nextId' = nextId + 1 ∧
        h' = setAggObj (dropIdFuel fuel h id) nextId (some ⟨1, .tagged tag targetLayout fields⟩) ∧
        res = .boxed nextId ∧
        HeapBalanced nextId' h' rank' (bound + 1)
          (fun k => roots k - (boxedTokens cand k + valsTokens fields k) + boxedTokens res k) ∧
        (∀ k o1 o2, h k = some o1 → h' k = some o2 → o1.slot = o2.slot) ∧
        h' nextId = some ⟨1, .tagged tag targetLayout fields⟩ := by
    intro id hcand ⟨obj, hobj⟩
    subst hcand
    have hidRoot : 0 < roots id := by
      have := hcnt id; simp [boxedTokens, boxedLeaves] at this; omega
    have hrank : rank id < fuel := Nat.lt_of_lt_of_le (hb.rankBound id obj hobj) hfuel
    obtain ⟨hb1, hsame1⟩ := (heapBalanced_dropIdFuel fuel).1 id hb hidRoot hrank
    have hfsCnt : ∀ k, (slotBoxedLeaves (.tagged tag targetLayout fields)).count k ≤
        (if k = id then roots id - 1 else roots k) := by
      intro k
      have hk := hcnt k
      simp only [slotBoxedLeaves]
      change valsTokens fields k ≤ _
      by_cases hkid : k = id
      · subst k; simp [boxedTokens, boxedLeaves] at hk ⊢; omega
      · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
        simp [boxedTokens, boxedLeaves, List.count_cons, hbeq] at hk
        simp [hkid]; omega
    obtain ⟨hb2, hsame2⟩ := heapBalanced_allocFresh (.tagged tag targetLayout fields) hb1 hfsCnt
    have hrEq : ∀ k, (if k = id then roots id - 1 else roots k) -
        (slotBoxedLeaves (.tagged tag targetLayout fields)).count k + (if k = nextId then 1 else 0) =
        roots k - (boxedTokens (.boxed id) k + valsTokens fields k) +
          boxedTokens (.boxed nextId) k := by
      intro k
      simp only [slotBoxedLeaves]
      change (if k = id then roots id - 1 else roots k) - valsTokens fields k +
        (if k = nextId then 1 else 0) = _
      by_cases hkid : k = id <;> by_cases hknext : k = nextId
      · subst k; subst hknext
        simp [boxedTokens, boxedLeaves]; omega
      · subst k
        have hbeq : (nextId == id) = false := beq_false_of_ne (Ne.symm hknext)
        simp [boxedTokens, boxedLeaves, List.count_cons, hbeq, hknext]; omega
      · subst k
        have hbeq : (id == nextId) = false := beq_false_of_ne (Ne.symm hkid)
        simp [boxedTokens, boxedLeaves, List.count_cons, hbeq, hkid]
      · have hbeq1 : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
        have hbeq2 : (nextId == k) = false := beq_false_of_ne (Ne.symm hknext)
        simp [boxedTokens, boxedLeaves, List.count_cons, hbeq1, hbeq2, hkid, hknext]
    refine ⟨nextId + 1, _, _, .boxed nextId, rfl, rfl, rfl, hb2.congrRoots hrEq, ?_,
      by simp [setAggObj]⟩
    intro k o1 o2 h1k h2k
    cases hmid : dropIdFuel fuel h id k with
    | none =>
        have hne : k ≠ nextId := by
          intro eq; subst k
          rw [hb.bounded nextId (Nat.le_refl _)] at h1k; cases h1k
        simp only [setAggObj, hne, ite_false] at h2k
        rw [hmid] at h2k; cases h2k
    | some oMid =>
        exact (hsame1 k o1 oMid h1k hmid).trans (hsame2 k oMid o2 hmid h2k)
  have hinplace : ∀ (h0 : AggHeap) (r0 : ObjId → Nat) (id : ObjId) (layout : CellLayout),
      HeapBalanced nextId h0 rank bound r0 →
      h0 id = some ⟨1, .reuseToken layout⟩ →
      (∀ k, boxedTokens (.boxed id) k + valsTokens fields k ≤ r0 k) →
      let h' := setAggObj h0 id (some ⟨1, .tagged tag targetLayout fields⟩)
      let rank' := fun k => if k = id then bound else rank k
      HeapBalanced nextId h' rank' (bound + 1)
        (fun k => r0 k - (boxedTokens (.boxed id) k + valsTokens fields k) + boxedTokens (.boxed id) k) ∧
      heapInternalRefs nextId h0 id = 0 ∧
      r0 id = 1 ∧ valsTokens fields id = 0 ∧
      (∀ k o1 o2, k ≠ id → h0 k = some o1 → h' k = some o2 → o1.slot = o2.slot) := by
    intro h0 r0 id layout hb0 htok hcnt0
    have hconId := hb0.conserve id
    simp only [rcOf, htok] at hconId
    have hcntId := hcnt0 id
    simp only [boxedTokens, boxedLeaves, List.count_cons_self, List.count_nil] at hcntId
    have hrId1 : r0 id = 1 := by omega
    have hintId0 : heapInternalRefs nextId h0 id = 0 := by omega
    have hfsId0 : valsTokens fields id = 0 := by omega
    have hidLt : id < nextId := hb0.live_lt htok
    have hnotInFs : id ∉ boxedLeavesList fields := List.count_eq_zero.mp hfsId0
    let h' := setAggObj h0 id (some ⟨1, .tagged tag targetLayout fields⟩)
    let rank' : ObjId → Nat := fun k => if k = id then bound else rank k
    refine ⟨⟨?_, ?_, ?_, ?_, ?_⟩, hintId0, hrId1, hfsId0, ?_⟩
    · intro k hk
      have hne : k ≠ id := Nat.ne_of_gt (Nat.lt_of_lt_of_le hidLt hk)
      simp [setAggObj, hne, hb0.bounded k hk]
    · intro k obj hk
      by_cases hkid : k = id
      · subst k
        simp only [setAggObj, ite_true, Option.some.injEq] at hk
        subst obj; simp
      · simp only [setAggObj, hkid, ite_false] at hk
        exact hb0.posRc k obj hk
    · intro p obj hp c hc
      by_cases hpid : p = id
      · subst p
        simp only [setAggObj, ite_true, Option.some.injEq] at hp
        subst obj
        simp only [slotBoxedLeaves] at hc
        have hcNe : c ≠ id := fun eq => hnotInFs (eq ▸ hc)
        have hcPos : 0 < r0 c := by
          have h1 : 0 < (boxedLeavesList fields).count c := List.count_pos_iff.mpr hc
          have h2 := hcnt0 c
          unfold valsTokens at h2
          omega
        obtain ⟨cobj, hcobj, _⟩ := hb0.live_of_pos_roots hcPos
        simp only [ite_true, hcNe, ite_false]
        exact hb0.rankBound c cobj hcobj
      · simp only [setAggObj, hpid, ite_false] at hp
        have hcNe : c ≠ id := by
          intro eq; subst c
          have := hb0.child_internal_pos hp hc
          omega
        simp only [hpid, hcNe, ite_false]
        exact hb0.acyclic p obj hp c hc
    · intro k obj hk
      by_cases hkid : k = id
      · subst k; simp
      · simp only [setAggObj, hkid, ite_false] at hk
        simp only [hkid, ite_false]
        exact Nat.lt_succ_of_lt (hb0.rankBound k obj hk)
    · intro k
      have hset := heapInternalRefs_set_lt nextId h0 id k
        (some ⟨1, .tagged tag targetLayout fields⟩) hidLt
      simp only [slotRefCount, htok, slotBoxedLeaves, optSlotCount, List.count_nil,
        Nat.add_zero] at hset
      have hcon := hb0.conserve k
      have hck := hcnt0 k
      by_cases hkid : k = id
      · subst k
        have hfs0 : (boxedLeavesList fields).count id = 0 := hfsId0
        simp only [rcOf, setAggObj, ite_true, boxedTokens, boxedLeaves,
          List.count_cons_self, List.count_nil, hrId1, hfsId0]
        change 1 = 1 - (1 + 0) + 1 +
          heapInternalRefs nextId (setAggObj h0 id (some ⟨1, .tagged tag targetLayout fields⟩)) id
        omega
      · have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
        simp only [rcOf, setAggObj, hkid, ite_false, boxedTokens, boxedLeaves,
          List.count_cons, hbeq, List.count_nil, Bool.false_eq_true, ite_false, Nat.zero_add] at hck ⊢
        change rcOf h0 k = r0 k - (boxedLeavesList fields).count k + 0 +
          heapInternalRefs nextId (setAggObj h0 id (some ⟨1, .tagged tag targetLayout fields⟩)) k
        unfold valsTokens at hck
        omega
    · intro k o1 o2 hne h1k h2k
      simp only [setAggObj, hne, ite_false] at h2k
      rw [h1k] at h2k; cases h2k; rfl
  cases cand with
  | agg t fs => exact (hnotAgg t fs rfl).elim
  | imm =>
      have hfsCnt : ∀ k, (slotBoxedLeaves (.tagged tag targetLayout fields)).count k ≤ roots k := by
        intro k; have := hcnt k; simp [boxedTokens, boxedLeaves, valsTokens] at this; exact this
      obtain ⟨hb', hsame'⟩ := heapBalanced_allocFresh (.tagged tag targetLayout fields) hb hfsCnt
      have hrEq : ∀ k, roots k - (slotBoxedLeaves (.tagged tag targetLayout fields)).count k +
          (if k = nextId then 1 else 0) =
          roots k - (boxedTokens .imm k + valsTokens fields k) + boxedTokens (.boxed nextId) k := by
        intro k
        by_cases hk : k = nextId
        · subst k; simp [boxedTokens, boxedLeaves, valsTokens, slotBoxedLeaves]
        · have hbeq : (nextId == k) = false := beq_false_of_ne (Ne.symm hk)
          simp [boxedTokens, boxedLeaves, List.count_cons, hbeq, hk, valsTokens, slotBoxedLeaves]
      refine ⟨nextId + 1, _, _, .boxed nextId, false, none, rfl, hb'.congrRoots hrEq,
        (fun _ hc => by cases hc), fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
        ⟨nextId, rfl, by simp [setAggObj]⟩⟩
  | boxed id =>
      have hidRoot : 0 < roots id := by
        have := hcnt id; simp [boxedTokens, boxedLeaves] at this; omega
      obtain ⟨⟨rc, slot⟩, hobj, hrcPos⟩ := hb.live_of_pos_roots hidRoot
      cases slot with
      | reuseToken candLayout =>
          by_cases hrc1 : rc = 1
          · subst hrc1
            by_cases hlay : candLayout = targetLayout
            · subst hlay
              obtain ⟨hb', hint0, hr1, hfs0, hsame'⟩ := hinplace h roots id candLayout hb hobj hcnt
              refine ⟨nextId, _, _, .boxed id, true, some id,
                by simp [reuseTypedCellRec, hobj], hb', ?_, ?_,
                ⟨id, rfl, by simp [setAggObj]⟩⟩
              · intro c hc; cases hc
                exact ⟨hint0, by simp [boxedTokens, boxedLeaves, hr1, hfs0],
                  by simp [boxedTokens, boxedLeaves], hfs0⟩
              · intro k o1 o2 hne h1 h2
                exact hsame' k o1 o2 (fun eq => hne (congrArg some eq)) h1 h2
            · obtain ⟨nextId', h', rank', res, hnEq, hhEq, hresEq, hb', hsame', hnew⟩ :=
                hfallback id rfl ⟨_, hobj⟩
              refine ⟨nextId', h', rank', res, false, none,
                by simp [reuseTypedCellRec, hobj, hlay, hnEq, hhEq, hresEq], hb',
                (fun _ hc => by cases hc),
                fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
                ⟨nextId, hresEq, hnew⟩⟩
          · obtain ⟨nextId', h', rank', res, hnEq, hhEq, hresEq, hb', hsame', hnew⟩ :=
              hfallback id rfl ⟨_, hobj⟩
            refine ⟨nextId', h', rank', res, false, none,
              by simp [reuseTypedCellRec, hobj, hrc1, hnEq, hhEq, hresEq], hb',
              (fun _ hc => by cases hc),
              fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
              ⟨nextId, hresEq, hnew⟩⟩
      | tagged oldTag candLayout oldFields =>
          by_cases hrc1 : rc = 1
          · subst hrc1
            by_cases hlay : candLayout = targetLayout
            · subst hlay
              have hcntCand : ∀ k, boxedTokens (.boxed id) k ≤ roots k := by
                intro k; have := hcnt k; omega
              obtain ⟨h1, res1, excl1, hdr, hb1, hexcl1, hsame1⟩ :=
                heapBalanced_dropReuseRec (.boxed id) hb hcntCand
                  (fun _ _ eq => by cases eq) hfuel
              simp only [dropReuseRec, hobj, ite_true, Option.some.injEq, Prod.mk.injEq] at hdr
              rcases hdr with ⟨hh1, hres1⟩
              subst hres1
              have hacyc0 : ∀ p o, setAggObj h id (some ⟨1, .reuseToken candLayout⟩) p = some o →
                  ∀ c ∈ slotBoxedLeaves o.slot, rank c < rank p := by
                intro p o hp c hc
                by_cases hpid : p = id
                · subst p
                  simp only [setAggObj, ite_true, Option.some.injEq] at hp
                  subst o; simp [slotBoxedLeaves] at hc
                · simp only [setAggObj, hpid, ite_false] at hp
                  exact hb.acyclic p o hp c hc
              have hchLt : ∀ c ∈ boxedLeavesList oldFields, rank c < rank id :=
                fun c hc => hb.acyclic id ⟨1, .tagged oldTag candLayout oldFields⟩ hobj c hc
              have htok1 : h1 id = some ⟨1, .reuseToken candLayout⟩ := by
                rw [← hh1, (dropFuel_preserves_gt_rank fuel rank).2 _ _ id hacyc0 hchLt]
                simp [setAggObj]
              have hrEq1 : ∀ k, roots k - boxedTokens (.boxed id) k + boxedTokens (.boxed id) k =
                  roots k := by
                intro k; have := hcntCand k; omega
              have hb1' : HeapBalanced nextId h1 rank bound roots := hb1.congrRoots hrEq1
              obtain ⟨hb2, hint1, hr1, hfs0, hsame2⟩ := hinplace h1 roots id candLayout hb1' htok1 hcnt
              have hconOrig := hb.conserve id
              simp only [rcOf, hobj] at hconOrig
              have hintOrig : heapInternalRefs nextId h id = 0 := by omega
              refine ⟨nextId, setAggObj h1 id (some ⟨1, .tagged tag candLayout fields⟩), _,
                .boxed id, true, some id, by simp [reuseTypedCellRec, hobj, hh1], hb2, ?_, ?_,
                ⟨id, rfl, by simp [setAggObj]⟩⟩
              · intro c hc; cases hc
                exact ⟨hintOrig, by simp [boxedTokens, boxedLeaves, hr1, hfs0],
                  by simp [boxedTokens, boxedLeaves], hfs0⟩
              · intro k o1 o2 hne h1k h2k
                have hkid : k ≠ id := fun eq => hne (congrArg some eq)
                simp only [setAggObj, hkid, ite_false] at h2k
                have hne1 : some k ≠ excl1 := by
                  intro heq
                  have hmid := (hexcl1 k heq.symm).2.2
                  have hbeq : (id == k) = false := beq_false_of_ne (Ne.symm hkid)
                  simp [boxedTokens, boxedLeaves, List.count_cons, hbeq] at hmid
                exact hsame1 k o1 o2 hne1 h1k h2k
            · obtain ⟨nextId', h', rank', res, hnEq, hhEq, hresEq, hb', hsame', hnew⟩ :=
                hfallback id rfl ⟨_, hobj⟩
              refine ⟨nextId', h', rank', res, false, none,
                by simp [reuseTypedCellRec, hobj, hlay, hnEq, hhEq, hresEq], hb',
                (fun _ hc => by cases hc),
                fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
                ⟨nextId, hresEq, hnew⟩⟩
          · obtain ⟨nextId', h', rank', res, hnEq, hhEq, hresEq, hb', hsame', hnew⟩ :=
              hfallback id rfl ⟨_, hobj⟩
            refine ⟨nextId', h', rank', res, false, none,
              by simp [reuseTypedCellRec, hobj, hrc1, hnEq, hhEq, hresEq], hb',
              (fun _ hc => by cases hc),
              fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
              ⟨nextId, hresEq, hnew⟩⟩
      | boxedAgg _ _ | «opaque» =>
          obtain ⟨nextId', h', rank', res, hnEq, hhEq, hresEq, hb', hsame', hnew⟩ :=
            hfallback id rfl ⟨_, hobj⟩
          refine ⟨nextId', h', rank', res, false, none,
            by simp [reuseTypedCellRec, hobj, hnEq, hhEq, hresEq], hb',
            (fun _ hc => by cases hc),
            fun k o1 o2 _ h1 h2 => hsame' k o1 o2 h1 h2,
            ⟨nextId, hresEq, hnew⟩⟩

end Flick.Memory.Soundness
