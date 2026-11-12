use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use raskin::{
    Error, Reason, Result, artifact,
    capture::{self, CaptureReport, Preset},
    codec,
    render::{self, GenerateOptions},
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(
    version,
    about = "Read macOS preferences into KDL and generate native commands",
    long_about = "Capture persisted macOS preferences as editable KDL, then generate a standalone .macos file. Raskin never executes generated commands. Capture is broad by default; desktop, input, and power select curated subsets.",
    arg_required_else_help = true,
    after_help = "Examples:\n  raskin capture -o myprefs.kdl\n  raskin capture --preset desktop -o desktop.kdl\n  raskin generate myprefs.kdl -o myprefs.macos\n\nReview the generated file before running it yourself. See docs/coverage.md for limits."
)]
struct Cli {
    /// Emit the jdx/usage interface specification.
    #[arg(long, hide = true)]
    usage_spec: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    #[command(
        about = "Read persisted preferences into KDL",
        long_about = "Read explicit persisted user, host, system and container preferences plus native power profiles. Unavailable settings are omitted from KDL; capture diagnostics go to stderr. No settings are changed. With --from, only the supplied offline tree is read; there is no live fallback.",
        after_help = "Examples:\n  raskin capture --preset input -o input.kdl\n  raskin capture --from ./saved-inputs -o saved.kdl"
    )]
    Capture {
        /// Show per-source capture diagnostics on stderr.
        #[arg(short, long)]
        verbose: bool,
        /// Capture broad settings or a curated subset.
        #[arg(long, default_value="broad", value_parser=["broad","desktop","input","power"])]
        preset: String,
        /// Read an offline tree containing user/, system/, containers/, host-id.txt, and pmset.txt.
        #[arg(long, value_name="DIRECTORY", value_hint=ValueHint::DirPath)]
        from: Option<PathBuf>,
        /// Write KDL to this path; otherwise use stdout.
        #[arg(short, long, value_name="FILE", value_hint=ValueHint::FilePath)]
        output: Option<PathBuf>,
        /// Atomically replace an existing output file.
        #[arg(long, requires = "output")]
        force: bool,
    },
    #[command(
        about = "Generate a standalone .macos file from saved KDL",
        long_about = "Validate saved KDL and emit native commands for a human to inspect and run. Generation never reads host settings or executes commands. Unsupported values fail before any output unless --skip-unsupported explicitly requests a partial script.",
        after_help = "Examples:\n  raskin generate myprefs.kdl -o myprefs.macos\n  raskin generate myprefs.kdl --skip-unsupported -o partial.macos\n  raskin generate - < myprefs.kdl"
    )]
    Generate {
        /// KDL input path, or - to read stdin.
        #[arg(value_hint=ValueHint::FilePath)]
        input: PathBuf,
        /// Write the script to this path; otherwise use stdout.
        #[arg(short, long, value_name="FILE", value_hint=ValueHint::FilePath)]
        output: Option<PathBuf>,
        /// Atomically replace an existing output file.
        #[arg(long, requires = "output")]
        force: bool,
        /// Omit unsupported settings and name every omission in comments and diagnostics.
        #[arg(long)]
        skip_unsupported: bool,
    },
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    if cli.usage_spec {
        let mut spec = Vec::new();
        clap_usage::generate(&mut Cli::command(), "raskin", &mut spec);
        while spec.last() == Some(&b'\n') {
            spec.pop();
        }
        spec.push(b'\n');
        return emit(None, &spec, false, false);
    }
    match cli.command {
        Some(Commands::Capture {
            verbose,
            preset,
            from,
            output,
            force,
        }) => {
            let preset = match preset.as_str() {
                "desktop" => Preset::Desktop,
                "input" => Preset::Input,
                "power" => Preset::Power,
                _ => Preset::Broad,
            };
            let report = match from {
                Some(root) => capture::capture_offline(&root, preset)?,
                None => capture::capture_live(preset)?,
            };
            let text = codec::encode(&report.document)?;
            emit(output.as_deref(), text.as_bytes(), force, false)?;
            capture_summary(&report, output.as_deref(), preset, verbose);
            Ok(())
        }
        Some(Commands::Generate {
            input,
            output,
            force,
            skip_unsupported,
        }) => {
            if input != Path::new("-")
                && let Some(path) = &output
            {
                artifact::reject_alias(&input, path)?;
            }
            let mut bytes = Vec::new();
            let limit = raskin::model::MAX_DOCUMENT_BYTES as u64 + 1;
            if input == Path::new("-") {
                std::io::stdin()
                    .take(limit)
                    .read_to_end(&mut bytes)
                    .map_err(|e| Error::io("read input", "<stdin>", e))?;
            } else {
                std::fs::File::open(&input)
                    .and_then(|f| f.take(limit).read_to_end(&mut bytes))
                    .map_err(|e| Error::io("read input", &input, e))?;
            }
            if bytes.len() > raskin::model::MAX_DOCUMENT_BYTES {
                return Err(Error::Invalid("input exceeds the 128 MiB limit".into()));
            }
            let text = String::from_utf8(bytes).map_err(|e| {
                Error::Invalid(format!("{}: KDL must be UTF-8: {e}", input.display()))
            })?;
            let document = codec::decode(&text, &input.to_string_lossy())?;
            let generated = render::generate(&document, GenerateOptions { skip_unsupported })?;
            for message in generated.diagnostics {
                eprintln!("{message}");
            }
            emit(output.as_deref(), generated.script.as_bytes(), force, true)
        }
        None => Err(Error::Invalid(
            "choose capture or generate; see --help".into(),
        )),
    }
}
fn emit(path: Option<&Path>, bytes: &[u8], force: bool, executable: bool) -> Result<()> {
    match path {
        Some(path) => artifact::write(path, bytes, force, executable),
        None => std::io::stdout()
            .lock()
            .write_all(bytes)
            .map_err(|e| Error::io("write stdout", "<stdout>", e)),
    }
}

/// Summaries belong on stderr, and only after the artifact was written.
fn capture_summary(report: &CaptureReport, output: Option<&Path>, preset: Preset, verbose: bool) {
    let partial = if report.omissions.is_empty() {
        ""
    } else {
        "partial "
    };
    match output {
        Some(path) => eprintln!("Saved {partial}capture to {path:?}."),
        None => eprintln!("Wrote {partial}capture to stdout."),
    }
    let keys: usize = report
        .document
        .domains
        .values()
        .chain(report.document.power.values())
        .map(|values| values.len())
        .sum();
    let profiles = if report.document.power.is_empty() {
        "none".to_owned()
    } else {
        report
            .document
            .power
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    eprintln!(
        "Captured {keys} settings across {} preference domains; power profiles: {profiles}. Preset: {preset}.",
        report.document.domains.len()
    );
    if !report.omissions.is_empty() {
        let counts = [
            (Reason::PermissionDenied, "permission denied"),
            (Reason::Unsupported, "unsupported"),
            (Reason::Missing, "missing"),
            (Reason::ReadFailed, "read failures"),
        ]
        .into_iter()
        .filter_map(|(reason, label)| {
            let count = report
                .omissions
                .iter()
                .filter(|entry| entry.reason == reason)
                .count();
            (count > 0).then(|| format!("{count} {label}"))
        })
        .collect::<Vec<_>>()
        .join(", ");
        eprintln!("Skipped sources/fields: {counts}.");
    }
    let failed_profiles = ["battery", "charger", "ups"]
        .into_iter()
        .filter(|profile| {
            report.omissions.iter().any(|entry| {
                entry.reason == Reason::ReadFailed && entry.source == format!("pmset:{profile}")
            })
        })
        .collect::<Vec<_>>();
    if !failed_profiles.is_empty() {
        eprintln!(
            "Power profiles not captured: {}.",
            failed_profiles.join(", ")
        );
    }
    if verbose {
        for message in &report.diagnostics {
            eprintln!("{message}");
        }
    } else if !report.omissions.is_empty() {
        eprintln!("Use --verbose for per-source diagnostics.");
    }
}
