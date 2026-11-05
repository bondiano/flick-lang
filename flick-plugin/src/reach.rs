//! What a plugin reaches outside its blocks, as the project grants it in
//! `[plugins.<package>]`: a file under the project's root, a variable of
//! the environment, an answer over HTTP. The calls are on [`Compiler`];
//! without a grant they are [`ReachError::Denied`].
//!
//! [`Compiler`]: crate::Compiler

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

/// Why a plugin does not reach what it asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReachError {
    /// The project grants no such thing; the message says what to add.
    Denied(String),
    /// The run is offline and no snapshot has the answer.
    Offline(String),
    /// Reaching it failed.
    Failed(String),
}

impl Display for ReachError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let (ReachError::Denied(m) | ReachError::Offline(m) | ReachError::Failed(m)) = self;
        f.write_str(m)
    }
}

/// A request over HTTP.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The answer to a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Functions between the types of this module and those bindings of
/// `wit/plugin.wit` generate, in a module `reach_convert` where it is used.
#[macro_export]
#[doc(hidden)]
macro_rules! reach_conversions {
    ($($wit:ident)::+) => {
        #[allow(dead_code)]
        mod reach_convert {
            use super::$($wit)::+ as w;

            pub fn to_error(e: $crate::ReachError) -> w::ReachError {
                match e {
                    $crate::ReachError::Denied(m) => w::ReachError::Denied(m),
                    $crate::ReachError::Offline(m) => w::ReachError::Offline(m),
                    $crate::ReachError::Failed(m) => w::ReachError::Failed(m),
                }
            }

            pub fn from_error(e: w::ReachError) -> $crate::ReachError {
                match e {
                    w::ReachError::Denied(m) => $crate::ReachError::Denied(m),
                    w::ReachError::Offline(m) => $crate::ReachError::Offline(m),
                    w::ReachError::Failed(m) => $crate::ReachError::Failed(m),
                }
            }

            pub fn to_request(r: &$crate::Request) -> w::Request {
                w::Request {
                    method: r.method.clone(),
                    url: r.url.clone(),
                    headers: r.headers.clone(),
                    body: r.body.clone(),
                }
            }

            pub fn from_request(r: w::Request) -> $crate::Request {
                $crate::Request { method: r.method, url: r.url, headers: r.headers, body: r.body }
            }

            pub fn to_response(r: $crate::Response) -> w::Response {
                w::Response { status: r.status, body: r.body }
            }

            pub fn from_response(r: w::Response) -> $crate::Response {
                $crate::Response { status: r.status, body: r.body }
            }
        }
    };
}
