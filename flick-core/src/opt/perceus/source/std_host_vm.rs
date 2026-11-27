//! Native Rust VM lowering for std host services (`File`, `FileSystem`,
//! `ChildProcess`, and `Host.Process.spawn`).
//!
//! Only active in closed-world VM lowering (`!self.allow_foreign_bridge`);
//! `HostBridge`-enabled model executions keep routing these bindings through
//! the single-owner Node/Effect-TS bridge unchanged.

use flick_ir::memory::HostServiceIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::rt;
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_host_vm_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if self.allow_foreign_bridge || value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$hvm");
        let expr = match (from, bind, value.mode, stages) {
            ("flick:rt", "fileOpen", ForeignMode::Effect, 1) => {
                scoped_1(&b, "HostService.FileOpen", "HostService.FileClose")
            }
            ("flick:rt", "fileReadHandle", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.FileReadHandle")
            }
            ("flick:rt", "fileRead", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.FileRead")
            }
            ("flick:rt", "fileWrite", ForeignMode::Effect, 2) => {
                fallible_2(&b, "HostService.FileWrite")
            }
            ("flick:rt", "processSpawn", ForeignMode::Effect, 1) => {
                scoped_1(&b, "HostService.ProcessSpawn", "HostService.ProcessClose")
            }
            ("flick:FileSystem", "flickNode", ForeignMode::Pure, 1) => fs_layer_template(&b),
            ("flick:FileSystem", "flickFileStat", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.FsFileStat")
            }
            ("flick:FileSystem", "flickSeek", ForeignMode::Effect, 3) => {
                fallible_3(&b, "HostService.FsSeek")
            }
            ("flick:FileSystem", "flickSync", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.FsSync")
            }
            ("flick:FileSystem", "flickReadAlloc", ForeignMode::Effect, 3) => {
                fallible_3(&b, "HostService.FsReadAlloc")
            }
            ("flick:FileSystem", "flickFileTruncate", ForeignMode::Effect, 2) => {
                fallible_2(&b, "HostService.FsFileTruncate")
            }
            ("flick:FileSystem", "flickWrite", ForeignMode::Effect, 2) => {
                fallible_2(&b, "HostService.FsWrite")
            }
            ("flick:FileSystem", "flickWriteAll", ForeignMode::Effect, 2) => {
                fallible_2(&b, "HostService.FsWriteAll")
            }
            ("flick:ChildProcess", "flickMake", ForeignMode::Pure, 2) => {
                rt(&b, "HostService.ProcessMake")
            }
            ("flick:ChildProcess", "setCwd", ForeignMode::Pure, 2) => {
                rt(&b, "HostService.ProcessSetCwd")
            }
            ("flick:ChildProcess", "flickSetEnv", ForeignMode::Pure, 2) => {
                rt(&b, "HostService.ProcessSetEnv")
            }
            ("flick:ChildProcess", "flickPrefix", ForeignMode::Pure, 3) => {
                rt(&b, "HostService.ProcessPrefix")
            }
            ("flick:ChildProcess", "pipeTo", ForeignMode::Pure, 2) => {
                rt(&b, "HostService.ProcessPipeTo")
            }
            ("flick:ChildProcess", "flickSetStdin", ForeignMode::Pure, 2) => {
                rt(&b, "HostService.ProcessSetStdin")
            }
            ("flick:ChildProcess", "flickInheritOutput", ForeignMode::Pure, 1) => {
                rt(&b, "HostService.ProcessInheritOutput")
            }
            ("flick:ChildProcess", "flickPid", ForeignMode::Pure, 1) => {
                rt(&b, "HostService.ProcessPid")
            }
            ("flick:ChildProcess", "flickWait", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.ProcessWait")
            }
            ("flick:ChildProcess", "flickIsRunning", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.ProcessIsRunning")
            }
            ("flick:ChildProcess", "flickKill", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.ProcessKill")
            }
            ("flick:ChildProcess", "flickKillWith", ForeignMode::Effect, 2) => {
                fallible_2(&b, "HostService.ProcessKillWith")
            }
            ("flick:ChildProcess", "flickStdout", ForeignMode::Pure, 1) => {
                process_stream_template(&b, "HostService.ProcessReadStdout")
            }
            ("flick:ChildProcess", "flickStderr", ForeignMode::Pure, 1) => {
                process_stream_template(&b, "HostService.ProcessReadStderr")
            }
            ("flick:ChildProcess", "flickOutputString", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.ProcessOutputString")
            }
            ("flick:ChildProcess", "flickOutputLines", ForeignMode::Effect, 1) => {
                fallible_1(&b, "HostService.ProcessOutputLines")
            }
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn unwrap_result(b: &ExprBuilder, call: Expr) -> Expr {
    b.app(rt(b, "Prelude.FromResult"), call)
}

fn fallible_1(b: &ExprBuilder, op: &str) -> Expr {
    b.lam(&["a"], unwrap_result(b, b.app(rt(b, op), b.loc("a"))))
}

fn fallible_2(b: &ExprBuilder, op: &str) -> Expr {
    b.lam(
        &["a", "c"],
        unwrap_result(b, b.app2(rt(b, op), b.loc("a"), b.loc("c"))),
    )
}

fn fallible_3(b: &ExprBuilder, op: &str) -> Expr {
    b.lam(
        &["a", "c", "d"],
        unwrap_result(b, b.app3(rt(b, op), b.loc("a"), b.loc("c"), b.loc("d"))),
    )
}

fn scoped_1(b: &ExprBuilder, acquire_op: &str, release_op: &str) -> Expr {
    let acquire = b.task(unwrap_result(b, b.app(rt(b, acquire_op), b.loc("a"))));
    let release = b.lam(&["h"], b.app(rt(b, release_op), b.loc("h")));
    b.lam(&["a"], b.app2(rt(b, "AcquireRelease"), acquire, release))
}

fn scoped_2(b: &ExprBuilder, acquire_op: &str, release_op: &str) -> Expr {
    let acquire = b.task(unwrap_result(
        b,
        b.app2(rt(b, acquire_op), b.loc("a"), b.loc("c")),
    ));
    let release = b.lam(&["h"], b.app(rt(b, release_op), b.loc("h")));
    b.lam(
        &["a", "c"],
        b.app2(rt(b, "AcquireRelease"), acquire, release),
    )
}

fn process_stream_template(b: &ExprBuilder, read_op: &str) -> Expr {
    let next = unwrap_result(b, b.app(rt(b, read_op), b.loc("proc")));
    let pull = b.lam(
        &["_u"],
        b.match_(
            next,
            vec![
                arm(b.some_pat("chunk"), b.ok(b.list([b.loc("chunk")]))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    b.lam(&["proc", "_up", "_own"], pull)
}

fn fs_layer_template(b: &ExprBuilder) -> Expr {
    let ops = b.at(ExprKind::Record(vec![
        ("access".into(), fallible_2(b, "HostService.FsAccess")),
        ("chmod".into(), fallible_2(b, "HostService.FsChmod")),
        ("chown".into(), fallible_3(b, "HostService.FsChown")),
        ("copy".into(), fallible_3(b, "HostService.FsCopy")),
        ("copy_file".into(), fallible_2(b, "HostService.FsCopyFile")),
        ("exists".into(), fallible_1(b, "HostService.FsExists")),
        ("glob".into(), fallible_2(b, "HostService.FsGlob")),
        ("link".into(), fallible_2(b, "HostService.FsLink")),
        (
            "make_directory".into(),
            fallible_2(b, "HostService.FsMakeDirectory"),
        ),
        (
            "make_temp_directory".into(),
            fallible_1(b, "HostService.FsMakeTempDirectory"),
        ),
        (
            "make_temp_directory_scoped".into(),
            scoped_1(
                b,
                "HostService.FsMakeTempDirectoryScoped",
                "HostService.FsRemoveTemp",
            ),
        ),
        (
            "make_temp_file".into(),
            fallible_1(b, "HostService.FsMakeTempFile"),
        ),
        (
            "make_temp_file_scoped".into(),
            scoped_1(
                b,
                "HostService.FsMakeTempFileScoped",
                "HostService.FsRemoveTempParent",
            ),
        ),
        (
            "open".into(),
            scoped_2(b, "HostService.FsOpen", "HostService.FsClose"),
        ),
        (
            "read_directory".into(),
            fallible_2(b, "HostService.FsReadDirectory"),
        ),
        ("read_file".into(), fallible_1(b, "HostService.FsReadFile")),
        (
            "read_file_string".into(),
            fallible_1(b, "HostService.FsReadFileString"),
        ),
        ("read_link".into(), fallible_1(b, "HostService.FsReadLink")),
        ("real_path".into(), fallible_1(b, "HostService.FsRealPath")),
        ("remove".into(), fallible_2(b, "HostService.FsRemove")),
        ("rename".into(), fallible_2(b, "HostService.FsRename")),
        ("stat".into(), fallible_1(b, "HostService.FsStat")),
        ("symlink".into(), fallible_2(b, "HostService.FsSymlink")),
        ("truncate".into(), fallible_2(b, "HostService.FsTruncate")),
        ("utimes".into(), fallible_3(b, "HostService.FsUtimes")),
        (
            "write_file".into(),
            fallible_3(b, "HostService.FsWriteFile"),
        ),
        (
            "write_file_string".into(),
            fallible_3(b, "HostService.FsWriteFileString"),
        ),
    ]));
    b.lam(&["tag"], b.app2(rt(b, "Layer.Succeed"), b.loc("tag"), ops))
}

pub(super) fn host_service_intrinsic_by_name(name: &str) -> Option<HostServiceIntrinsic> {
    use HostServiceIntrinsic as H;
    Some(match name {
        "FileOpen" => H::FileOpen,
        "FileClose" => H::FileClose,
        "FileReadHandle" => H::FileReadHandle,
        "FileRead" => H::FileRead,
        "FileWrite" => H::FileWrite,
        "FsAccess" => H::FsAccess,
        "FsCopy" => H::FsCopy,
        "FsCopyFile" => H::FsCopyFile,
        "FsChmod" => H::FsChmod,
        "FsChown" => H::FsChown,
        "FsGlob" => H::FsGlob,
        "FsExists" => H::FsExists,
        "FsLink" => H::FsLink,
        "FsMakeDirectory" => H::FsMakeDirectory,
        "FsMakeTempDirectory" => H::FsMakeTempDirectory,
        "FsMakeTempDirectoryScoped" => H::FsMakeTempDirectoryScoped,
        "FsMakeTempFile" => H::FsMakeTempFile,
        "FsMakeTempFileScoped" => H::FsMakeTempFileScoped,
        "FsRemoveTemp" => H::FsRemoveTemp,
        "FsRemoveTempParent" => H::FsRemoveTempParent,
        "FsOpen" => H::FsOpen,
        "FsClose" => H::FsClose,
        "FsReadDirectory" => H::FsReadDirectory,
        "FsReadFile" => H::FsReadFile,
        "FsReadFileString" => H::FsReadFileString,
        "FsReadLink" => H::FsReadLink,
        "FsRealPath" => H::FsRealPath,
        "FsRemove" => H::FsRemove,
        "FsRename" => H::FsRename,
        "FsStat" => H::FsStat,
        "FsSymlink" => H::FsSymlink,
        "FsTruncate" => H::FsTruncate,
        "FsUtimes" => H::FsUtimes,
        "FsWriteFile" => H::FsWriteFile,
        "FsWriteFileString" => H::FsWriteFileString,
        "FsFileStat" => H::FsFileStat,
        "FsSeek" => H::FsSeek,
        "FsSync" => H::FsSync,
        "FsReadAlloc" => H::FsReadAlloc,
        "FsFileTruncate" => H::FsFileTruncate,
        "FsWrite" => H::FsWrite,
        "FsWriteAll" => H::FsWriteAll,
        "ProcessMake" => H::ProcessMake,
        "ProcessSetCwd" => H::ProcessSetCwd,
        "ProcessSetEnv" => H::ProcessSetEnv,
        "ProcessPrefix" => H::ProcessPrefix,
        "ProcessPipeTo" => H::ProcessPipeTo,
        "ProcessSetStdin" => H::ProcessSetStdin,
        "ProcessInheritOutput" => H::ProcessInheritOutput,
        "ProcessSpawn" => H::ProcessSpawn,
        "ProcessClose" => H::ProcessClose,
        "ProcessPid" => H::ProcessPid,
        "ProcessWait" => H::ProcessWait,
        "ProcessIsRunning" => H::ProcessIsRunning,
        "ProcessKill" => H::ProcessKill,
        "ProcessKillWith" => H::ProcessKillWith,
        "ProcessReadStdout" => H::ProcessReadStdout,
        "ProcessReadStderr" => H::ProcessReadStderr,
        "ProcessOutputString" => H::ProcessOutputString,
        "ProcessOutputLines" => H::ProcessOutputLines,
        _ => return None,
    })
}
