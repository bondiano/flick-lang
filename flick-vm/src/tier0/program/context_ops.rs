// LayerIntrinsic dispatch, Context.*, and Context.Reference operations (Stage 4A).

use flick_core::ir::memory::LayerIntrinsic;

impl Machine<'_, '_> {
    pub(super) fn layer_std_intrinsic(
        &mut self,
        op: LayerIntrinsic,
        binding: Local,
    ) -> Result<(), Error> {
        match op {
            LayerIntrinsic::ContextEmpty
            | LayerIntrinsic::ContextMake
            | LayerIntrinsic::ContextAdd
            | LayerIntrinsic::ContextAddOrOmit
            | LayerIntrinsic::ContextGet
            | LayerIntrinsic::ContextGetOption
            | LayerIntrinsic::ContextMerge
            | LayerIntrinsic::ContextMergeAll
            | LayerIntrinsic::ContextPick
            | LayerIntrinsic::ContextOmit
            | LayerIntrinsic::ContextReference => self.context_pure_intrinsic(op, binding),
            LayerIntrinsic::ContextRead => self.exec_context_read(binding),
            LayerIntrinsic::ContextWithReference => self.exec_context_with_ref(binding),
            LayerIntrinsic::LayerEmpty
            | LayerIntrinsic::LayerSucceedContext
            | LayerIntrinsic::LayerFromBuild
            | LayerIntrinsic::LayerFresh
            | LayerIntrinsic::LayerOrDie
            | LayerIntrinsic::LayerMergeAll => self.layer_ctor_intrinsic(op, binding),
            LayerIntrinsic::LayerMakeMemoMap
            | LayerIntrinsic::LayerForkMemoMap
            | LayerIntrinsic::LayerCurrentMemoMap
            | LayerIntrinsic::LayerBuild
            | LayerIntrinsic::LayerBuildWithMemoMap => self.layer_memo_intrinsic(op, binding),
            LayerIntrinsic::LayerProvideContext => {
                let task = self.scratch.pop().expect("task");
                let ctx = self.scratch.pop().expect("context");
                self.expect_context_map(&ctx)?;
                self.scratch.extend([task, Value::Unit]);
                self.await_task_fresh(Destination::ProvideContext {
                    context: ctx,
                    binding,
                })
            }
            LayerIntrinsic::LayerLookupService => {
                let tag_val = self.scratch.pop().expect("service tag");
                let key = self.service_tag_key(&tag_val)?.to_owned();
                self.heap.drop_value(tag_val)?;
                if let Some(impl_val) = self.lookup_service_impl(&key)? {
                    return self.store(binding, impl_val);
                }
                if self.foreign.bridge_opened {
                    return self.foreign_lookup_service(binding, &key);
                }
                self.raise_defect(DefectSpec::custom(format!("Service not found: {key}")))
            }
            LayerIntrinsic::OwnerNew
            | LayerIntrinsic::OwnerRun
            | LayerIntrinsic::OwnerClose
            | LayerIntrinsic::EnvCapture
            | LayerIntrinsic::EnvRun => self.manual_scope_intrinsic(op, binding),
            LayerIntrinsic::ContextSetFiberRef => self.exec_context_set_fiber_ref(binding),
            LayerIntrinsic::MetricRuntimeFibersStarted => {
                self.release_operands()?;
                #[expect(clippy::cast_precision_loss, reason = "fiber count fits exact f64")]
                let count = self.runtime_fibers_started as f64;
                self.store(binding, Value::Float(count))
            }
        }
    }

    fn layer_ctor_intrinsic(&mut self, op: LayerIntrinsic, binding: Local) -> Result<(), Error> {
        let data = match op {
            LayerIntrinsic::LayerEmpty => LayerData::Empty,
            LayerIntrinsic::LayerSucceedContext => {
                let ctx = self.scratch.pop().expect("context");
                self.expect_context_map(&ctx)?;
                LayerData::Context(ctx)
            }
            LayerIntrinsic::LayerFromBuild => {
                let build = self.scratch.pop().expect("build fn");
                let Value::Bool(memo) = self.scratch.pop().expect("memo bool") else {
                    self.heap.drop_value(build)?;
                    return Err(Error::ExpectedBoolean);
                };
                LayerData::Dynamic { memo, build }
            }
            LayerIntrinsic::LayerFresh => {
                let inner = self.scratch.pop().expect("layer");
                LayerData::Fresh(inner)
            }
            LayerIntrinsic::LayerOrDie => {
                let inner = self.scratch.pop().expect("layer");
                LayerData::OrDie(inner)
            }
            LayerIntrinsic::LayerMergeAll => {
                let rest = self.scratch.pop().expect("rest layers");
                let first = self.scratch.pop().expect("first layer");
                let mut all = vec![first];
                all.extend(self.clone_list_values(&rest)?);
                self.heap.drop_value(rest)?;
                LayerData::MergeAll(all)
            }
            _ => unreachable!("non-constructor layer intrinsic"),
        };
        let val = self.heap.allocate(Data::Layer(data));
        self.store(binding, val)
    }

    fn layer_memo_intrinsic(&mut self, op: LayerIntrinsic, binding: Local) -> Result<(), Error> {
        match op {
            LayerIntrinsic::LayerMakeMemoMap => {
                self.release_operands()?;
                let id = self.alloc_memo_map(None);
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store(binding, val)
            }
            LayerIntrinsic::LayerForkMemoMap => {
                let parent_val = self.scratch.pop().expect("parent memo");
                let parent_id = self.memo_map_id_of(&parent_val)?;
                self.heap.drop_value(parent_val)?;
                let id = self.alloc_memo_map(Some(parent_id));
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store(binding, val)
            }
            LayerIntrinsic::LayerCurrentMemoMap => {
                self.release_operands()?;
                let id = self.current_or_forked_memo_id()?;
                let val = self.heap.allocate(Data::MemoMap {
                    execution: self.execution,
                    id,
                });
                self.store(binding, val)
            }
            LayerIntrinsic::LayerBuild => {
                let layer = self.scratch.pop().expect("layer");
                let memo_id = self.current_or_forked_memo_id()?;
                let owner = self.current_owner;
                self.begin_build_layer(
                    layer,
                    memo_id,
                    owner,
                    false,
                    LayerBuildTarget::Local(binding),
                )
            }
            LayerIntrinsic::LayerBuildWithMemoMap => {
                let layer = self.scratch.pop().expect("layer");
                let memo_val = self.scratch.pop().expect("memo");
                let memo_id = self.memo_map_id_of(&memo_val)?;
                self.heap.drop_value(memo_val)?;
                let owner = self.current_owner;
                self.begin_build_layer(
                    layer,
                    memo_id,
                    owner,
                    false,
                    LayerBuildTarget::Local(binding),
                )
            }
            _ => unreachable!("non-memo layer intrinsic"),
        }
    }

    fn exec_context_read(&mut self, binding: Local) -> Result<(), Error> {
        let ref_val = self.scratch.pop().expect("context ref");
        let (key, default_rep) = {
            let Value::Object(tok) = &ref_val else {
                self.heap.drop_value(ref_val)?;
                return Err(Error::ExpectedContextRef);
            };
            let Data::ContextRef { key, default_fn } = &self.heap.object(tok.id)?.data else {
                self.heap.drop_value(ref_val)?;
                return Err(Error::ExpectedContextRef);
            };
            (key.clone(), default_fn.representation())
        };
        match self.lookup_service_impl(&key) {
            Ok(Some(bound)) => {
                self.heap.drop_value(ref_val)?;
                return self.store(binding, bound);
            }
            Ok(None) => {}
            Err(err) => {
                self.heap.drop_value(ref_val)?;
                return Err(err);
            }
        }
        if let Some(bound) = self.fiber_refs.get(&key) {
            let rep = bound.representation();
            let dup = self.heap.dup(&rep)?;
            self.heap.drop_value(ref_val)?;
            return self.store(binding, dup);
        }
        let default_fn = self.heap.dup(&default_rep)?;
        self.heap.drop_value(ref_val)?;
        self.scratch.extend([default_fn, Value::Unit]);
        self.apply_fresh(Destination::Bind(binding))
    }

    fn exec_context_set_fiber_ref(&mut self, binding: Local) -> Result<(), Error> {
        let val = self.scratch.pop().expect("fiber ref val");
        let ref_val = self.scratch.pop().expect("context ref");
        let Value::Object(tok) = &ref_val else {
            self.heap.drop_values(vec![ref_val, val])?;
            return Err(Error::ExpectedContextRef);
        };
        let Data::ContextRef { key, .. } = &self.heap.object(tok.id)?.data else {
            self.heap.drop_values(vec![ref_val, val])?;
            return Err(Error::ExpectedContextRef);
        };
        let key = key.clone();
        self.heap.drop_value(ref_val)?;
        if let Some(old) = self.fiber_refs.insert(key, val) {
            self.heap.drop_value(old)?;
        }
        self.store(binding, Value::Unit)
    }

    fn exec_context_with_ref(&mut self, binding: Local) -> Result<(), Error> {
        let task = self.scratch.pop().expect("task");
        let val = self.scratch.pop().expect("value");
        let ref_val = self.scratch.pop().expect("ref");
        let Value::Object(tok) = &ref_val else {
            self.heap.drop_values(vec![ref_val, val, task])?;
            return Err(Error::ExpectedContextRef);
        };
        let Data::ContextRef { key, .. } = &self.heap.object(tok.id)?.data else {
            self.heap.drop_values(vec![ref_val, val, task])?;
            return Err(Error::ExpectedContextRef);
        };
        let key = key.clone();
        self.heap.drop_value(ref_val)?;
        if key.starts_with("flick/Scheduler/")
            || key == MEMO_REF_KEY
            || key == "effect/References/CurrentLogAnnotations"
        {
            let prev = self.fiber_refs.insert(key.clone(), val);
            self.scratch.extend([task, Value::Unit]);
            return self.await_task_fresh(Destination::RestoreFiberRef {
                key,
                previous: prev,
                binding,
            });
        }
        let mut map = BTreeMap::new();
        map.insert(key, val);
        let ctx = self.heap.allocate(Data::Context(map));
        self.scratch.extend([task, Value::Unit]);
        self.await_task_fresh(Destination::ProvideContext {
            context: ctx,
            binding,
        })
    }

    fn service_tag_key(&self, val: &Value) -> Result<&str, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedServiceTag);
        };
        match &self.heap.object(tok.id)?.data {
            Data::ServiceTag { key } | Data::ContextRef { key, .. } => Ok(key),
            _ => Err(Error::ExpectedServiceTag),
        }
    }

    fn expect_context_map(&self, val: &Value) -> Result<&BTreeMap<String, Value>, Error> {
        let Value::Object(tok) = val else {
            return Err(Error::ExpectedContext);
        };
        let Data::Context(map) = &self.heap.object(tok.id)?.data else {
            return Err(Error::ExpectedContext);
        };
        Ok(map)
    }

    fn clone_context_map(&mut self, val: &Value) -> Result<BTreeMap<String, Value>, Error> {
        let pairs: Vec<(String, Value)> = self
            .expect_context_map(val)?
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect();
        let mut out = BTreeMap::new();
        for (k, rep) in pairs {
            match self.heap.dup(&rep) {
                Ok(v) => {
                    out.insert(k, v);
                }
                Err(err) => {
                    self.heap.drop_values(out.into_values().collect())?;
                    return Err(err);
                }
            }
        }
        Ok(out)
    }

    pub(super) fn merge_context_values(&mut self, contexts: Vec<Value>) -> Result<Value, Error> {
        let mut merged: BTreeMap<String, Value> = BTreeMap::new();
        let mut iter = contexts.into_iter();
        while let Some(ctx) = iter.next() {
            let map = match self.clone_context_map(&ctx) {
                Ok(m) => m,
                Err(err) => {
                    self.heap.drop_value(ctx)?;
                    self.heap.drop_values(iter.collect())?;
                    self.heap.drop_values(merged.into_values().collect())?;
                    return Err(err);
                }
            };
            self.heap.drop_value(ctx)?;
            for (k, v) in map {
                if k == HOST_CTX_KEY
                    && let Some(prev) = merged.remove(HOST_CTX_KEY)
                {
                    let combined = self.concat_host_context_lists(prev, v)?;
                    merged.insert(k, combined);
                } else if let Some(old) = merged.insert(k, v) {
                    self.heap.drop_value(old)?;
                }
            }
        }
        Ok(self.heap.allocate(Data::Context(merged)))
    }
}
