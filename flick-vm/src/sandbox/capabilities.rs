//! Capability taxonomy, sets, and static link-time + dynamic checks.
//!
//! Pure computation and console output (`Host.Console`, `PrintLine`,
//! `StdoutWrite`, `StderrLine`) are always permitted. All external host
//! capabilities (`fs`, `net`, `process`, `env`, `clock`, `random`, `ffi`,
//! `plugins`) are denied by [`CapabilitySet::default_untrusted`] unless
//! explicitly granted by the host.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::{error, fmt};

use flick_core::ir::memory::{
    Block, CodeId, Exit, ForeignCallSpec, HostPrimitive, HostServiceIntrinsic, Instruction,
    Intrinsic, MemorySpan, Name, Operation, Program,
};

use super::SandboxError;

/// Individual host capability that a sandboxed VM session may grant or deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// Filesystem access (`File`, `FileSystem`, `Host.File`, `Host.FileSystem`).
    Fs,
    /// Network sockets, listeners, HTTP clients/servers, RPC servers, DNS.
    Net,
    /// Child process spawning and management (`ChildProcess`, `Host.Process`).
    Process,
    /// Process environment variables and CLI arguments (`Host.Environment`, `Host.Arguments`).
    Env,
    /// Real wall/monotonic host clocks (`Host.Clock`).
    Clock,
    /// Host entropy / randomness (`Host.Random`).
    Random,
    /// Foreign function calls and host bridge invocation (`ForeignCall`).
    Ffi,
    /// Compiler/runtime plugin hooks (`Plugin`, `Plugins`).
    Plugins,
}

impl Capability {
    /// All defined capabilities in canonical order.
    pub const ALL: [Self; 8] = [
        Self::Fs,
        Self::Net,
        Self::Process,
        Self::Env,
        Self::Clock,
        Self::Random,
        Self::Ffi,
        Self::Plugins,
    ];

    /// Canonical lowercase CLI/diagnostic name of this capability.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fs => "fs",
            Self::Net => "net",
            Self::Process => "process",
            Self::Env => "env",
            Self::Clock => "clock",
            Self::Random => "random",
            Self::Ffi => "ffi",
            Self::Plugins => "plugins",
        }
    }

    const fn bit(self) -> u16 {
        match self {
            Self::Fs => 1 << 0,
            Self::Net => 1 << 1,
            Self::Process => 1 << 2,
            Self::Env => 1 << 3,
            Self::Clock => 1 << 4,
            Self::Random => 1 << 5,
            Self::Ffi => 1 << 6,
            Self::Plugins => 1 << 7,
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing a [`Capability`] name fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCapabilityError(pub String);

impl fmt::Display for ParseCapabilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown capability `{}` (expected one of: fs, net, process, env, clock, random, ffi, plugins)",
            self.0
        )
    }
}

impl error::Error for ParseCapabilityError {}

impl FromStr for Capability {
    type Err = ParseCapabilityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fs" => Ok(Self::Fs),
            "net" => Ok(Self::Net),
            "process" => Ok(Self::Process),
            "env" => Ok(Self::Env),
            "clock" => Ok(Self::Clock),
            "random" => Ok(Self::Random),
            "ffi" => Ok(Self::Ffi),
            "plugins" => Ok(Self::Plugins),
            other => Err(ParseCapabilityError(other.to_owned())),
        }
    }
}

/// Compact set of granted [`Capability`] values.
///
/// Defaults to [`CapabilitySet::default_untrusted`] (pure computation + console
/// only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CapabilitySet {
    bits: u16,
}

impl CapabilitySet {
    const ALL_MASK: u16 = 0b1111_1111;

    /// Pure preset: allows only pure computation and console output.
    #[must_use]
    pub const fn pure() -> Self {
        Self { bits: 0 }
    }

    /// Default untrusted preset: identical to [`Self::pure`].
    #[must_use]
    pub const fn default_untrusted() -> Self {
        Self::pure()
    }

    /// Unrestricted preset granting every [`Capability`].
    #[must_use]
    pub const fn all() -> Self {
        Self {
            bits: Self::ALL_MASK,
        }
    }

    /// Returns a copy with `cap` granted.
    #[must_use]
    pub const fn allow(mut self, cap: Capability) -> Self {
        self.bits |= cap.bit();
        self
    }

    /// Returns a copy with `cap` denied.
    #[must_use]
    pub const fn deny(mut self, cap: Capability) -> Self {
        self.bits &= !cap.bit();
        self
    }

    /// Grants `cap` in place.
    pub const fn grant(&mut self, cap: Capability) {
        self.bits |= cap.bit();
    }

    /// Revokes `cap` in place.
    pub const fn revoke(&mut self, cap: Capability) {
        self.bits &= !cap.bit();
    }

    /// Returns whether `cap` is currently granted.
    #[must_use]
    pub const fn contains(self, cap: Capability) -> bool {
        (self.bits & cap.bit()) != 0
    }

    /// Returns whether no host capabilities are granted (pure + console only).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Returns whether every capability is granted.
    #[must_use]
    pub const fn is_all(self) -> bool {
        self.bits == Self::ALL_MASK
    }

    /// Returns the union of two capability sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Returns the intersection of two capability sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self {
            bits: self.bits & other.bits,
        }
    }

    /// Iterates over all granted capabilities in canonical order.
    pub fn iter(self) -> impl Iterator<Item = Capability> {
        Capability::ALL
            .into_iter()
            .filter(move |&cap| self.contains(cap))
    }
}

/// Classifies a [`HostPrimitive`] into its required [`Capability`] and binding name.
///
/// Console primitives (`StdoutWrite`, `StderrLine`) return `None` (always allowed).
#[must_use]
pub const fn required_capability_for_host_primitive(
    primitive: HostPrimitive,
) -> Option<(Capability, &'static str)> {
    match primitive {
        HostPrimitive::StdoutWrite | HostPrimitive::StderrLine => None,
        HostPrimitive::Env => Some((Capability::Env, "Host.Environment.env")),
        HostPrimitive::Vars => Some((Capability::Env, "Host.Environment.vars")),
        HostPrimitive::Args => Some((Capability::Env, "Host.Arguments.args")),
        HostPrimitive::CurrentTimeMillis => {
            Some((Capability::Clock, "Host.Clock.current_time_millis"))
        }
        HostPrimitive::CurrentTimeNanos => {
            Some((Capability::Clock, "Host.Clock.current_time_nanos"))
        }
        HostPrimitive::MonotonicTimeNanos => {
            Some((Capability::Clock, "Host.Clock.monotonic_time_nanos"))
        }
        HostPrimitive::RandomNext => Some((Capability::Random, "Host.Random.next")),
        HostPrimitive::ProcessSpawn => Some((Capability::Process, "Host.Process.spawn")),
    }
}

/// Classifies a native [`HostServiceIntrinsic`] into its required [`Capability`]
/// and human-readable binding name.
#[must_use]
pub fn required_capability_for_host_service(
    op: HostServiceIntrinsic,
) -> Option<(Capability, String)> {
    use HostServiceIntrinsic as H;
    match op {
        H::ProcessMake
        | H::ProcessSetCwd
        | H::ProcessSetEnv
        | H::ProcessPrefix
        | H::ProcessPipeTo
        | H::ProcessSetStdin
        | H::ProcessInheritOutput
        | H::ProcessSpawn
        | H::ProcessClose
        | H::ProcessPid
        | H::ProcessWait
        | H::ProcessIsRunning
        | H::ProcessKill
        | H::ProcessKillWith
        | H::ProcessReadStdout
        | H::ProcessReadStderr
        | H::ProcessOutputString
        | H::ProcessOutputLines => Some((Capability::Process, format!("ChildProcess.{op:?}"))),
        H::FileOpen | H::FileClose | H::FileReadHandle | H::FileRead | H::FileWrite => {
            Some((Capability::Fs, format!("File.{op:?}")))
        }
        _ => Some((Capability::Fs, format!("FileSystem.{op:?}"))),
    }
}

/// Classifies an effect/service `Perform { effect, op }` into a required
/// [`Capability`], returning `None` for pure effects and `Host.Console`.
#[must_use]
pub fn required_capability_for_effect(effect: &str, op: &str) -> Option<(Capability, String)> {
    if is_console_effect(effect) {
        return None;
    }
    let cap = classify_effect_name(effect)?;
    Some((cap, format!("{effect}.{op}")))
}

fn is_console_effect(effect: &str) -> bool {
    matches!(effect, "Host.Console" | "Console" | "Stdio.Stdio" | "Stdio")
}

fn classify_effect_name(effect: &str) -> Option<Capability> {
    if matches!(
        effect,
        "File.FileSystem"
            | "FileSystem.FileSystem"
            | "Host.File"
            | "Host.FileSystem"
            | "File"
            | "FileSystem"
            | "Platform.FileSystem"
    ) || effect.starts_with("File.")
        || effect.starts_with("FileSystem.")
        || effect.starts_with("Host.File.")
        || effect.starts_with("Host.FileSystem.")
    {
        return Some(Capability::Fs);
    }
    if matches!(
        effect,
        "Net"
            | "Host.Net"
            | "Http"
            | "Http.Server"
            | "Http.Client"
            | "Http.HttpClient"
            | "Http.Platform.HttpPlatform"
            | "HttpServer"
            | "HttpClient"
            | "HttpPlatform"
            | "Rpc.Server"
            | "Tcp"
            | "Udp"
            | "Socket"
            | "Listener"
            | "Net.Listener"
            | "Net.Tcp"
            | "Net.Udp"
            | "Net.Socket"
            | "Net.Dns"
            | "WebSocket"
    ) || effect.starts_with("Net.")
        || effect.starts_with("Host.Net.")
        || effect.starts_with("Http.")
        || effect.starts_with("Socket.")
        || effect.starts_with("Tcp.")
        || effect.starts_with("Udp.")
        || effect.starts_with("Rpc.Server")
    {
        return Some(Capability::Net);
    }
    if matches!(
        effect,
        "Host.Process" | "Process" | "ChildProcess" | "Platform.CommandExecutor"
    ) || effect.starts_with("Host.Process.")
        || effect.starts_with("ChildProcess.")
    {
        return Some(Capability::Process);
    }
    if matches!(
        effect,
        "Host.Environment" | "Host.Arguments" | "Environment" | "Arguments"
    ) {
        return Some(Capability::Env);
    }
    if matches!(effect, "Host.Clock" | "Clock") {
        return Some(Capability::Clock);
    }
    if matches!(effect, "Host.Random" | "Random") {
        return Some(Capability::Random);
    }
    if matches!(
        effect,
        "Plugin" | "Plugins" | "Host.Plugin" | "Host.Plugins"
    ) || effect.starts_with("Plugin.")
        || effect.starts_with("Plugins.")
        || effect.starts_with("Host.Plugin.")
        || effect.starts_with("Host.Plugins.")
    {
        return Some(Capability::Plugins);
    }
    None
}

/// Classifies a bridged [`ForeignCallSpec`] into its required [`Capability`]
/// and human-readable binding name.
///
/// Domain-specific host modules (e.g. `node:net`, `node:http`, `node:fs`,
/// `node:child_process`) require their specific capability so granting only
/// `net` allows linking a network listener; arbitrary foreign bindings require
/// [`Capability::Ffi`].
#[must_use]
pub fn required_capability_for_foreign_call(spec: &ForeignCallSpec) -> (Capability, String) {
    let binding = if spec.module.is_empty() {
        spec.name.clone()
    } else {
        format!("{}.{}", spec.module, spec.name)
    };
    let from = spec.from.as_deref().unwrap_or("");
    let cap = if is_net_foreign(from, &spec.module, &spec.name) {
        Capability::Net
    } else if is_fs_foreign(from, &spec.module) {
        Capability::Fs
    } else if is_process_foreign(from, &spec.module) {
        Capability::Process
    } else if is_plugin_foreign(from, &spec.module) {
        Capability::Plugins
    } else {
        Capability::Ffi
    };
    (cap, binding)
}

fn is_net_foreign(from: &str, module: &str, name: &str) -> bool {
    matches!(
        from,
        "node:net"
            | "net"
            | "node:http"
            | "http"
            | "node:https"
            | "https"
            | "node:http2"
            | "http2"
            | "node:tls"
            | "tls"
            | "node:dgram"
            | "dgram"
            | "node:dns"
            | "dns"
    ) || from.contains("/http")
        || from.contains("/net")
        || module == "Net"
        || module.starts_with("Net.")
        || module == "Http"
        || module.starts_with("Http.")
        || module == "Rpc.Server"
        || name == "createServer"
        || name == "listen"
}

fn is_fs_foreign(from: &str, module: &str) -> bool {
    matches!(from, "node:fs" | "fs" | "node:fs/promises" | "fs/promises")
        || module == "File"
        || module.starts_with("File.")
        || module == "FileSystem"
        || module.starts_with("FileSystem.")
}

fn is_process_foreign(from: &str, module: &str) -> bool {
    matches!(from, "node:child_process" | "child_process")
        || module == "ChildProcess"
        || module.starts_with("ChildProcess.")
}

fn is_plugin_foreign(from: &str, module: &str) -> bool {
    from.contains("plugin")
        || module == "Plugin"
        || module.starts_with("Plugin.")
        || module == "Plugins"
        || module.starts_with("Plugins.")
}

/// Statically checks every function reachable from `roots` in `program` against
/// `capabilities` before linking or execution.
///
/// When `roots` is `None`, every global in `program` (excluding `Host.live`
/// unless it is the sole global) is checked. Returns the first denied
/// capability along with the binding name and source span from
/// [`Program::op_spans`] (falling back to the user callsite span when crossing
/// into helper functions).
///
/// # Errors
/// Returns [`SandboxError::CapabilityDenied`] if any reachable operation
/// requires a capability not present in `capabilities`.
pub fn check_program_capabilities(
    program: &Program,
    roots: Option<&[Name]>,
    capabilities: CapabilitySet,
) -> Result<(), SandboxError> {
    if capabilities.is_all() {
        return Ok(());
    }

    let global_init: BTreeMap<&Name, CodeId> = program
        .globals
        .iter()
        .map(|g| (&g.name, g.initializer))
        .collect();

    let mut work: Vec<(CodeId, Option<MemorySpan>)> = Vec::new();
    if let Some(names) = roots {
        for name in names {
            if let Some(&code) = global_init.get(name) {
                work.push((code, None));
            }
        }
    } else {
        let skip_host_live = program.globals.len() > 1;
        for global in &program.globals {
            if skip_host_live && global.name.module == "Host" && global.name.name == "live" {
                continue;
            }
            work.push((global.initializer, None));
        }
    }

    let mut visited: BTreeSet<CodeId> = BTreeSet::new();
    while let Some((code, inherited_span)) = work.pop() {
        if !visited.insert(code) {
            continue;
        }
        let Some(func) = program.functions.get(&code) else {
            continue;
        };
        let mut op_idx = 0usize;
        scan_block(
            program,
            &global_init,
            code,
            &func.body,
            inherited_span,
            capabilities,
            &mut op_idx,
            &mut work,
        )?;
    }
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "pre-order block capability walk")]
fn scan_block(
    program: &Program,
    global_init: &BTreeMap<&Name, CodeId>,
    code: CodeId,
    block: &Block,
    inherited_span: Option<MemorySpan>,
    capabilities: CapabilitySet,
    op_idx: &mut usize,
    work: &mut Vec<(CodeId, Option<MemorySpan>)>,
) -> Result<(), SandboxError> {
    for inst in &block.instructions {
        let current_idx = *op_idx;
        *op_idx += 1;
        let Instruction::Let { value, .. } = inst else {
            continue;
        };
        let op_span = program.op_spans.get(&(code, current_idx)).copied();
        let span = op_span.or(inherited_span);
        check_operation(value, span, capabilities)?;
        enqueue_callees(value, global_init, span, work);
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        scan_block(
            program,
            global_init,
            code,
            then_,
            inherited_span,
            capabilities,
            op_idx,
            work,
        )?;
        scan_block(
            program,
            global_init,
            code,
            else_,
            inherited_span,
            capabilities,
            op_idx,
            work,
        )?;
    }
    Ok(())
}

fn check_operation(
    op: &Operation,
    span: Option<MemorySpan>,
    capabilities: CapabilitySet,
) -> Result<(), SandboxError> {
    let required: Option<(Capability, String)> = match op {
        Operation::Intrinsic {
            kind: Intrinsic::Host(prim),
            ..
        } => required_capability_for_host_primitive(*prim).map(|(c, b)| (c, b.to_owned())),
        Operation::Intrinsic {
            kind: Intrinsic::HostService(svc),
            ..
        } => required_capability_for_host_service(*svc),
        Operation::Perform { effect, op, .. } => required_capability_for_effect(effect, op),
        Operation::ForeignCall { spec, .. } => Some(required_capability_for_foreign_call(spec)),
        Operation::Global(name) => required_capability_for_external_global(name),
        _ => None,
    };
    if let Some((capability, binding)) = required
        && !capabilities.contains(capability)
    {
        return Err(SandboxError::CapabilityDenied {
            capability,
            binding,
            span,
        });
    }
    Ok(())
}

/// Checks references to external capability globals even when their defining
/// module is linked separately (e.g. a partial module image calling `Net.listen`
/// or `File.read_file` whose body lives outside this image slice).
fn required_capability_for_external_global(name: &Name) -> Option<(Capability, String)> {
    if name.module == "Host" && name.name == "live" {
        return None;
    }
    if name.module == "Host.Console" || name.module == "Console" {
        return None;
    }
    let cap = classify_effect_name(&name.module)?;
    Some((cap, format!("{}.{}", name.module, name.name)))
}

fn enqueue_callees(
    op: &Operation,
    global_init: &BTreeMap<&Name, CodeId>,
    span: Option<MemorySpan>,
    work: &mut Vec<(CodeId, Option<MemorySpan>)>,
) {
    match op {
        Operation::Closure { code, .. }
        | Operation::Task { code, .. }
        | Operation::Call { code, .. }
        | Operation::Within { code, .. }
        | Operation::WithOwnerEvidence { code, .. }
        | Operation::Loop { code, .. }
        | Operation::FunctionBoundary { code, .. }
        | Operation::GenerateStrict { code, .. }
        | Operation::GenerateLazy { code, .. } => {
            work.push((*code, span));
        }
        Operation::Global(name) => {
            if let Some(&code) = global_init.get(name) {
                work.push((code, span));
            }
        }
        _ => {}
    }
}
