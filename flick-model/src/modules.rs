//! Executable static module boundary model, paired with `HigherModules.lean`.
//! Code references are opaque: evaluating a module expression never executes
//! an exported operation or a layer. Source type/equality checking remains in
//! the compiler; this model isolates closure scope, grouping and signature shape.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Identity {
    Abstract,
    Manifest(String),
}

impl Identity {
    fn accepts(&self, actual: &Self) -> bool {
        matches!(self, Self::Abstract) || self == actual
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signature {
    Structure {
        members: BTreeSet<String>,
        identities: BTreeMap<String, Identity>,
    },
    Arrow {
        group_size: usize,
        parameter: Box<Self>,
        result: Box<Self>,
    },
}

impl Signature {
    /// A provider accepts at least the advertised inputs and supplies at
    /// least the advertised outputs. Atomic and curried groups stay distinct.
    pub fn is_subtype_of(&self, expected: &Self) -> bool {
        match (self, expected) {
            (
                Self::Structure {
                    members: actual,
                    identities,
                },
                Self::Structure {
                    members,
                    identities: required,
                },
            ) => {
                members.is_subset(actual)
                    && required.iter().all(|(name, identity)| {
                        identities
                            .get(name)
                            .is_some_and(|actual| identity.accepts(actual))
                    })
            }
            (
                Self::Arrow {
                    group_size: a,
                    parameter: ap,
                    result: ar,
                },
                Self::Arrow {
                    group_size: b,
                    parameter: bp,
                    result: br,
                },
            ) => a == b && bp.is_subtype_of(ap) && ar.is_subtype_of(br),
            _ => false,
        }
    }
}

/// An operation/layer body identifier, not the operation's result.
pub type Code = u32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Structure(Vec<(String, Code)>),
    Variable(usize),
    Parameterized {
        arity: usize,
        body: Box<Self>,
    },
    Apply {
        function: Box<Self>,
        arguments: Vec<Self>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Structure(Vec<(String, Code)>),
    Closure {
        arity: usize,
        captured: Vec<Self>,
        body: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Fuel,
    Unbound(usize),
    NotParameterizedModule,
    GroupSize { expected: usize, actual: usize },
}

pub fn evaluate(expr: &Expr, environment: &[Value], fuel: usize) -> Result<Value, Error> {
    let fuel = fuel.checked_sub(1).ok_or(Error::Fuel)?;
    match expr {
        Expr::Structure(code) => Ok(Value::Structure(code.clone())),
        Expr::Variable(index) => environment
            .get(*index)
            .cloned()
            .ok_or(Error::Unbound(*index)),
        Expr::Parameterized { arity, body } => Ok(Value::Closure {
            arity: *arity,
            captured: environment.to_vec(),
            body: body.clone(),
        }),
        Expr::Apply {
            function,
            arguments,
        } => {
            let Value::Closure {
                arity,
                captured,
                body,
            } = evaluate(function, environment, fuel)?
            else {
                return Err(Error::NotParameterizedModule);
            };
            if arity != arguments.len() {
                return Err(Error::GroupSize {
                    expected: arity,
                    actual: arguments.len(),
                });
            }
            let mut scope: Vec<_> = arguments
                .iter()
                .map(|arg| evaluate(arg, environment, fuel))
                .collect::<Result<_, _>>()?;
            scope.reverse();
            scope.extend(captured);
            evaluate(&body, &scope, fuel)
        }
    }
}

pub fn project(visible: &BTreeSet<String>, exports: &[(String, Code)]) -> Vec<(String, Code)> {
    exports
        .iter()
        .filter(|(name, _)| visible.contains(name))
        .cloned()
        .collect()
}

/// Scope lists run from innermost to outermost. An explicit given wins;
/// otherwise the first local dictionary wins over the global dictionary.
pub fn dictionary<'a, D>(
    key: &str,
    given: Option<&'a D>,
    locals: &'a [BTreeMap<String, D>],
    global: &'a BTreeMap<String, D>,
) -> Option<&'a D> {
    given
        .or_else(|| locals.iter().find_map(|scope| scope.get(key)))
        .or_else(|| global.get(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(code: Code) -> Expr {
        Expr::Structure(vec![("layer".into(), code)])
    }

    #[test]
    fn a_returned_factory_uses_its_definition_environment() {
        // (parameterized module A -> parameterized module B -> A)(repo) returns a closure over repo.
        let return_first = Expr::Parameterized {
            arity: 1,
            body: Box::new(Expr::Parameterized {
                arity: 1,
                body: Box::new(Expr::Variable(1)),
            }),
        };
        let selected = evaluate(
            &Expr::Apply {
                function: Box::new(return_first),
                arguments: vec![module(41)],
            },
            &[],
            10,
        )
        .expect("static closure");
        // The caller has an unrelated module in the same de Bruijn position.
        let caller = [selected, Value::Structure(vec![("layer".into(), 99)])];
        let value = evaluate(
            &Expr::Apply {
                function: Box::new(Expr::Variable(0)),
                arguments: vec![module(5)],
            },
            &caller,
            10,
        )
        .expect("applied closure");
        assert_eq!(value, Value::Structure(vec![("layer".into(), 41)]));
        // 41 still denotes code; it was never run to produce a connection.
        assert_eq!(caller[1], Value::Structure(vec![("layer".into(), 99)]));
    }

    #[test]
    fn an_atomic_group_does_not_partially_apply() {
        let factory = Expr::Parameterized {
            arity: 2,
            body: Box::new(Expr::Variable(1)),
        };
        assert_eq!(
            evaluate(
                &Expr::Apply {
                    function: Box::new(factory.clone()),
                    arguments: vec![module(1)]
                },
                &[],
                10
            ),
            Err(Error::GroupSize {
                expected: 2,
                actual: 1
            })
        );
        assert_eq!(
            evaluate(
                &Expr::Apply {
                    function: Box::new(factory),
                    arguments: vec![module(1), module(2)]
                },
                &[],
                10
            ),
            Ok(Value::Structure(vec![("layer".into(), 1)]))
        );
    }

    #[test]
    fn a_signature_projects_code_without_evaluating_hidden_members() {
        let code = vec![
            ("get".into(), 1),
            ("layer".into(), 2),
            ("private".into(), 3),
        ];
        let visible = BTreeSet::from(["get".into(), "layer".into()]);
        assert_eq!(project(&visible, &code), code[..2]);
        assert_eq!(code[2], ("private".into(), 3));
    }

    #[test]
    fn dictionaries_remain_local_and_explicit() {
        let outer = BTreeMap::from([("Core.Eq/String".into(), 1)]);
        let inner = BTreeMap::from([("Core.Eq/String".into(), 2)]);
        let global = BTreeMap::from([("Core.Eq/String".into(), 3)]);
        assert_eq!(
            dictionary(
                "Core.Eq/String",
                Some(&4),
                &[inner.clone(), outer.clone()],
                &global
            ),
            Some(&4)
        );
        assert_eq!(
            dictionary("Core.Eq/String", None, &[inner, outer.clone()], &global),
            Some(&2)
        );
        assert_eq!(
            dictionary("Core.Eq/String", None, &[outer], &global),
            Some(&1)
        );
        assert_eq!(dictionary("Other.Eq/String", None, &[], &global), None);
        assert_eq!(dictionary("Core.Eq/String", None, &[], &global), Some(&3));
    }

    fn shape(names: &[&str]) -> Signature {
        Signature::Structure {
            members: names.iter().map(|n| (*n).into()).collect(),
            identities: BTreeMap::new(),
        }
    }

    fn arrow(group_size: usize, parameter: Signature, result: Signature) -> Signature {
        Signature::Arrow {
            group_size,
            parameter: Box::new(parameter),
            result: Box::new(result),
        }
    }

    #[test]
    fn higher_order_contracts_preserve_variance_groups_and_nominal_identity() {
        let expected = arrow(1, shape(&["seed"]), shape(&["answer"]));
        let provider = arrow(1, shape(&[]), shape(&["answer", "extra"]));
        assert!(provider.is_subtype_of(&expected));
        assert!(!expected.is_subtype_of(&provider));
        assert!(!arrow(2, shape(&["seed"]), shape(&["answer"])).is_subtype_of(&expected));
        let nominal = |path: &str| Signature::Structure {
            members: BTreeSet::from(["now".into()]),
            identities: BTreeMap::from([("Clock".into(), Identity::Manifest(path.into()))]),
        };
        assert!(nominal("Core.Clock").is_subtype_of(&nominal("Core.Clock")));
        assert!(!nominal("Other.Clock").is_subtype_of(&nominal("Core.Clock")));
    }
}
