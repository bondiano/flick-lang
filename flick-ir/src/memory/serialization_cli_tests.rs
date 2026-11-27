#[test]
#[cfg(feature = "json")]
fn cli_intrinsics_round_trip() {
    let mut operations = Vec::new();
    push_intrinsics(
        &mut operations,
        &[
            Intrinsic::Cli(CliIntrinsic::Command),
            Intrinsic::Cli(CliIntrinsic::Description),
            Intrinsic::Cli(CliIntrinsic::Flag),
            Intrinsic::Cli(CliIntrinsic::Arg),
            Intrinsic::Cli(CliIntrinsic::Subcommand),
            Intrinsic::Cli(CliIntrinsic::Handle),
            Intrinsic::Cli(CliIntrinsic::Group),
            Intrinsic::Cli(CliIntrinsic::Required),
            Intrinsic::Cli(CliIntrinsic::Optional),
            Intrinsic::Cli(CliIntrinsic::Many),
            Intrinsic::Cli(CliIntrinsic::RunVersion),
        ],
    );
    round_trip_operations(&operations);
}
