//! Mutation fuzzer and checked-in malformed corpus for the VM image loader
//! (`verify_image_bytes` / `VmImage::decode`) and tier-1 bytecode verifier
//! (`verify_bytecode`).
//!
//! Mirrors `flick-core/tests/fuzz.rs` and honors `FLICK_FUZZ_ITERATIONS`
//! (default 20 000; supports 100 000+ in release). Every malformed corpus seed
//! and every mutated input must be rejected or accepted cleanly without a panic
//! or out-of-bounds access.

use std::collections::BTreeMap;
use std::num::NonZero;
use std::{env, iter, panic, thread};

use flick_core::ir::memory::{
    Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Name, Operation,
    Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::types::OpClass;
use flick_vm::image::{
    IMAGE_FORMAT_VERSION, IMAGE_MAGIC, VmImage, build_static_image, compiler_hash, memory_ir_hash,
};
use flick_vm::sandbox::bytecode_fuzz::{
    fuzz_mutate_and_verify_bytecode, malformed_bytecode_corpus,
};
use flick_vm::sandbox::{verify_bytecode, verify_image_bytes};
use flick_vm::session::VmSession;
use flick_vm::tier0::HostMode;
use flick_vm::tier1::{self, Prepared};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).expect("fits")
    }
}

fn valid_seed_programs() -> Vec<(Program, Name)> {
    vec![arith_seed_program(), effect_seed_program()]
}

fn arith_seed_program() -> (Program, Name) {
    let mut f1 = BTreeMap::new();
    f1.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Bool(true)),
                }],
                exit: Exit::Branch {
                    condition: 0,
                    then_: Box::new(Block {
                        instructions: vec![
                            Instruction::Let {
                                binding: 1,
                                value: Operation::Literal(Lit::Int(20)),
                            },
                            Instruction::Let {
                                binding: 2,
                                value: Operation::Literal(Lit::Int(22)),
                            },
                            Instruction::Let {
                                binding: 3,
                                value: Operation::Binary {
                                    op: BinOp::Arith(Arith::Add, Num::Int),
                                    left: 1,
                                    right: 2,
                                },
                            },
                        ],
                        exit: Exit::Return(3),
                    }),
                    else_: Box::new(Block {
                        instructions: vec![Instruction::Let {
                            binding: 4,
                            value: Operation::Literal(Lit::Int(0)),
                        }],
                        exit: Exit::Return(4),
                    }),
                },
            },
        },
    );
    (
        Program {
            functions: f1,
            globals: vec![Global {
                name: Name::new("Fuzz.Arith", "main"),
                initializer: 0,
            }],
            op_spans: BTreeMap::new(),
        },
        Name::new("Fuzz.Arith", "main"),
    )
}

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn effect_seed_program() -> (Program, Name) {
    let mut f2 = BTreeMap::new();
    f2.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    let_op(
                        0,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    ),
                    let_op(
                        1,
                        Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    ),
                    let_op(
                        2,
                        Operation::Handle {
                            body: 0,
                            effects: vec![HandlerEffect {
                                effect: "Ask".into(),
                                operations: vec![HandlerOperation {
                                    name: "get".into(),
                                    class: OpClass::Tail,
                                    dicts: 0,
                                    arity: 1,
                                    function: 1,
                                }],
                            }],
                            ret: None,
                            errors: None,
                        },
                    ),
                ],
                exit: Exit::Return(2),
            },
        },
    );
    f2.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    let_op(
                        2,
                        Operation::Perform {
                            effect: "Ask".into(),
                            op: "get".into(),
                            dicts: vec![],
                            args: vec![1],
                        },
                    ),
                    let_op(
                        3,
                        Operation::Construct {
                            tag: "Wrap".into(),
                            fields: vec![2],
                            rep: ValueRep::Cell,
                            layout: None,
                        },
                    ),
                ],
                exit: Exit::Return(3),
            },
        },
    );
    f2.insert(
        2,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Drop(1),
                    let_op(2, Operation::Literal(Lit::String("ok".into()))),
                ],
                exit: Exit::Return(2),
            },
        },
    );
    (
        Program {
            functions: f2,
            globals: vec![Global {
                name: Name::new("Fuzz.Eff", "main"),
                initializer: 0,
            }],
            op_spans: BTreeMap::new(),
        },
        Name::new("Fuzz.Eff", "main"),
    )
}

fn valid_image_seeds() -> Vec<Vec<u8>> {
    let mut images = Vec::new();
    for (prog, entry) in valid_seed_programs() {
        let img = build_static_image(prog.clone(), vec![], Some(entry.clone()), None)
            .expect("static seed image");
        images.push(img.encode().expect("encode static seed"));

        // Also produce a live-session snapshot image with initialized heap values.
        let mut session = VmSession::with_host_mode(HostMode::Deterministic);
        let _ = session.eval_phrase(&prog, &entry, None, false, None);
        if let Ok(live) = session.save_image(Some(entry))
            && let Ok(bytes) = live.encode()
        {
            images.push(bytes);
        }
        let _ = session.shutdown();
    }
    images
}

fn valid_bytecode_seeds() -> Vec<Prepared> {
    valid_seed_programs()
        .into_iter()
        .map(|(prog, entry)| tier1::prepare(&prog, &entry).expect("prepare seed"))
        .collect()
}

/// Builds a synthetic `FLKM` container with valid header hashes and arbitrary
/// section directory + payloads for negative corpus coverage.
fn build_raw_flkm(section_count: u32, reserved: u32, tail: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(80 + tail.len());
    out.extend_from_slice(IMAGE_MAGIC);
    out.extend_from_slice(&IMAGE_FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&compiler_hash());
    out.extend_from_slice(&memory_ir_hash());
    out.extend_from_slice(&section_count.to_le_bytes());
    out.extend_from_slice(&reserved.to_le_bytes());
    out.extend_from_slice(tail);
    out
}

/// Checked-in corpus of malformed `.fvm` images that MUST all be rejected cleanly.
fn malformed_image_corpus() -> Vec<(&'static str, Vec<u8>)> {
    let mut corpus: Vec<(&'static str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("short_magic", b"FLK".to_vec()),
        ("wrong_magic", b"NOPE\x01\x00\x00\x00".to_vec()),
        ("truncated_header_79", vec![b'F'; 79]),
        ("nonzero_reserved", build_raw_flkm(0, 1, &[])),
        ("too_many_sections", build_raw_flkm(999, 0, &[])),
        ("zero_sections_missing_required", build_raw_flkm(0, 0, &[])),
        ("truncated_section_dir", build_raw_flkm(5, 0, &[0; 10])),
    ];

    // Wrong format version with valid magic.
    let mut bad_ver = build_raw_flkm(0, 0, &[]);
    bad_ver[4..8].copy_from_slice(&99_u32.to_le_bytes());
    corpus.push(("unsupported_version", bad_ver));

    // Section pointing inside header.
    let mut inside_hdr = Vec::new();
    inside_hdr.extend_from_slice(&1_u32.to_le_bytes());
    inside_hdr.extend_from_slice(&10_u64.to_le_bytes());
    inside_hdr.extend_from_slice(&4_u64.to_le_bytes());
    corpus.push(("section_inside_header", build_raw_flkm(1, 0, &inside_hdr)));

    // Overlapping sections.
    let mut overlap = Vec::new();
    overlap.extend_from_slice(&1_u32.to_le_bytes());
    overlap.extend_from_slice(&120_u64.to_le_bytes());
    overlap.extend_from_slice(&10_u64.to_le_bytes());
    overlap.extend_from_slice(&2_u32.to_le_bytes());
    overlap.extend_from_slice(&125_u64.to_le_bytes());
    overlap.extend_from_slice(&10_u64.to_le_bytes());
    overlap.extend_from_slice(&[0u8; 32]);
    corpus.push(("overlapping_sections", build_raw_flkm(2, 0, &overlap)));

    // Duplicate section tag.
    let mut dup_tag = Vec::new();
    dup_tag.extend_from_slice(&1_u32.to_le_bytes());
    dup_tag.extend_from_slice(&120_u64.to_le_bytes());
    dup_tag.extend_from_slice(&2_u64.to_le_bytes());
    dup_tag.extend_from_slice(&1_u32.to_le_bytes());
    dup_tag.extend_from_slice(&122_u64.to_le_bytes());
    dup_tag.extend_from_slice(&2_u64.to_le_bytes());
    dup_tag.extend_from_slice(&[0u8; 8]);
    corpus.push(("duplicate_section_tag", build_raw_flkm(2, 0, &dup_tag)));

    // Unknown section tag 99.
    let mut unknown_tag = Vec::new();
    unknown_tag.extend_from_slice(&99_u32.to_le_bytes());
    unknown_tag.extend_from_slice(&100_u64.to_le_bytes());
    unknown_tag.extend_from_slice(&1_u64.to_le_bytes());
    unknown_tag.push(0);
    corpus.push(("unknown_section_tag", build_raw_flkm(1, 0, &unknown_tag)));

    // Offset + length u64 overflow.
    let mut overflow = Vec::new();
    overflow.extend_from_slice(&1_u32.to_le_bytes());
    overflow.extend_from_slice(&u64::MAX.to_le_bytes());
    overflow.extend_from_slice(&16_u64.to_le_bytes());
    corpus.push(("section_range_overflow", build_raw_flkm(1, 0, &overflow)));

    // Deeply nested CBOR array bomb inside manifest section (tag 1).
    let mut cbor_bomb_payload = vec![0x81_u8; 200];
    cbor_bomb_payload.push(0x00);
    let mut cbor_bomb_tail = Vec::new();
    let dir_end: u64 = 80 + 5 * 20;
    let payload_len = cbor_bomb_payload.len() as u64;
    for tag in [1_u32, 2, 3, 5, 6] {
        let off = dir_end + u64::from(tag - 1) * payload_len;
        cbor_bomb_tail.extend_from_slice(&tag.to_le_bytes());
        cbor_bomb_tail.extend_from_slice(&off.to_le_bytes());
        cbor_bomb_tail.extend_from_slice(&payload_len.to_le_bytes());
    }
    for _ in 0..6 {
        cbor_bomb_tail.extend_from_slice(&cbor_bomb_payload);
    }
    corpus.push(("cbor_nesting_bomb", build_raw_flkm(5, 0, &cbor_bomb_tail)));

    // Mutated valid image seeds: truncated payload, flipped section payload bytes, trailing bytes.
    if let Some(valid) = valid_image_seeds().into_iter().next() {
        let mut trunc = valid.clone();
        trunc.truncate(valid.len() - 5);
        corpus.push(("truncated_valid_image", trunc));

        let mut flipped = valid.clone();
        let mid = 80 + (flipped.len() - 80) / 2;
        flipped[mid] ^= 0xff;
        corpus.push(("flipped_payload_byte", flipped));

        let mut bad_hash = valid;
        bad_hash[12] ^= 0x55;
        corpus.push(("compiler_hash_mismatch", bad_hash));
    }

    corpus
}

fn mutate_image_bytes(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut out = seed.to_vec();
    // Preserve the 72-byte header hashes ~75% of the time so mutations deeply
    // exercise section framing, CBOR decoding, memory-IR, heap, and bytecode checks.
    let preserve_header = out.len() > 80 && rng.below(4) != 0;
    let min_pos = if preserve_header { 72 } else { 0 };
    let ops = 1 + rng.below(5);
    for _ in 0..ops {
        if out.len() <= min_pos {
            break;
        }
        let span = out.len() - min_pos;
        match rng.below(6) {
            0 => {
                // Bit flip.
                let pos = min_pos + rng.below(span);
                out[pos] ^= 1_u8 << (rng.next() & 7);
            }
            1 => {
                // Byte overwrite with boundary values (0x00, 0x7f, 0x80, 0x81, 0x9f, 0xbf, 0xff).
                const INTERESTING: [u8; 8] = [0x00, 0x01, 0x7f, 0x80, 0x81, 0x9f, 0xbf, 0xff];
                let pos = min_pos + rng.below(span);
                out[pos] = INTERESTING[rng.below(INTERESTING.len())];
            }
            2 => {
                // Insert a small slice (including nested CBOR prefixes).
                let pos = min_pos + rng.below(span + 1);
                let len = 1 + rng.below(16);
                let byte = (rng.next() & 0xff) as u8;
                out.splice(pos..pos, iter::repeat_n(byte, len));
            }
            3 => {
                // Delete a small slice.
                let pos = min_pos + rng.below(span);
                let max_del = (out.len() - pos).min(32);
                let del = 1 + rng.below(max_del);
                out.drain(pos..pos + del);
            }
            4 => {
                // Overwrite a 4-byte or 8-byte integer field.
                if out.len() >= min_pos + 8 {
                    let pos = min_pos + rng.below(out.len() - min_pos - 7);
                    let val = rng.next();
                    out[pos..pos + 8].copy_from_slice(&val.to_le_bytes());
                }
            }
            _ => {
                // Truncate at an arbitrary point past min_pos.
                let new_len = min_pos + rng.below(span);
                out.truncate(new_len);
            }
        }
    }
    out
}

#[test]
fn malformed_image_and_bytecode_corpus_rejected() {
    for (name, bytes) in malformed_image_corpus() {
        let res = panic::catch_unwind(|| verify_image_bytes(&bytes))
            .unwrap_or_else(|_| panic!("verify_image_bytes panicked on corpus seed `{name}`"));
        assert!(
            res.is_err(),
            "malformed image corpus seed `{name}` unexpectedly accepted"
        );
    }
    for (name, bad_bytecode) in malformed_bytecode_corpus() {
        let res = panic::catch_unwind(|| verify_bytecode(&bad_bytecode))
            .unwrap_or_else(|_| panic!("verify_bytecode panicked on corpus seed `{name}`"));
        assert!(
            res.is_err(),
            "malformed bytecode corpus seed `{name}` unexpectedly accepted"
        );
    }
}

#[test]
fn vm_image_and_bytecode_fuzzer() {
    let iterations: usize = env::var("FLICK_FUZZ_ITERATIONS")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(20_000);

    let image_seeds = valid_image_seeds();
    let bytecode_seeds = valid_bytecode_seeds();
    assert!(!image_seeds.is_empty());
    assert!(!bytecode_seeds.is_empty());

    for seed in &image_seeds {
        verify_image_bytes(seed).expect("valid image seed verifies");
    }
    for seed in &bytecode_seeds {
        verify_bytecode(seed).expect("valid bytecode seed verifies");
    }

    let workers = thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(iterations.max(1));

    thread::scope(|scope| {
        let image_seeds = &image_seeds;
        let bytecode_seeds = &bytecode_seeds;
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                thread::Builder::new()
                    .stack_size(8 << 20)
                    .spawn_scoped(scope, move || {
                        fuzz_worker(image_seeds, bytecode_seeds, iterations, worker, workers);
                    })
                    .expect("spawn fuzz worker")
            })
            .collect();
        for handle in handles {
            handle.join().expect("fuzz worker completed without panic");
        }
    });
}

fn fuzz_worker(
    image_seeds: &[Vec<u8>],
    bytecode_seeds: &[Prepared],
    iterations: usize,
    worker: usize,
    workers: usize,
) {
    let base_seed = 0x5eed_f11c_0992_0001_u64;
    let mut rng = Rng(base_seed.wrapping_add((worker as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)));

    for i in (worker..iterations).step_by(workers) {
        if i % 2 == 0 {
            let seed = &image_seeds[(i / 2) % image_seeds.len()];
            let mutated = mutate_image_bytes(&mut rng, seed);
            panic::catch_unwind(|| {
                let _ = VmImage::decode(&mutated);
                let _ = verify_image_bytes(&mutated);
            })
            .unwrap_or_else(|_| panic!("image decoder/verifier panicked on iteration {i}"));
        } else {
            let seed = &bytecode_seeds[(i / 2) % bytecode_seeds.len()];
            let mut_seed = rng.next();
            panic::catch_unwind(|| {
                let _ = fuzz_mutate_and_verify_bytecode(seed, mut_seed);
            })
            .unwrap_or_else(|_| panic!("bytecode verifier panicked on iteration {i}"));
        }
    }
}
