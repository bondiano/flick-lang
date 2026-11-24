//! The Go emitter of the official Go backend, linked natively for the
//! compiler's own parity tests and its development adapter.

pub use flick_backend_go::codegen::*;

#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod decide_tests;
#[cfg(test)]
mod direct_tests;
#[cfg(test)]
mod foreign_pkg_tests;
#[cfg(test)]
mod foreign_tests;
#[cfg(test)]
mod loops_tests;
#[cfg(test)]
mod option_tests;
#[cfg(test)]
mod own_tests;
#[cfg(test)]
mod perform_tests;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod scalar_fields_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod typed_tests;
