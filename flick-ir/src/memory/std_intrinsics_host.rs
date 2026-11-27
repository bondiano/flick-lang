/// Native Rust host service intrinsics (`File`, `FileSystem`, `ChildProcess`)
/// shared across all VM tiers via `flick_vm::stdlib::host`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostServiceIntrinsic {
    // File (`packages/std/src/File.fl`)
    FileOpen,
    FileClose,
    FileReadHandle,
    FileRead,
    FileWrite,

    // FileSystem (`packages/std/src/FileSystem.fl`)
    FsAccess,
    FsCopy,
    FsCopyFile,
    FsChmod,
    FsChown,
    FsGlob,
    FsExists,
    FsLink,
    FsMakeDirectory,
    FsMakeTempDirectory,
    FsMakeTempDirectoryScoped,
    FsMakeTempFile,
    FsMakeTempFileScoped,
    FsRemoveTemp,
    FsRemoveTempParent,
    FsOpen,
    FsClose,
    FsReadDirectory,
    FsReadFile,
    FsReadFileString,
    FsReadLink,
    FsRealPath,
    FsRemove,
    FsRename,
    FsStat,
    FsSymlink,
    FsTruncate,
    FsUtimes,
    FsWriteFile,
    FsWriteFileString,

    // FileSystem open-file operations
    FsFileStat,
    FsSeek,
    FsSync,
    FsReadAlloc,
    FsFileTruncate,
    FsWrite,
    FsWriteAll,

    // ChildProcess & Host.Process (`packages/process/src/ChildProcess.fl`)
    ProcessMake,
    ProcessSetCwd,
    ProcessSetEnv,
    ProcessPrefix,
    ProcessPipeTo,
    ProcessSetStdin,
    ProcessInheritOutput,
    ProcessSpawn,
    ProcessClose,
    ProcessPid,
    ProcessWait,
    ProcessIsRunning,
    ProcessKill,
    ProcessKillWith,
    ProcessReadStdout,
    ProcessReadStderr,
    ProcessOutputString,
    ProcessOutputLines,
}

impl HostServiceIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::FileOpen
            | Self::FileClose
            | Self::FileReadHandle
            | Self::FileRead
            | Self::FsExists
            | Self::FsMakeTempDirectory
            | Self::FsMakeTempDirectoryScoped
            | Self::FsMakeTempFile
            | Self::FsMakeTempFileScoped
            | Self::FsRemoveTemp
            | Self::FsRemoveTempParent
            | Self::FsClose
            | Self::FsReadFile
            | Self::FsReadFileString
            | Self::FsReadLink
            | Self::FsRealPath
            | Self::FsStat
            | Self::FsFileStat
            | Self::FsSync
            | Self::ProcessInheritOutput
            | Self::ProcessSpawn
            | Self::ProcessClose
            | Self::ProcessPid
            | Self::ProcessWait
            | Self::ProcessIsRunning
            | Self::ProcessKill
            | Self::ProcessReadStdout
            | Self::ProcessReadStderr
            | Self::ProcessOutputString
            | Self::ProcessOutputLines => 1,

            Self::FileWrite
            | Self::FsAccess
            | Self::FsCopyFile
            | Self::FsChmod
            | Self::FsGlob
            | Self::FsLink
            | Self::FsMakeDirectory
            | Self::FsOpen
            | Self::FsReadDirectory
            | Self::FsRemove
            | Self::FsRename
            | Self::FsSymlink
            | Self::FsTruncate
            | Self::FsFileTruncate
            | Self::FsWrite
            | Self::FsWriteAll
            | Self::ProcessMake
            | Self::ProcessSetCwd
            | Self::ProcessSetEnv
            | Self::ProcessPipeTo
            | Self::ProcessSetStdin
            | Self::ProcessKillWith => 2,

            Self::FsCopy
            | Self::FsChown
            | Self::FsUtimes
            | Self::FsWriteFile
            | Self::FsWriteFileString
            | Self::FsSeek
            | Self::FsReadAlloc
            | Self::ProcessPrefix => 3,
        }
    }
}
