//! Memoized, deterministic data layout engine.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{IrType, LangItem, Module, Program, TypeDecl, TypeInterner, TypeRef, sort_by_utf16};

use super::catalog::{NamedKind, TypeCatalog, instantiate_constructors};
use super::indirection::StaticIndirection;
use super::inst_graph::{InstIndirectMap, compute_instantiated_indirection};
use super::pack::{PackItem, PreparedField, PreparedVariant, layout_data, pack_items};
use super::{
    FieldLayout, FieldStorage, LayoutError, Primitive, PrimitiveTable, RecordFieldLayout,
    RecordLayout, TupleLayout, TypeLayout, TypeRepr,
};

/// Computes and caches deterministic [`TypeLayout`]s for monomorphic IR types
/// against a program's type declarations and a backend's [`PrimitiveTable`].
pub struct LayoutEngine<'p> {
    catalog: TypeCatalog<'p>,
    primitives: PrimitiveTable,
    static_ind: StaticIndirection,
    interner: TypeInterner,
    cache: BTreeMap<TypeRef, TypeLayout>,
}

impl<'p> LayoutEngine<'p> {
    /// Creates an engine for `program` and validates `primitives`.
    pub fn new(
        program: &'p Program,
        primitives: PrimitiveTable,
    ) -> Result<LayoutEngine<'p>, LayoutError> {
        Self::from_parts(&program.modules, &program.lang, primitives)
    }

    /// Creates an engine directly from module slices and lang items.
    pub fn from_parts(
        modules: &'p [Module],
        lang: &'p [LangItem],
        primitives: PrimitiveTable,
    ) -> Result<LayoutEngine<'p>, LayoutError> {
        primitives.validate()?;
        let catalog = TypeCatalog::new(modules, lang);
        let static_ind = StaticIndirection::analyze(&catalog);
        Ok(LayoutEngine {
            catalog,
            primitives,
            static_ind,
            interner: TypeInterner::new(),
            cache: BTreeMap::new(),
        })
    }

    /// Returns the primitive table configured on this engine.
    pub fn primitives(&self) -> &PrimitiveTable {
        &self.primitives
    }

    /// Computes (or retrieves from cache) the layout of monomorphic type `ty`.
    pub fn layout(&mut self, ty: &TypeRef) -> Result<TypeLayout, LayoutError> {
        if let Some(cached) = self.cache.get(ty) {
            return Ok(cached.clone());
        }
        let indirect = compute_instantiated_indirection(
            &self.catalog,
            &self.static_ind,
            ty,
            &mut self.interner,
        )?;
        self.compute(ty, &indirect)
    }

    fn compute(
        &mut self,
        ty: &TypeRef,
        indirect: &InstIndirectMap,
    ) -> Result<TypeLayout, LayoutError> {
        if let Some(cached) = self.cache.get(ty) {
            return Ok(cached.clone());
        }
        let layout = match &**ty {
            IrType::Named { module, name, args } => {
                self.layout_named(ty, module, name, args, indirect)?
            }
            IrType::Tuple(items) => self.layout_tuple(items, indirect)?,
            IrType::Record { fields, tail } => {
                if let Some(row) = tail {
                    return Err(LayoutError::OpenRecord(row.clone()));
                }
                self.layout_record(fields, indirect)?
            }
            IrType::Fun { .. } => self.primitive_layout(Primitive::Closure),
            IrType::Exists { .. } => self.primitive_layout(Primitive::Pointer),
            IrType::Forall {
                vars,
                constraints,
                body,
            } => {
                if vars.is_empty() && constraints.is_empty() {
                    self.compute(body, indirect)?
                } else {
                    self.primitive_layout(Primitive::Closure)
                }
            }
            IrType::Var(_) | IrType::App { .. } => {
                return Err(LayoutError::PolymorphicType(ty.to_string()));
            }
        };
        self.cache.insert(ty.clone(), layout.clone());
        Ok(layout)
    }

    fn primitive_layout(&self, prim: Primitive) -> TypeLayout {
        let p = self.primitives.get(prim);
        TypeLayout {
            size: p.size,
            align: p.align,
            niche: p.niche,
            repr: TypeRepr::Primitive(prim),
        }
    }

    fn layout_named(
        &mut self,
        ty: &TypeRef,
        module: &str,
        name: &str,
        args: &[TypeRef],
        indirect: &InstIndirectMap,
    ) -> Result<TypeLayout, LayoutError> {
        let key = (module.to_owned(), name.to_owned());
        if self.static_ind.cyclic_aliases.contains(&key) {
            return Err(LayoutError::CyclicAlias {
                module: module.to_owned(),
                name: name.to_owned(),
            });
        }
        let decl = match self.catalog.classify(module, name, args.len())? {
            NamedKind::Primitive(prim) => return Ok(self.primitive_layout(prim)),
            NamedKind::Decl(decl) => decl,
        };
        if let Some(alias_body) = &decl.alias {
            let subst: Vec<(&str, &TypeRef)> = decl
                .params
                .iter()
                .map(String::as_str)
                .zip(args.iter())
                .collect();
            let expanded = alias_body.substitute(&subst, &mut self.interner);
            return self.compute(&expanded, indirect);
        }
        let is_transparent_unboxed = self.static_ind.transparent_unboxed.contains(&key);
        self.layout_adt(ty, decl, args, is_transparent_unboxed, indirect)
    }

    fn layout_adt(
        &mut self,
        ty: &TypeRef,
        decl: &'p TypeDecl,
        args: &[TypeRef],
        is_unboxed: bool,
        indirect: &InstIndirectMap,
    ) -> Result<TypeLayout, LayoutError> {
        let ctors = instantiate_constructors(decl, args, &mut self.interner);
        let empty = BTreeSet::new();
        let node_indirect = indirect.get(ty).unwrap_or(&empty);
        let ptr_layout = self.primitives.pointer;

        let mut prepared = Vec::with_capacity(ctors.len());
        for inst in ctors {
            let mut fields = Vec::with_capacity(inst.fields.len());
            for (field_idx, field_ty) in inst.fields.into_iter().enumerate() {
                if node_indirect.contains(&(inst.decl_index, field_idx)) {
                    fields.push(PreparedField {
                        index: field_idx,
                        ty: field_ty,
                        storage: FieldStorage::Pointer,
                        size: ptr_layout.size,
                        align: ptr_layout.align,
                        niche: ptr_layout.niche,
                    });
                } else {
                    let field_layout = self.compute(&field_ty, indirect)?;
                    fields.push(PreparedField {
                        index: field_idx,
                        ty: field_ty,
                        storage: FieldStorage::ByValue,
                        size: field_layout.size,
                        align: field_layout.align,
                        niche: field_layout.niche,
                    });
                }
            }
            prepared.push(PreparedVariant {
                name: inst.ctor.name.clone(),
                decl_index: inst.decl_index,
                fields,
                dict_count: inst.ctor.constraints.len(),
            });
        }
        layout_data(prepared, is_unboxed, ptr_layout)
    }

    fn layout_tuple(
        &mut self,
        items: &[TypeRef],
        indirect: &InstIndirectMap,
    ) -> Result<TypeLayout, LayoutError> {
        let mut pack_input = Vec::with_capacity(items.len());
        let mut layouts = Vec::with_capacity(items.len());
        for item in items {
            let l = self.compute(item, indirect)?;
            pack_input.push(PackItem {
                size: l.size,
                align: l.align,
                niche: l.niche,
            });
            layouts.push(l);
        }
        let packed = pack_items(0, 1, &pack_input)?;
        let fields = items
            .iter()
            .enumerate()
            .map(|(i, ty)| FieldLayout {
                index: i,
                offset: packed.offsets[i],
                size: layouts[i].size,
                align: layouts[i].align,
                ty: ty.clone(),
                storage: FieldStorage::ByValue,
            })
            .collect();
        Ok(TypeLayout {
            size: packed.size,
            align: packed.align,
            niche: packed.niche,
            repr: TypeRepr::Tuple(TupleLayout { fields }),
        })
    }

    fn layout_record(
        &mut self,
        raw_fields: &[(String, TypeRef)],
        indirect: &InstIndirectMap,
    ) -> Result<TypeLayout, LayoutError> {
        let mut canonical: Vec<(String, TypeRef)> = raw_fields.to_vec();
        sort_by_utf16(&mut canonical, |(l, _)| l.as_str());

        let mut pack_input = Vec::with_capacity(canonical.len());
        let mut layouts = Vec::with_capacity(canonical.len());
        for (_, field_ty) in &canonical {
            let l = self.compute(field_ty, indirect)?;
            pack_input.push(PackItem {
                size: l.size,
                align: l.align,
                niche: l.niche,
            });
            layouts.push(l);
        }
        let packed = pack_items(0, 1, &pack_input)?;
        let fields = canonical
            .into_iter()
            .enumerate()
            .map(|(i, (label, ty))| RecordFieldLayout {
                label,
                field: FieldLayout {
                    index: i,
                    offset: packed.offsets[i],
                    size: layouts[i].size,
                    align: layouts[i].align,
                    ty,
                    storage: FieldStorage::ByValue,
                },
            })
            .collect();
        Ok(TypeLayout {
            size: packed.size,
            align: packed.align,
            niche: packed.niche,
            repr: TypeRepr::Record(RecordLayout { fields }),
        })
    }
}
