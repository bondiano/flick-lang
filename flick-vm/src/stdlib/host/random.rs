//! Host randomness primitive (`Host.Random.next`), preserving deterministic
//! `Random.with_seed` (`IsaacRng`) and `HostMode::Deterministic` (`DRAW`).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher as _, Hasher as _};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::stdlib::hooks::VmHooks;
use crate::stdlib::primitives::IsaacRng;

use super::{DRAW, HostMode};

static ENTROPY_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn next(hooks: &mut dyn VmHooks) -> f64 {
    if let Some(draw) = hooks.next_seeded_random() {
        return draw;
    }
    match hooks.host_mode() {
        HostMode::Deterministic => DRAW,
        HostMode::LiveOs => {
            if let Some(svc) = hooks.host_services() {
                let rng = svc.os_rng.get_or_insert_with(os_seeded_rng);
                rng.next_f64()
            } else {
                os_seeded_rng().next_f64()
            }
        }
    }
}

fn os_seeded_rng() -> IsaacRng {
    let seed_u32 = os_entropy_u32();
    IsaacRng::from_seed(f64::from(seed_u32))
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "mixing 64-bit OS entropy down to 32-bit ISAAC seed"
)]
fn os_entropy_u32() -> u32 {
    #[cfg(unix)]
    {
        use std::fs::File;
        use std::io::Read as _;
        if let Ok(mut f) = File::open("/dev/urandom") {
            let mut buf = [0u8; 4];
            if f.read_exact(&mut buf).is_ok() {
                return u32::from_le_bytes(buf);
            }
        }
    }
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    hasher.write_u128(nanos);
    hasher.write_u32(process::id());
    hasher.write_u64(ENTROPY_COUNTER.fetch_add(1, Ordering::Relaxed));
    let mixed = hasher.finish();
    (mixed ^ (mixed >> 32)) as u32
}

pub(crate) fn random_hex(bytes: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut rng = os_seeded_rng();
    let mut out = String::with_capacity(bytes * 2);
    for _ in 0..bytes {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "scaled [0, 1) draw into byte range"
        )]
        let b = (rng.next_f64() * 256.0) as u8;
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
