//! The logic against an evaluator of its own: random formulas over a few
//! numbers and calls, random facts, and random values the facts allow. A
//! value is NaN, an infinity, a signed zero, a bound or near one; a call
//! may give either result or never return. Whatever `holds` proves of a
//! formula, the formula gives when it runs, and whatever it refutes, too.

use std::rc::Rc;

use super::interval::{Bound, Op};
use super::logic::{Atomic, Formula, Knowledge};

/// An atom of the test: `x_i op c`, or the call `f_i` of `x_0`, which the
/// world gives a result or none.
#[derive(Clone, Copy, PartialEq, Debug)]
enum T {
    Compare(usize, Op, Bound),
    Call(usize),
}

impl Atomic for T {
    type Subject = usize;

    fn comparison(&self) -> Option<(&usize, Op, Bound)> {
        match self {
            T::Compare(i, op, bound) => Some((i, *op, *bound)),
            T::Call(_) => None,
        }
    }

    fn constant(&self) -> Option<bool> {
        None
    }
}

/// xorshift: the same cases on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("small")
    }
}

const FLOATS: [f64; 9] = [-1.0, -0.0, 0.0, 0.5, 1.0, 65535.5, 65536.0, 1e308, -1e308];
const INTS: [i32; 6] = [i32::MIN, -1, 0, 1, 65536, i32::MAX];
const OPS: [Op; 4] = [Op::Lt, Op::Le, Op::Gt, Op::Ge];

/// The world: `x_0`, `x_1` floats, `x_2` an int; each call's result.
struct World {
    xs: [f64; 3],
    calls: [Option<bool>; 2],
}

impl World {
    fn random(rng: &mut Rng) -> World {
        let special = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        let float = |rng: &mut Rng| match rng.below(4) {
            0 => special[rng.below(3)],
            _ => FLOATS[rng.below(FLOATS.len())] + [0.0, 1e-9, -1e-9][rng.below(3)],
        };
        let int = |rng: &mut Rng| {
            let base = i64::from(INTS[rng.below(INTS.len())]);
            let near = base + [0, 1, -1][rng.below(3)];
            f64::from(i32::try_from(near).unwrap_or(i32::try_from(base).expect("an int")))
        };
        let call = |rng: &mut Rng| [Some(true), Some(false), None][rng.below(3)];
        World {
            xs: [float(rng), float(rng), int(rng)],
            calls: [call(rng), call(rng)],
        }
    }

    /// What `formula` gives when it runs here; none when a call it runs
    /// never returns.
    fn eval(&self, formula: &Formula<T>) -> Option<bool> {
        match formula {
            Formula::Const(b) => Some(*b),
            Formula::Atom(T::Compare(i, op, bound)) => Some(op.of(self.xs[*i], bound.value())),
            Formula::Atom(T::Call(i)) => self.calls[*i],
            Formula::Not(x) => self.eval(x).map(|b| !b),
            Formula::And(x, y) => {
                if self.eval(x)? {
                    self.eval(y)
                } else {
                    Some(false)
                }
            }
            Formula::Or(x, y) => {
                if self.eval(x)? {
                    Some(true)
                } else {
                    self.eval(y)
                }
            }
            Formula::Opaque => None,
        }
    }
}

fn atom(rng: &mut Rng) -> T {
    if rng.below(5) == 0 {
        return T::Call(rng.below(2));
    }
    let i = rng.below(3);
    let op = OPS[rng.below(4)];
    let bound = if i == 2 {
        Bound::Int(INTS[rng.below(INTS.len())])
    } else {
        Bound::Float(FLOATS[rng.below(FLOATS.len())].to_bits())
    };
    T::Compare(i, op, bound)
}

fn formula(rng: &mut Rng, depth: u32) -> Formula<T> {
    if depth == 0 || rng.below(3) == 0 {
        return match rng.below(10) {
            0 => Formula::Const(rng.below(2) == 0),
            1 => Formula::Opaque,
            _ => Formula::Atom(atom(rng)),
        };
    }
    let (x, y) = (formula(rng, depth - 1), formula(rng, depth - 1));
    match rng.below(3) {
        0 => Formula::Not(Box::new(x)),
        1 => Formula::And(Box::new(x), Box::new(y)),
        _ => Formula::Or(Box::new(x), Box::new(y)),
    }
}

#[test]
fn what_the_logic_proves_holds_in_every_world_its_facts_allow() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let (mut proved, mut checked) = (0, 0);
    for _ in 0..3000 {
        let facts: Vec<_> = (0..rng.below(4))
            .map(|_| (Rc::new(formula(&mut rng, 3)), rng.below(2) == 0))
            .collect();
        let knowledge = Knowledge::of(&facts, &|_| None);
        let goal = formula(&mut rng, 3);
        let Some(result) = knowledge.holds(&goal) else {
            continue;
        };
        proved += 1;
        // the worlds where every fact ran to its result
        for _ in 0..200 {
            let world = World::random(&mut rng);
            if facts.iter().all(|(f, b)| world.eval(f) == Some(*b)) {
                checked += 1;
                assert_eq!(world.eval(&goal), Some(result), "{goal:?} under {facts:?}");
            }
        }
    }
    // the test reaches proofs and worlds, not only unknowns
    assert!(
        proved > 300 && checked > 3000,
        "{proved} proofs, {checked} worlds"
    );
}
