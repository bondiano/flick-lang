// Pure `Context.*` map and reference intrinsics (Stage 4A).

impl Machine<'_, '_> {
    fn context_pure_intrinsic(&mut self, op: LayerIntrinsic, binding: Local) -> Result<(), Error> {
        let out = match op {
            LayerIntrinsic::ContextEmpty => {
                self.release_operands()?;
                self.heap.allocate(Data::Context(BTreeMap::new()))
            }
            LayerIntrinsic::ContextMake => {
                let imp = self.scratch.pop().expect("impl");
                let tag = self.scratch.pop().expect("tag");
                let key = self.service_tag_key(&tag)?.to_owned();
                self.heap.drop_value(tag)?;
                let mut map = BTreeMap::new();
                map.insert(key, imp);
                self.heap.allocate(Data::Context(map))
            }
            LayerIntrinsic::ContextAdd => {
                let ctx = self.scratch.pop().expect("ctx");
                let imp = self.scratch.pop().expect("impl");
                let tag = self.scratch.pop().expect("tag");
                let key = self.service_tag_key(&tag)?.to_owned();
                self.heap.drop_value(tag)?;
                let mut map = self.clone_context_map(&ctx)?;
                self.heap.drop_value(ctx)?;
                if let Some(old) = map.insert(key, imp) {
                    self.heap.drop_value(old)?;
                }
                self.heap.allocate(Data::Context(map))
            }
            LayerIntrinsic::ContextAddOrOmit => return self.exec_context_add_or_omit(binding),
            LayerIntrinsic::ContextGet => return self.exec_context_get(binding),
            LayerIntrinsic::ContextGetOption => return self.exec_context_get_option(binding),
            LayerIntrinsic::ContextMerge => {
                // `Context.merge that self`: `that` replaces `self` (matches Go ContextMerge).
                let self_ctx = self.scratch.pop().expect("self ctx");
                let that_ctx = self.scratch.pop().expect("that ctx");
                self.merge_context_values(vec![self_ctx, that_ctx])?
            }
            LayerIntrinsic::ContextMergeAll => {
                let rest = self.scratch.pop().expect("rest contexts");
                let first = self.scratch.pop().expect("first context");
                let mut all = vec![first];
                all.extend(self.clone_list_values(&rest)?);
                self.heap.drop_value(rest)?;
                self.merge_context_values(all)?
            }
            LayerIntrinsic::ContextPick => return self.exec_context_pick(binding),
            LayerIntrinsic::ContextOmit => {
                let ctx = self.scratch.pop().expect("ctx");
                let tag = self.scratch.pop().expect("tag");
                let key = self.service_tag_key(&tag)?.to_owned();
                self.heap.drop_value(tag)?;
                let mut map = self.clone_context_map(&ctx)?;
                self.heap.drop_value(ctx)?;
                if let Some(old) = map.remove(&key) {
                    self.heap.drop_value(old)?;
                }
                self.heap.allocate(Data::Context(map))
            }
            LayerIntrinsic::ContextReference => {
                let default_fn = self.scratch.pop().expect("default fn");
                let key_val = self.scratch.pop().expect("ref key");
                let key = self.heap.string(&key_val)?.to_owned();
                self.heap.drop_value(key_val)?;
                self.heap.allocate(Data::ContextRef { key, default_fn })
            }
            _ => unreachable!("non-pure context intrinsic"),
        };
        self.store(binding, out)
    }

    fn exec_context_add_or_omit(&mut self, binding: Local) -> Result<(), Error> {
        let ctx = self.scratch.pop().expect("ctx");
        let opt = self.scratch.pop().expect("opt impl");
        let tag = self.scratch.pop().expect("tag");
        let key = self.service_tag_key(&tag)?.to_owned();
        self.heap.drop_value(tag)?;
        let mut map = self.clone_context_map(&ctx)?;
        self.heap.drop_value(ctx)?;
        if let Some(imp) = self.extract_opt_value(opt)? {
            if let Some(old) = map.insert(key, imp) {
                self.heap.drop_value(old)?;
            }
        } else if let Some(old) = map.remove(&key) {
            self.heap.drop_value(old)?;
        }
        let out = self.heap.allocate(Data::Context(map));
        self.store(binding, out)
    }

    fn exec_context_get(&mut self, binding: Local) -> Result<(), Error> {
        let ctx = self.scratch.pop().expect("ctx");
        let tag = self.scratch.pop().expect("tag");
        let key = self.service_tag_key(&tag)?.to_owned();
        self.heap.drop_value(tag)?;
        let rep = self
            .expect_context_map(&ctx)?
            .get(&key)
            .map(Value::representation);
        let Some(r) = rep else {
            self.heap.drop_value(ctx)?;
            return self.raise_defect(DefectSpec::custom(format!("Service not found: {key}")));
        };
        let out = self.heap.dup(&r)?;
        self.heap.drop_value(ctx)?;
        self.store(binding, out)
    }

    fn exec_context_get_option(&mut self, binding: Local) -> Result<(), Error> {
        let ctx = self.scratch.pop().expect("ctx");
        let tag = self.scratch.pop().expect("tag");
        let key = self.service_tag_key(&tag)?.to_owned();
        self.heap.drop_value(tag)?;
        let rep = self
            .expect_context_map(&ctx)?
            .get(&key)
            .map(Value::representation);
        let out = match rep {
            Some(r) => {
                let dup = self.heap.dup(&r)?;
                self.heap.construct("Prelude.Option.Some", vec![dup])?
            }
            None => self.heap.construct("Prelude.Option.None", vec![])?,
        };
        self.heap.drop_value(ctx)?;
        self.store(binding, out)
    }

    fn exec_context_pick(&mut self, binding: Local) -> Result<(), Error> {
        let ctx = self.scratch.pop().expect("ctx");
        let tag = self.scratch.pop().expect("tag");
        let key = self.service_tag_key(&tag)?.to_owned();
        self.heap.drop_value(tag)?;
        let rep = self
            .expect_context_map(&ctx)?
            .get(&key)
            .map(Value::representation);
        let mut map = BTreeMap::new();
        if let Some(r) = rep {
            map.insert(key, self.heap.dup(&r)?);
        }
        self.heap.drop_value(ctx)?;
        let out = self.heap.allocate(Data::Context(map));
        self.store(binding, out)
    }
}
