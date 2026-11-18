//! What comparisons with constants establish of one number: an interval,
//! and for `Float` whether it may be NaN. Every ordered comparison with NaN
//! is `false`, so a comparison that gave `false` leaves NaN possible: `not
//! (x <= 0)` does not prove `x > 0`. `Float` compares as IEEE binary64
//! does, so the two zeros are one point and the infinities are values; an
//! `Int` is a 32-bit integer. No arithmetic is read, so wrapping never
//! enters (`docs/formal/refinements.md` §5).

/// An ordered comparison of a value with a constant: `value op bound`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Op {
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    /// The comparison with its operands swapped: `c < x` is `x > c`.
    pub fn flipped(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }

    /// Whether `a op b` holds of two numbers, as the program compares them.
    #[allow(clippy::float_cmp, reason = "IEEE comparison is the semantics")]
    pub fn of(self, a: f64, b: f64) -> bool {
        match self {
            Op::Lt => a < b,
            Op::Le => a <= b,
            Op::Gt => a > b,
            Op::Ge => a >= b,
        }
    }
}

/// The constant of a comparison, of the operand's type.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Bound {
    Int(i32),
    /// A finite binary64, by its bits.
    Float(u64),
}

impl Bound {
    pub fn value(self) -> f64 {
        match self {
            Bound::Int(i) => f64::from(i),
            Bound::Float(bits) => f64::from_bits(bits),
        }
    }
}

/// One side of the number line, from a constant on.
#[derive(Clone, Copy)]
enum Side {
    /// `x > v` when open, `x >= v` when not.
    Above(f64, bool),
    Below(f64, bool),
}

/// The values one number may still be.
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub struct Interval {
    int: bool,
    nan: bool,
    lo: End,
    hi: End,
}

/// An end of an interval, and whether it is open.
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq))]
struct End {
    at: f64,
    open: bool,
}

#[allow(
    clippy::float_cmp,
    reason = "the ends are compared as the program compares them"
)]
impl Interval {
    /// Every value of the type of `bound`.
    pub fn of(bound: Bound) -> Interval {
        match bound {
            Bound::Int(_) => Interval {
                int: true,
                nan: false,
                lo: End {
                    at: f64::from(i32::MIN),
                    open: false,
                },
                hi: End {
                    at: f64::from(i32::MAX),
                    open: false,
                },
            },
            Bound::Float(_) => Interval {
                int: false,
                nan: true,
                lo: End {
                    at: f64::NEG_INFINITY,
                    open: false,
                },
                hi: End {
                    at: f64::INFINITY,
                    open: false,
                },
            },
        }
    }

    /// The values that give `result` to `x op bound`, and whether NaN does.
    fn side(&self, op: Op, bound: Bound, result: bool) -> (Side, bool) {
        let c = bound.value();
        let side = match (op, result) {
            (Op::Gt, true) | (Op::Le, false) => Side::Above(c, true),
            (Op::Ge, true) | (Op::Lt, false) => Side::Above(c, false),
            (Op::Lt, true) | (Op::Ge, false) => Side::Below(c, true),
            (Op::Le, true) | (Op::Gt, false) => Side::Below(c, false),
        };
        // an integer above `c` is at least `c + 1`
        let side = match side {
            Side::Above(c, true) if self.int => Side::Above(c + 1.0, false),
            Side::Below(c, true) if self.int => Side::Below(c - 1.0, false),
            side => side,
        };
        (side, !result)
    }

    /// Learns that `x op bound` gave `result`.
    pub fn learn(&mut self, op: Op, bound: Bound, result: bool) {
        let (side, nan) = self.side(op, bound, result);
        self.nan &= nan;
        match side {
            Side::Above(v, open) => {
                if v > self.lo.at || (v == self.lo.at && open) {
                    self.lo = End { at: v, open };
                }
            }
            Side::Below(v, open) => {
                if v < self.hi.at || (v == self.hi.at && open) {
                    self.hi = End { at: v, open };
                }
            }
        }
    }

    fn empty(&self) -> bool {
        let (lo, hi) = (self.lo, self.hi);
        lo.at > hi.at || (lo.at == hi.at && (lo.open || hi.open))
    }

    /// What `x op bound` gives for every value the range still holds.
    pub fn decide(&self, op: Op, bound: Bound) -> Option<bool> {
        let (side, _) = self.side(op, bound, true);
        if self.empty() {
            // NaN alone compares false; no value at all is a path no run takes
            return Some(!self.nan);
        }
        let (lo, hi) = (self.lo, self.hi);
        let within = match side {
            Side::Above(v, open) => lo.at > v || (lo.at == v && (!open || lo.open)),
            Side::Below(v, open) => hi.at < v || (hi.at == v && (!open || hi.open)),
        };
        let apart = match side {
            Side::Above(v, open) => hi.at < v || (hi.at == v && (open || hi.open)),
            Side::Below(v, open) => lo.at > v || (lo.at == v && (open || lo.open)),
        };
        if within && !self.nan {
            Some(true)
        } else if apart {
            Some(false)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Bound, Interval, Op};

    fn float(x: f64) -> Bound {
        Bound::Float(x.to_bits())
    }

    #[test]
    fn a_false_comparison_leaves_nan_possible() {
        let mut r = Interval::of(float(0.0));
        r.learn(Op::Le, float(0.0), false);
        assert_eq!(r.decide(Op::Gt, float(0.0)), None);
        r.learn(Op::Gt, float(-1.0), true);
        assert_eq!(r.decide(Op::Gt, float(0.0)), Some(true));
    }

    #[test]
    fn bounds_weaken_and_strict_ends_hold() {
        let mut r = Interval::of(float(0.0));
        r.learn(Op::Gt, float(0.0), true);
        r.learn(Op::Lt, float(65536.0), true);
        assert_eq!(r.decide(Op::Gt, float(-5.0)), Some(true));
        assert_eq!(r.decide(Op::Ge, float(0.0)), Some(true));
        assert_eq!(r.decide(Op::Gt, float(0.5)), None);
        assert_eq!(r.decide(Op::Lt, float(65536.0)), Some(true));
        assert_eq!(r.decide(Op::Le, float(0.0)), Some(false));
        assert_eq!(r.decide(Op::Ge, float(65536.0)), Some(false));
    }

    #[test]
    fn the_two_zeros_are_one_point_and_infinity_is_a_value() {
        let mut r = Interval::of(float(0.0));
        r.learn(Op::Gt, float(-0.0), true);
        assert_eq!(r.decide(Op::Gt, float(0.0)), Some(true));
        // above every finite bound is still a value: +infinity
        assert_eq!(r.decide(Op::Lt, float(1e308)), None);
    }

    #[test]
    fn an_integer_above_c_is_at_least_c_plus_one() {
        let mut r = Interval::of(Bound::Int(0));
        r.learn(Op::Gt, Bound::Int(0), true);
        assert_eq!(r.decide(Op::Ge, Bound::Int(1)), Some(true));
        r.learn(Op::Lt, Bound::Int(2), true);
        assert_eq!(r.decide(Op::Le, Bound::Int(1)), Some(true));
        // an `Int` is never NaN
        let mut s = Interval::of(Bound::Int(0));
        s.learn(Op::Le, Bound::Int(0), false);
        assert_eq!(s.decide(Op::Gt, Bound::Int(0)), Some(true));
    }
}
