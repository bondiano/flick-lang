use std::cell::Cell;
use std::time::Duration;

use flick_backend::*;

use super::common::{self, descriptor, request, result};

struct TimedMock {
    calls: Cell<usize>,
    unstable: bool,
}

impl Backend for TimedMock {
    fn describe(&self) -> Descriptor {
        descriptor()
    }

    fn check_target(
        &self,
        _request: &BuildRequest,
        _inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        Ok(TargetSupport {
            capabilities: descriptor().capabilities,
        })
    }

    fn generate(
        &self,
        _request: &BuildRequest,
        _inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        let mut out = result();
        if self.unstable && n > 1 {
            out.artifacts[0].bytes.push(1);
        }
        Ok(out)
    }
}

#[test]
fn timed_generation_records_stages_and_detects_unstable_replay() {
    let request = request();
    let policy = Policy {
        authorize_step: &common::authorize,
        authorize_launch: &common::authorize_launch,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    let expected = generate(
        &TimedMock {
            calls: Cell::new(0),
            unstable: false,
        },
        &request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    )
    .expect("stable generation");

    let timed_backend = TimedMock {
        calls: Cell::new(0),
        unstable: false,
    };
    let (timed_generated, timings) = generate_with_timings(
        &timed_backend,
        &request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    )
    .expect("timed stable generation");
    assert_eq!(timed_backend.calls.get(), 2);
    assert_eq!(timed_generated.result(), expected.result());
    assert_eq!(timed_generated.cache_key(), expected.cache_key());
    assert!(
        timings.handshake
            + timings.input_snapshot
            + timings.request_validation
            + timings.check_target
            + timings.generate_first
            + timings.result_validation
            + timings.generate_replay
            + timings.replay_validation
            + timings.cache_key
            > Duration::ZERO
    );

    let unstable_backend = TimedMock {
        calls: Cell::new(0),
        unstable: true,
    };
    assert!(matches!(
        generate_with_timings(
            &unstable_backend,
            &request,
            &policy,
            |_| Some(common::TEXT.as_bytes().to_vec()),
            true,
        ),
        Err(Failure::Unstable)
    ));
}
